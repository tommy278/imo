/*
 * Based on the 'simple.rs' example from the gimli project.
 * Source: https://github.com/gimli-rs/gimli/blob/main/crates/examples/src/bin/simple.rs
 * * The implementation below was adapted to support specific address lookup
 * and integrated into the project's native debugger architecture.
 */

// style: allow verbose lifetimes
#![allow(clippy::needless_lifetimes)]

use gimli::{Encoding, EndianSlice, Expression, Reader as _, RunTimeEndian, constants};
use object::{BinaryFormat, Object, ObjectSection};
use rustc_hash::FxHashMap;
use std::{borrow, error, fs};

use crate::helpers::dwarf::evaluate_frame_base_bytes;
use crate::interface::RegisterViewer;

#[derive(Debug, Default)]
pub enum Abi {
    #[default]
    SystemV,
    WindowsMsvc,
    Unknown,
}

impl Abi {
    fn new(format: BinaryFormat) -> Self {
        match format {
            BinaryFormat::Xcoff => Self::SystemV,
            BinaryFormat::Elf => Self::SystemV,
            BinaryFormat::MachO => Self::SystemV,
            BinaryFormat::Coff => Self::WindowsMsvc,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug)]
pub enum DwarfType {
    /// Primitive types such as (int, float ...)
    Base {
        name: String,
        encoding: u8,
        byte_size: u64,
    },

    Pointer {
        byte_size: u64,
        target_type_offset: usize,
    },

    Const {
        target_type_offset: usize,
    },

    Array {
        target_type_offset: usize,
        sibling_offset: usize,
    },
}

#[derive(Debug)]
pub enum ExecutionScope {
    Function {
        display_name: String,
        linkage_name: String,
        low_pc: u64,
        high_pc: u64,

        // Bytes for the instruction on how to get the frame_base
        bytes: Option<Vec<u8>>,
    },
    Inlined {
        abstract_origin_offset: usize,
        low_pc: u64,
        high_pc: u64,
    },
}

#[derive(Debug)]
pub struct DebugVariable {
    pub name: String,
    pub target_type_offset: usize,
    pub location: Vec<u8>,
}

impl DebugVariable {
    pub fn parse_value(
        &self,
        regs: &RegisterViewer,
        encoding: Encoding,
        endian: RunTimeEndian,
        abi: &Abi,
        bytes: &[u8],
    ) -> Option<u64> {
        let expression = Expression(EndianSlice::new(&self.location, endian));

        let mut evaluation = expression.evaluation(encoding);
        let mut result = evaluation.evaluate().unwrap();

        loop {
            match result {
                gimli::EvaluationResult::RequiresFrameBase => {
                    let frame_base = evaluate_frame_base_bytes(bytes, regs, abi);
                    result = evaluation.resume_with_frame_base(frame_base).unwrap();
                }
                gimli::EvaluationResult::Complete => {
                    let pieces = evaluation.result();

                    if let Some(piece) = pieces.first() {
                        if let gimli::Location::Address { address } = piece.location {
                            return Some(address);
                        }
                    }
                    break;
                }
                gimli::EvaluationResult::RequiresRegister {
                    register,
                    base_type,
                } => {
                    todo!()
                }
                _ => todo!("Other results"),
            }
        }
        None
    }
}

#[derive(Debug)]
pub struct ScopeCacheNode {
    pub scope: ExecutionScope,
    pub offset: usize,
    pub variables: Vec<DebugVariable>,
}

impl ScopeCacheNode {
    pub fn get_addresses(
        &self,
        regs: &RegisterViewer,
        encoding: Encoding,
        endian: RunTimeEndian,
        abi: &Abi,
    ) -> Vec<u64> {
        let mut values = Vec::new();

        let bytes = match &self.scope {
            ExecutionScope::Function { bytes, .. } => bytes.clone(),
            ExecutionScope::Inlined { .. } => None,
        };

        if bytes.is_none() {
            unimplemented!()
        }

        let bytes = bytes.unwrap();

        self.variables.iter().for_each(|var| {
            if let Some(value) = var.parse_value(regs, encoding, endian, abi, &bytes) {
                values.push(value);
            }
        });

        values
    }
}

#[derive(Debug)]
pub struct TypeCacheNode {
    pub dwarf_type: DwarfType,
    pub offset: usize,
}

#[derive(Debug, Default)]
pub struct DebuggerMetadataCache {
    /// List all execution scopes (Functions and inlines subroutines)
    pub execution_scopes: Vec<ScopeCacheNode>,

    /// Global offset for all type layouts
    pub type_index: FxHashMap<usize, TypeCacheNode>,

