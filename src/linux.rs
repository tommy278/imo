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

use crate::helpers::linux::{get_process_base_address, lookup_address_by_line};

// TODO: Add to a dedicated file
struct BreakPoint {
    addr: u64,
    original_byte: u8,
    is_enabled: bool,
}

impl BreakPoint {
    /// Creates a new instance of breakpoint from address
    fn new(addr: u64) -> Self {
        Self {
            addr,
            original_byte: 0,
            is_enabled: false,
        }
    }

    /// Overwrite the lowest byte with 0xCC (INT3) while saving the original byte
    fn enable(&mut self, pid: Pid) {
        // Read the current memory word
        let word = ptrace::read(pid, self.addr as ptrace::AddressType).unwrap();

        // Save the original lowest byte
        self.original_byte = (word & 0xFF) as u8;

        // Overwrite the lowest byte with 0xCC (INT3)
        let breakpoint_word = (word & !0xFF) | 0xCC;

        // Write word back to child memory
        unsafe {
            ptrace::write(pid, self.addr as ptrace::AddressType, breakpoint_word).unwrap();
        }
        self.is_enabled = true;
    }

    /// Swaps 0xCC out, puts original byte back
    fn disable(&mut self, pid: Pid) {
        if !self.is_enabled {
            return;
        }

        let word = ptrace::read(pid, self.addr as ptrace::AddressType).unwrap();
        let restored_word = (word & !0xFF) | (self.original_byte as i64);

        unsafe {
            ptrace::write(pid, self.addr as ptrace::AddressType, restored_word);
        }
        self.is_enabled = false;
    }
}

pub fn debug(exec: &str) {
    let pid = unsafe { fork() }.unwrap();
    let offset = lookup_address_by_line(exec, "running_task.c", 6).unwrap();
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
            // Catch the initial SIGSTOP from the child
            let _status_1 = waitpid(child, None).unwrap();

            // Tell child to continue to the execv call
            ptrace::cont(child, None).unwrap();

            // Catch the automatic SIGTRAP generated after execv finishes loading
            let _status_2 = waitpid(child, None).unwrap();

            // Read the base address and inject 0xCC (INT3)
            let base = get_process_base_address(child);
            let final_target_address = base + offset;

            let mut breakpoint = BreakPoint::new(final_target_address);
            breakpoint.enable(child);

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
                            println!("{:?}", regs);

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
                                    Ok(word) => {
                                        // Extract the lowest byte of the word
                                        let first_byte = (word & 0xFF) as u8;

                                        assert_eq!(
                                            breakpoint.addr, breakpoint_addr,
                                            "These must be equal"
                                        );

                                        if first_byte == 0xCC {
                                            // Rollback the instruction pointer by 1 byte

                                            // TODO: Update the instruction back to a valid
                                            // instruction instead of going back to the INT3
                                            regs.rip = breakpoint_addr;

                                            // Update pid register for future instruction continuation
                                            ptrace::setregs(pid, regs).unwrap();

                                            breakpoint.disable(pid);
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

/// Display an interactive menu at breakpoints
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
            "n" | "no" => {
                // TODO: Find the idiomatic way to continue from here
                ptrace::kill(pid).unwrap();
                break;
            }
            _ => {
                println!("Not handled yet");
            }
        }
    }
}
