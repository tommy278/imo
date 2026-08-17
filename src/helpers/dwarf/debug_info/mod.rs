pub mod error;
pub mod helpers;
pub mod utils;

/*
 * Based on the 'simple.rs' example from the gimli project.
 * Source: https://github.com/gimli-rs/gimli/blob/main/crates/examples/src/bin/simple.rs
 * * The implementation below was adapted to support specific address lookup
 * and integrated into the project's native debugger architecture.
 */

use gimli::{Encoding, EndianSlice, Expression, RunTimeEndian};
use object::BinaryFormat;
use rustc_hash::FxHashMap;

use crate::helpers::dwarf::debug_info::error::DebugInfoError;
use crate::helpers::dwarf::debug_info::utils::lookup_vars;
use crate::helpers::dwarf::evaluate_frame_base_bytes;
use crate::session::error::VariableParseError;
use crate::sys::SystemError;
use crate::sys::os;
use crate::sys::{
    DebugStructField, DebugValue, os::syscalls, registers::RegisterViewer, to_buffer,
};

pub type Reader<'data> =
    gimli::RelocateReader<gimli::EndianSlice<'data, gimli::RunTimeEndian>, &'data RelocationMap>;

#[derive(Debug, Default)]
pub struct RelocationMap(object::read::RelocationMap);

impl<'a> gimli::read::Relocate for &'a RelocationMap {
    fn relocate_address(&self, offset: usize, value: u64) -> gimli::Result<u64> {
        Ok(self.0.relocate(offset as u64, value))
    }

