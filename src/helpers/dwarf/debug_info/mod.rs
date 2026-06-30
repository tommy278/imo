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
use crate::interface::{DebugValue, RegisterViewer};
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
    Pointer { target_type_offset: usize },

    /// Constant type
    Const { target_type_offset: usize },

    /// Array type
    Array {
        target_type_offset: usize,
        count: u64,
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

            // TODO: Figure out how to handle nested arrays
            DwarfType::Array {
                count,
                target_type_offset,
            } => {
                let ty = type_index.get(target_type_offset).unwrap();
                let resolved_size = ty.dwarf_type.get_byte_size(type_index);

                count * resolved_size
            }
        }
    }
}

impl DwarfType {
    pub fn to_debug_value(
        &self,
        type_index: &FxHashMap<usize, TypeCacheNode>,
        address: u64,
        pid: ProcessId,
        raw_data: i64,
    ) -> Option<DebugValue> {
        match self {
            DwarfType::Base {
                name,
                encoding,
                byte_size,
            } => match encoding {
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
                _ => None,
            },
            // Pointer
            DwarfType::Pointer { target_type_offset } => {
                Some(DebugValue::Pointer(raw_data as usize))
            }
            // TODO: Handle arrays
            DwarfType::Array {
                target_type_offset,
                count,
            } => {
                let mut array = Vec::new();
                let ty = type_index.get(target_type_offset).unwrap();

                let offset_num = ty.dwarf_type.get_byte_size(type_index);

                let mut offset = 0;

                for _ in 0..*count {
                    println!("Offset is {}", offset);
                    let resolved = address + offset;
                    let raw_data = crate::session::linux::peek_data(pid, resolved);
                    let var = ty
                        .dwarf_type
                        .to_debug_value(type_index, resolved, pid, raw_data);

                    offset += offset_num;
                    array.push(var.unwrap());
                }
                Some(DebugValue::Array(array))
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
