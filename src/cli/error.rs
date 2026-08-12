use crate::session::error::SystemError;
use thiserror;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("Failed to execute system command")]
    System(#[from] SystemError),

    #[error("Failed to locate instruction pointer")]
    InstructionPointer,

    #[error("Failed to locate stack pointer")]
    StackPointer,
}
