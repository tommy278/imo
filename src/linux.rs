use nix::sys::personality::{self, Persona};
use nix::sys::ptrace;
use nix::sys::signal::{Signal, raise};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, fork};

use crate::cli::handle_user_debugger_menu;
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
                    WaitStatus::Stopped(_pid, Signal::SIGSTOP) => {
                        println!("Stopped! and cmd is {:?}", session.current_cmd);
                        // match session.current_cmd {
                        //     CurrentStopCmd::SingleStep => session.complete_single_step(),
                        //     CurrentStopCmd::StepInto {
                        //         ref start_file,
                        //         start_line,
                        //     } => {
                        //         session.step(start_file.clone(), start_line);
                        //     }
                        //     CurrentStopCmd::StepOver {
                        //         ref start_file,
                        //         start_line,
                        //     } => {
                        //         session.next(start_file.clone(), start_line);
                        //     }
                        //     CurrentStopCmd::SearchingForValidLocation => {
                        //         session.continue_searching()
                        //     }
                        //     _ => println!("{:?}", session.current_cmd),
                        // }
                        if session.current_cmd.is_completed() {
                            println!("{:?}", session.current_location());
                            handle_user_debugger_menu(&mut session);
                        }
                    }
                    WaitStatus::Stopped(pid, sig) => {
                        if let Ok(mut regs) = ptrace::getregs(pid) {
                            // If stop was due to SIGTRAP, do not forward it to the child.
                            // Pass None to let the child continue its execution.
                            if sig == Signal::SIGTRAP {
                                match session.current_cmd {
                                    CurrentStopCmd::StepInto {
                                        ref start_file,
                                        start_line,
                                    } => {
                                        session.step(start_file.clone(), start_line);
                                        continue;
                                    }
                                    CurrentStopCmd::SearchingForValidLocation => {
                                        session.continue_searching();
                                        continue;
                                    }
                                    CurrentStopCmd::Completed => {}
                                    _ => unreachable!("{:?}", session.current_cmd),
                                }
                                // On x86/x86_64 CPU the CPU has already advanced to the next instruction before handing control back

                                // INT3 is exacly 1 byte long so checking the previous byte
                                // signifies whether there was a breakpoint instruction
                                let breakpoint_addr = regs.rip - 1;

                                // Read a word from the child's memory
                                match ptrace::read(pid, breakpoint_addr as ptrace::AddressType) {
                                    Ok(_word) => {
                                        if let Some(bp) =
                                            session.active_breakpoints.get_mut(&breakpoint_addr)
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
                                        println!("Failed to read child's memory: {:?}", err);
                                        ptrace::cont(pid, None).unwrap();
                                    }
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
