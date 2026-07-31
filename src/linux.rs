use nix::sys::personality::{self, Persona};
use nix::sys::ptrace;
use nix::sys::signal::{Signal, raise};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, fork};

use crate::cli::handle_user_debugger_menu;
use crate::session::linux::{PlatformBreakpoint, peek_data};
use crate::session::{CurrentStopCmd, DebugSession};

/// Begin the parent and child processes
/// Child Process executes the binary
/// Parent Process begins the loop that monitors child process
pub fn debug(binary_path: &str) {
    let fork_result = unsafe { fork() }.unwrap();
    println!("{}", binary_path);

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

            let err = nix::unistd::execv(&path, &[&path]).unwrap_err();
            panic!("Failed to execute process: {}", err);
        }
        ForkResult::Parent { child } => {
            // Catch the initial SIGSTOP from the child
            let _status_1 = waitpid(child, None).unwrap();

            // Tell child to continue to the execv call
            ptrace::cont(child, None).unwrap();

            // Catch the automatic SIGTRAP generated after execv finishes loading
            let _status_2 = waitpid(child, None).unwrap();

            // Setup session cache
            let mut session = DebugSession::new(child, binary_path);

            // Handle initial user input
            handle_user_debugger_menu(&mut session);

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
                                assert!(!session.is_idle());
                                println!("{:?}", session.current_cmd);
                                match session.current_cmd {
                                    CurrentStopCmd::SingleStep => {
                                        session.current_cmd = CurrentStopCmd::Completed;
                                    }
                                    CurrentStopCmd::StepInto {
                                        ref start_file,
                                        start_line,
                                    } => {
                                        // Location is not valid so step until a valid one is found
                                        let Some(current_location) = session.current_location()
                                        else {
                                            session.single_step();
                                            continue;
                                        };

                                        // If the location changed then stop
                                        if &current_location.file != start_file
                                            || current_location.line != start_line
                                        {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        } else {
                                            // Still on the same location keep going
                                            session.single_step();
                                        }
                                    }
                                    CurrentStopCmd::StepOver {
                                        start_cfa,
                                        start_line,
                                        ref start_file,
                                    } => {
                                        let (current_cfa, return_address) =
                                            session.get_cfa_and_ret_addr().unwrap();

                                        let file = start_file.clone();

                                        // If current base pointer is greater, we are not in a child function
                                        // The stepping is complete
                                        if current_cfa > start_cfa {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        }
                                        // If the current base pointer is the same then we're in the same function
                                        // In that case check the start line to ensure we actually moved to a new line
                                        else if current_cfa == start_cfa {
                                            let Some(current_line) =
                                                session.current_location().map(|l| l.line)
                                            else {
                                                session.single_step();
                                                continue;
                                            };

                                            // If we are not on the same line then we are done stepping
                                            if start_line != current_line {
                                                session.current_cmd = CurrentStopCmd::Completed;
                                            } else {
                                                session.single_step();
                                            }
                                        } else {
                                            // If current base pointer is lower than the intial base pointer then we are in a child function
                                            // In that case set a breakpoint at the return address and continue till it is intercepted
                                            let relative_address =
                                                session.get_relative_address(return_address);

                                            session.create_specific_breakpoint(relative_address);

                                            session.current_cmd = CurrentStopCmd::StepOut {
                                                resume_cfa: start_cfa,
                                                start_line,
                                                start_file: file,
                                            };
                                            session.continue_session();
                                        }
                                    }
                                    CurrentStopCmd::StepOut {
                                        resume_cfa,
                                        start_line,
                                        ref start_file,
                                    } => {
                                        let current_cfa =
                                            session.get_cfa_and_ret_addr().map(|c| c.0).unwrap();
                                        let file = start_file.clone();

                                        if current_cfa == resume_cfa {
                                            let breakpoint_addr = regs.rip - 1;

                                            let relative_address =
                                                session.get_relative_address(breakpoint_addr);
                                            session.clear_specific_breakpoint(relative_address);

                                            regs.rip = breakpoint_addr;
                                            ptrace::setregs(pid, regs).unwrap();

                                            session.current_cmd =
                                                CurrentStopCmd::SearchingForNextValidLocation {
                                                    start_line,
                                                    start_file: file,
                                                };
                                            session.single_step();
                                        } else {
                                            session.continue_session();
                                        }
                                    }
                                    CurrentStopCmd::SearchingForNextValidLocation {
                                        start_line,
                                        ref start_file,
                                    } => {
                                        if let Some(l) = session.current_location() {
                                            let current_file = l.file.to_path_buf();

                                            if &current_file != start_file {
                                                session.single_step();
                                            }

                                            if l.line != start_line {
                                                session.current_cmd = CurrentStopCmd::Completed;
                                            }
                                        }

                                        session.single_step();
                                    }
                                    CurrentStopCmd::SearchingForValidLocation => {
                                        // If the location is valid then complete the search
                                        if session.current_location().is_some() {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        } else {
                                            // Location is not valid continue searching
                                            session.single_step();
                                        }
                                    }
                                    CurrentStopCmd::Completed => {
                                        break;
                                    }
                                    CurrentStopCmd::Running | CurrentStopCmd::Continuing => {
                                        // On x86/x86_64 CPU the CPU has already advanced to the next instruction before handing control back

                                        // INT3 is exacly 1 byte long so checking the previous byte
                                        // signifies whether there was a breakpoint instruction
                                        let breakpoint_addr = regs.rip - 1;

                                        // Read a word from the child's memory
                                        match ptrace::read(
                                            pid,
                                            breakpoint_addr as ptrace::AddressType,
                                        ) {
                                            Ok(_word) => {
                                                if let Some(bp) = session
                                                    .active_breakpoints
                                                    .get_mut(&breakpoint_addr)
                                                {
                                                    // Rollback the instruction pointer by 1 byte
                                                    regs.rip = breakpoint_addr;

                                                    // Update pid register for future instruction continuation
                                                    ptrace::setregs(pid, regs).unwrap();

                                                    // Replace the 0xCC (INT3) back with the previous instruction
                                                    bp.breakpoint.disable(pid);

                                                    // Open interactive menu
                                                    handle_user_debugger_menu(&mut session);
                                                } else {
                                                    // It was probably a SIGTRAP from the step
                                                    handle_user_debugger_menu(&mut session);
                                                }
                                            }
                                            Err(err) => {
                                                println!(
                                                    "Failed to read child's memory: {:?}",
                                                    err
                                                );
                                                ptrace::cont(pid, None).unwrap();
                                            }
                                        }
                                    }
                                    _ => todo!(),
                                }
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
