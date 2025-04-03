use lazy_static::lazy_static;
use regex::Regex;

/// The style for strong emphasis (MD050)
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[derive(Default)]
pub enum StrongStyle {
    /// Consistent with the first strong style found
    #[default]
    Consistent,
    /// Asterisk style (**)
    Asterisk,
    /// Underscore style (__)
    Underscore,
}


/// Get regex pattern for finding strong emphasis markers
pub fn get_strong_pattern() -> &'static Regex {
    lazy_static! {
        static ref STRONG_PATTERN: Regex = Regex::new(r"(\*\*|__)(?!\s)(?:(?!\1).)+?(?<!\s)(\1)").unwrap();
    }
    &STRONG_PATTERN
}

/// Determine the strong style from a marker
pub fn get_strong_style(marker: &str) -> Option<StrongStyle> {
    match marker {
        "**" => Some(StrongStyle::Asterisk),
        "__" => Some(StrongStyle::Underscore),
        _ => None,
    }
} 