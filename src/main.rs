use std::path::PathBuf;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        eprintln!("Incorrect usage.");
        eprintln!("Usage: imo <path_to_binary>");
        // TODO: Add a help branch to give more info on usage
        eprintln!("Use --help for more info");
        exit(1);
    }

    let target_binary = &args[1];

    let mut full_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    full_path.push(target_binary);

    if !full_path.exists() {
        eprintln!("Error: Binary '{}' does not exist.", target_binary);
        exit(1);
    }

    if full_path.is_dir() {
        eprintln!("Error: Binary target cannot be a directory.");
        exit(1);
    }

    #[cfg(target_os = "linux")]
    {
        imo::linux::debug(target_binary);
    }

    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("Error: imo currently only supports Linux operating systems.");
        exit(1);
    }
}