    // Store additional data
    pub encoding: Option<Encoding>,
    pub endian: RunTimeEndian,
    pub abi: Abi,
}

impl DebuggerMetadataCache {
    pub fn new(binary_path: &str) -> Self {
        let mut default_cache = Self::default();

        // Populate the cache with data
        lookup_vars(binary_path, &mut default_cache);

        // Sort the cache for binary seach later
        default_cache.sort();

        default_cache
    }

    pub fn sort(&mut self) {
        self.execution_scopes.sort_by_key(|node| match &node.scope {
            ExecutionScope::Function { low_pc, .. } => *low_pc,
            ExecutionScope::Inlined { low_pc, .. } => *low_pc,
        });
    }

    pub fn find_scope_by_pc(&self, pc: u64) -> Option<usize> {
        // Binary search to find where this PC would fit based on the sorted low_pc values
        let search_result = self.execution_scopes.binary_search_by(|node| {
            let node_low_pc = match &node.scope {
                ExecutionScope::Function { low_pc, .. } => *low_pc,
                ExecutionScope::Inlined { low_pc, .. } => *low_pc,
            };
            node_low_pc.cmp(&pc)
        });

        // Detarmine starting idx from the binary search result
        let starting_idx = match search_result {
            Ok(exact_idx) => exact_idx,
            Err(insertion_idx) => {
                if insertion_idx == 0 {
                    return None;
                }
                insertion_idx - 1
            }
        };

        // Linear scan backward slightly because multiple inline scopes might share the same low_pc
        for idx in (0..=starting_idx).rev() {
            let node = &self.execution_scopes[idx];
            let (low, high) = match &node.scope {
                ExecutionScope::Function {
                    low_pc, high_pc, ..
                } => (*low_pc, *high_pc),
                ExecutionScope::Inlined {
                    low_pc, high_pc, ..
                } => (*low_pc, *high_pc),
            };

            // Found and index within scope
            if pc >= low && pc < high {
                return Some(idx);
            }
        }

        None
    }
}

// This is a simple wrapper around `object::read::RelocationMap` that implements
// `gimli::read::Relocate` for use with `gimli::RelocateReader`.
// You only need this if you are parsing relocatable object files.
#[derive(Debug, Default)]
struct RelocationMap(object::read::RelocationMap);

impl<'a> gimli::read::Relocate for &'a RelocationMap {
    fn relocate_address(&self, offset: usize, value: u64) -> gimli::Result<u64> {
        Ok(self.0.relocate(offset as u64, value))
    }

    fn relocate_offset(&self, offset: usize, value: usize) -> gimli::Result<usize> {
        <usize as gimli::ReaderOffset>::from_u64(self.0.relocate(offset as u64, value as u64))
    }
}

// The section data that will be stored in `DwarfSections` and `DwarfPackageSections`.
#[derive(Default)]
struct Section<'data> {
    data: borrow::Cow<'data, [u8]>,
    relocations: RelocationMap,
}

// The reader type that will be stored in `Dwarf` and `DwarfPackage`.
// If you don't need relocations, you can use `gimli::EndianSlice` directly.
type Reader<'data> =
    gimli::RelocateReader<gimli::EndianSlice<'data, gimli::RunTimeEndian>, &'data RelocationMap>;

pub fn lookup_vars(binary_path: &str, info_cache: &mut DebuggerMetadataCache) {
    let file = fs::File::open(binary_path).unwrap();
    // SAFETY: This is not safe. `gimli` does not mitigate against modifications to the
    // file while it is being read. See the `memmap2` documentation and take your own
    // precautions. `fs::read` could be used instead if you don't mind loading the entire
    // file into memory.
    let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };
    let object = object::File::parse(&*mmap).unwrap();

    info_cache.abi = Abi::new(object.format());

    // Update endian in info_cache
    info_cache.endian = if object.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };

    dump_file(&object, info_cache).unwrap();
}

fn dump_file(
    object: &object::File,
    info_cache: &mut DebuggerMetadataCache,
) -> Result<(), Box<dyn error::Error>> {
    // Load a `Section` that may own its data.
    fn load_section<'data>(
        object: &object::File<'data>,
        name: &str,
    ) -> Result<Section<'data>, Box<dyn error::Error>> {
        Ok(match object.section_by_name(name) {
            Some(section) => Section {
                data: section.uncompressed_data()?,
                relocations: section.relocation_map().map(RelocationMap)?,
            },
            None => Default::default(),
        })
    }

    // Borrow a `Section` to create a `Reader`.
    fn borrow_section<'data>(
        section: &'data Section<'data>,
        endian: gimli::RunTimeEndian,
    ) -> Reader<'data> {
        let slice = gimli::EndianSlice::new(borrow::Cow::as_ref(&section.data), endian);
        gimli::RelocateReader::new(slice, &section.relocations)
    }

    // Load all of the sections.
    let dwarf_sections = gimli::DwarfSections::load(|id| load_section(object, id.name()))?;

    // Create `Reader`s for all of the sections and do preliminary parsing.
    // Alternatively, we could have used `Dwarf::load` with an owned type such as `EndianRcSlice`.
    let dwarf = dwarf_sections.borrow(|section| borrow_section(section, info_cache.endian));
    // Iterate over the compilation units.
    let mut iter = dwarf.units();
    while let Some(header) = iter.next()? {
        // println!("Unit at <.debug_info+0x{:x}>", header.offset().0);
        let unit = dwarf.unit(header)?;
        let unit_ref = unit.unit_ref(&dwarf);
        dump_unit(unit_ref, info_cache)?;
    }

    Ok(())
}

