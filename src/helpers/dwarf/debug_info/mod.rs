pub mod utils;

/*
 * Based on the 'simple.rs' example from the gimli project.
 * Source: https://github.com/gimli-rs/gimli/blob/main/crates/examples/src/bin/simple.rs
 * * The implementation below was adapted to support specific address lookup
 * and integrated into the project's native debugger architecture.
 */

use rustc_hash::FxHashMap;

use gimli::{Encoding, EndianSlice, Expression, RunTimeEndian};
use object::BinaryFormat;

use crate::helpers::dwarf::debug_info::utils::lookup_vars;
use crate::helpers::dwarf::evaluate_frame_base_bytes;
use crate::interface::{DebugStructField, DebugValue, RegisterViewer, to_buffer};
use crate::session::ProcessId;

/// Store different binary format to extract register values safely
#[derive(Debug, Default)]
pub enum Abi {
    #[default]
    SystemV,
    WindowsMsvc,
    Unknown,
}

impl Abi {
    /// Generate Abi from binary format
    pub fn new(format: BinaryFormat) -> Self {
        match format {
            BinaryFormat::Xcoff => Self::SystemV,
            BinaryFormat::Elf => Self::SystemV,
            BinaryFormat::MachO => Self::SystemV,
            BinaryFormat::Coff => Self::WindowsMsvc,
            _ => Self::Unknown,
        }
    }

