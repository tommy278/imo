use owo_colors::OwoColorize;

// TODO: Make this safe for any OS
// For example windows uses backslash instead of forward slash
/// Trim file path for the main source code
pub fn trim_file_path<P: AsRef<std::path::Path>>(path: &P) -> &str {
    let path = path.as_ref().to_str().unwrap();
    path.split("/").last().unwrap_or("main")
}

/// Display the code in a user friendly format
pub fn display_source_code(f: &mut std::fmt::Formatter<'_>, code: &str) -> std::fmt::Result {
    if code.starts_with("//") {
        write!(f, "{}", code.fg_rgb::<118, 118, 118>())?;
        return Ok(());
    }
    for token in code.split_whitespace() {
        match token {
            "let" | "fn" | "pub" | "impl" => write!(f, "{} ", token.red())?,
            "mut" | "return" | "if" => write!(f, "{} ", token.purple())?,
            "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "usize" | "isize"
            | "f32" | "f64" | "bool" => write!(f, "{} ", token.bright_yellow())?,
            _ => write!(f, "{token} ")?,
        }
    }
    Ok(())
}
