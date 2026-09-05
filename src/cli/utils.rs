use std::{
    io::{self, Write},
    path::Path,
};

use owo_colors::OwoColorize;
use rustc_hash::FxHashSet;

use crate::session::{breakpoint, DebugSession};
use crate::sys::SystemError;
use crate::utils::trim_file_path;

/// Flush so the print statement is immediately displayed on screen
/// Used for print statement since its not flushed automcatically unlike println
pub fn flush_output() {
    io::stdout().flush().unwrap();
}

pub fn handle_event_by_index<F>(
    session: &mut DebugSession,
    user_index: usize,
    func: F,
    action: &str,
) where
    F: FnOnce(
        &mut DebugSession,
        usize,
    ) -> Result<breakpoint::BreakpointMutationResult, SystemError>,
{
    // Index 0 never exists
    // Index starts at 1
    if user_index == 0 {
        display_error!("No such index");
        return;
    }

    // Get the actual 0 based index once safe
    let vec_index = user_index - 1;

    // Index out of bounds or already deleted
    if vec_index >= session.current_index() {
        display_error!("No such index");
        return;
    }

    match func(session, vec_index) {
        Ok(breakpoint::BreakpointMutationResult::Updated) => {
            // Format action into being past tense
            println!(
                "{} {}{} {}",
                "Successfully".green(),
                action.green(),
                "d breakpoint".green(),
                user_index.cyan()
            );
        }
        Ok(breakpoint::BreakpointMutationResult::AlreadyInState) => {
            println!(
                "{} {} {} {}{}",
                "Breakpoint".yellow(),
                user_index.cyan(),
                "is already".yellow(),
                action.yellow(),
                "d".yellow()
            );
        }
        Ok(breakpoint::BreakpointMutationResult::NotFound) => {
            display_error!("Could not {} breakpoint", action);
        }
        Err(e) => display_error!("{}", e),

        // This is for creation which is not index based
        _ => unreachable!(),
    }
}

pub fn handle_breakpoint_clearing(
    session: &mut DebugSession,
    line_number: u32,
    file: Option<&str>,
) {
    match session.clear_breakpoint(line_number, file) {
        Ok(bp) => {
            if bp.is_empty() {
                display_error!("No breakpoint cleared");
                return;
            }
            println!("{:?}", bp)
        }
        Err(e) => display_error!("{}", e),
    }
}

pub fn handle_breakpoint_setting(
    session: &mut DebugSession,
    line_index: &[breakpoint::BreakpointTarget],
    line_number: u32,
) {
    if line_index.is_empty() {
        display_error!("Cannot set breakpoint at target");
        return;
    }

    let unique_files: FxHashSet<&Path> = line_index
        .iter()
        .map(|bp| bp.file.as_ref())
        .filter(|file| {
            let path = file.to_string_lossy();
            !path.contains("/rustc/") && !path.contains("/rust/deps")
        })
        .collect();

    if unique_files.is_empty() {
        display_error!("Cannot set breakpoint at target");
        return;
    }

    // All addresses belong to the same file
    if unique_files.len() == 1 {
        // Safe unwrap since there is one element in the set
        // Since there is only one file extract it and it is the target file
        let target_file = unique_files.into_iter().next().unwrap();

        let result = session.create_breakpoint(line_number, target_file);

        match result {
            Ok(breakpoint::BreakpointMutationResult::Created { count, target }) => {
                handle_break_metadata(session, count, target, line_number);
                return;
            }
            Ok(breakpoint::BreakpointMutationResult::NotFound) => {
                display_error!("Could not find breakpoint")
            }
            Err(e) => display_error!("{}", e),
            _ => unreachable!(),
        }

        return;
    }

    // All addresses are not the same file
    // The user has to specifically pick the file they want
    let file_choices: Vec<&Path> = unique_files.into_iter().collect();

    println!("Ambigous line number");
    for (idx, file) in file_choices.iter().enumerate() {
        println!("[{}] {}", idx, file.display());
    }
    print!("Please choose a file: ");
    flush_output();

    // Listen for user's choice for path
    let mut buffer = String::new();

    // Record user's choice in index
    let index: usize = loop {
        buffer.clear();
        io::stdin()
            .read_line(&mut buffer)
            .expect("Could not read input");

        if let Ok(idx) = buffer.trim().parse::<usize>() {
            if idx >= file_choices.len() {
                display_error!("{} is not a valid option", idx);
                continue;
            }

            break idx;
        }
    };

    let chosen_file = file_choices[index];

    let result = session.create_breakpoint(line_number, chosen_file);
    match result {
        Ok(breakpoint::BreakpointMutationResult::Created { count, target }) => {
            handle_break_metadata(session, count, target, line_number);
            return;
        }
        Ok(breakpoint::BreakpointMutationResult::NotFound) => {
            display_error!("Could not find breakpoint")
        }
        Err(e) => display_error!("{}", e),
        _ => unreachable!(),
    }
}

