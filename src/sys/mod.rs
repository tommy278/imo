pub mod registers;

#[cfg(not(target_os = "linux"))]
use thiserror::Error;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub use linux as os;

#[cfg(target_os = "macos")]
pub use macos as os;

#[derive(Debug, Default)]
pub struct ProcessMemoryMap {
    pub ranges: Vec<MemoryRegion>,
}

impl ProcessMemoryMap {
    pub fn from(ranges: Vec<MemoryRegion>) -> Self {
        Self { ranges }
    }
    pub fn is_ip_valid(&self, ip: u64) -> bool {
        self.ranges
            .iter()
            .any(|r| r.within_range(ip) && r.is_executable)
    }
    pub fn is_address_readable(&self, address: u64) -> bool {
        self.ranges
            .iter()
            .any(|r| r.within_range(address) && r.is_readable)
    }
}

#[derive(Debug, Default)]
pub struct MemoryRegion {
    pub start_address: u64,
    pub end_address: u64,
    pub is_readable: bool,
    pub is_executable: bool,
    pub is_writable: bool,
}

impl MemoryRegion {
    pub fn within_range(&self, address: u64) -> bool {
        self.start_address <= address && address < self.end_address
    }
}

#[cfg(target_os = "linux")]
pub type SystemError = linux::error::LinuxError;

#[cfg(target_os = "macos")]
pub type SystemError = macos::error::MacOSError;

#[cfg(all(not(target_os = "linux"), not(target_os = "macos")))]
#[derive(Debug, Error)]
pub enum DefaultError {
    #[error("Not handled yet")]
    Error,
}

#[cfg(all(not(target_os = "linux"), not(target_os = "macos")))]
pub type SystemError = DefaultError;

#[cfg(all(not(target_os = "linux"), not(target_os = "macos")))]
// If not supported yet, add dummy values for compilation
pub mod os {
    // Dummy types to satisfy the type aliases
    pub type ProcessHandle = i32;
    pub type PlatformRegStruct = ();

    use crate::sys::SystemError;

    #[derive(Debug, Clone)]
    pub struct PlatformBreakpoint;

    impl PlatformBreakpoint {
        pub fn new(_absolute_address: u64) -> Self {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn enable(&self, _pid: ProcessHandle) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn disable(&self, _pid: ProcessHandle) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on linux")
        }
    }

    pub mod syscalls {
        pub(super) type ProcessHandle = i32;
        pub(super) type PlatformRegStruct = ();
        use crate::sys::SystemError;

        use crate::dwarf::error::CacheSetupError;
        pub fn send_trap_signal(_pid: ProcessHandle) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn update_process_addresses(
            _: &mut crate::session::DebugSession,
        ) -> Result<(), CacheSetupError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn read_bytes(
            _pid: ProcessHandle,
            _ptr: usize,
            _len: usize,
        ) -> Result<Vec<u8>, SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn step(_pid: ProcessHandle) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn continue_session(_pid: ProcessHandle) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn kill_session(_pid: ProcessHandle) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn peek_data(_pid: ProcessHandle, _address: u64) -> Result<i64, SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn get_regs(_pid: ProcessHandle) -> Result<PlatformRegStruct, SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }
    }
}
