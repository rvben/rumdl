use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use std::fmt;

/// The style for strong emphasis (MD050)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, Hash)]
pub enum StrongStyle {
    /// Consistent with the first strong style found
    #[default]
    Consistent,
    /// Asterisk style (**)
    Asterisk,
    /// Underscore style (__)
    Underscore,
}

impl fmt::Display for StrongStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StrongStyle::Asterisk => write!(f, "asterisk"),
            StrongStyle::Underscore => write!(f, "underscore"),
            StrongStyle::Consistent => write!(f, "consistent"),
        }
    }
}

/// Get regex pattern for finding strong emphasis markers
pub fn get_strong_pattern() -> &'static FancyRegex {
    lazy_static! {
        static ref STRONG_REGEX: FancyRegex =
            FancyRegex::new(r"(\*\*|__)(?!\s)(?:(?!\1).)+?(?<!\s)(\1)").unwrap();
    }
    &STRONG_REGEX
}

/// Determine the strong style from a marker
pub fn get_strong_style(marker: &str) -> Option<StrongStyle> {
    match marker {
        "**" => Some(StrongStyle::Asterisk),
        "__" => Some(StrongStyle::Underscore),
        _ => None,
    }
}