fn dump_unit(
    unit: gimli::UnitRef<Reader>,
    info_cache: &mut DebuggerMetadataCache,
) -> Result<(), gimli::Error> {
    // Update encoding in cache
    if info_cache.encoding.is_none() {
        info_cache.encoding = Some(unit.encoding());
    }
    // Iterate over the Debugging Information Entries (DIEs) in the unit.
    let mut entries = unit.entries();
    // Keep track of the index of the function being processed inside the vec
    let mut current_scope_idx = None;

    while let Some(entry) = entries.next_dfs()? {
        /* println!(
            "<{}><{:x}> {}",
            entry.depth(),
            entry.offset().0,
            entry.tag()
        ); */

        let offset = entry.offset().0;
        match entry.tag() {
            constants::DW_TAG_base_type => {
                let mut name: Option<String> = None;
                let mut encoding = None;
                let mut byte_size = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_byte_size => {
                            byte_size = attr.value().udata_value();
                        }
                        gimli::DW_AT_encoding => {
                            if let gimli::AttributeValue::Encoding(enc) = attr.value() {
                                encoding = Some(enc.0);
                            }
                        }
                        gimli::DW_AT_name => {
                            if let Ok(str) = unit.attr_string(attr.value()) {
                                name = Some(str.to_string_lossy().unwrap().to_string())
                            }
                        }
                        _ => {
                            continue;
                        }
                    }
                }

                if let (Some(name), Some(encoding), Some(byte_size)) = (name, encoding, byte_size) {
                    let base_type = DwarfType::Base {
                        name,
                        encoding,
                        byte_size,
                    };
                    let cache_node = TypeCacheNode {
                        dwarf_type: base_type,
                        offset,
                    };
                    info_cache.type_index.insert(offset, cache_node);
                }
            }
            constants::DW_TAG_pointer_type => {
                let mut byte_size = None;
                let mut target_type_offset = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_byte_size => {
                            byte_size = attr.value().udata_value();
                        }
                        gimli::DW_AT_type => {
                            if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                                target_type_offset = Some(offset.0);
                            }
                        }
                        _ => continue,
                    }
                }

                if let (Some(byte_size), Some(target_type_offset)) = (byte_size, target_type_offset)
                {
                    let pointer_type = DwarfType::Pointer {
                        byte_size,
                        target_type_offset,
                    };
                    let cache_node = TypeCacheNode {
                        dwarf_type: pointer_type,
                        offset,
                    };
                    info_cache.type_index.insert(offset, cache_node);
                }
            }
            constants::DW_TAG_const_type => {
                let mut target_type_offset = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_type => {
                            if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                                target_type_offset = Some(offset.0);
                            }
                        }
                        _ => continue,
                    }
                }

                if let Some(tto) = target_type_offset {
                    let const_type = DwarfType::Const {
                        target_type_offset: tto,
                    };
                    let cache_node = TypeCacheNode {
                        dwarf_type: const_type,
                        offset,
                    };
                    info_cache.type_index.insert(offset, cache_node);
                }
            }
            constants::DW_TAG_array_type => {
                let mut target_type_offset = None;
                let mut sibling_offset = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_type => {
                            if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                                target_type_offset = Some(offset.0);
                            }
                        }
                        gimli::DW_AT_sibling => {
                            if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                                sibling_offset = Some(offset.0);
                            }
                        }
                        _ => continue,
                    }
                }

                if let (Some(target_type_offset), Some(sibling_offset)) =
                    (target_type_offset, sibling_offset)
                {
                    let array_type = DwarfType::Array {
                        target_type_offset,
                        sibling_offset,
                    };
                    let cache_node = TypeCacheNode {
                        dwarf_type: array_type,
                        offset,
                    };
                    info_cache.type_index.insert(offset, cache_node);
                }
            }
            constants::DW_TAG_variable => {
                let mut name = None;
                let mut target_type_offset = None;
                let mut location = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_name => {
                            if let Ok(str) = unit.attr_string(attr.value()) {
                                name = Some(str.to_string_lossy().unwrap().to_string());
                            }
                        }
                        gimli::DW_AT_type => {
                            if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                                target_type_offset = Some(offset.0);
                            }
                        }
                        gimli::DW_AT_location => {
                            if let gimli::AttributeValue::Exprloc(expression) = attr.value() {
                                let slice = expression.0.inner();
                                location = Some(slice.to_vec());
                            }
                        }
                        _ => continue,
                    }
                }

                if let (Some(name), Some(target_type_offset), Some(location)) =
                    (name, target_type_offset, location)
                {
                    let debug_var = DebugVariable {
                        name,
                        target_type_offset,
                        location,
                    };

                    if let Some(idx) = current_scope_idx {
                        let node: &mut ScopeCacheNode =
                            info_cache.execution_scopes.get_mut(idx).unwrap();

                        node.variables.push(debug_var);
                    }
                }
            }
            constants::DW_TAG_subprogram => {
                let mut low_pc = None;
                let mut high_pc_attr = None;
                let mut display_name = None;
                let mut linkage_name = None;

                let mut bytes = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_low_pc => {
                            if let gimli::AttributeValue::Addr(addr) = attr.value() {
                                low_pc = Some(addr);
                            }
                        }
                        gimli::DW_AT_high_pc => {
                            high_pc_attr = Some(attr.value());
                        }
                        gimli::DW_AT_name => {
                            if let Ok(str) = unit.attr_string(attr.value()) {
                                display_name = Some(str.to_string_lossy().unwrap().to_string());
                            }
                        }
                        gimli::DW_AT_linkage_name => {
                            if let Ok(str) = unit.attr_string(attr.value()) {
                                linkage_name = Some(str.to_string_lossy().unwrap().to_string());
                            }
                        }
                        gimli::DW_AT_frame_base => {
                            if let gimli::AttributeValue::Exprloc(expression) = attr.value() {
                                bytes = Some(expression.0.inner().to_vec());
                            }
                        }
                        _ => continue,
                    }
                }

                // Ignore entries where the low_pc is 0
                if low_pc.is_some_and(|pc| pc == 0) {
                    continue;
                }

                let mut high_pc = None;
                if let (Some(low), Some(high)) = (low_pc, high_pc_attr) {
                    high_pc = match high {
                        gimli::AttributeValue::Addr(addr) => Some(addr),
                        gimli::AttributeValue::Udata(offset) => Some(low + offset),
                        _ => None,
                    };
                }

                if let (Some(low_pc), Some(high_pc), Some(display_name), Some(linkage_name)) =
                    (low_pc, high_pc, display_name, linkage_name)
                {
                    let inlined = ExecutionScope::Function {
                        display_name,
                        linkage_name,
                        low_pc,
                        high_pc,
                        bytes,
                    };
                    let node = ScopeCacheNode {
                        scope: inlined,
                        offset: entry.offset().0,
                        variables: Vec::new(),
                    };

                    info_cache.execution_scopes.push(node);
                    current_scope_idx = Some(info_cache.execution_scopes.len() - 1);
                }
            }
            constants::DW_TAG_inlined_subroutine => {
                let mut low_pc = None;
                let mut high_pc_attr = None;
                let mut abstract_origin_offset = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_low_pc => {
                            if let gimli::AttributeValue::Addr(addr) = attr.value() {
                                low_pc = Some(addr);
                            }
                        }
                        gimli::DW_AT_high_pc => {
                            high_pc_attr = Some(attr.value());
                        }
                        gimli::DW_AT_abstract_origin => {
                            if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                                abstract_origin_offset = Some(offset.0);
                            }
                        }
                        _ => continue,
                    }
                }

                // Ignore entries where the low_pc is 0
                if low_pc.is_some_and(|pc| pc == 0) {
                    continue;
                }

                let mut high_pc = None;
                if let (Some(low), Some(high)) = (low_pc, high_pc_attr) {
                    high_pc = match high {
                        gimli::AttributeValue::Addr(addr) => Some(addr),
                        gimli::AttributeValue::Udata(offset) => Some(low + offset),
                        _ => None,
                    };
                }

                if let (Some(low_pc), Some(high_pc), Some(abstract_origin_offset)) =
                    (low_pc, high_pc, abstract_origin_offset)
                {
                    let inlined = ExecutionScope::Inlined {
                        abstract_origin_offset,
                        low_pc,
                        high_pc,
                    };

                    let node = ScopeCacheNode {
                        scope: inlined,
                        offset: entry.offset().0,
                        variables: Vec::new(),
                    };

                    info_cache.execution_scopes.push(node);
                    current_scope_idx = Some(info_cache.execution_scopes.len() - 1);
                }
            }
            _ => continue,
        }
    }
    Ok(())
}
