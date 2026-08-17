use thiserror::Error;

#[derive(Debug, Error)]
pub enum LinuxError {
    #[error("Failed to execute ptrace command. Errno: {0}")]
    Ptrace(#[from] nix::errno::Errno),
    #[error("Failed to read bytes at given address")]
    ByteRead,
    #[error("Failed to create C string: {0}")]
    CString(#[from] std::ffi::NulError),
}