/// Create breakpoints and give user the data regarding them
fn handle_break_metadata(
    session: &mut DebugSession,
    bp_for_line: u8,
    first_bp: breakpoint::BreakpointTarget,
    line_number: u32,
) {
    // Handle metadata correctly
    let first_bp_relative_address = first_bp.relative_address;
    let trimmed_path = trim_file_path(&first_bp.file);

    let location_detail = if bp_for_line == 1 {
        format!("line {}", line_number)
    } else {
        format!("({} locations)", bp_for_line)
    };

    println!(
        "Breakpoint {} at {:#x}: file {}, {}",
        session.current_index().cyan(),
        first_bp_relative_address.bright_blue(),
        trimmed_path.green(),
        location_detail
    );
}

#[inline]
pub fn display_help() {
    println!(
        r#"Debugger Commands:
    run                 - Begin the debugging process
    b / break           - Pause program execution at a specific point
    clear               - Clear an existing breakpoint
    e / enable          - Enable an existing breakpoint
    dis / disable       - Disable an existing breakpoint
    d / delete          - Delete an existing breakpoint
    c / cont / continue - Resume program execution
    n / next            - Step over the next line of code
    si / stepi          - Execute the current instructtion
    s / step            - Step into the next line of code
    f / fin / finish    - Step out of the current function
    p / print           - Print the specified variable within the current scope
    i / info            - Display informating about the running process
    bt / backtrace      - Display the current stack trace
    l / ls / list       - Display the surrounding source code around current location
    q / quit            - Exit the debugger"#
    );
}

#[inline]
pub fn display_break_help() {
    println!(
        r#"Command: break
Usage: break <line> | <file:line> | <function> | *<addr>
Aliases: b

Create a breakpoint at specified location.

Options: 
    <line>              Break at the line within the current running program
    <file:line>         Break at the the line within the specified file
    <function>          Break at the beginning of the specified function
    *<addr>             Break at the exact address "#
    );
}

#[inline]
pub fn display_clear_help() {
    println!(
        r#"Command: clear 
Usage: clear <line> | <file:line> | <function> | *<addr>
Aliases: [NONE] 

Clear breakpoint at specific location. 

Options: 
    <line>              Clear breakpoint(s) that match the line number 
    <file:line>         Clear breakpoint(s) that match line number within specified file 
    <function>          Clear breakpoint(s) at the beginning of specified function
    *<addr>             Clear breakpoint at the exact address"#
    );
}

#[inline]
pub fn display_enable_help() {
    println!(
        r#"Command: enable
Usage: enable <idx> 
Aliases: e

Enable breakpoint at specific index. 
[NOTE]: Index refers to the number assigned to the breakpoint starting from 1 and incrementing by 1 

Options:
    <idx>               Enable breakpoint that matches the index "#
    );
}

#[inline]
pub fn display_disable_help() {
    println!(
        r#"Command: disable 
Usage: disable <idx> 
Aliases: dis

Disable breakpoint at specific index. 
[NOTE]: Index refers to the number assigned to the breakpoint starting from 1 and incrementing by 1 

Options:
    <idx>               Disable breakpoint that matches the index "#
    );
}

#[inline]
pub fn display_delete_help() {
    println!(
        r#"Command: delete 
Usage: delete <idx> 
Aliases: d 

Delete breakpoint at specific index. 
[NOTE]: Index refers to the number assigned to the breakpoint starting from 1 and incrementing by 1 

Options:
    <idx>               Delete breakpoint that matches the index "#
    );
}

#[inline]
pub fn display_continue_help() {
    println!(
        r#"Command: continue
Usage: continue
Aliases: c, cont

Resume program at full speed till the end or until another breakpoint is encountered.

Options: [NONE]"#
    );
}

#[inline]
pub fn display_next_help() {
    println!(
        r#"Command: next
Usage: next
Aliases: n

Step over the next line of code

Options: [NONE]"#
    );
}

#[inline]
pub fn display_single_step_help() {
    println!(
        r#"Command: stepi 
Usage: stepi 
Aliases: si 

Exectute the current instruction and immediately break

Options: [NONE]"#
    );
}

#[inline]
pub fn display_step_help() {
    println!(
        r#"Command: step 
Usage: step 
Aliases: s 

Step into the next line of code if possible. If stepping into the line is not possible it simply finished the line and step onto the next.

Options: [NONE]"#
    );
}

#[inline]
pub fn display_finish_help() {
    println!(
        r#"Command: finish 
Usage: finish 
Aliases: f, fin 

Step out of the current function.

Options: [NONE]"#
    );
}

#[inline]
pub fn display_print_help() {
    println!(
        r#"Command: print
Usage: print <var>
Aliases: p

Print the specified variable if it exists within the current function scope
[NOTE]: Shadows variable by default

Options:
    <var>               Print the variable by the name specified"#
    );
}

