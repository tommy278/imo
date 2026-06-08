pub mod linux;

use std::io::{self, Write};

/// Display an interactive menu at breakpoints
pub fn handle_user_debugger_menu(pid: nix::unistd::Pid) {
    let mut buffer = String::new();
    print!("Would you like to continue at this breakpoint (y/n): ");

    // Flush so the print statement is immediately displayed on screen
    io::stdout().flush().unwrap();

    loop {
        // Clear previous data that could corrupt input
        buffer.clear();

        io::stdin()
            .read_line(&mut buffer)
            .expect("Could not read line");

        match buffer.trim().to_lowercase().as_str() {
            "y" | "yes" => {
                nix::sys::ptrace::cont(pid, None).unwrap();
                break;
            }
            "n" | "no" => {
                // TODO: Find the idiomatic way to continue from here
                nix::sys::ptrace::kill(pid).unwrap();
                break;
            }
            _ => {
                println!("Not handled yet");
            }
        }
    }
}
