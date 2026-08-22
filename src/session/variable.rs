use owo_colors::OwoColorize;
use std::{fmt, path};

#[derive(Debug, Clone)]
pub struct DebugStructField {
    pub name: String,
    pub value: DebugValue,
}

#[derive(Clone, Copy, Debug)]
pub enum WrapperKind {
    Box,
    Rc,
    Arc,
}

impl Into<String> for WrapperKind {
    fn into(self) -> String {
        match self {
            Self::Box => "Box".to_string(),
            Self::Rc => "Rc".to_string(),
            Self::Arc => "Arc".to_string(),
        }
    }
}

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
    FilePath(String),
    FilePathBuf(String),
    Boolean(bool),
    Pointer(usize),
    Array(Vec<DebugValue>),
    Vec(Vec<DebugValue>),
    Tuple(Vec<DebugValue>),
    PointerWrapper {
        kind: WrapperKind,
        value: Box<DebugValue>,
    },
    HashMap {
        entries: Vec<(DebugValue, DebugValue)>,
    },
    HashSet {
        elements: Vec<DebugValue>,
    },
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
    RawTableInner {
        bucket_mask: u64,
        ctrl: usize,
        growth_left: u64,
        items: u64,
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
            DebugValue::FilePath(path) => {
                write!(f, "{}(\"{}\")", "Path".bright_yellow(), path.green(),)
            }
            DebugValue::FilePathBuf(path_buf) => {
                write!(f, "{}(\"{}\")", "PathBuf".bright_yellow(), path_buf.green(),)
            }
            DebugValue::Boolean(bool) => write!(f, "{}", bool.yellow()),
            DebugValue::Pointer(ptr) => {
                write!(f, "{:#x}", ptr.bright_blue())?;
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
            DebugValue::HashMap { entries } => {
                write!(f, "{} {{", "HashMap".bright_yellow())?;

                for (key, value) in entries {
                    writeln!(f, "")?;
                    write!(f, "    {}", key)?;
                    write!(f, ":   {},", value)?;
                }

                write!(f, "\n}}")?;

                Ok(())
            }
            DebugValue::HashSet { elements } => {
                write!(f, "{} (", "HashSet".bright_yellow())?;

                let len = elements.len();

                for i in 0..len {
                    write!(f, "{}", elements[i]);

                    if i != len - 1 {
                        write!(f, ",")?;
                    }
                }

                write!(f, ")")?;

                Ok(())
            }
            DebugValue::PointerWrapper { kind, value } => {
                let name: String = (*kind).into();
                write!(f, "{}({})", name.cyan(), value.magenta())
            }
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
                "RawVecInner {{\nptr: {:#x}\ncapacity: {}}}",
                heap_pointer_value.bright_blue(),
                cap.blue()
            ),
            DebugValue::RawParts {
                heap_pointer_value,
                len,
                cap,
            } => write!(
                f,
                "RawParts {{\nptr: {:#x}\nlen: {}\ncap: {}}}",
                heap_pointer_value.bright_blue(),
                len,
                cap
            ),
            DebugValue::RawTableInner {
                bucket_mask,
                ctrl,
                growth_left,
                items,
            } => {
                write!(
                    f,
                    "RawTableInner {{\n bucket_mask: {}\nctrl: {:#x}\ngrowth_left: {}\nitems: {}}}",
                    bucket_mask.blue(),
                    ctrl.bright_blue(),
                    growth_left.blue(),
                    items.blue()
                )
            }
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
