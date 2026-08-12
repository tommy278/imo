use thiserror::Error;

use crate::helpers::dwarf::debug_frame::DebugFrameError;
use crate::helpers::dwarf::debug_info::error::DebugInfoError;
use crate::helpers::dwarf::debug_line::DebugLineError;

#[derive(Error, Debug)]
pub enum CacheSetupError {
    #[error("Failed to open the file: {0}")]
    OpeningFile(#[source] std::io::Error),
    #[error("Failed to parse the file")]
    ParsingFile(#[from] object::Error),
    #[error("Failed to memory map the file: {0}")]
    MappingFile(#[from] std::io::Error),
    #[error("Failed to read debug info section")]
    DebugInfo(#[from] DebugInfoError),
    #[error("Failed to read debug line section")]
    DebugLine(#[from] DebugLineError),
    #[error("Failed to read debug frame section")]
    DebugFrame(#[from] DebugFrameError),
}
