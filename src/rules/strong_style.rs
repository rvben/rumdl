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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strong_style_default() {
        let style: StrongStyle = Default::default();
        assert_eq!(style, StrongStyle::Consistent);
    }

    #[test]
    fn test_strong_style_display() {
        assert_eq!(StrongStyle::Asterisk.to_string(), "asterisk");
        assert_eq!(StrongStyle::Underscore.to_string(), "underscore");
        assert_eq!(StrongStyle::Consistent.to_string(), "consistent");
    }

    #[test]
    fn test_strong_style_clone() {
        let style = StrongStyle::Asterisk;
        let cloned = style;
        assert_eq!(style, cloned);
    }

    #[test]
    fn test_strong_style_debug() {
        let style = StrongStyle::Underscore;
        let debug_str = format!("{style:?}");
        assert_eq!(debug_str, "Underscore");
    }
}
