use regex::Regex;
use std::fmt;
use std::sync::LazyLock;

/// The style for emphasis (MD049)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, Hash)]
pub enum EmphasisStyle {
    /// Consistent with the most prevalent emphasis style found (or first found if tied)
    #[default]
    Consistent,
    /// Asterisk style (*)
    Asterisk,
    /// Underscore style (_)
    Underscore,
}

impl fmt::Display for EmphasisStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmphasisStyle::Asterisk => write!(f, "asterisk"),
            EmphasisStyle::Underscore => write!(f, "underscore"),
            EmphasisStyle::Consistent => write!(f, "consistent"),
        }
    }
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

/// Get regex pattern for finding emphasis markers based on style
pub fn get_emphasis_pattern(style: EmphasisStyle) -> &'static Regex {
    static ASTERISK_EMPHASIS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\*([^\s*][^*]*?[^\s*]|[^\s*])\*").unwrap());
    static UNDERSCORE_EMPHASIS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"_([^\s_][^_]*?[^\s_]|[^\s_])_").unwrap());

    match style {
        EmphasisStyle::Asterisk => &ASTERISK_EMPHASIS,
        EmphasisStyle::Underscore => &UNDERSCORE_EMPHASIS,
        EmphasisStyle::Consistent => &ASTERISK_EMPHASIS, // Default to asterisk for consistent style
    }
}

/// Determine the emphasis style from a marker
pub fn get_emphasis_style(marker: &str) -> Option<EmphasisStyle> {
    match marker {
        "*" => Some(EmphasisStyle::Asterisk),
        "_" => Some(EmphasisStyle::Underscore),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emphasis_style_from_str() {
        assert_eq!(EmphasisStyle::from("asterisk"), EmphasisStyle::Asterisk);
        assert_eq!(EmphasisStyle::from("underscore"), EmphasisStyle::Underscore);
        assert_eq!(EmphasisStyle::from(""), EmphasisStyle::Consistent);
        assert_eq!(EmphasisStyle::from("unknown"), EmphasisStyle::Consistent);
    }

    #[test]
    fn test_get_emphasis_pattern() {
        // Test asterisk pattern
        let asterisk_pattern = get_emphasis_pattern(EmphasisStyle::Asterisk);
        assert!(asterisk_pattern.is_match("*text*"));
        assert!(asterisk_pattern.is_match("*t*"));
        assert!(!asterisk_pattern.is_match("* text*"));
        assert!(!asterisk_pattern.is_match("*text *"));

        // Test underscore pattern
        let underscore_pattern = get_emphasis_pattern(EmphasisStyle::Underscore);
        assert!(underscore_pattern.is_match("_text_"));
        assert!(underscore_pattern.is_match("_t_"));
        assert!(!underscore_pattern.is_match("_ text_"));
        assert!(!underscore_pattern.is_match("_text _"));

        // Test consistent pattern (default to asterisk)
        let consistent_pattern = get_emphasis_pattern(EmphasisStyle::Consistent);
        assert!(consistent_pattern.is_match("*text*"));
        assert!(!consistent_pattern.is_match("_text_"));
    }

    #[test]
    fn test_get_emphasis_style() {
        assert_eq!(get_emphasis_style("*"), Some(EmphasisStyle::Asterisk));
        assert_eq!(get_emphasis_style("_"), Some(EmphasisStyle::Underscore));
        assert_eq!(get_emphasis_style(""), None);
        assert_eq!(get_emphasis_style("**"), None);
        assert_eq!(get_emphasis_style("__"), None);
    }
}
