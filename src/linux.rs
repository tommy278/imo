use iced_x86::Code;
use nix::sys::personality::{self, Persona};
use nix::sys::ptrace;
use nix::sys::signal::{Signal, raise};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, fork};

use crate::cli::handle_user_debugger_menu;
use crate::session::linux::{get_instruction_info, peek_data, read_bytes};
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
                                        start_rsp,
                                        start_line,
                                        start_file,
                                    } => {
                                        let rip = regs.rip as usize;
                                        let instr = get_instruction_info(pid, rip).unwrap();

                                        let code = instr.code();
                                        let instr_size = instr.len();

                                        println!("{:?}", code);

                                        match code {
                                            Code::Call_ptr1616
                                            | Code::Call_ptr1632
                                            | Code::Call_rel16
                                            | Code::Call_rel32_32
                                            | Code::Call_rel32_64
                                            | Code::Call_rm16
                                            | Code::Call_rm32
                                            | Code::Call_rm64
                                            | Code::Call_m1616
                                            | Code::Call_m1632
                                            | Code::Call_m1664 => {
                                                let return_address = rip + instr_size;

                                                let relative_address = session
                                                    .get_relative_address(return_address as u64);
                                                session
                                                    .create_specific_breakpoint(relative_address);

                                                session.current_cmd = CurrentStopCmd::StepOut {
                                                    start_rsp,
                                                    start_line,
                                                    start_file,
                                                };
                                                session.continue_session();
                                                continue;
                                            }
                                            Code::Jmp_rel16
                                            | Code::Jmp_rel32_32
                                            | Code::Jmp_rel32_64
                                            | Code::Jmp_ptr1616
                                            | Code::Jmp_ptr1632
                                            | Code::Jmp_rel8_16
                                            | Code::Jmp_rel8_32
                                            | Code::Jmp_rel8_64
                                            | Code::Jmp_rm16
                                            | Code::Jmp_rm32
                                            | Code::Jmp_rm64
                                            | Code::Jmp_m1616
                                            | Code::Jmp_m1632
                                            | Code::Jmp_m1664 => {
                                                let next_ip = rip + instr_size;
                                                let offset = instr.memory_displacement64() as i64;

                                                let jump_target = (next_ip as i64 + offset) as u64;
                                                let relative_address =
                                                    session.get_relative_address(jump_target);

                                                session
                                                    .create_specific_breakpoint(relative_address);

                                                session.current_cmd = CurrentStopCmd::StepOut {
                                                    start_rsp,
                                                    start_line,
                                                    start_file,
                                                };
                                                session.continue_session();
                                                continue;
                                            }
                                            Code::Jne_rel8_16
                                            | Code::Jne_rel8_32
                                            | Code::Jne_rel8_64
                                            | Code::Jne_rel16
                                            | Code::Jne_rel32_32
                                            | Code::Jne_rel32_64
                                            | Code::Je_rel8_16
                                            | Code::Je_rel8_32
                                            | Code::Je_rel8_64
                                            | Code::Je_rel16
                                            | Code::Je_rel32_32
                                            | Code::Je_rel32_64 => {
                                                let next_ip = rip + instr_size;
                                                let offset = instr.memory_displacement64() as i64;

                                                let jump_target = (next_ip as i64 + offset) as u64;
                                                let relative_address =
                                                    session.get_relative_address(jump_target);

                                                session
                                                    .create_specific_breakpoint(relative_address);

                                                let fallthrough_target = rip + instr_size;
                                                let relative_address = session
                                                    .get_relative_address(
                                                        fallthrough_target as u64,
                                                    );

                                                session
                                                    .create_specific_breakpoint(relative_address);

                                                session.current_cmd = CurrentStopCmd::StepOut {
                                                    start_rsp,
                                                    start_line,
                                                    start_file,
                                                };
                                                session.continue_session();
                                                continue;
                                            }
                                            Code::Retnw_imm16
                                            | Code::Retnd_imm16
                                            | Code::Retnq_imm16
                                            | Code::Retfw_imm16
                                            | Code::Retfd_imm16
                                            | Code::Retfq_imm16 => {
                                                session.single_step();
                                                continue;
                                            }
                                            _ => {
                                                let line_range = session
                                                    .find_line_ranges(start_line, start_file);

                                                let is_at_final_address =
                                                    *line_range.last().unwrap() == rip as u64;

                                                if is_at_final_address {
                                                    let next_instruction = rip + instr_size;
                                                    let relative_address = session
                                                        .get_relative_address(
                                                            next_instruction as u64,
                                                        );
                                                    session.create_specific_breakpoint(
                                                        relative_address,
                                                    );

                                                    session.current_cmd = CurrentStopCmd::StepOut {
                                                        start_rsp,
                                                        start_line,
                                                        start_file,
                                                    };
                                                    session.continue_session();
                                                    continue;
                                                }

                                                session.single_step();
                                            }
                                        }
                                    }
                                    CurrentStopCmd::StepOut { .. } => {
                                        let breakpoint_addr = regs.rip - 1;

                                        let relative_address =
                                            session.get_relative_address(breakpoint_addr);
                                        session.clear_specific_breakpoint(relative_address);

                                        session.current_cmd = CurrentStopCmd::Completed;
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
