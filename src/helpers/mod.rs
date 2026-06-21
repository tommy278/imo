pub mod dwarf;

// TODO: Make this safe for any OS
// For example windows uses backslash instead of forward slash
/// Trim file path for the main source code
pub fn trim_file_path(path: &std::path::Path) -> String {
    let path = path.display().to_string();
    path.split("/").last().unwrap_or("main").to_owned()
}
