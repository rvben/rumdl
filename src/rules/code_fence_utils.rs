use lazy_static::lazy_static;
use regex::Regex;
use std::fmt;

/// The style for code fence markers (MD048)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, Hash)]
pub enum CodeFenceStyle {
    /// Consistent with the first code fence style found
    #[default]
    Consistent,
    /// Backtick style (```)
    Backtick,
    /// Tilde style (~~~)
    Tilde,
}

impl fmt::Display for CodeFenceStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeFenceStyle::Backtick => write!(f, "backtick"),
            CodeFenceStyle::Tilde => write!(f, "tilde"),
            CodeFenceStyle::Consistent => write!(f, "consistent"),
        }
    }
}

/// Get regex pattern for finding code fence markers
pub fn get_code_fence_pattern() -> &'static Regex {
    lazy_static! {
        static ref CODE_FENCE_PATTERN: Regex = Regex::new(r"^(```|~~~)").unwrap();
    }
    &CODE_FENCE_PATTERN
}

/// Determine the code fence style from a marker
pub fn get_fence_style(marker: &str) -> Option<CodeFenceStyle> {
    match marker {
        "```" => Some(CodeFenceStyle::Backtick),
        "~~~" => Some(CodeFenceStyle::Tilde),
        _ => None,
    }
}
