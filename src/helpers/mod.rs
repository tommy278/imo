pub mod linux;

use std::io::{self, Write};

use crate::helpers::linux::DebugSession;

/// Display an interactive menu at breakpoints
pub fn handle_user_debugger_menu(session: &DebugSession) {
    // Flush so the print statement is immediately displayed on screen
    let flush_output = || {
        io::stdout().flush().unwrap();
    };

    let mut buffer = String::new();
    print!("(imo) ");
    flush_output();

    loop {
        // Clear previous data that could corrupt input
        buffer.clear();

        io::stdin()
            .read_line(&mut buffer)
            .expect("Could not read line");

        match buffer.trim().to_lowercase().as_str() {
            "y" | "yes" => {
                nix::sys::ptrace::cont(session.pid, None).unwrap();
                break;
            }
            "n" | "no" => {
                // TODO: Find the idiomatic way to continue from here
                nix::sys::ptrace::kill(session.pid).unwrap();
                break;
            }
            _ => {
                println!("Not handled yet");
                print!("(imo) ");
                flush_output();
            }
        }
    }
}
