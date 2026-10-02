pub mod breakpoint;
pub mod error;
pub mod syscalls;

pub type ProcessHandle = u32; // task_port on mac
pub type PlatformBreakpoint = breakpoint::BreakPoint;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use mach2::structs::x86_thread_state64_t;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub type PlatformRegStruct = x86_thread_state64_t;

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub type PlatformRegStruct = ();
