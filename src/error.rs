use super::helpers::dwarf;
use super::session::error::SystemError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DebuggerError {
    #[error("Failed to set up cache: {0}")]
    Cache(#[from] dwarf::error::CacheSetupError),

    #[error("Failed to perform system command: {0}")]
    System(#[from] SystemError),

    #[error("Child exited with the status code: {0}")]
    Exit(i32),
}
