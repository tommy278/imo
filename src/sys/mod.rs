use owo_colors::OwoColorize;

pub mod registers;

#[cfg(not(target_os = "linux"))]
use thiserror::Error;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "linux")]
pub use linux as os;

#[cfg(target_os = "linux")]
pub type SystemError = linux::error::LinuxError;

#[cfg(not(target_os = "linux"))]
#[derive(Debug, Error)]
pub enum DefaultError {
    #[error("Not handled yet")]
    Error,
}

#[cfg(not(target_os = "linux"))]
pub type SystemError = DefaultError;

#[cfg(not(target_os = "linux"))]
// If not supported yet, add dummy values for compilation
pub mod os {
    // Dummy types to satisfy the type aliases
    pub type ProcessId = i32;
    pub type PlatformRegStruct = ();

    use crate::sys::SystemError;

    #[derive(Debug, Clone)]
    pub struct PlatformBreakpoint;

    impl PlatformBreakpoint {
        pub fn new(_absolute_address: u64) -> Self {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn enable(&self, _pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn disable(&self, _pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on linux")
        }
    }

    pub mod syscalls {
        pub(super) type ProcessId = i32;
        pub(super) type PlatformRegStruct = ();
        use crate::sys::SystemError;

        use crate::helpers::dwarf::error::CacheSetupError;
        pub fn send_trap_signal(_pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn get_process_base_address(_pid: ProcessId) -> Result<u64, CacheSetupError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn read_bytes(
            _pid: ProcessId,
            _ptr: usize,
            _len: usize,
        ) -> Result<Vec<u8>, SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn step(_pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn continue_session(_pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn kill_session(_pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn peek_data(_pid: ProcessId, _address: u64) -> Result<i64, SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn get_regs(_pid: ProcessId) -> Result<PlatformRegStruct, SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }
    }
}

#[derive(Debug, Clone)]
pub struct DebugStructField {
    pub name: String,
    pub value: DebugValue,
}

use std::fmt;

/// Covers all possible return values for the data found
/// Add enums
#[derive(Debug, Clone)]
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
    Box(Box<DebugValue>),
    Enum {
        name: String,
        inner_name: String,
    },
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
        field: Option<Box<DebugValue>>,
    },
    Struct {
        name: String,
        fields: Vec<DebugStructField>,
    },
    Err(String),
}

pub fn to_buffer(collection: &[DebugValue]) -> Vec<u8> {
    let mut buffer = Vec::new();

    for c in collection {
        match c {
            DebugValue::Integer(num) => buffer.push(*num as u8),
            DebugValue::Unsigned(num) => buffer.push(*num as u8),
            _ => continue,
        }
    }

    buffer
}

impl DebugValue {
    fn is_tuple(&self) -> bool {
        match self {
            DebugValue::Tuple(..) => true,
            _ => false,
        }
    }
}

impl fmt::Display for DebugValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DebugValue::Integer(int) => write!(f, "{}", int.blue()),
            DebugValue::Unsigned(u) => write!(f, "{}", u.blue()),
            DebugValue::Usize(usize) => write!(f, "{}", usize.blue()),
            DebugValue::Isize(isize) => write!(f, "{}", isize.blue()),
            DebugValue::Float(fl) => write!(f, "{}", fl.blue()),
            DebugValue::Char(c) => write!(f, "\'{}\'", c.cyan()),
            DebugValue::String(s) => write!(f, "\"{}\"", s.green()),
            DebugValue::StringSlice(slice) => write!(f, "\"{}\"", slice.green()),
            DebugValue::Boolean(bool) => write!(f, "{}", bool.yellow()),
            DebugValue::Pointer(ptr) => {
                write!(f, "{}", "0x".bright_blue())?;
                write!(f, "{:016x}", ptr.bright_blue())?;
                Ok(())
            }
            DebugValue::Array(arr) => {
                if arr.is_empty() {
                    write!(f, "[]")?;
                    return Ok(());
                }
                write!(f, "[")?;

                for (i, val) in arr.iter().enumerate() {
                    write!(f, "{}", val)?;

                    if i != arr.len() - 1 {
                        write!(f, ",")?;
                    }
                }

                write!(f, "]")?;
                Ok(())
            }
            DebugValue::Vec(vec) => {
                if vec.is_empty() {
                    write!(f, "[]")?;
                    return Ok(());
                }
                write!(f, "[")?;

                for (i, val) in vec.iter().enumerate() {
                    write!(f, "{}", val)?;

                    if i != vec.len() - 1 {
                        write!(f, ",")?;
                    }
                }

                write!(f, "]")?;
                Ok(())
            }
            DebugValue::Box(val) => write!(f, "{}({})", "Box".cyan(), val.magenta()),
            DebugValue::Tuple(tup) => {
                if tup.is_empty() {
                    return Ok(());
                }
                write!(f, "(")?;

                for (i, t) in tup.iter().enumerate() {
                    write!(f, "{}", t)?;

                    if i != tup.len() - 1 {
                        write!(f, ",")?;
                    }
                }

                write!(f, "{}", ")".white())?;
                Ok(())
            }
            DebugValue::Enum { name, inner_name } => {
                write!(f, "{}::{}", name.cyan(), inner_name.bright_yellow())
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
                if let Some(field) = field {
                    // Avoid double parentheses from tuple and variant
                    if field.is_tuple() {
                        write!(f, "{}{}", name.cyan(), field)?;
                    } else {
                        write!(f, "{}({})", name.bright_yellow(), field)?;
                    }
                    return Ok(());
                }
                write!(f, "{}", name)
            }
            DebugValue::Struct { name, fields } => {
                if fields.is_empty() {
                    write!(f, "{name}")?;
                    return Ok(());
                }

                write!(f, "{} ", name.bright_blue())?;
                write!(f, "{{")?;

                for field in fields {
                    writeln!(f, "")?;
                    write!(f, "    {}", field.name.bright_red())?;
                    write!(f, ":   {}", field.value)?;
                }
                write!(f, "\n}}")?;
                Ok(())
            }
            DebugValue::Err(err) => write!(f, "{}", err.red()),
        }
    }
}
