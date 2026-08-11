use thiserror::Error;

#[derive(Debug, Error)]
pub enum DebugInfoError {
    #[error("Error reading {0} section")]
    ReadingSection(String),

    #[error("Eror loading section {0}")]
    LoadingSection(#[from] object::Error),

    #[error("Error parsing section: {0}")]
    ParsingSection(#[from] gimli::Error),
}
