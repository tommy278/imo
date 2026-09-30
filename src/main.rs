use std::path::PathBuf;
use std::process::exit;

use rustyline::DefaultEditor;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        eprintln!("Incorrect usage.");
        eprintln!("Usage: imo <path_to_binary>");
        // TODO: Add a help branch to give more info on usage
        eprintln!("Use --help for more info");
        exit(1);
    }

    let arg = &args[1];

    let mut full_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    full_path.push(arg);

    if !full_path.exists() {
        eprintln!("Error: Binary '{}' does not exist.", arg);
        exit(1);
    }

    if full_path.is_dir() {
        eprintln!("Error: Binary target cannot be a directory.");
        exit(1);
    }

    #[allow(unused_variables)]
    #[allow(unused_mut)]
    let Ok(mut rl) = DefaultEditor::new() else {
        eprintln!("Failed to create editor instance");
        return;
    };

    #[cfg(target_os = "linux")]
    {
        if let Err(e) = imo::linux::debug(&mut rl, arg) {
            eprintln!("Failed to start debugger: {e}");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = imo::mac_os::debug(&mut rl, arg) {
            eprintln!("Failed to start debugger: {e}");
        }
    }

    eprintln!("Error: imo currently only supports Linux operating systems.");
    exit(1);
}
