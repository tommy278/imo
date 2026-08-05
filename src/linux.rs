use nix::sys::personality::{self, Persona};
use nix::sys::ptrace;
use nix::sys::signal::{Signal, raise};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, fork};

use crate::cli::handle_user_debugger_menu;
use crate::session::linux::{peek_data, read_bytes};
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
                if session.current_cmd.is_completed() {
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
                            // If stop was due to SIGTRAP, do not forward it to the child.
                            // Pass None to let the child continue its execution.
                            if sig == Signal::SIGTRAP {
                                println!("{:?}", session.current_cmd);
                                assert!(!session.is_idle());
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
                                        start_file,
                                    } => {
                                        let rip = regs.rip;

                                        let pc = rip - session.base_address;

                                        let op_code = peek_data(pid, rip) as u8;

                                        let mut prefix_bytes_skipped = 0;
                                        {
                                            let mut current_rip = rip;
                                            let mut op_code = peek_data(pid, current_rip);

                                            while (0x40..=0x4F).contains(&op_code) {
                                                prefix_bytes_skipped += 1;
                                                current_rip += 1;
                                                op_code = peek_data(pid, current_rip);
                                            }
                                        }

                                        let instruction_size = match op_code {
                                            0xE8 => 5,
                                            0x9A => 7,
                                            0xEB => 2,
                                            0xE9 => 5,
                                            0x70..=0x7F => 2,
                                            0x0F => {
                                                let next_byte = peek_data(pid, rip + 1);
                                                if (0x80..=0x8F).contains(&next_byte) {
                                                    6
                                                } else {
                                                    1
                                                }
                                            }
                                            // TODO: add the 0xFF /2 and 0xFF /3
                                            _ => 1,
                                        };

                                        if op_code == 0x9A || op_code == 0xE8 {
                                            let return_address = rip + instruction_size;

                                            println!("Here: 0x{:x}, {}", op_code, return_address);

                                            let relative_address =
                                                session.get_relative_address(return_address);

                                            session.create_specific_breakpoint(relative_address);

                                            session.current_cmd = CurrentStopCmd::StepOut {
                                                resume_cfa: start_cfa,
                                                start_line,
                                                start_file,
                                            };
                                            session.continue_session();
                                        } else {
                                            let current_line_ranges =
                                                session.find_line_ranges(start_line, start_file);

                                            let is_at_final_address = rip
                                                == *current_line_ranges
                                                    .last()
                                                    .expect("Range not found");

                                            if is_at_final_address {
                                                // Condional and unconditional jump
                                                let fallthrough_address =
                                                    rip + instruction_size + prefix_bytes_skipped;

                                                let jump_target_address = match instruction_size {
                                                    2 => {
                                                        let offset_byte =
                                                            peek_data(pid, rip + 1) as i8;
                                                        let target = (fallthrough_address as i64)
                                                            + (offset_byte as i64);
                                                        target as u64
                                                    }
                                                    5 => {
                                                        let offset_bytes =
                                                            read_bytes(pid, (rip + 1) as usize, 4)
                                                                .unwrap();
                                                        let offset_val = i32::from_le_bytes(
                                                            offset_bytes.try_into().unwrap(),
                                                        );
                                                        let target = (fallthrough_address as i64)
                                                            + (offset_val as i64);
                                                        target as u64
                                                    }
                                                    6 => {
                                                        let offset_bytes =
                                                            read_bytes(pid, (rip + 2) as usize, 4)
                                                                .unwrap();
                                                        let offset_val = i32::from_le_bytes(
                                                            offset_bytes.try_into().unwrap(),
                                                        );
                                                        let target = (fallthrough_address as i64)
                                                            + (offset_val as i64);
                                                        target as u64
                                                    }

                                                    _ => unreachable!(),
                                                };

                                                let relative_address = session
                                                    .get_relative_address(fallthrough_address);
                                                session
                                                    .create_specific_breakpoint(relative_address);

                                                let relative_address = session
                                                    .get_relative_address(jump_target_address);
                                                session.clear_specific_breakpoint(relative_address);

                                                session.current_cmd = CurrentStopCmd::StepOut {
                                                    resume_cfa: start_cfa,
                                                    start_line,
                                                    start_file,
                                                };
                                                session.continue_session();
                                            } else {
                                                // TODO: handle this properly
                                                session.single_step();
                                            }
                                        }
                                    }
                                    CurrentStopCmd::StepOut {
                                        resume_cfa,
                                        start_line,
                                        start_file,
                                    } => {
                                        let breakpoint_addr = regs.rip - 1;

                                        let relative_address =
                                            session.get_relative_address(breakpoint_addr);
                                        session.clear_specific_breakpoint(relative_address);

                                        regs.rip = breakpoint_addr;
                                        ptrace::setregs(pid, regs).unwrap();

                                        // Search for a valid location to stop on
                                        // Avoid stoppiing on assembly
                                        session.current_cmd =
                                            CurrentStopCmd::SearchingForNextValidLocation {
                                                start_line,
                                                start_file,
                                            };
                                        session.single_step();
                                    }
                                    CurrentStopCmd::SearchingForNextValidLocation {
                                        start_line,
                                        start_file,
                                    } => {
                                        if let Some(l) = session.current_location() {
                                            let current_file = l.file;

                                            // We are not back in the start file so keep stepping
                                            if current_file != start_file {
                                                session.single_step();
                                                continue;
                                            }

                                            // We are back in the main file so check to ensure we are not on the same line
                                            if l.line != start_line {
                                                session.current_cmd = CurrentStopCmd::Completed;
                                                continue;
                                            }
                                        }

                                        // If no valid location continue stepping
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
                                    CurrentStopCmd::SearchingForValidStartLocation => {
                                        // If the current location is now valid then begin step over
                                        if session.current_location().is_some() {
                                            session.begin_step_over();
                                        } else {
                                            // Continue stepping
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
