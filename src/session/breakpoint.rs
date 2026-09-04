use crate::sys::os;
use owo_colors::OwoColorize;
use std::path::Path;

use crate::utils::trim_file_path;

#[derive(Debug, Clone)]
pub struct BreakpointTarget {
    pub file: Box<Path>,
    pub relative_address: u64,
}

#[derive(Debug, Clone)]
pub struct BreakpointData {
    pub target: Vec<BreakpointTarget>,
    pub line: u32,
    pub file: Box<Path>,
    pub enabled: bool,
}

impl BreakpointData {
    pub fn from_target(target: Vec<BreakpointTarget>, line: u32, file: &Path) -> Self {
        Self {
            target,
            line,
            file: Box::from(file),
            enabled: true,
        }
    }
}

impl std::fmt::Display for BreakpointData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let enabled = if self.enabled { "y" } else { "n" };

        if self.target.len() == 1 {
            let target = &self.target[0];
            let file_path = trim_file_path(&target.file);

            write!(
                f,
                "breakpoint\tkeep {}\t{:#x} at {}:{}",
                enabled,
                target.relative_address.bright_blue(),
                file_path.green(),
                self.line
            )?;
        } else {
            writeln!(f, "breakpoint\tkeep {}\t<MULTIPLE>", enabled)?;

            for (idx, target) in self.target.iter().enumerate() {
                let user_idx = idx + 1;
                write!(
                    f,
                    "  .{}\t\t     {}\t{:#x} at {}:{}",
                    user_idx.cyan(),
                    enabled,
                    target.relative_address.blue(),
                    trim_file_path(&target.file).green(),
                    self.line
                )?;

                // Avoid an extra trailing newline at the very last location
                if idx < self.target.len() - 1 {
                    writeln!(f)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum BreakpointMutationResult {
    Created { count: u8, target: BreakpointTarget },
    Updated,
    AlreadyInState,
    NotFound,
}

#[derive(Debug)]
pub struct ManagedBreakpoint {
    pub breakpoint: os::PlatformBreakpoint,
    pub ref_count: usize,
}

impl ManagedBreakpoint {
    pub fn new(breakpoint: os::PlatformBreakpoint) -> Self {
        Self {
            breakpoint,
            ref_count: 1,
        }
    }
}
