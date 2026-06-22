pub mod helpers;

use std::io;

use crate::cli::helpers::{
    flush_output, handle_breakpoint_clearing, handle_breakpoint_setting, handle_event_by_index,
};
use crate::session::DebugSession;

/// Display an interactive menu at breakpoints
pub fn handle_user_debugger_menu(session: &mut DebugSession) {
    let mut buffer = String::new();

    // TODO: Dont forget to remove this once done with the lookup
    crate::helpers::dwarf::lookup_variables::lookup_vars("TODO");

    loop {
        print!("(imo) ");
        flush_output();

        // Clear previous data that could corrupt input
        buffer.clear();

        io::stdin()
            .read_line(&mut buffer)
            .expect("Could not read line");

        let input = buffer.trim().to_lowercase();
        let mut parts = input.split_whitespace();

        match parts.next().unwrap() {
            "run" | "c" | "continue" => {
                session.continue_session();
                break;
            }
            "s" | "step" => {
                session.begin_step_process();
                break;
            }
            "b" | "break" => {
                let arg = parts.next().expect("Did not provide a second argument");

                // Handle setting breakpoint if filename and line_number are provided
                // eg: break running_task:6
                if let Some((file_name, line_number)) = arg.split_once(":") {
                    let line_number = line_number.parse::<u64>().expect("Could not parse number");

                    let line_index = session.get_specific_breakpoint_target(file_name, line_number);
                    handle_breakpoint_setting(session, &line_index, line_number);
                }

                // Handle setting breakpoint if only line_number is provided
                // eg: break 12
                if let Ok(line_number) = arg.parse::<u64>() {
                    let line_index = session.get_breakpoint_target(line_number).unwrap();
                    handle_breakpoint_setting(session, &line_index, line_number);
                }
            }
            "clear" => {
                let arg = parts.next().expect("Did not provide a second argument");

                // Handle clearing breakpoint if filename and line_number are provided
                // eg: clear running_task:6
                if let Some((file_name, line_number)) = arg.split_once(":") {
                    let line_number = line_number.parse::<u64>().expect("Could not parse number");
                    handle_breakpoint_clearing(session, line_number, Some(file_name));
                }

                // Handle clearing breakpoint if only line_number is provided
                // eg: clear 12
                if let Ok(line_number) = arg.parse::<u64>() {
                    handle_breakpoint_clearing(session, line_number, None);
                }
            }
            "d" | "delete" => {
                let arg = parts.next().expect("No args");

                if let Ok(user_index) = arg.parse::<usize>() {
                    handle_event_by_index(
                        session,
                        user_index,
                        |s, idx| s.delete_breakpoint(idx),
                        "delete",
                    );
                }
            }
            "e" | "enable" => {
                let arg = parts.next().expect("No args");

                if let Ok(user_index) = arg.parse::<usize>() {
                    handle_event_by_index(
                        session,
                        user_index,
                        |s, idx| s.enable_breakpoint(idx),
                        "enable",
                    );
                }
            }
            "dis" | "disable" => {
                let arg = parts.next().expect("No args");

                if let Ok(user_index) = arg.parse::<usize>() {
                    handle_event_by_index(
                        session,
                        user_index,
                        |s, idx| s.disable_breakpoint(idx),
                        "disable",
                    );
                }
            }
            "i" | "info" => {
                let arg = parts.next().expect("No args");

                match arg {
                    "b" | "breakpoints" => {
                        for (idx, bp) in session.breakpoint_index_tracker.iter().enumerate() {
                            // NOTE: User index is 1 based
                            if let Some(bp) = bp {
                                let user_idx = idx + 1;
                                println!("{} {}", user_idx, bp);
                            }
                        }
                    }
                    "reg" => {
                        let regs = session.get_regs();
                        println!("{}", regs);
                    }
                    _ => {
                        todo!()
                    }
                }
            }
            "debug" => {
                for entry in session.breakpoint_index_tracker.iter() {
                    println!("{:?}", entry);
                }
            }
            "q" | "quit" => {
                // End current debug session
                session.kill_session();
                break;
            }
            _ => {
                println!("Not handled yet");
            }
        }
    }
}
