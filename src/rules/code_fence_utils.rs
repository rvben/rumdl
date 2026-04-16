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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_fence_style_default() {
        let style = CodeFenceStyle::default();
        assert_eq!(style, CodeFenceStyle::Consistent);
    }

    #[test]
    fn test_code_fence_style_equality() {
        assert_eq!(CodeFenceStyle::Backtick, CodeFenceStyle::Backtick);
        assert_eq!(CodeFenceStyle::Tilde, CodeFenceStyle::Tilde);
        assert_eq!(CodeFenceStyle::Consistent, CodeFenceStyle::Consistent);

        assert_ne!(CodeFenceStyle::Backtick, CodeFenceStyle::Tilde);
        assert_ne!(CodeFenceStyle::Backtick, CodeFenceStyle::Consistent);
        assert_ne!(CodeFenceStyle::Tilde, CodeFenceStyle::Consistent);
    }

    #[test]
    fn test_code_fence_style_display() {
        assert_eq!(format!("{}", CodeFenceStyle::Backtick), "backtick");
        assert_eq!(format!("{}", CodeFenceStyle::Tilde), "tilde");
        assert_eq!(format!("{}", CodeFenceStyle::Consistent), "consistent");
    }

    #[test]
    fn test_code_fence_style_debug() {
        assert_eq!(format!("{:?}", CodeFenceStyle::Backtick), "Backtick");
        assert_eq!(format!("{:?}", CodeFenceStyle::Tilde), "Tilde");
        assert_eq!(format!("{:?}", CodeFenceStyle::Consistent), "Consistent");
    }

    #[test]
    fn test_code_fence_style_clone() {
        let style1 = CodeFenceStyle::Backtick;
        let style2 = style1;
        assert_eq!(style1, style2);
    }

    #[test]
    fn test_code_fence_style_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(CodeFenceStyle::Backtick);
        set.insert(CodeFenceStyle::Tilde);
        set.insert(CodeFenceStyle::Consistent);

        assert_eq!(set.len(), 3);
        assert!(set.contains(&CodeFenceStyle::Backtick));
        assert!(set.contains(&CodeFenceStyle::Tilde));
        assert!(set.contains(&CodeFenceStyle::Consistent));
    }
}
