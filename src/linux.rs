use std::collections::HashMap;

use nix::sys::personality::{self, Persona};
use nix::sys::ptrace;
use nix::sys::signal::{Signal, raise};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, fork};

use crate::helpers::{
    handle_user_debugger_menu,
    linux::{get_process_base_address, lookup_address_by_line},
};
use crate::interface::linux::BreakPoint;

pub fn debug(binary_path: &str, user_breakpoints: &[(&str, u64)]) {
    let fork_result = unsafe { fork() }.unwrap();

    match fork_result {
        ForkResult::Child => {
            // Disable memory randomization
            let mut current_persona = personality::get().unwrap();
            current_persona.insert(Persona::ADDR_NO_RANDOMIZE);
            personality::set(current_persona).unwrap();

            ptrace::traceme().unwrap();

            // Stop child to avoid race condition with parent
            raise(Signal::SIGSTOP).unwrap();

            let path = std::ffi::CString::new(binary_path).unwrap();
            nix::unistd::execv(&path, &[&path]).expect("Failed to run command");
        }
        ForkResult::Parent { child } => {
            // Catch the initial SIGSTOP from the child
            let _status_1 = waitpid(child, None).unwrap();

            // Tell child to continue to the execv call
            ptrace::cont(child, None).unwrap();

            // Catch the automatic SIGTRAP generated after execv finishes loading
            let _status_2 = waitpid(child, None).unwrap();

            let mut active_breakpoints = HashMap::new();
            let base = get_process_base_address(child);

            for (file, line) in user_breakpoints {
                if let Some(offset) = lookup_address_by_line(binary_path, file, *line) {
                    let absolute_address = base + offset;

                    // Create new brekpoint for each line given, enable and also insert them for later use
                    let mut break_point = BreakPoint::new(absolute_address);
                    break_point.enable(child);
                    active_breakpoints.insert(absolute_address, break_point);
                }
            }

            // Let the program run
            ptrace::cont(child, None).unwrap();

            // Enter the event execution loop
            loop {
                let status = waitpid(child, None).unwrap();
                match status {
                    WaitStatus::Exited(_, code) => {
                        println!("Child process exited with the code {}", code);
                        break;
                    }
                    WaitStatus::Stopped(pid, sig) => {
                        if let Ok(mut regs) = ptrace::getregs(pid) {
                            // If stop was due to SIGTRAP, do not forward it to the child.
                            // Pass None to let the child continue its execution.
                            if sig == Signal::SIGTRAP {
                                // On x86/x86_64 CPU the CPU has already advanced to the next
                                // instruction before handing control back

                                // INT3 is exacly 1 byte long so checking the previous byte
                                // signifies whether there was a breakpoint instruction
                                let breakpoint_addr = regs.rip - 1;

                                // Read a word from the child's memory
                                match ptrace::read(pid, breakpoint_addr as ptrace::AddressType) {
                                    Ok(_word) => {
                                        if let Some(bp) =
                                            active_breakpoints.get_mut(&breakpoint_addr)
                                        {
                                            // Rollback the instruction pointer by 1 byte
                                            regs.rip = breakpoint_addr;

                                            // Update pid register for future instruction continuation
                                            ptrace::setregs(pid, regs).unwrap();

                                            // Replace the 0xCC (INT3) back with the previous instruction
                                            bp.disable(pid);

                                            // Open interactive menu
                                            handle_user_debugger_menu(pid);
                                        } else {
                                            // It was a system SIGTRAP, not the breakpoint
                                            ptrace::cont(pid, None).unwrap();
                                        }
                                    }
                                    Err(err) => {
                                        println!("Failed to read child's memory: {:?}", err);
                                        ptrace::cont(pid, None).unwrap();
                                    }
                                }
                            } else if sig == Signal::SIGSTOP {
                                ptrace::cont(pid, None).unwrap()
                            } else {
                                // Forward other unexpected signals (like SIGINT, SIGSEGV) to the child
                                ptrace::cont(pid, sig).unwrap();
                            }
                        }
                    }
                    WaitStatus::Signaled(_, sig, _) => {
                        println!("Child process was killed by {:?} signal", sig);
                        break;
                    }
                    _ => ptrace::cont(child, None).unwrap(),
                }
            }
        }
    }
}
