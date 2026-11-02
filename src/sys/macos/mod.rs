pub mod breakpoint;
pub mod error;
pub mod syscalls;

#[cfg(target_arch = "x86_64")]
pub use mach2::thread_status::x86_THREAD_STATE64 as THREAD_STATE64;
#[cfg(target_arch = "aarch64")]
pub use mach2::thread_status::ARM_THREAD_STATE64 as THREAD_STATE64;

pub const TRAP_FLAG: u64 = 1u64 << 8;

#[cfg(target_arch = "aarch64")]
use mach2::structs::arm_thread_state64_t as thread_state64_t;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use mach2::structs::x86_thread_state64_t as thread_state64_t;

pub type ProcessHandle = u32; // task_port on mac
pub type PlatformBreakpoint = breakpoint::BreakPoint;
pub type PlatformRegStruct = thread_state64_t;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MessageReply {
    pub header: mach2::message::mach_msg_header_t,
    pub NDR: mach2::ndr::NDR_record_t,
    pub kern_return: mach2::kern_return::kern_return_t,
}