    /// Get register value using the dw_register index
    /// Use ABI to differentiate between different binary layout
    pub fn get_register_value(&self, dw_reg: u16, registers: &RegisterViewer) -> u64 {
        let raw_regs = registers.regs;

        match self {
            Abi::SystemV => {
                // Linux, macOS, BSD, Solaris
                match dw_reg {
                    6 => raw_regs.rbp,
                    7 => raw_regs.rsp,
                    _ => unimplemented!(),
                }
            }
            Abi::WindowsMsvc => match dw_reg {
                13 => raw_regs.rbp,
                23 => raw_regs.rsp,
                _ => unimplemented!(),
            },
            _ => unimplemented!("Does not support abi yet"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub discr_value: Option<u8>,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub type_offset: usize,
    pub location: u64,
}

#[derive(Debug, Clone)]
pub struct GenericField {
    pub name: String,
    pub type_offset: usize,
}

#[derive(Debug, Clone)]
pub struct Enumerator {
    name: String,
    value: u64,
}

/// Store different variations of data that can be generated from the dwarf data
#[derive(Debug)]
pub enum DwarfType {
    /// Primitive types such as (int, float ...)
    Base {
        name: String,
        encoding: u8,
        byte_size: u64,
    },

    /// Pointer type
    Pointer {
        name: Option<String>,
        target_type_offset: usize,
    },

    /// Constant type
    Const { target_type_offset: usize },

    /// Array type
    Array {
        target_type_offset: usize,
        count: u64,
    },

    // Structures such as String, &str, Box ...
    Structure {
        name: String,
        byte_size: u64,
        alignment: u64,
        generics: Vec<GenericField>,
        fields: Vec<StructField>,
    },

    // Basic C like enums
    Enum {
        name: String,
        byte_size: u64,
        fields: Vec<Enumerator>,
    },
    // Rust like enums, with fields
    Variant {
        name: String,
        byte_size: u64,
        alignment: u64,
        discr_member_offset: Option<u64>,
        variants: Vec<EnumVariant>,
    },
}

impl DwarfType {
    fn get_byte_size(&self, type_index: &FxHashMap<usize, TypeCacheNode>) -> u64 {
        match self {
            DwarfType::Base { byte_size, .. } => *byte_size,

            // Pointer has a fixed size
            DwarfType::Pointer { .. } => 8,

            // TODO: this will probably change
            DwarfType::Const { .. } => 8,

            // Recursively find the size and multiply by count all the way down
            DwarfType::Array {
                count,
                target_type_offset,
            } => {
                let ty = type_index.get(target_type_offset).unwrap();
                let resolved_size = ty.dwarf_type.get_byte_size(type_index);

                count * resolved_size
            }

            DwarfType::Enum { byte_size, .. } => *byte_size,

            DwarfType::Structure { byte_size, .. } => *byte_size,
            DwarfType::Variant { byte_size, .. } => *byte_size,
        }
    }
}

impl DwarfType {
    pub fn get_name(&self) -> String {
        match self {
            DwarfType::Base { name, .. } => name.to_owned(),
            DwarfType::Pointer { .. } => "ptr".to_owned(),
            DwarfType::Const { .. } => "const".to_owned(),
            DwarfType::Array { .. } => "array".to_owned(),
            DwarfType::Enum { name, .. } => name.to_owned(),
            DwarfType::Structure { name, .. } => name.to_owned(),
            DwarfType::Variant { name, .. } => name.to_owned(),
        }
    }
    pub fn to_debug_value(
        &self,
        type_index: &FxHashMap<usize, TypeCacheNode>,
        address: u64,
        pid: ProcessId,
    ) -> Option<DebugValue> {
        match self {
            DwarfType::Base {
                name,
                encoding,
                byte_size,
            } => {
                let raw_data = crate::session::linux::peek_data(pid, address);

                if name == "usize" {
                    return Some(DebugValue::Usize(raw_data as u64));
                } else if name == "isize" {
                    return Some(DebugValue::Isize(raw_data as i64));
                }
                match encoding {
                    // Boolean
                    2 => {
                        let masked = raw_data & 0xFF;
                        Some(DebugValue::Boolean(masked != 0))
                    }

                    // Float
                    4 => match byte_size {
                        4 => {
                            let bits_32 = (raw_data & 0xFFFF_FFFF) as u32;
                            let float_val = f32::from_bits(bits_32);
                            Some(DebugValue::Float(float_val as f64))
                        }
                        8 => {
                            let bits_64 = raw_data as u64;
                            let float_val = f64::from_bits(bits_64);
                            Some(DebugValue::Float(float_val))
                        }
                        _ => None,
                    },

                    // Signed integers
                    5 => match byte_size {
                        1 => Some(DebugValue::Integer((raw_data as i8) as i64)),
                        2 => Some(DebugValue::Integer((raw_data as i16) as i64)),
                        4 => Some(DebugValue::Integer((raw_data as i32) as i64)),
                        8 => Some(DebugValue::Integer(raw_data)),
                        _ => Some(DebugValue::Integer(raw_data)),
                    },

                    // Unsigned integers
                    7 => match byte_size {
                        1 => Some(DebugValue::Unsigned((raw_data as u8) as u64)),
                        2 => Some(DebugValue::Unsigned((raw_data as u16) as u64)),
                        4 => Some(DebugValue::Unsigned((raw_data as u32) as u64)),
                        8 => Some(DebugValue::Unsigned(raw_data as u64)),
                        _ => Some(DebugValue::Unsigned(raw_data as u64)),
                    },

                    // Chars
                    16 => {
                        let raw_char = raw_data as u32;
                        if let Some(char) = char::from_u32(raw_char) {
                            return Some(DebugValue::Char(char));
                        };

                        None
                    }
                    _ => None,
                }
            }
            // Pointer
            DwarfType::Pointer {
                name,
                target_type_offset,
            } => {
                let raw_data = crate::session::linux::peek_data(pid, address);
                if let Some(name) = name {
                    if name.starts_with("alloc::boxed::Box<") {
                        let ty = type_index.get(target_type_offset).unwrap();
                        let val = ty
                            .dwarf_type
                            .to_debug_value(type_index, raw_data as u64, pid)
                            .unwrap();
                        return Some(DebugValue::Box(Box::new(val)));
                    }
                }
                Some(DebugValue::Pointer(raw_data as usize))
            }
            // Array
            DwarfType::Array {
                target_type_offset,
                count,
            } => {
                let mut array = Vec::new();
                let ty = type_index.get(target_type_offset).unwrap();

                let offset_num = ty.dwarf_type.get_byte_size(type_index);

                for i in 0..*count {
                    let var =
                        ty.dwarf_type
                            .to_debug_value(type_index, address + offset_num * i, pid);

                    array.push(var.unwrap());
                }
                Some(DebugValue::Array(array))
            }
            DwarfType::Enum {
                name,
                byte_size,
                fields,
            } => {
                let data = crate::session::linux::peek_data(pid, address);

                let const_value = match byte_size {
                    1 => data as u8 as u64,
                    2 => data as u16 as u64,
                    4 => data as u32 as u64,
                    8 | _ => data as u64,
                };

                let inner_name = fields
                    .iter()
                    .find(|f| f.value == const_value)
                    .expect("Could not find target")
                    .name
                    .clone();

                return Some(DebugValue::Enum {
                    name: name.to_owned(),
                    inner_name,
                });
            }
            DwarfType::Variant {
                name,
                byte_size,
                alignment,
                discr_member_offset,
                variants,
            } => {
                let tag_byte = if let Some(discr_member_offset) = discr_member_offset {
                    let tag =
                        crate::session::linux::peek_data(pid, address + discr_member_offset) as u8;

                    Some(tag)
                } else {
                    None
                };

                let active_variant = variants
                    .iter()
                    .find(|v| {
                        if let Some(tag_byte) = tag_byte {
                            if let Some(value) = v.discr_value {
                                return tag_byte == value;
                            }
                        }
                        return false;
                    })
                    .or_else(|| variants.iter().find(|v| v.discr_value == None));

                if let Some(active_variant) = active_variant {
                    if let Some(field_def) = active_variant.fields.first() {
                        let ty = type_index.get(&field_def.type_offset).unwrap();

                        let inner_name = ty.dwarf_type.get_name();

                        let mut inner_value = ty
                            .dwarf_type
                            .to_debug_value(type_index, address + field_def.location, pid)
                            .unwrap();

                        // Handle all possible variations to display names
                        if let DebugValue::Struct {
                            name: ref mut struct_name,
                            fields,
                        } = inner_value
                        {
                            // Skip long rust names with angle brackets
                            if !name.contains("<") {
                                *struct_name = format!("{}::{}", name, struct_name);
                            }

                            // No field because for structs the variant doesnt have the fields but it belongs to the struct
                            // Example Some(12) would be displayed as Option<u32>::Some(12) otherwise
                            if fields.is_empty() {
                                return Some(DebugValue::Variant {
                                    name: struct_name.to_string(),
                                    field: None,
                                });
                            }
                            return Some(DebugValue::Variant {
                                name: inner_name,
                                field: None,
                            });
                        }

                        // Again skip long rust names with angle brackets
                        if !name.contains("<") {
                            return Some(DebugValue::Variant {
                                name: format!("{}::{}", name, inner_name),
                                field: Some(Box::new(inner_value)),
                            });
                        }

                        return Some(DebugValue::Variant {
                            name: inner_name,
                            field: Some(Box::new(inner_value)),
                        });
                    } else {
                        return Some(DebugValue::Err("Could not find active field".to_string()));
                    }
                } else {
                    return Some(DebugValue::Err("Could not find active enum".to_string()));
                }
            }
            DwarfType::Structure {
                name,
                byte_size,
                alignment,
                generics,
                fields,
            } => {
                // Handle base case for known rust types
                if name == "String" {
                    let vec_field = fields.iter().find(|f| f.name == "vec").unwrap();
                    let vec_ty = type_index.get(&vec_field.type_offset).unwrap();

                    let buffer = vec_ty.dwarf_type.to_debug_value(
                        type_index,
                        address + vec_field.location,
                        pid,
                    );

                    if let Some(DebugValue::Vec(buf)) = buffer {
                        let raw_values = to_buffer(&buf);
                        let string = String::from_utf8_lossy(&raw_values).into_owned();

                        return Some(DebugValue::String(string));
                    }
                }

                if name.starts_with("Vec<") {
                    let len_field = fields.iter().find(|f| f.name == "len").unwrap();
                    let len_ty = type_index.get(&len_field.type_offset).unwrap();

                    let len = len_ty.dwarf_type.to_debug_value(
                        type_index,
                        address + len_field.location,
                        pid,
                    );

                    let buf_field = fields.iter().find(|f| f.name == "buf").unwrap();
                    let buf_ty = type_index.get(&buf_field.type_offset).unwrap();

                    let buf = buf_ty.dwarf_type.to_debug_value(
                        type_index,
                        address + buf_field.location,
                        pid,
                    );

                    let value = generics.iter().find(|g| g.name == "T").unwrap();
                    let ty = type_index.get(&value.type_offset).unwrap();

                    let size = ty.dwarf_type.get_byte_size(type_index);

                    if let (
                        Some(DebugValue::RawVecInner {
                            heap_pointer_value,
                            cap,
                        }),
                        Some(DebugValue::Usize(len)),
                    ) = (buf, len)
                    {
                        let mut buffer = Vec::with_capacity(cap as usize);

                        for i in 0..len {
                            let data = ty
                                .dwarf_type
                                .to_debug_value(
                                    type_index,
                                    heap_pointer_value as u64 + i * size,
                                    pid,
                                )
                                .unwrap();
                            buffer.push(data);
                        }
                        return Some(DebugValue::Vec(buffer));
                    }
                }

                if name == "RawVecInner<alloc::alloc::Global>" {
                    let ptr_field = fields.iter().find(|f| f.name == "ptr").unwrap();
                    let ptr_ty = type_index.get(&ptr_field.type_offset).unwrap();

                    let heap_pointer = ptr_ty.dwarf_type.to_debug_value(
                        type_index,
                        address + ptr_field.location,
                        pid,
                    );

                    let cap_field = fields.iter().find(|f| f.name == "cap").unwrap();
                    let cap_ty = type_index.get(&cap_field.type_offset).unwrap();

                    let capacity = cap_ty.dwarf_type.to_debug_value(
                        type_index,
                        address + cap_field.location,
                        pid,
                    );

                    if let (Some(DebugValue::Pointer(ptr)), Some(DebugValue::Usize(cap))) =
                        (heap_pointer, capacity)
                    {
                        return Some(DebugValue::RawVecInner {
                            heap_pointer_value: ptr,
                            cap,
                        });
                    }
                }

                // Filter the field for only those that take space (Ignore phantom data)
                let physical_fields: Vec<&StructField> = fields
                    .iter()
                    .filter(|field| {
                        if let Some(ty) = type_index.get(&field.type_offset) {
                            ty.dwarf_type.get_byte_size(type_index) > 0
                        } else {
                            true
                        }
                    })
                    .collect();

                if physical_fields.len() == 1 {
                    let single_field = physical_fields[0];
                    let ty = type_index.get(&single_field.type_offset);

                    if let Some(ty) = ty {
                        return ty.dwarf_type.to_debug_value(
                            type_index,
                            address + single_field.location,
                            pid,
                        );
                    }
                }

                let mut values = Vec::new();

                for field in fields {
                    let ty = type_index.get(&field.type_offset).unwrap();

                    let value = ty
                        .dwarf_type
                        .to_debug_value(type_index, address + field.location, pid)
                        .unwrap();

                    values.push(DebugStructField {
                        name: field.name.to_string(),
                        value,
                    });
                }

                if name == "&str" {
                    assert!(values.len() == 2);

                    let ptr = &values[0].value;
                    let len = &values[1].value;

                    if let (DebugValue::Pointer(ptr), DebugValue::Usize(len)) = (ptr, len) {
                        let res =
                            crate::session::linux::read_bytes(pid, *ptr as usize, *len as usize);

                        if let Some(res) = res {
                            let string = String::from_utf8_lossy(&res).into_owned();

                            return Some(DebugValue::StringSlice(string));
                        }
                        return None;
                    }
                }

                let is_tuple = fields.iter().all(|f| f.name.starts_with("__"));

                if is_tuple {
                    let tup_values: Vec<DebugValue> =
                        values.iter().map(|f| f.value.clone()).collect();
                    return Some(DebugValue::Tuple(tup_values));
                }

                let structure = DebugValue::Struct {
                    name: name.to_owned(),
                    fields: values,
                };

                return Some(structure);
            }
            _ => todo!(),
        }
    }
}

/// Store values within the current scope whether within a function or inlined
#[derive(Debug)]
pub enum ExecutionScope {
    /// Store values within a function
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

impl ExecutionScope {
    /// Returns the bytes instruction if it is a function and has bytes available
    pub fn get_bytes(&self) -> Option<&Vec<u8>> {
        match self {
            ExecutionScope::Function { bytes, .. } => {
                bytes.as_ref().map_or(None, |bytes| Some(bytes))
            }
            ExecutionScope::Inlined { .. } => None,
        }
    }
}

#[derive(Debug)]
pub struct DebugVariable {
    pub name: String,
    pub target_type_offset: usize,
    pub location: Vec<u8>,
    pub decl_line: u64,
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

/// Current scope of event being executed
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

        let bytes = self.scope.get_bytes();

        if bytes.is_none() {
            unimplemented!()
        }

        let bytes = bytes.unwrap();

        self.variables.iter().for_each(|var| {
            if let Some(value) = var.parse_value(regs, encoding, endian, abi, bytes) {
                values.push(value);
            }
        });

        values
    }

    /// Get value of a specific variable in the current scope
    pub fn get_variable_with_name(&self, name: &str) -> Option<&DebugVariable> {
        self.variables.iter().find(|var| var.name == name)
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
    /// Populate the cache with the debug_info
    pub fn new(binary_path: &str) -> Self {
        let mut default_cache = Self::default();

        // Populate the cache with data
        lookup_vars(binary_path, &mut default_cache);

        // Sort the cache for binary seach later
        default_cache.sort();

        default_cache
    }

    /// Sort the scopes by their low_pc to make binary search possible
    pub fn sort(&mut self) {
        self.execution_scopes.sort_by_key(|node| match &node.scope {
            ExecutionScope::Function { low_pc, .. } => *low_pc,
            ExecutionScope::Inlined { low_pc, .. } => *low_pc,
        });
    }

    /// Binary search to find a range where the pc can fit within a scope
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
