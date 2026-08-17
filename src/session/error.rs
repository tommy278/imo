use crate::sys::SystemError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VariableParseError {
    #[error("Failed to execute system command: {0}")]
    System(#[from] SystemError),

    #[error("Failed to parse variable expression: {0}")]
    Expression(#[from] gimli::Error),

    #[error("Could not resolve variable address")]
    Address,

    #[error("Could not resolve frame base")]
    FrameBase,

    #[error("Could not resolve encoding")]
    Encoding,
}
