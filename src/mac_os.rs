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
            // TODO: Handle this more elegantly
            session.registers = None;

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

                                match session.current_cmd {
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

                                            let mut message = MessageReply {
                                                header: mach2::message::mach_msg_header_t {
                                                    msgh_bits:
                                                        mach2::message::MACH_MSG_TYPE_MOVE_SEND_ONCE,
                                                    msgh_size: std::mem::size_of::<MessageReply>()
                                                        as u32,
                                                    msgh_remote_port: header.msgh_remote_port,
                                                    msgh_local_port: mach2::port::MACH_PORT_NULL,
                                                    msgh_voucher_port: mach2::port::MACH_PORT_NULL,
                                                    msgh_id: header.msgh_id + 100,
                                                },
                                                NDR: unsafe { mach2::ndr::NDR_record },
                                                kern_return: kern_return::KERN_SUCCESS,
                                            };

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
