use thiserror::Error;

#[derive(Debug, Error)]
pub enum MacOSError {
    #[error("Failed to handle command: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("Failed to create C string: {0}")]
    CString(#[from] std::ffi::NulError),
}
