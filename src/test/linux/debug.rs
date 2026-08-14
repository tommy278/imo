use rustyline::DefaultEditor;

use crate::linux;

/// Run the test on the path only given the path from the linux directory
fn test_path(path: &str) {
    let current_dir = std::env::current_dir().unwrap();

    let mut dir = current_dir.display().to_string();

    // Remove the first two chars which in this case are '.' and '/'
    let formatted_path = &path[2..];

    let internal_dir = format!("/src/test/linux/{}", formatted_path);
    dir.push_str(&internal_dir);

    let Ok(mut rl) = DefaultEditor::new() else {
        eprintln!("Failed to create editor instance");
        return;
    };

    if let Err(err) = linux::debug(&mut rl, &dir) {
        eprintln!("{}", err);
    }
}

// NOTE: Chose this format for lsp support with finding path names

#[test]
fn running_task() {
    test_path("./running_task/running_task");
}

#[test]
fn multiple() {
    test_path("./multiple/inline_test");
}

#[test]
fn rust_with_vars() {
    test_path("./rust_with_vars/rust_with_vars");
}

#[test]
fn types() {
    test_path("./types/types");
}
