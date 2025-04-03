use lazy_static::lazy_static;
use regex::Regex;

/// The style for code fence markers (MD048)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum CodeFenceStyle {
    /// Consistent with the first code fence style found
    #[default]
    Consistent,
    /// Backtick style (```)
    Backtick,
    /// Tilde style (~~~)
    Tilde,
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
