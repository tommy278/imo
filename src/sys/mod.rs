pub mod registers;

#[cfg(not(target_os = "linux"))]
use thiserror::Error;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "linux")]
pub use linux as os;


#[derive(Debug, Default)]
pub struct ProcessAddressRanges {
    ranges: Vec<ProcessAddressRange>
}

impl ProcessAddressRanges {
    pub fn from(ranges: Vec<ProcessAddressRange>) -> Self {
        Self { ranges }
    }
    pub fn within_range(&self, address: u64) -> bool {
        self.ranges.iter().any(|r| r.base_address <= address && address < r.max_address)
    }
}

#[derive(Debug, Default)]
pub struct ProcessAddressRange {
    pub base_address: u64,
    pub max_address: u64
}

impl ProcessAddressRange {
    pub fn within_range(&self, address: u64) -> bool {
        println!("{:?}", (self, address));
       self.base_address <= address && address < self.max_address 
    }
}


#[cfg(target_os = "linux")]
pub type SystemError = linux::error::LinuxError;

#[cfg(not(target_os = "linux"))]
#[derive(Debug, Error)]
pub enum DefaultError {
    #[error("Not handled yet")]
    Error,
}

#[cfg(not(target_os = "linux"))]
pub type SystemError = DefaultError;


#[cfg(not(target_os = "linux"))]
// If not supported yet, add dummy values for compilation
pub mod os {
    // Dummy types to satisfy the type aliases
    pub type ProcessId = i32;
    pub type PlatformRegStruct = ();

    use crate::sys::SystemError;

    #[derive(Debug, Clone)]
    pub struct PlatformBreakpoint;

    impl PlatformBreakpoint {
        pub fn new(_absolute_address: u64) -> Self {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn enable(&self, _pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on linux")
        }
        pub fn disable(&self, _pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on linux")
        }
    }

    pub mod syscalls {
        pub(super) type ProcessId = i32;
        pub(super) type PlatformRegStruct = ();
        use crate::sys::SystemError;

        use crate::helpers::dwarf::error::CacheSetupError;
        pub fn send_trap_signal(_pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn get_process_base_address(_pid: ProcessId) -> Result<u64, CacheSetupError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn read_bytes(
            _pid: ProcessId,
            _ptr: usize,
            _len: usize,
        ) -> Result<Vec<u8>, SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn step(_pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn continue_session(_pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn kill_session(_pid: ProcessId) -> Result<(), SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn peek_data(_pid: ProcessId, _address: u64) -> Result<i64, SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }

        pub fn get_regs(_pid: ProcessId) -> Result<PlatformRegStruct, SystemError> {
            unimplemented!("imo debugger only runs on Linux")
        }
    }
}
