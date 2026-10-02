pub mod breakpoint;
pub mod error;
pub mod syscalls;

use nix::libc::user_regs_struct;

pub type ProcessHandle = nix::unistd::Pid; // process id for linux
pub type PlatformBreakpoint = breakpoint::BreakPoint;
pub type PlatformRegStruct = user_regs_struct;
