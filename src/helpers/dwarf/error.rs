use thiserror::Error;

#[derive(Error, Debug)]
pub enum CacheSetupError {
    #[error("Failed to open the file: {0}")]
    OpeningFile(#[source] std::io::Error),
    #[error("Failed to parse the file")]
    ParsingFile(#[from] object::Error),
    #[error("Failed to memory map the file: {0}")]
    MappingFile(#[from] std::io::Error),
}
