use thiserror::Error;

#[cfg(target_os = "linux")]
pub type SystemError = crate::session::linux::LinuxError;

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

#[cfg(not(target_os = "linux"))]
#[derive(Debug, Error)]
pub enum SystemError {
    #[error("Not handled yet")]
    Error,
}
