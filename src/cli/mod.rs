pub mod helpers;

use std::io;

use crate::cli::helpers::{
    flush_output, handle_breakpoint_clearing, handle_breakpoint_setting, handle_cmd,
    handle_event_by_index,
};
use crate::session::DebugSession;

/// Display an interactive menu at breakpoints
pub fn handle_user_debugger_menu(session: &mut DebugSession) {
    let mut buffer = String::new();

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

        let Some(part) = parts.next() else {
            continue;
        };

        let is_idle = session.is_idle();

        match part {
            "run" => {
                if is_idle {
                    session.toggle_running();
                    if let Err(err) = session.continue_session() {
                        println!("{err}")
                    }
                    break;
                } else {
                    println!("Session is already running")
                }
            }
            "c" | "continue" => {
                session.toggle_continue();
                if handle_cmd(is_idle, || session.continue_session()) {
                    break;
                }
            }
            // Step a single instruction
            "si" | "stepi" => {
                if handle_cmd(is_idle, || session.complete_single_step()) {
                    break;
                }
            }
            // Step into
            "s" | "step" => {
                if handle_cmd(is_idle, || session.begin_step_into()) {
                    break;
                }
            }
            // Step Over / Next
            "n" | "next" => {
                if handle_cmd(is_idle, || session.begin_step_over()) {
                    break;
                }
            }
            "f" | "fin" | "finish" => {
                if handle_cmd(is_idle, || session.begin_finish()) {
                    break;
                }
            }
            "b" | "break" => {
                let arg = parts.next().expect("Did not provide a second argument");

                // Handle setting breakpoint if filename and line_number are provided
                // eg: break running_task:6
                if let Some((file_name, line_number)) = arg.split_once(":") {
                    let line_number = line_number.parse::<u32>().expect("Could not parse number");

                    let line_index = session.get_specific_breakpoint_target(file_name, line_number);
                    handle_breakpoint_setting(session, &line_index, line_number);
                }

                // Handle setting breakpoint if only line_number is provided
                // eg: break 12
                if let Ok(line_number) = arg.parse::<u32>() {
                    let line_index = session.get_breakpoint_target(line_number).unwrap();
                    handle_breakpoint_setting(session, &line_index, line_number);
                }
            }
            "clear" => {
                let arg = parts.next().expect("Did not provide a second argument");

                // Handle clearing breakpoint if filename and line_number are provided
                // eg: clear running_task:6
                if let Some((file_name, line_number)) = arg.split_once(":") {
                    let line_number = line_number.parse::<u32>().expect("Could not parse number");
                    handle_breakpoint_clearing(session, line_number, Some(file_name));
                }

                // Handle clearing breakpoint if only line_number is provided
                // eg: clear 12
                if let Ok(line_number) = arg.parse::<u32>() {
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
                    "r" | "reg" => {
                        let regs = session.get_regs().unwrap();
                        println!("{}", regs);
                    }
                    _ => {
                        todo!()
                    }
                }
            }
            "p" | "print" => {
                let arg = parts.next().expect("No args");

                let scope_result = session.find_current_scope();

                match scope_result {
                    Ok(scope) => {
                        let var_result = session.get_var_value(&scope, arg);

                        match var_result {
                            Ok(var) => {
                                if let Some(val) = var {
                                    println!("{} = {}", arg, val);
                                } else {
                                    println!("Var '{}' not in scope at current breakpoint", arg);
                                }
                            }
                            Err(err) => eprintln!("Error occured parsing variable: {err}"),
                        }
                    }
                    Err(err) => eprintln!("Could not resolve current scope: {err}"),
                }
            }
            "q" | "quit" => {
                // End current debug session
                if let Err(err) = session.kill_session() {
                    eprintln!("{err}")
                }
                break;
            }
            _ => {
                println!("Not handled yet");
            }
        }
    }
}
