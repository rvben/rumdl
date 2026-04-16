use std::fmt;

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
        match s.trim().to_ascii_lowercase().as_str() {
            "asterisk" => EmphasisStyle::Asterisk,
            "underscore" => EmphasisStyle::Underscore,
            _ => EmphasisStyle::Consistent,
        }
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
}
