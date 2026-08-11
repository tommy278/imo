use super::helpers::dwarf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DebuggerError {
    #[error("Error setting up cache: {0}")]
    Cache(#[from] dwarf::error::CacheSetupError),
    #[error("Child exited with the status code: {0}")]
    Exit(i32),
}