#[inline]
pub fn display_info_help() {
    println!(
        r#"Command: info 
Usage: info <breakpoint> | <registers> | <locals> | <arguments>
Aliases: i

Get the information for the current running process. 

Options: 
    <breakpoint>        Print all enabled and disabled breakpoints
        Aliases: b, break, breakpoint, breakpoints
    <registers>         Print all register values
        Aliases: r, reg, regs, register, registers
    <locals>            Print all variables and their values within the current scope
        Aliases: l, lo, local, locals
    <arguments>:        Print all arguments and their values within the current scope
        Aliases: a, arg, args, argument, arguments"#
    );
}

#[inline]
pub fn display_backtrace_help() {
    println!(
        r#"Command: backtrace
Usage: backtrace
Aliases: bt

Display the current stack trace.

Options: [NONE]"#
    );
}

#[inline]
pub fn display_list_help() {
    println!(
        r#"Command: list 
Usage: list 
Aliases: l, ls 

Display surrounding source code around current location.

Options: [NONE]"#
    );
}

#[inline]
pub fn display_quit_help() {
    println!(
        r#"Command: quit
Usage: quit
Aliases: q

Quit the current debugging session

Options: [NONE]"#
    );
}

#[macro_export]
macro_rules! handle_cmd {
    ($session:expr, $cmd_call:expr) => {
        if $session.is_idle() {
            println!("Session not started yet");
        } else {
            if let Err(err) = $cmd_call {
                display_error!("{}", err);
            }
            return Ok(());
        }
    };
}

#[macro_export]
macro_rules! parse_line_arg {
    ($line_str:expr, $target_type:ty) => {
        match $line_str.parse::<$target_type>() {
            Ok(num) => num,
            Err(_) => {
                eprintln!(
                    "Error: '{}' is not a valid {} value",
                    $line_str.red(),
                    stringify!($target_type).yellow()
                );
                continue;
            }
        }
    };
}

#[macro_export]
macro_rules! parse_arg {
    ($parts:expr, $cmd: expr) => {
        match $parts.next() {
            Some(arg) => arg,
            None => {
                display_error!("Error: '{}' requires a location", $cmd);
                continue;
            }
        }
    };
}

#[macro_export]
macro_rules! display_error {
    ($error:expr) => {
        eprintln!("{}", $error.red())
    };
    ($error:expr, $arg:expr) => {
        eprintln!($error, $arg.red())
    };
}

#[macro_export]
macro_rules! display_success {
    ($error:expr) => {
        println!("{}", $error.green())
    };
    ($error:expr, $arg:expr) => {
        println!($error, $arg.green())
    };
}

#[macro_export]
macro_rules! get_breakpoint_address {
    ($arg: expr) => {{
        let breakpoint_addr = if *(&$arg[1..].starts_with("0x")) {
            match u64::from_str_radix(&$arg[3..], 16) {
                Ok(addr) => addr,
                Err(_) => {
                    eprintln!("Could not convert hexadecimal address");
                    continue;
                }
            }
        } else {
            match u64::from_str_radix(&$arg[1..], 10) {
                Ok(addr) => addr,
                Err(_) => {
                    eprintln!("Could not convert decimal address");
                    continue;
                }
            }
        };
        breakpoint_addr
    }};
}

#[macro_export]
macro_rules! handle_breakpoint_with_addr {
    (create, $session: expr, $bp_addr:expr) => {
        match $session.create_specific_breakpoint($bp_addr) {
            Ok(_) => {
                if let Some(location) = $session.get_location_with_address($bp_addr) {
                    let Some(path) = $session.interner.get_str(location.file) else {
                        display_error!("Could not find corresponding file to given address");
                        continue;
                    };
                    let line_index =
                        $session.get_specific_breakpoint_target(path.into(), location.line);
                    handle_breakpoint_setting($session, &line_index, location.line);
                } else {
                    display_error!("Could not find corresponding location to address");
                    continue;
                }
            }
            Err(e) => display_error!("{}", e),
        }
    };
    (clear, $session: expr, $bp_addr:expr) => {
        match $session.create_specific_breakpoint($bp_addr) {
            Ok(_) => {
                if let Some(location) = $session.get_location_with_address($bp_addr) {
                    let Some(path) = $session.interner.get_string(location.file) else {
                        display_error!("Could not find corresponding file to given address");
                        continue;
                    };
                    handle_breakpoint_clearing($session, location.line, Some(path.as_str().into()));
                } else {
                    display_error!("Could not find corresponding location to address");
                    continue;
                }
            }
            Err(e) => display_error!("{}", e),
        }
    };
}

pub use crate::{
    display_error, display_success, get_breakpoint_address, handle_breakpoint_with_addr,
    handle_cmd, parse_arg, parse_line_arg,
};
