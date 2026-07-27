use std::fmt;

/// Numbers assigned to colors based on the ANSI sheet
pub enum Color {
    // Standard Colors
    Black = 30,
    Red = 31,
    Green = 32,
    Yellow = 33,
    Blue = 34,
    Magenta = 35,
    Cyan = 36,
    White = 37,

    // Bright Colors
    BrightBlack = 90,
    BrightRed = 91,
    BrightGreen = 92,
    BrightYellow = 93,
    BrightBlue = 94,
    BrightMagenta = 95,
    BrightCyan = 96,
    BrightWhite = 97,
}

/// Print color dynamically based on the enum provided
pub fn print_color<T>(f: &mut fmt::Formatter<'_>, val: T, color: Color) -> Result<(), fmt::Error>
where
    T: fmt::Display,
{
    write!(f, "\x1b[{}m{val}\x1b[0m", color as u8)
}
