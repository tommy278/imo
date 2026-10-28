use crate::cli::handle_user_debugger_menu;
use crate::error::DebuggerError;
use crate::session::{self, execution::CurrentStopCmd};
use crate::sys::{
    macos::error::MacOSError,
    os::{syscalls, MessageReply, PlatformRegStruct},
    registers::RegisterViewer,
};

use mach2::{kern_return, mach_types, traps};
use rustyline::DefaultEditor;

#[cfg(target_arch = "x86_64")]
use mach2::thread_status::x86_THREAD_STATE64 as THREAD_STATE64;
#[cfg(target_arch = "aarch64")]
use mach2::thread_status::ARM_THREAD_STATE64 as THREAD_STATE64;

#[cfg(target_arch = "x86_64")]
macro_rules! get_pc {
    ($regs: expr, $base_address: expr) => {{
        let offset = $regs.__rip - $base_address;
        0x1_0000_0000 + offset
    }};
}

pub fn debug(rl: &mut DefaultEditor, binary_path: &str) -> Result<(), DebuggerError> {
    let mut pid: i32 = 0;
    let path = std::ffi::CString::new(binary_path).map_err(|e| MacOSError::CString(e))?;

    let mut spawn_attr: libc::posix_spawnattr_t = std::ptr::null_mut();
    let return_value = unsafe { libc::posix_spawnattr_init(&mut spawn_attr) };

    if return_value != 0 {
        eprintln!("Error creating attr");
        return Ok(());
    }

    let flag = libc::POSIX_SPAWN_START_SUSPENDED as i16;
    let return_value = unsafe { libc::posix_spawnattr_setflags(&mut spawn_attr, flag) };

    if return_value != 0 {
        eprintln!("Error creating flag");
        return Ok(());
    }

    let return_value = unsafe {
        libc::posix_spawn(
            &mut pid,
            path.as_ptr(),
            std::ptr::null(),
            &mut spawn_attr,
            std::ptr::null(),
            std::ptr::null(),
        )
    };

    if return_value != 0 {
        eprintln!("Error creating process");
        return Ok(());
    }

    let mut child_task_port: mach2::port::mach_port_name_t = 0;

    let mut kern_return =
        unsafe { traps::task_for_pid(traps::mach_task_self(), pid, &mut child_task_port) };

    if kern_return == kern_return::KERN_SUCCESS {
        let mut session = session::DebugSession::new(child_task_port, binary_path)?;
        let mut exception_port: mach2::port::mach_port_name_t = 0;

        unsafe {
            let mut child_name: mach2::port::mach_port_name_t = 0;

            // Allocate the exception port
            kern_return = mach2::mach_port::mach_port_allocate(
                traps::mach_task_self(),
                mach2::port::MACH_PORT_RIGHT_RECEIVE,
                &mut exception_port,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                eprintln!("Failed to create exception port")
            }

            kern_return = mach2::mach_port::mach_port_insert_right(
                traps::mach_task_self(),
                exception_port,
                exception_port,
                mach2::message::MACH_MSG_TYPE_MAKE_SEND,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                eprintln!("Failed to insert exception port")
            }

            // set specific exceptions to be tracked
            kern_return = mach2::task::task_set_exception_ports(
                child_task_port,
                mach2::exception_types::EXC_MASK_BREAKPOINT,
                exception_port,
                mach2::exception_types::EXCEPTION_DEFAULT as i32,
                THREAD_STATE64,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                println!("kern return: {}", kern_return);
                eprintln!("failed to set exception port")
            }

            let mut prev = 0;
            // include tracking for when the task finishes
            kern_return = mach2::mach_port::mach_port_request_notification(
                traps::mach_task_self(),
                child_task_port,
                mach2::notify::MACH_NOTIFY_DEAD_NAME,
                0,
                exception_port,
                mach2::message::MACH_MSG_TYPE_MAKE_SEND_ONCE,
                &mut prev,
            );

            if kern_return != kern_return::KERN_SUCCESS {
                println!("KERN RETURN: {}", kern_return);
                eprintln!("Failed to create destroyed exception");
            }

            kern_return = mach2::task::task_suspend(child_task_port);

            if kern_return != kern_return::KERN_SUCCESS {
                println!("Could not suspend child");
            }

            let res = libc::ptrace(libc::PT_ATTACHEXC, pid, std::ptr::null_mut(), 0);

            if res != 0 {
                eprintln!("failed to attach to process");
            }
        }

        handle_user_debugger_menu(&mut session, rl)?;

        let mut buffer: [u8; 256] = [0; 256];

        let mut msg_header = mach2::message::mach_msg_header_t::default();
        let option = mach2::message::MACH_RCV_MSG;
        let send_size = 0;
        let recieve_size = 256;
        let timeout = mach2::message::MACH_MSG_TIMEOUT_NONE;
        let notify = mach2::port::MACH_PORT_NULL;

        loop {
            println!("{:?}", session.current_cmd);
            if session.current_cmd.is_completed() {
                let source_display = session.get_current_source_file();
                println!("{}", source_display);
                handle_user_debugger_menu(&mut session, rl)?;
            }

            kern_return = unsafe {
                mach2::message::mach_msg(
                    buffer.as_mut_ptr() as *mut mach2::message::mach_msg_header_t,
                    option,
                    send_size,
                    recieve_size,
                    exception_port,
                    timeout,
                    notify,
                )
            };

            if kern_return != kern_return::KERN_SUCCESS {
                eprintln!("Error retrieving message");
            } else {
                let base_ptr = buffer.as_ptr();
                let header = unsafe { *(base_ptr as *const mach2::message::mach_msg_header_t) };

                match header.msgh_id {
                    65 => {
                        println!("Successfully quit program");

                        // Cleanup exception port
                        kern_return = unsafe {
                            mach2::mach_port::mach_port_destroy(
                                mach2::traps::mach_task_self(),
                                exception_port,
                            )
                        };

                        if kern_return != mach2::kern_return::KERN_SUCCESS {
                            println!("Failed to clean up exception port");
                        }

                        return Ok(());
                    }
                    72 => {
                        println!("Child finished");
                        return Ok(());
                    }
                    2401 | 2405 => {
                        let exception_offset = if header.msgh_id == 2401 { 60 } else { 64 };
                        let exception =
                            unsafe { *(base_ptr.wrapping_add(exception_offset) as *const u32) };

                        match exception {
                            // Software Breakpoint
                            6 => {
                                kern_return = unsafe { mach2::task::task_suspend(child_task_port) };

                                let crashed_thread =
                                    unsafe { *(base_ptr.wrapping_add(28) as *const u32) };

                                session.crashed_thread = crashed_thread;

                                let mut regs = PlatformRegStruct::new();
                                let mut regs_count = PlatformRegStruct::count();

                                kern_return = unsafe {
                                    mach2::thread_act::thread_get_state(
                                        crashed_thread,
                                        THREAD_STATE64,
                                        std::mem::transmute(&regs),
                                        &mut regs_count,
                                    )
                                };

                                let mut message = MessageReply {
                                    header: mach2::message::mach_msg_header_t {
                                        msgh_bits: mach2::message::MACH_MSG_TYPE_MOVE_SEND_ONCE,
                                        msgh_size: std::mem::size_of::<MessageReply>() as u32,
                                        msgh_remote_port: header.msgh_remote_port,
                                        msgh_local_port: mach2::port::MACH_PORT_NULL,
                                        msgh_voucher_port: mach2::port::MACH_PORT_NULL,
                                        msgh_id: header.msgh_id + 100,
                                    },
                                    NDR: unsafe { mach2::ndr::NDR_record },
                                    kern_return: kern_return::KERN_SUCCESS,
                                };

                                // Send message to signify that the thread exception will be handled
                                kern_return = unsafe {
                                    mach2::message::mach_msg(
                                        &mut message as *mut MessageReply
                                            as *mut mach2::message::mach_msg_header_t,
                                        mach2::message::MACH_SEND_MSG,
                                        std::mem::size_of::<MessageReply>() as u32,
                                        0,
                                        mach2::port::MACH_PORT_NULL,
                                        mach2::message::MACH_MSG_TIMEOUT_NONE,
                                        mach2::port::MACH_PORT_NULL,
                                    )
                                };

                                if kern_return != mach2::kern_return::KERN_SUCCESS {
                                    println!("Could not revive thread: {}", kern_return);
                                }

                                session.message = Some(message);

                                // Ensure trap flag is cleared after each iteration
                                if regs.__rflags & (1u64 << 8) > 0 {
                                    regs.__rflags &= !(1u64 << 8);
                                    // Update register with the cleared Trap Flag
                                    kern_return = unsafe {
                                        mach2::thread_act::thread_set_state(
                                            crashed_thread,
                                            THREAD_STATE64,
                                            std::mem::transmute(&regs),
                                            regs_count,
                                        )
                                    };

                                    if kern_return != mach2::kern_return::KERN_SUCCESS {
                                        println!("Failed to clean regs: {}", kern_return);
                                    }
                                }

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
                                            session.single_step()?;
                                            continue;
                                        };

                                        // If the location changed then stop
                                        if current_location.file != start_file
                                            || current_location.line != start_line
                                        {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        } else {
                                            // Still on the same location keep going
                                            session.single_step()?;
                                        }
                                    }
                                    CurrentStopCmd::StepOver {
                                        start_stack_pointer,
                                        start_file,
                                        start_line,
                                        started_from_inline,
                                    } => {
                                        let current_rsp = regs.__rsp;

                                        // If current rsp is greater than the start rsp then we stepped out of a function and should stop there
                                        if current_rsp > start_stack_pointer {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        }
                                        // If rsp are the same then we are in the same function or it was inlined
                                        else if current_rsp == start_stack_pointer {
                                            // We need location so keep stepping until we find a valid one
                                            let pc = get_pc!(regs, session.base_address);
                                            let Some(line_row) = session.find_line_range(pc) else {
                                                session.single_step()?;
                                                continue;
                                            };

                                            if !line_row.is_stmt {
                                                session.single_step()?;
                                                continue;
                                            }

                                            let current_location = &line_row.location;

                                            // If its not an inline function just step till we areaddress_range. back in same file but not same line
                                            if !started_from_inline {
                                                if start_file == current_location.file
                                                    && start_line != current_location.line
                                                {
                                                    session.current_cmd = CurrentStopCmd::Completed;
                                                } else {
                                                    session.single_step()?;
                                                }
                                                continue;
                                            } else {
                                                // If we are in an inline function, behave exactly like a
                                                // normal stepover with the inclusion
                                                // that it can now step out to a function that is not inlined
                                                let pc = get_pc!(regs, session.base_address);
                                                if (start_file == current_location.file
                                                    && start_line != current_location.line)
                                                    || !session.metadata.is_in_inline(pc)
                                                {
                                                    session.current_cmd = CurrentStopCmd::Completed;
                                                } else {
                                                    session.single_step()?;
                                                }
                                                continue;
                                            }
                                        } else {
                                            // If current rsp is lower then we stepped into a child
                                            // In that case get the return address and continue there
                                            let Some(return_address) = session.get_return_address(
                                                &mut session.virtual_registers()?,
                                            ) else {
                                                session.single_step()?;
                                                continue;
                                            };

                                            let relative_address =
                                                session.get_relative_address(return_address);
                                            session.create_specific_breakpoint(relative_address)?;
                                            session.current_cmd = CurrentStopCmd::StepOut {
                                                original_stack_pointer: start_stack_pointer,
                                                original_file: start_file,
                                                original_line: start_line,
                                                return_address,
                                                started_from_inline,
                                            };
                                            session.continue_session()?;
                                        }
                                    }
                                    CurrentStopCmd::StepOut {
                                        original_stack_pointer,
                                        original_file,
                                        original_line,
                                        return_address,
                                        started_from_inline,
                                    } => {
                                        // Simply clear breakpoint and send data back to finish stepover to handle completion
                                        let breakpoint_addr = regs.__rip - 1;

                                        let relative_address =
                                            session.get_relative_address(breakpoint_addr);
                                        session.clear_specific_breakpoint(relative_address)?;

                                        regs.__rip = breakpoint_addr;
                                        kern_return = unsafe {
                                            mach2::thread_act::thread_set_state(
                                                crashed_thread,
                                                THREAD_STATE64,
                                                std::mem::transmute(&regs),
                                                regs_count,
                                            )
                                        };

                                        if kern_return != kern_return::KERN_SUCCESS {
                                            println!("Failed to update regs: {}", kern_return);
                                        }
                                        session.registers = Some(RegisterViewer { regs });

                                        if return_address == breakpoint_addr {
                                            // If we are at the specific breakpoint to step out from then continue step over like normal
                                            session.current_cmd = CurrentStopCmd::FinishStepOver {
                                                original_stack_pointer,
                                                original_file,
                                                original_line,
                                                started_from_inline,
                                            };
                                            session.single_step()?;
                                        } else {
                                            // If we are at a different breakpoint continue till we reach the target
                                            session.continue_session()?;
                                        }
                                    }
                                    CurrentStopCmd::FinishStepOver {
                                        original_stack_pointer,
                                        original_file,
                                        original_line,
                                        started_from_inline,
                                    } => {
                                        // If we stepped out of original function or back to
                                        // original function then complete else go back to stepping over
                                        if original_stack_pointer >= regs.__rsp {
                                            session.current_cmd = CurrentStopCmd::StepOver {
                                                start_stack_pointer: original_stack_pointer,
                                                start_file: original_file,
                                                start_line: original_line,
                                                started_from_inline,
                                            };
                                            session.single_step()?;
                                        } else {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        }
                                    }
                                    CurrentStopCmd::Finish {
                                        start_stack_pointer,
                                        started_from_inline,
                                    } => {
                                        let current_rsp = regs.__rsp;

                                        if current_rsp > start_stack_pointer {
                                            session.current_cmd = CurrentStopCmd::Completed;
                                        } else if started_from_inline
                                            && start_stack_pointer == current_rsp
                                        {
                                            let pc = get_pc!(regs, session.base_address);
                                            if !session.metadata.is_in_inline(pc) {
                                                session.current_cmd = CurrentStopCmd::Completed;
                                            } else {
                                                session.single_step()?;
                                            }
                                            continue;
                                        } else {
                                            let Some(return_address) = session.get_return_address(
                                                &mut session.virtual_registers()?,
                                            ) else {
                                                session.single_step()?;
                                                continue;
                                            };

                                            let relative_address =
                                                session.get_relative_address(return_address);
                                            session.create_specific_breakpoint(relative_address)?;
                                            session.current_cmd = CurrentStopCmd::StepOutFinish {
                                                return_address,
                                                original_stack_pointer: start_stack_pointer,
                                                started_from_inline,
                                            };
                                            session.continue_session()?;
                                        }
                                    }

                                    CurrentStopCmd::StepOutFinish {
                                        return_address,
                                        original_stack_pointer,
                                        started_from_inline,
                                    } => {
                                        // A handshake from the step over
                                        // Simply clear breakpoint and send data back to finish stepover to handle completion
                                        let breakpoint_addr = regs.__rip - 1;

                                        let relative_address =
                                            session.get_relative_address(breakpoint_addr);
                                        session.clear_specific_breakpoint(relative_address)?;

                                        regs.__rip = breakpoint_addr;
                                        kern_return = unsafe {
                                            mach2::thread_act::thread_set_state(
                                                crashed_thread,
                                                THREAD_STATE64,
                                                std::mem::transmute(&regs),
                                                regs_count,
                                            )
                                        };

                                        if kern_return != kern_return::KERN_SUCCESS {
                                            println!("Failed to update regs: {}", kern_return);
                                        }

                                        session.registers = Some(RegisterViewer { regs });
                                        if return_address == breakpoint_addr {
                                            // If we are at the specific breakpoint to step out from then continue step over like normal
                                            session.current_cmd = CurrentStopCmd::CompleteFinish {
                                                start_stack_pointer: original_stack_pointer,
                                                started_from_inline: started_from_inline,
                                            };
                                            session.single_step()?;
                                        } else {
                                            // If we are at a different breakpoint continue till we reach the target
                                            session.continue_session()?;
                                        }
                                    }

                                    CurrentStopCmd::CompleteFinish {
                                        start_stack_pointer,
                                        started_from_inline,
                                    } => {
                                        // If we stepped out of original function or back to
                                        // original function then complete else go back to stepping over
                                        if start_stack_pointer >= regs.__rsp {
                                            session.current_cmd = CurrentStopCmd::Finish {
                                                start_stack_pointer,
                                                started_from_inline,
                                            };
                                            session.single_step()?;
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
                                            session.single_step()?;
                                        }
                                    }
                                    CurrentStopCmd::Running | CurrentStopCmd::Continuing => {
                                        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                                        let breakpoint_addr = regs.__rip - 1;

                                        #[cfg(target_arch = "aarch64")]
                                        let breakpoint_addr = regs.__pc;

                                        if let Some(bp) =
                                            session.active_breakpoints.get_mut(&breakpoint_addr)
                                        {
                                            regs.__rip = breakpoint_addr;

                                            kern_return = unsafe {
                                                mach2::thread_act::thread_set_state(
                                                    crashed_thread,
                                                    THREAD_STATE64,
                                                    std::mem::transmute(&regs),
                                                    regs_count,
                                                )
                                            };

                                            if kern_return != kern_return::KERN_SUCCESS {
                                                println!("Failed to update regs: {}", kern_return);
                                            }

                                            session.registers = Some(RegisterViewer { regs });
                                            bp.breakpoint.disable(child_task_port);
                                            session.current_cmd = CurrentStopCmd::Completed;

                                            if kern_return != mach2::kern_return::KERN_SUCCESS {
                                                println!(
                                                    "Could not revive thread: {}",
                                                    kern_return
                                                );
                                            }
                                        } else {
                                            handle_user_debugger_menu(&mut session, rl)?;
                                        }
                                    }
                                    _ => unimplemented!(),
                                }
                            }
                            _ => todo!(),
                        }
                    }
                    _ => todo!(),
                }
            }
        }
    }
    Ok(())
}
