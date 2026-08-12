#[cfg(target_os = "linux")]
pub type SystemError = crate::session::linux::LinuxError;

#[cfg(not(target_os = "linux"))]
#[derive(Debug, Error)]
pub enum SystemError {
    #[error("Not handled yet")]
    Error,
}
