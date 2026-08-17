pub mod breakpoint;
pub mod error;
pub mod syscalls;

use nix::libc::user_regs_struct;

pub type ProcessId = nix::unistd::Pid;
pub type PlatformBreakpoint = breakpoint::BreakPoint;
pub type PlatformRegStruct = user_regs_struct;
