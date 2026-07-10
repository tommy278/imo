/*
 * Based on the 'simple.rs' example from the gimli project.
 * Source: https://github.com/gimli-rs/gimli/blob/main/crates/examples/src/bin/simple.rs
 * * The implementation below was adapted to support specific address lookup
 * and integrated into the project's native debugger architecture.
 */

use gimli::{Reader as _, constants};
use object::{Object, ObjectSection};
use std::{borrow, error, fs};

use crate::helpers::dwarf::debug_info::{
    Abi, DebugVariable, DebuggerMetadataCache, DwarfType, EnumVariant, ExecutionScope,
    ScopeCacheNode, StructField, TypeCacheNode,
};

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
        let offset = entry.offset().0;

        // Parse each entries for the needed values while skipping redundant values
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

                if let Some(target_type_offset) = target_type_offset {
                    let pointer_type = DwarfType::Pointer { target_type_offset };
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

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_type => {
                            if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
                                target_type_offset = Some(offset);
                            }
                        }
                        _ => continue,
                    }
                }

                let mut element_count = None;

                if entry.has_children() {
                    let mut cursor = unit.entries_at_offset(entry.offset()).unwrap();

                    // Skip array entry, move to first child
                    cursor.next_dfs().unwrap();

                    while let Some(entry) = cursor.next_dfs().unwrap() {
                        if entry.tag() == constants::DW_TAG_subrange_type {
                            for attr in entry.attrs() {
                                match attr.name() {
                                    gimli::DW_AT_count => {
                                        element_count = match attr.value() {
                                            gimli::AttributeValue::Data1(v) => Some(v as u64),
                                            _ => None,
                                        };
                                    }
                                    _ => continue,
                                }
                            }
                            break;
                        }
                    }
                    if let (Some(target_type_offset), Some(count)) =
                        (target_type_offset, element_count)
                    {
                        let array_type = DwarfType::Array {
                            target_type_offset: target_type_offset.0,
                            count,
                        };

                        let cache_node = TypeCacheNode {
                            dwarf_type: array_type,
                            offset,
                        };

                        info_cache.type_index.insert(offset, cache_node);
                    }
                }
            }
            constants::DW_TAG_structure_type => {
                let mut name = None;
                let mut byte_size = None;
                let mut alignment = None;

                for attr in entry.attrs() {
                    match attr.name() {
                        gimli::DW_AT_name => {
                            if let Ok(str) = unit.attr_string(attr.value()) {
                                name = Some(str.to_string_lossy().unwrap().to_string());
                            }
                        }
                        gimli::DW_AT_byte_size => {
                            if let gimli::AttributeValue::Udata(size) = attr.value() {
                                byte_size = Some(size);
                            }
                        }
                        gimli::DW_AT_alignment => {
                            if let gimli::AttributeValue::Udata(ali) = attr.value() {
                                alignment = Some(ali);
                            }
                        }
                        _ => continue,
                    }
                }

                let mut is_enum = false;
                let mut discr_member_offset = None;

                // Storage for each enum variants
                let mut enum_variants = Vec::new();

                // Fallback for Niche Optimized Variants that are not captured under the variant path
                // Also doubles down as storage for regular structs
                let mut fallback_fields = Vec::new();

                if entry.has_children() {
                    // Iterate through children
                    let mut cursor = unit.entries_at_offset(entry.offset()).unwrap();
                    cursor.next_dfs().unwrap(); // Skip first since it is the current entry
                    let start_depth = cursor.current().map(|e| e.depth()).unwrap_or_default();

                    while let Some(child_entry) = cursor.next_dfs().unwrap() {
                        if child_entry.depth() <= start_depth {
                            break;
                        }

                        match child_entry.tag() {
                            // Capture fields that are in the Niche Optimized Variants
                            gimli::DW_TAG_member => {
                                let mut type_offset = None;
                                let mut location = None;
                                let mut name = None;

                                for attr in child_entry.attrs() {
                                    match attr.name() {
                                        gimli::DW_AT_name => {
                                            if let Ok(str) = unit.attr_string(attr.value()) {
                                                name = Some(
                                                    str.to_string_lossy().unwrap().to_string(),
                                                );
                                            }
                                        }
                                        gimli::DW_AT_type => {
                                            if let gimli::AttributeValue::UnitRef(offset) =
                                                attr.value()
                                            {
                                                type_offset = Some(offset.0);
                                            }
                                        }
                                        gimli::DW_AT_data_member_location => {
                                            if let gimli::AttributeValue::Udata(loc) = attr.value()
                                            {
                                                location = Some(loc);
                                            }
                                        }
                                        _ => continue,
                                    }
                                }

                                if let (Some(name), Some(type_offset), Some(location)) =
                                    (name, type_offset, location)
                                {
                                    let field = StructField {
                                        name,
                                        type_offset,
                                        location,
                                    };

                                    fallback_fields.push(field);
                                }
                            }
                            // Descend to the variant part
                            gimli::DW_TAG_variant_part => {
                                is_enum = true;

                                if let Some(gimli::AttributeValue::UnitRef(discr_offset)) =
                                    child_entry.attr_value(gimli::DW_AT_discr)
                                {
                                    let discr_entry = unit.entry(discr_offset).unwrap();
                                    if let Some(gimli::AttributeValue::Udata(off)) =
                                        discr_entry.attr_value(gimli::DW_AT_data_member_location)
                                    {
                                        discr_member_offset = Some(off);
                                    }
                                }

                                // Further iterate through the children
                                let mut entries =
                                    unit.entries_at_offset(child_entry.offset()).unwrap();
                                entries.next_dfs().unwrap();

                                let variant_part_depth = entries.current().unwrap().depth();

                                // Keep track of the current variant
                                let mut variant: Option<EnumVariant> = None;

                                while let Some(sub_entry) = entries.next_dfs().unwrap() {
                                    if sub_entry.depth() <= variant_part_depth {
                                        break;
                                    }

                                    match sub_entry.tag() {
                                        gimli::DW_TAG_variant => {
                                            let mut discr_value = None;

                                            for attr in sub_entry.attrs() {
                                                match attr.name() {
                                                    gimli::DW_AT_discr_value => {
                                                        discr_value = match attr.value() {
                                                            gimli::AttributeValue::Data1(v) => {
                                                                Some(v as u8)
                                                            }
                                                            gimli::AttributeValue::Data2(v) => {
                                                                Some(v as u8)
                                                            }
                                                            gimli::AttributeValue::Data4(v) => {
                                                                Some(v as u8)
                                                            }
                                                            gimli::AttributeValue::Data8(v) => {
                                                                Some(v as u8)
                                                            }
                                                            gimli::AttributeValue::Sdata(v) => {
                                                                Some(v as u8)
                                                            }
                                                            _ => panic!(
                                                                "Discr value belongs to a type not parsed yet.\nValue is {:?}",
                                                                attr.value()
                                                            ),
                                                        }
                                                    }
                                                    _ => continue,
                                                }
                                            }

                                            if let Some(var) = variant.take() {
                                                enum_variants.push(var);
                                            }

                                            // Update varaint when encountering the variant tag
                                            // Only store variants with discr_value.
                                            // If it is missing it has been optimized and it will be in the fallback
                                            if let Some(discr_value) = discr_value {
                                                variant = Some(EnumVariant {
                                                    discr_value: Some(discr_value),
                                                    fields: Vec::new(),
                                                });
                                            }
                                        }
                                        gimli::DW_TAG_member => {
                                            let mut type_offset = None;
                                            let mut location = None;
                                            let mut name = None;

                                            for attr in sub_entry.attrs() {
                                                match attr.name() {
                                                    gimli::DW_AT_name => {
                                                        if let Ok(str) =
                                                            unit.attr_string(attr.value())
                                                        {
                                                            name = Some(
                                                                str.to_string_lossy()
                                                                    .unwrap()
                                                                    .to_string(),
                                                            );
                                                        }
                                                    }
                                                    gimli::DW_AT_type => {
                                                        if let gimli::AttributeValue::UnitRef(
                                                            offset,
                                                        ) = attr.value()
                                                        {
                                                            type_offset = Some(offset.0);
                                                        }
                                                    }
                                                    gimli::DW_AT_data_member_location => {
                                                        if let gimli::AttributeValue::Udata(loc) =
                                                            attr.value()
                                                        {
                                                            location = Some(loc);
                                                        }
                                                    }
                                                    _ => continue,
                                                }
                                            }

                                            if let (Some(name), Some(type_offset), Some(location)) =
                                                (name, type_offset, location)
                                            {
                                                let field = StructField {
                                                    name,
                                                    type_offset,
                                                    location,
                                                };

                                                if let Some(ref mut current_variant) = variant {
                                                    current_variant.fields.push(field);
                                                }
                                            }
                                        }
                                        _ => continue,
                                    }
                                }
                                if let Some(var) = variant {
                                    enum_variants.push(var);
                                }
                            }
                            _ => continue,
                        }
                    }
                }

                if let (Some(name), Some(byte_size), Some(alignment)) = (name, byte_size, alignment)
                {
                    if !is_enum {
                        let struct_type = DwarfType::Structure {
                            name,
                            byte_size,
                            alignment,
                            fields: fallback_fields,
                        };

                        let cache_node = TypeCacheNode {
                            dwarf_type: struct_type,
                            offset,
                        };

                        info_cache.type_index.insert(offset, cache_node);
                    } else {
                        // Add the fallback fields to a part of the main field
                        if !fallback_fields.is_empty() {
                            fallback_fields.retain(|fallback| {
                                !enum_variants.iter().any(|variant| {
                                    variant.fields.iter().any(|f| {
                                        f.type_offset == fallback.type_offset
                                            && f.location == fallback.location
                                    })
                                })
                            });
                            // Remove optimized fields, reserved for tuples to use
                            let filtered: Vec<StructField> = fallback_fields
                                .into_iter()
                                .filter(|var| !var.name.starts_with("__"))
                                .collect();

                            // Store the fallback fields
                            enum_variants.push(EnumVariant {
                                discr_value: None,
                                fields: filtered,
                            })
                        }

                        let enum_type = DwarfType::Enum {
                            name,
                            byte_size,
                            alignment,
                            discr_member_offset,
                            variants: enum_variants,
                        };

                        let cache_node = TypeCacheNode {
                            dwarf_type: enum_type,
                            offset,
                        };

                        info_cache.type_index.insert(offset, cache_node);
                    }
                }
            }
            constants::DW_TAG_variable => {
                let mut name = None;
                let mut target_type_offset = None;
                let mut location = None;
                let mut line = None;

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
                        gimli::DW_AT_decl_line => {
                            if let gimli::AttributeValue::Udata(decl_line) = attr.value() {
                                line = Some(decl_line as u64);
                            } else {
                                println!("Skipped: {:?}", attr.value());
                            }
                        }
                        _ => continue,
                    }
                }

                if let (Some(name), Some(target_type_offset), Some(location), Some(decl_line)) =
                    (name, target_type_offset, location, line)
                {
                    let debug_var = DebugVariable {
                        name,
                        target_type_offset,
                        location,
                        decl_line,
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
                        offset,
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
                        offset,
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
