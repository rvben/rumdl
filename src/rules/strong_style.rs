use std::fmt;

/// The style for strong emphasis (MD050)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, Hash, schemars::JsonSchema)]
pub enum StrongStyle {
    /// Consistent with the first strong style found
    #[default]
    #[schemars(rename = "consistent")]
    Consistent,
    /// Asterisk style (**)
    #[schemars(rename = "asterisk")]
    Asterisk,
    /// Underscore style (__)
    #[schemars(rename = "underscore")]
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
}