    fn relocate_offset(&self, offset: usize, value: usize) -> gimli::Result<usize> {
        <usize as gimli::ReaderOffset>::from_u64(self.0.relocate(offset as u64, value as u64))
    }
}

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
        match self {
            Abi::SystemV => {
                // Linux, macOS, BSD, Solaris
                match dw_reg {
                    6 => registers.base_pointer(),
                    7 => registers.stack_pointer(),
                    _ => unimplemented!(),
                }
            }
            Abi::WindowsMsvc => match dw_reg {
                13 => registers.base_pointer(),
                23 => registers.stack_pointer(),
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

macro_rules! get_field {
    ($fields:expr, $target_field:expr) => {
        match $fields.iter().find(|f| f.name == $target_field) {
            Some(field) => field,
            None => {
                return Ok(Some(DebugValue::Err(format!(
                    "Failed to get field: {}",
                    $target_field
                ))))
            }
        }
    };
}

macro_rules! get_type {
    ($type_index:expr, $offset:expr) => {
        match $type_index.get($offset) {
            Some(field) => field,
            None => {
                return Ok(Some(DebugValue::Err(format!(
                    "Failed to get type at offset: {}",
                    $offset
                ))))
            }
        }
    };
}

macro_rules! get_size {
    ($ty:expr, $type_index: expr) => {
        match $ty.dwarf_type.get_byte_size($type_index) {
            Some(size) => size,
            None => {
                return Ok(Some(DebugValue::Err(format!(
                    "Failed to get size of type: {:?}",
                    $ty
                ))))
            }
        }
    };
}

macro_rules! get_value {
    ($fields:expr, $type_index:expr, $target_field:expr, $address: expr, $pid: expr) => {{
        let field = get_field!($fields, $target_field);
        let ty = get_type!($type_index, &field.type_offset);

        let value = ty
            .dwarf_type
            .to_debug_value($type_index, $address + field.location, $pid)?;

        value
    }};
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
    fn get_byte_size(&self, type_index: &FxHashMap<usize, TypeCacheNode>) -> Option<u64> {
        match self {
            DwarfType::Base { byte_size, .. } => Some(*byte_size),

            // Pointer has a fixed size
            // TODO: this changes on different systems
            DwarfType::Pointer { .. } => Some(8),

            // TODO: this will probably change
            DwarfType::Const { .. } => Some(8),

            // Recursively find the size and multiply by count all the way down
            DwarfType::Array {
                count,
                target_type_offset,
            } => {
                let ty = type_index.get(target_type_offset)?;
                let resolved_size = ty.dwarf_type.get_byte_size(type_index)?;

                Some(count * resolved_size)
            }

            DwarfType::Enum { byte_size, .. } => Some(*byte_size),

            DwarfType::Structure { byte_size, .. } => Some(*byte_size),
            DwarfType::Variant { byte_size, .. } => Some(*byte_size),
        }
    }

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
        pid: os::ProcessId,
    ) -> Result<Option<DebugValue>, SystemError> {
        match self {
            DwarfType::Base {
                name,
                encoding,
                byte_size,
            } => {
                let raw_data = syscalls::peek_data(pid, address)?;

                if name == "usize" {
                    return Ok(Some(DebugValue::Usize(raw_data as u64)));
                } else if name == "isize" {
                    return Ok(Some(DebugValue::Isize(raw_data as i64)));
                }
                match encoding {
                    // Boolean
                    2 => {
                        let masked = raw_data & 0xFF;
                        Ok(Some(DebugValue::Boolean(masked != 0)))
                    }

                    // Float
                    4 => match byte_size {
                        4 => {
                            let bits_32 = (raw_data & 0xFFFF_FFFF) as u32;
                            let float_val = f32::from_bits(bits_32);
                            Ok(Some(DebugValue::Float(float_val as f64)))
                        }
                        8 => {
                            let bits_64 = raw_data as u64;
                            let float_val = f64::from_bits(bits_64);
                            Ok(Some(DebugValue::Float(float_val)))
                        }
                        _ => Ok(None),
                    },

                    // Signed integers
                    5 => match byte_size {
                        1 => Ok(Some(DebugValue::Integer((raw_data as i8) as i64))),
                        2 => Ok(Some(DebugValue::Integer((raw_data as i16) as i64))),
                        4 => Ok(Some(DebugValue::Integer((raw_data as i32) as i64))),
                        8 => Ok(Some(DebugValue::Integer(raw_data))),
                        _ => Ok(Some(DebugValue::Integer(raw_data))),
                    },

                    // Unsigned integers
                    7 => match byte_size {
                        1 => Ok(Some(DebugValue::Unsigned((raw_data as u8) as u64))),
                        2 => Ok(Some(DebugValue::Unsigned((raw_data as u16) as u64))),
                        4 => Ok(Some(DebugValue::Unsigned((raw_data as u32) as u64))),
                        8 => Ok(Some(DebugValue::Unsigned(raw_data as u64))),
                        _ => Ok(Some(DebugValue::Unsigned(raw_data as u64))),
                    },

                    // Chars
                    16 => {
                        let raw_char = raw_data as u32;
                        if let Some(char) = char::from_u32(raw_char) {
                            return Ok(Some(DebugValue::Char(char)));
                        };

                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }
            // Pointer
            DwarfType::Pointer {
                name,
                target_type_offset,
            } => {
                let raw_data = syscalls::peek_data(pid, address)?;
                if let Some(name) = name {
                    let ty = get_type!(type_index, target_type_offset);
                    let Some(val) =
                        ty.dwarf_type
                            .to_debug_value(type_index, raw_data as u64, pid)?
                    else {
                        return Ok(None);
                    };

                    if name.starts_with("alloc::boxed::Box<") {
                        return Ok(Some(DebugValue::Box(Box::new(val))));
                    }
                    if name.contains(";") {
                        return Ok(Some(val));
                    }
                }
                Ok(Some(DebugValue::Pointer(raw_data as usize)))
            }
            // Array
            DwarfType::Array {
                target_type_offset,
                count,
            } => {
                let mut array = Vec::new();
                let ty = get_type!(type_index, target_type_offset);

                let offset_num = get_size!(ty, type_index);

                for i in 0..*count {
                    let Some(var) = ty.dwarf_type.to_debug_value(
                        type_index,
                        address + offset_num * i as u64,
                        pid,
                    )?
                    else {
                        return Ok(None);
                    };

                    array.push(var);
                }
                Ok(Some(DebugValue::Array(array)))
            }
            DwarfType::Enum {
                name,
                byte_size,
                fields,
            } => {
                let data = syscalls::peek_data(pid, address)?;

                let const_value = match byte_size {
                    1 => data as u8 as u64,
                    2 => data as u16 as u64,
                    4 => data as u32 as u64,
                    8 | _ => data as u64,
                };

                let Some(inner_name) = fields
                    .iter()
                    .find(|f| f.value == const_value)
                    .map(|i| i.name.clone())
                else {
                    return Ok(None);
                };

                return Ok(Some(DebugValue::Enum {
                    name: name.to_owned(),
                    inner_name,
                }));
            }
            DwarfType::Variant {
                name,
                discr_member_offset,
                variants,
                ..
            } => {
                let tag_byte = if let Some(discr_member_offset) = discr_member_offset {
                    let tag = syscalls::peek_data(pid, address + discr_member_offset)? as u8;

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
                        let ty = get_type!(type_index, &field_def.type_offset);

                        let inner_name = ty.dwarf_type.get_name();

                        let Some(mut inner_value) = ty.dwarf_type.to_debug_value(
                            type_index,
                            address + field_def.location,
                            pid,
                        )?
                        else {
                            return Ok(None);
                        };

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
                                return Ok(Some(DebugValue::Variant {
                                    name: struct_name.to_string(),
                                    field: None,
                                }));
                            }
                            return Ok(Some(DebugValue::Variant {
                                name: inner_name,
                                field: None,
                            }));
                        }

                        // Again skip long rust names with angle brackets
                        if !name.contains("<") {
                            return Ok(Some(DebugValue::Variant {
                                name: format!("{}::{}", name, inner_name),
                                field: Some(Box::new(inner_value)),
                            }));
                        }

                        return Ok(Some(DebugValue::Variant {
                            name: inner_name,
                            field: Some(Box::new(inner_value)),
                        }));
                    } else {
                        return Ok(Some(DebugValue::Err(
                            "Could not find active field".to_string(),
                        )));
                    }
                } else {
                    return Ok(Some(DebugValue::Err(
                        "Could not find active enum".to_string(),
                    )));
                }
            }
            DwarfType::Structure {
                name,
                generics,
                fields,
                ..
            } => {
                // Handle base case for known rust types
                if name == "String" {
                    let buffer = get_value!(fields, type_index, "vec", address, pid);

                    if let Some(DebugValue::Vec(buf)) = buffer {
                        let raw_values = to_buffer(&buf);
                        let string = String::from_utf8_lossy(&raw_values).into_owned();

                        return Ok(Some(DebugValue::String(string)));
                    }
                }

                if name.starts_with("Vec<") {
                    let len = get_value!(fields, type_index, "len", address, pid);
                    let buf = get_value!(fields, type_index, "buf", address, pid);

                    let value = get_field!(generics, "T");
                    let ty = get_type!(type_index, &value.type_offset);

                    let size = get_size!(ty, type_index);

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
                            let Some(data) = ty.dwarf_type.to_debug_value(
                                type_index,
                                heap_pointer_value as u64 + i * size,
                                pid,
                            )?
                            else {
                                return Ok(None);
                            };
                            buffer.push(data);
                        }
                        return Ok(Some(DebugValue::Vec(buffer)));
                    }
                }

                if name == "RawVecInner<alloc::alloc::Global>" {
                    let heap_pointer = get_value!(fields, type_index, "ptr", address, pid);
                    let capacity = get_value!(fields, type_index, "cap", address, pid);

                    if let (Some(DebugValue::Pointer(ptr)), Some(DebugValue::Usize(cap))) =
                        (heap_pointer, capacity)
                    {
                        return Ok(Some(DebugValue::RawVecInner {
                            heap_pointer_value: ptr,
                            cap,
                        }));
                    }
                }

                // Filter the field for only those that take space (Ignore phantom data)
                let physical_fields: Vec<&StructField> = fields
                    .iter()
                    .filter(|field| {
                        if let Some(ty) = type_index.get(&field.type_offset) {
                            let Some(size) = ty.dwarf_type.get_byte_size(type_index) else {
                                return false;
                            };
                            size > 0
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
                    let ty = get_type!(type_index, &field.type_offset);

                    let Some(value) =
                        ty.dwarf_type
                            .to_debug_value(type_index, address + field.location, pid)?
                    else {
                        return Ok(None);
                    };

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
                        let res = syscalls::read_bytes(pid, *ptr as usize, *len as usize)?;
                        let string = String::from_utf8_lossy(&res).into_owned();
                        return Ok(Some(DebugValue::StringSlice(string)));
                    }
                }

                let is_tuple = fields.iter().all(|f| f.name.starts_with("__"));

                if is_tuple {
                    let tup_values: Vec<DebugValue> =
                        values.iter().map(|f| f.value.clone()).collect();
                    return Ok(Some(DebugValue::Tuple(tup_values)));
                }

                let structure = DebugValue::Struct {
                    name: name.to_owned(),
                    fields: values,
                };

                return Ok(Some(structure));
            }
            _ => todo!(),
        }
    }
}

