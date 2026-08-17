use thiserror::Error;

#[derive(Debug, Error)]
pub enum DebugInfoError {
    #[error("Error reading {0} section")]
    ReadingSection(String),

    #[error("Failed to load debug info section")]
    LoadingSection(#[from] object::Error),

    #[error("Failed to parse DWARF debug info")]
    ParsingSection(#[from] gimli::Error),
}
