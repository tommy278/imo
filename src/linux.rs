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

    match fork_result {
        ForkResult::Child => {
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
                if session.current_cmd.is_completed() {
                    println!("{:?}", session.current_location());
                    handle_user_debugger_menu(&mut session);
                }

                let status = waitpid(child, None).unwrap();
                match status {
                    WaitStatus::Exited(_, code) => {
                        println!("Child process exited with the code {}", code);
                        break;
                    }
                    WaitStatus::Stopped(pid, sig) => {
                        if let Ok(mut regs) = ptrace::getregs(pid) {
                            session.registers = Some(crate::interface::RegisterViewer { regs });
                            // If stop was due to SIGTRAP, do not forward it to the child.
                            // Pass None to let the child continue its execution.
                            if sig == Signal::SIGTRAP {
                                assert!(!session.is_idle());
                                match session.current_cmd {
                                    CurrentStopCmd::SingleStep => {
                                        session.current_cmd = CurrentStopCmd::Completed;
                                    }
                                    CurrentStopCmd::StepInto {
                                        start_file,
                                        start_line,
                                    } => {
                                        // Location is not valid so step until a valid one is found
                                        let Some(current_location) = session.current_location()
                                        else {
                                            session.single_step();
                                            continue;
                                        };

                                        // If the location changed then stop
                                        if current_location.file != start_file
                                            || current_location.line != start_line
                                        {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        } else {
                                            // Still on the same location keep going
                                            session.single_step();
                                        }
                                    }
                                    CurrentStopCmd::StepOver {
                                        start_rsp,
                                        start_file,
                                        start_line,
                                        started_from_inline,
                                    } => {
                                        let current_rsp = regs.rsp;

                                        // If current rsp is greater than the start rsp then we stepped out of a function and should stop there
                                        if current_rsp > start_rsp {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        }
                                        // If rsp are the same then we are in the same function or it was inlined
                                        else if current_rsp == start_rsp {
                                            // We need location so keep stepping until we find a valid one
                                            let Some(line_row) = session
                                                .find_line_range(regs.rip - session.base_address)
                                            else {
                                                session.single_step();
                                                continue;
                                            };

                                            if !line_row.is_stmt {
                                                session.single_step();
                                                continue;
                                            }

                                            let current_location = &line_row.location;

                                            // If its not an inline function just step till we are back in same file but not same line
                                            if !started_from_inline {
                                                if start_file == current_location.file
                                                    && start_line != current_location.line
                                                {
                                                    session.current_cmd = CurrentStopCmd::Completed;
                                                } else {
                                                    session.single_step();
                                                }
                                                continue;
                                            } else {
                                                // If we are in an inline function, behave exactly like a
                                                // normal stepover with the inclusion
                                                // that it can now step out to a function that is not inlined
                                                let pc = regs.rip - session.base_address;
                                                if (start_file == current_location.file
                                                    && start_line != current_location.line)
                                                    || !session.metadata.is_in_inline(pc)
                                                {
                                                    session.current_cmd = CurrentStopCmd::Completed;
                                                } else {
                                                    session.single_step();
                                                }
                                                continue;
                                            }
                                        } else {
                                            // If current rsp is lower then we stepped into a child
                                            // In that case get the return address and continue there
                                            let Some(return_address) = session.get_return_address()
                                            else {
                                                session.single_step();
                                                continue;
                                            };

                                            let relative_address =
                                                session.get_relative_address(return_address);
                                            session.create_specific_breakpoint(relative_address);
                                            session.current_cmd = CurrentStopCmd::StepOut {
                                                original_rsp: start_rsp,
                                                original_file: start_file,
                                                original_line: start_line,
                                                return_address,
                                                started_from_inline,
                                            };
                                            session.continue_session();
                                        }
                                    }
                                    CurrentStopCmd::StepOut {
                                        original_rsp,
                                        original_file,
                                        original_line,
                                        return_address,
                                        started_from_inline,
                                    } => {
                                        // A handshake from the step over
                                        // Simply clear breakpoint and send data back to finish stepover to handle completion
                                        let breakpoint_addr = regs.rip - 1;

                                        let relative_address =
                                            session.get_relative_address(breakpoint_addr);
                                        session.clear_specific_breakpoint(relative_address);

                                        regs.rip = breakpoint_addr;
                                        ptrace::setregs(pid, regs).unwrap();
                                        session.registers =
                                            Some(crate::interface::RegisterViewer { regs });

                                        if return_address == breakpoint_addr {
                                            // If we are at the specific breakpoint to step out from then continue step over like normal
                                            session.current_cmd = CurrentStopCmd::FinishStepOver {
                                                original_rsp,
                                                original_file,
                                                original_line,
                                                started_from_inline,
                                            };
                                            session.single_step();
                                        } else {
                                            // If we are at a different breakpoint continue till we reach the target
                                            session.continue_session();
                                        }
                                    }
                                    CurrentStopCmd::FinishStepOver {
                                        original_rsp,
                                        original_file,
                                        original_line,
                                        started_from_inline,
                                    } => {
                                        // If we stepped out of original function or back to
                                        // original function then complete else go back to stepping over
                                        if original_rsp >= regs.rsp {
                                            session.current_cmd = CurrentStopCmd::StepOver {
                                                start_rsp: original_rsp,
                                                start_file: original_file,
                                                start_line: original_line,
                                                started_from_inline,
                                            };
                                            session.single_step();
                                        } else {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        }
                                    }
                                    CurrentStopCmd::Finish {
                                        start_rsp,
                                        started_from_inline,
                                    } => {
                                        let current_rsp = regs.rsp;

                                        if current_rsp > start_rsp {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        } else if started_from_inline && start_rsp == current_rsp {
                                            let pc = regs.rip - session.base_address;
                                            if !session.metadata.is_in_inline(pc) {
                                                session.current_cmd = CurrentStopCmd::Completed;
                                            } else {
                                                session.single_step();
                                            }
                                            continue;
                                        } else {
                                            let Some(return_address) = session.get_return_address()
                                            else {
                                                session.single_step();
                                                continue;
                                            };

                                            let relative_address =
                                                session.get_relative_address(return_address);
                                            session.create_specific_breakpoint(relative_address);
                                            session.current_cmd = CurrentStopCmd::StepOutFinish {
                                                return_address,
                                                original_rsp: start_rsp,
                                                started_from_inline,
                                            };
                                            session.continue_session();
                                        }
                                    }

                                    CurrentStopCmd::StepOutFinish {
                                        return_address,
                                        original_rsp,
                                        started_from_inline,
                                    } => {
                                        // A handshake from the step over
                                        // Simply clear breakpoint and send data back to finish stepover to handle completion
                                        let breakpoint_addr = regs.rip - 1;

                                        let relative_address =
                                            session.get_relative_address(breakpoint_addr);
                                        session.clear_specific_breakpoint(relative_address);

                                        regs.rip = breakpoint_addr;
                                        ptrace::setregs(pid, regs).unwrap();

                                        session.registers =
                                            Some(crate::interface::RegisterViewer { regs });
                                        if return_address == breakpoint_addr {
                                            // If we are at the specific breakpoint to step out from then continue step over like normal
                                            session.current_cmd = CurrentStopCmd::CompleteFinish {
                                                start_rsp: original_rsp,
                                                started_from_inline: started_from_inline,
                                            };
                                            session.single_step();
                                        } else {
                                            // If we are at a different breakpoint continue till we reach the target
                                            session.continue_session();
                                        }
                                    }

                                    CurrentStopCmd::CompleteFinish {
                                        start_rsp,
                                        started_from_inline,
                                    } => {
                                        // If we stepped out of original function or back to
                                        // original function then complete else go back to stepping over
                                        if start_rsp >= regs.rsp {
                                            session.current_cmd = CurrentStopCmd::Finish {
                                                start_rsp,
                                                started_from_inline,
                                            };
                                            session.single_step();
                                        } else {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        }
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

                                                    session.registers =
                                                        Some(crate::interface::RegisterViewer {
                                                            regs,
                                                        });

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
                                    // At this state the session should not be Idle or completed
                                    // Completion occurs on the next iteration which is intercepted by the debugger menu to avoid waiting for the child
                                    _ => unreachable!(),
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
