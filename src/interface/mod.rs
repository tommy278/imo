#[cfg(target_os = "linux")]
pub mod linux;

use crate::session::PlatformRegStruct;
use std::{fmt, rc::Rc};

pub struct RegisterViewer {
    pub regs: PlatformRegStruct,
}

impl RegisterViewer {
    pub fn new(regs: PlatformRegStruct) -> Self {
        Self { regs }
    }
}

#[cfg(target_os = "linux")]
impl fmt::Display for RegisterViewer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // For registers display a third column that shows decimal value
        let write_reg = |f: &mut fmt::Formatter<'_>, name: &str, val: u64| -> fmt::Result {
            writeln!(f, "{:<8}0x{:<18x}{}", name, val, val)
        };

        // For stack registers simply display the same hex
        let write_stack_reg = |f: &mut fmt::Formatter<'_>, name: &str, val: u64| -> fmt::Result {
            writeln!(f, "{:<8}0x{:<18x}0x{:x}", name, val, val)
        };

        write_reg(f, "rax", self.regs.rax)?;
        write_reg(f, "rbx", self.regs.rbx)?;
        write_reg(f, "rcx", self.regs.rcx)?;
        write_reg(f, "rdx", self.regs.rdx)?;
        write_reg(f, "rsi", self.regs.rsi)?;
        write_reg(f, "rdi", self.regs.rdi)?;

        write_stack_reg(f, "rbp", self.regs.rbp)?;
        write_stack_reg(f, "rsp", self.regs.rsp)?;

        write_reg(f, "r8", self.regs.r8)?;
        write_reg(f, "r9", self.regs.r9)?;
        write_reg(f, "r10", self.regs.r10)?;
        write_reg(f, "r11", self.regs.r11)?;
        write_reg(f, "r12", self.regs.r12)?;
        write_reg(f, "r13", self.regs.r13)?;
        write_reg(f, "r14", self.regs.r14)?;
        write_reg(f, "r15", self.regs.r15)?;

        write_stack_reg(f, "rip", self.regs.rip)?;

        let eflags_val = self.regs.eflags;
        let mut flags_vec = Vec::new();

        // Perform bitwise masking to check if individual state switches are active
        if (eflags_val & 0x0004) != 0 {
            flags_vec.push("PF");
        }
        if (eflags_val & 0x0040) != 0 {
            flags_vec.push("ZF");
        }
        if (eflags_val & 0x0200) != 0 {
            flags_vec.push("IF");
        }

        // Join the found flags together separated by clean spaces
        let flags_str = if flags_vec.is_empty() {
            String::new()
        } else {
            format!(" {} ", flags_vec.join(" "))
        };

        writeln!(f, "{:<8}0x{:<18x}[{}]", "eflags", eflags_val, flags_str)?;

        write_reg(f, "cs", self.regs.cs)?;
        write_reg(f, "ss", self.regs.ss)?;
        write_reg(f, "ds", self.regs.ds)?;
        write_reg(f, "es", self.regs.es)?;
        write_reg(f, "fs", self.regs.fs)?;
        write_reg(f, "gs", self.regs.gs)?;

        write_reg(f, "fs_base", self.regs.fs_base)?;
        write_reg(f, "gs_base", self.regs.gs_base)?;

        Ok(())
    }
}

// Dummy for mac and windows interface
#[cfg(not(target_os = "linux"))]
impl fmt::Display for RegisterViewer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // This block doesn't access self.regs fields, so the compiler doesn't care that it's empty!
        write!(
            f,
            "Register visualization is only supported on Linux targets."
        )
    }
}

/// Covers all possible return values for the data found
/// Add enums
#[derive(Debug)]
pub enum DebugValue {
    Integer(i64),
    Unsigned(u64),
    Usize(u64),
    Isize(i64),
    Float(f64),
    Char(char),
    StringSlice(String),
    String(String),
    Boolean(bool),
    Pointer(usize),
    Array(Vec<DebugValue>),
    Vec(Vec<DebugValue>),
    Tuple(Vec<DebugValue>),
    RawVecInner {
        heap_pointer_value: usize,
        cap: u64,
    },
    RawParts {
        heap_pointer_value: usize,
        len: u64,
        cap: u64,
    },
    Variant {
        name: String,
        field: Vec<DebugValue>,
    },
    Struct {
        name: String,
        fields: Vec<DebugValue>,
    },
}

impl fmt::Display for DebugValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DebugValue::Integer(int) => write!(f, "{int}"),
            DebugValue::Unsigned(u) => write!(f, "{u}"),
            DebugValue::Usize(usize) => write!(f, "{usize}"),
            DebugValue::Isize(isize) => write!(f, "{isize}"),
            DebugValue::Float(fl) => write!(f, "{fl}"),
            DebugValue::Char(c) => write!(f, "{c}"),
            DebugValue::String(s) => write!(f, "{s}"),
            DebugValue::StringSlice(slice) => write!(f, "{slice}"),
            DebugValue::Boolean(bool) => write!(f, "{bool}"),
            DebugValue::Pointer(ptr) => write!(f, "{ptr}"),
            DebugValue::Array(arr) => write!(f, "{:?}", arr),
            DebugValue::Vec(vec) => write!(f, "{:?}", vec),
            DebugValue::Tuple(tup) => {
                if tup.is_empty() {
                    return Ok(());
                }
                let mut format = String::from("(");

                for t in tup {
                    format.push_str(&format!("{},", t));
                }

                format.pop();
                format.push_str(")");

                writeln!(f, "{format}")
            }
            DebugValue::RawVecInner {
                heap_pointer_value,
                cap,
            } => write!(
                f,
                "RawVecInner {{\nptr: {}\ncapacity: {}}}",
                heap_pointer_value, cap
            ),
            DebugValue::RawParts {
                heap_pointer_value,
                len,
                cap,
            } => write!(
                f,
                "RawParts {{\nptr: {}\nlen: {}\ncap: {}}}",
                heap_pointer_value, len, cap
            ),
            DebugValue::Variant { name, field } => {
                if field.is_empty() {
                    write!(f, "{name}")?;
                    return Ok(());
                }

                let first = field.first().unwrap();

                write!(f, "{}({})", name, first)
            }
            DebugValue::Struct { name, fields } => {
                if fields.is_empty() {
                    write!(f, "{name}")?;
                    return Ok(());
                }

                let mut format = format!("{} {{", name);

                for field in fields {
                    format.push_str(&format!("\n{}", field));
                }

                format.push_str("\n}");

                write!(f, "{format}")
            }
        }
    }
}
