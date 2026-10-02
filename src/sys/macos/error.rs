use thiserror::Error;

#[derive(Debug, Error)]
pub enum MacOSError {
    #[error("Failed to handle command: {0}")]
    Command(#[from] std::io::Error),
}
