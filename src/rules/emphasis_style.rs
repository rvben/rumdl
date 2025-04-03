use lazy_static::lazy_static;
use regex::Regex;

/// The style for emphasis (MD049)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum EmphasisStyle {
    /// Consistent with the first emphasis style found
    #[default]
    Consistent,
    /// Asterisk style (*)
    Asterisk,
    /// Underscore style (_)
    Underscore,
}

impl From<&str> for EmphasisStyle {
    fn from(s: &str) -> Self {
        match s {
            "asterisk" => EmphasisStyle::Asterisk,
            "underscore" => EmphasisStyle::Underscore,
            _ => EmphasisStyle::Consistent,
        }
    }
}

/// Get regex pattern for finding emphasis markers
pub fn get_emphasis_pattern() -> &'static Regex {
    lazy_static! {
        static ref EMPHASIS_PATTERN: Regex =
            Regex::new(r"(\*|_)(?!\s)(?:(?!\1).)+?(?<!\s)(\1)").unwrap();
    }
    &EMPHASIS_PATTERN
}

/// Determine the emphasis style from a marker
pub fn get_emphasis_style(marker: &str) -> Option<EmphasisStyle> {
    match marker {
        "*" => Some(EmphasisStyle::Asterisk),
        "_" => Some(EmphasisStyle::Underscore),
        _ => None,
    }
}
