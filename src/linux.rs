use std::ffi::CString;
use std::path::Path;

use nix::sys::personality::{self, Persona};
use nix::sys::ptrace;
use nix::sys::signal::{Signal, raise};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::Pid;
use nix::unistd::{
    ForkResult::{self, Child},
    fork,
};

pub fn debug(exec: &str) {
    let pid = unsafe { fork() }.unwrap();

    match pid {
        ForkResult::Child => {
            // Disable memory randomization
            let mut current_persona = personality::get().unwrap();
            current_persona.insert(Persona::ADDR_NO_RANDOMIZE);
            personality::set(current_persona).unwrap();

            ptrace::traceme().unwrap();

            // Stop child to avoid race condition with parent
            raise(Signal::SIGSTOP);

            let path = std::ffi::CString::new(exec).unwrap();
            nix::unistd::execv(&path, &[&path]).expect("Failed to run command");
        }
        ForkResult::Parent { child } => {
            loop {
                let status = waitpid(child, None).unwrap();
                match status {
                    WaitStatus::Exited(_, code) => {
                        println!("Child process exited with the code {}", code);
                        break;
                    }
                    WaitStatus::Stopped(pid, sig) => {
                        if let Ok(mut regs) = ptrace::getregs(pid) {
                            println!("{:?}", regs);

                            // If stop was due to SIGTRAP, do not forward it to the child.
                            // Pass None to let the child continue its execution.
                            if sig == Signal::SIGTRAP {
                                let breakpoint_addr = regs.rip - 1;

                                // Read a word from the child's memory
                                match ptrace::read(pid, breakpoint_addr as ptrace::AddressType) {
                                    Ok(word) => {
                                        // Extract the lowest byte of the word
                                        let first_byte = (word & 0xFF) as u8;

                                        if first_byte == 0xCC {
                                            println!(
                                                "Found 0xCC instruction as addr 0x{:X}",
                                                breakpoint_addr
                                            );

                                            // Rollback the instruction pointer by 1 byte
                                            regs.rip = breakpoint_addr;

                                            // Update pid register for future instruction continuation
                                            ptrace::setregs(pid, regs).unwrap();

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

fn handle_user_debugger_menu(pid: Pid) {
    let mut buffer = String::new();
    println!("Would you like to continue at this breakpoint (y/n)");
    loop {
        buffer.clear();
        std::io::stdin()
            .read_line(&mut buffer)
            .expect("Could not read line");

        match buffer.trim().to_lowercase().as_str() {
            "y" | "yes" => {
                ptrace::cont(pid, None).unwrap();
                break;
            }
            _ => {
                println!("Not handled yet");
            }
        }
    }
}

fn has_interrupt(instruction_pointer: u64) -> bool {
    // OxCC being present means the instruction is interrupted
    (instruction_pointer as u8) == 0xCC
}
