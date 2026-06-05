use std::ffi::CString;
use std::path::Path;

use nix::sys::ptrace;
use nix::sys::signal::{Signal, raise};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{
    ForkResult::{self, Child},
    fork,
};

pub fn debug(exec: &str) {
    let pid = unsafe { fork() }.unwrap();

    match pid {
        ForkResult::Child => {
            ptrace::traceme().unwrap();

            // Stop child to avoid race condition with parent
            raise(Signal::SIGSTOP);

            let path = std::ffi::CString::new(exec).unwrap();
            nix::unistd::execv(&path, &[&path]).expect("Failed to run command");
        }
        ForkResult::Parent { child } => loop {
            let status = waitpid(child, None).unwrap();
            match status {
                WaitStatus::Exited(_, code) => {
                    println!("Child process exited with the code {}", code);
                    break;
                }
                WaitStatus::Stopped(pid, sig) => {
                    if let Ok(regs) = ptrace::getregs(pid) {
                        println!("{:?}", regs);
                    }
                    ptrace::cont(pid, sig).unwrap()
                }
                WaitStatus::Signaled(_, sig, _) => {
                    println!("Child process was killed by {:?} signal", sig);
                    break;
                }
                _ => ptrace::cont(child, None).unwrap(),
            }
        },
    }
}
