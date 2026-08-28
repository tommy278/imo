use crate::utils::display_source_code;
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub struct StringId(u16);

#[derive(Debug, Clone, Copy)]
pub struct SourceLocation {
    pub file: StringId,
    pub line: u32,
}

#[derive(Debug, Default)]
pub struct SourceCodeInfo {
    source_code: String,
    line_offsets: Vec<usize>,
}

pub enum SourceCodeDisplay<'a> {
    FullyResolved {
        source_code: &'a str,
        line_number: u32,
    },
    PartiallyResolved {
        path: Box<Path>,
        line_number: u32,
    },
    CacheCorrupt {
        id: StringId,
    },
    Unresolved,
}

impl std::fmt::Display for SourceCodeDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FullyResolved {
                source_code,
                line_number,
            } => {
                write!(f, "{}\t", line_number.cyan())?;
                display_source_code(f, *source_code)?;
            }
            Self::PartiallyResolved { path, line_number } => write!(
                f,
                "Could not locate {}. Line number: {}.",
                path.display().bright_blue(),
                line_number.cyan()
            )?,
            Self::Unresolved => write!(f, "Could not resolve current location")?,
            Self::CacheCorrupt { id } => write!(
                f,
                "Could not resolve string with id {:?}. Restarting session might fix this issue",
                id
            )?,
        }
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct SourceCodeCache {
    pub entries: FxHashMap<PathBuf, SourceCodeInfo>,
}

impl SourceCodeCache {
    pub fn get_line_entry(&self, path: &Path, line: u32) -> Option<&str> {
        if let Some(entry) = self.entries.get(path) {
            let index = line.saturating_sub(1) as usize;
            let line_offset = entry.line_offsets.get(index)?;
            let next_line_offset = entry.line_offsets.get(index + 1)?;

            return Some(&entry.source_code[*line_offset..*next_line_offset]);
        }

        None
    }

    pub fn create_and_get_line_entry(&mut self, path: &Path, line: u32) -> Option<&str> {
        if !path.exists() {
            return None;
        }

        let file = std::fs::File::open(path).ok()?;
        let reader = BufReader::new(file);

        let mut source_code: String = String::new();
        let mut line_offsets: Vec<usize> = Vec::new();

        for line in reader.lines() {
            line_offsets.push(source_code.len());
            source_code.push_str(line.ok()?.as_ref());
        }

        let source_code_info = SourceCodeInfo {
            source_code,
            line_offsets,
        };

        self.entries.insert(path.into(), source_code_info);
        self.get_line_entry(path, line)
    }
}

pub type FileIndices = FxHashMap<u64, StringId>;

#[derive(Debug, Clone, Default)]
pub struct StringInterner {
    buffer: Vec<String>,
    map: FxHashMap<String, StringId>,
}

impl StringInterner {
    pub fn get_or_intern(&mut self, str: &str) -> StringId {
        if let Some(id) = self.map.get(str) {
            return *id;
        }

        let new_id = StringId(self.buffer.len() as u16);
        self.buffer.push(str.to_string());
        self.map.insert(str.to_string(), new_id);

        new_id
    }

    pub fn get_string(&self, id: StringId) -> Option<String> {
        self.buffer.get(id.0 as usize).cloned()
    }

    pub fn get_str(&self, id: StringId) -> Option<&str> {
        self.buffer.get(id.0 as usize).map(|s| s.as_str())
    }
}

#[derive(Debug)]
pub struct LineRow {
    pub location: SourceLocation,
    pub start_address: u64,
    pub end_address: u64,
    pub is_stmt: bool,
}

// The command to be ran when the debugger hits a sigtrap
#[derive(Default, Debug)]
pub enum CurrentStopCmd {
    SingleStep,
    StepOver {
        start_stack_pointer: u64,
        start_file: StringId,
        start_line: u32,
        started_from_inline: bool,
    },
    StepInto {
        start_file: StringId,
        start_line: u32,
    },
    StepOut {
        original_stack_pointer: u64,
        original_file: StringId,
        original_line: u32,
        return_address: u64,
        started_from_inline: bool,
    },
    FinishStepOver {
        original_stack_pointer: u64,
        original_file: StringId,
        original_line: u32,
        started_from_inline: bool,
    },
    Finish {
        start_stack_pointer: u64,
        started_from_inline: bool,
    },
    StepOutFinish {
        original_stack_pointer: u64,
        return_address: u64,
        started_from_inline: bool,
    },
    CompleteFinish {
        start_stack_pointer: u64,
        started_from_inline: bool,
    },
    #[default]
    Idle,
    Running,
    Continuing,
    SearchingForValidLocation,
    Completed,
}

impl CurrentStopCmd {
    pub fn is_completed(&self) -> bool {
        match self {
            CurrentStopCmd::Completed => true,
            _ => false,
        }
    }

    pub fn is_idle(&self) -> bool {
        match self {
            CurrentStopCmd::Idle => true,
            _ => false,
        }
    }
}