/// Store values within the current scope whether within a function or inlined
#[derive(Debug, Clone)]
pub enum ExecutionScope {
    /// Store values within a function
    Function {
        display_name: String,
        linkage_name: String,
        // Bytes for the instruction on how to get the frame_base
        bytes: Option<Vec<u8>>,
    },

    Inlined,

    LexicalBlock,
}

impl ExecutionScope {
    /// Returns the bytes instruction if it is a function and has bytes available
    pub fn get_bytes(&self) -> Option<&Vec<u8>> {
        match self {
            ExecutionScope::Function { bytes, .. } => {
                bytes.as_ref().map_or(None, |bytes| Some(bytes))
            }
            ExecutionScope::Inlined | ExecutionScope::LexicalBlock => None,
        }
    }

    pub fn is_inline(&self) -> bool {
        match self {
            ExecutionScope::Inlined => true,
            _ => false,
        }
    }

    pub fn is_func(&self) -> bool {
        match self {
            ExecutionScope::Function { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DebugVariable {
    pub name: String,
    pub target_type_offset: usize,
    pub location: Vec<u8>,
    pub decl_line: u32,
}

impl DebugVariable {
    pub fn parse_value(
        &self,
        regs: &RegisterViewer,
        encoding: Encoding,
        endian: RunTimeEndian,
        abi: &Abi,
        bytes: &[u8],
        type_index: &FxHashMap<usize, TypeCacheNode>,
        pid: os::ProcessId,
    ) -> Result<Option<u64>, VariableParseError> {
        let expression = Expression(EndianSlice::new(&self.location, endian));

        let mut evaluation = expression.evaluation(encoding);
        let mut result = evaluation.evaluate()?;

        loop {
            match result {
                gimli::EvaluationResult::RequiresFrameBase => {
                    let frame_base = evaluate_frame_base_bytes(bytes, regs, abi);
                    result = evaluation.resume_with_frame_base(frame_base)?;
                }
                gimli::EvaluationResult::Complete => {
                    let pieces = evaluation.result();

                    if let Some(piece) = pieces.first() {
                        if let gimli::Location::Address { address } = piece.location {
                            return Ok(Some(address));
                        }
                    }
                    break;
                }
                gimli::EvaluationResult::RequiresMemory {
                    address, base_type, ..
                } => {
                    let offset = base_type.0;

                    // If offset if 0 then it is a generic and gimli can handle that case
                    if offset == 0 {
                        let raw_data = syscalls::peek_data(pid, address)? as u64;
                        let value = gimli::Value::U64(raw_data);
                        result = evaluation.resume_with_memory(value)?;
                    } else {
                        // NOTE: not sure how effectively this works because I cannot produce the case where the value does not have offset 0
                        let Some(ty) = type_index.get(&offset) else {
                            return Ok(None);
                        };
                        let Some(raw_value) =
                            ty.dwarf_type.to_debug_value(type_index, address, pid)?
                        else {
                            return Ok(None);
                        };

                        let value = match raw_value {
                            DebugValue::Integer(int) => gimli::Value::I64(int),
                            DebugValue::Unsigned(unsigned) => gimli::Value::U64(unsigned),
                            _ => unreachable!(),
                        };

                        result = evaluation.resume_with_memory(value)?;
                    }
                }
                gimli::EvaluationResult::RequiresRegister { .. } => {
                    todo!()
                }
                _ => todo!("Other result : {:?}", result),
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct AddressRange {
    pub low_pc: u64,
    pub high_pc: u64,
}

/// Current scope of event being executed
#[derive(Debug, Clone)]
pub struct ScopeCacheNode {
    pub scope: ExecutionScope,
    pub offset: usize,
    pub variables: Vec<DebugVariable>,

    pub ranges: Vec<AddressRange>,
    pub children: Vec<ScopeCacheNode>,
}

impl ScopeCacheNode {
    /// Get value of a specific variable in the current scope
    pub fn get_variable_with_name(&self, name: &str) -> Option<&DebugVariable> {
        self.variables.iter().find(|var| var.name == name)
    }

    /// Check if the pc is within the range of the scope
    pub fn is_in_scope(&self, pc: u64) -> bool {
        self.ranges.iter().any(|r| r.low_pc <= pc && pc < r.high_pc)
    }

    /// Recursively find the scope that captures the pc best
    pub fn find_active_scope(&self, pc: u64) -> Option<&ScopeCacheNode> {
        if !self.is_in_scope(pc) {
            return None;
        }

        for child in self.children.iter() {
            if let Some(deepest_match) = child.find_active_scope(pc) {
                return Some(deepest_match);
            }
        }

        // If no children match then the current scope is the best match
        Some(self)
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

    pub base_addresses: gimli::BaseAddresses,

    pub text_address: u64,

    // Store additional data
    pub encoding: Option<Encoding>,
    pub endian: RunTimeEndian,
    pub abi: Abi,
}

#[derive(Debug, Default)]
pub struct ActiveVariablesContext<'a> {
    pub variables: Vec<&'a DebugVariable>,
    pub frame_base: Option<&'a Vec<u8>>,
}

impl ActiveVariablesContext<'_> {
    /// Search for the first instance of the variable
    pub fn get_variable_with_name(&self, name: &str) -> Option<&DebugVariable> {
        self.variables.iter().find(|n| n.name == name).map(|&v| v)
    }
}

impl DebuggerMetadataCache {
    /// Populate the cache with the debug_info
    pub fn new(object: &object::File<'_>) -> Result<Self, DebugInfoError> {
        let mut default_cache = Self::default();

        // Populate the cache with data
        lookup_vars(&mut default_cache, object)?;

        Ok(default_cache)
    }

    pub fn is_in_inline(&self, pc: u64) -> bool {
        for scope in self.execution_scopes.iter() {
            if scope.is_in_scope(pc) {
                if scope.find_active_scope(pc).is_some() {
                    // Update down the tree
                    // An inline can call a function
                    if scope.scope.is_func() {
                        return false;
                    }
                    if scope.scope.is_inline() {
                        return true;
                    }
                }
            }
        }
        true
    }

    /// Find current variables and frame base with the current pc
    pub fn find_scope_by_pc(&self, pc: u64) -> ActiveVariablesContext<'_> {
        let mut context = ActiveVariablesContext::default();

        for scope in self.execution_scopes.iter() {
            if scope.is_in_scope(pc) {
                if let Some(bytes) = scope.scope.get_bytes() {
                    context.frame_base = Some(bytes);
                }
                if scope.find_active_scope(pc).is_some() {
                    let refs = scope.variables.iter();
                    context.variables.extend(refs);
                }
            }
        }
        context
    }
}
