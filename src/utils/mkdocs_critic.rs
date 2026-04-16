/// MkDocs Critic Markup detection.
///
/// Critic Markup is a PyMdown Extensions feature for tracking changes in
/// documents using dedicated syntax for insertions, deletions, substitutions,
/// highlights, and comments:
///
/// - `{++addition++}`
/// - `{--deletion--}`
/// - `{~~old~>new~~}`
/// - `{==highlight==}`
/// - `{>>comment<<}`
///
/// These patterns should be skipped from processing by most rules to avoid
/// false positives.
use regex::Regex;
use std::sync::LazyLock;

/// Pattern to match Critic Markup syntax. Simplified without lookahead/lookbehind
/// for regex-crate compatibility.
static CRITIC_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        \{                          # Opening brace
        (?:
            \+\+                    # Addition marker
            [^}]*?                  # Content (non-greedy)
            \+\+                    # Closing addition marker
        |
            --                      # Deletion marker
            [^}]*?                  # Content (non-greedy)
            --                      # Closing deletion marker
        |
            ~~                      # Substitution start
            [^}]*?                  # Content including ~> (non-greedy)
            ~~                      # Substitution end
        |
            ==                      # Highlight marker
            [^}]*?                  # Content (non-greedy)
            ==                      # Closing highlight marker
        |
            >>                      # Comment start
            [^}]*?                  # Content (non-greedy)
            <<                      # Comment end
        )
        \}                          # Closing brace
        ",
    )
    .unwrap()
});

/// Fast pre-filter that avoids the full regex when the line can't contain
/// Critic Markup.
static CRITIC_QUICK_CHECK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{(?:\+\+|--|~~|==|>>)").unwrap());

/// Check if a line contains Critic Markup.
pub fn contains_critic_markup(line: &str) -> bool {
    if !CRITIC_QUICK_CHECK.is_match(line) {
        return false;
    }
    CRITIC_PATTERN.is_match(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critic_addition() {
        assert!(contains_critic_markup("{++add this++}"));
        assert!(contains_critic_markup("Text {++inserted here++} more text"));
    }

    #[test]
    fn test_critic_deletion() {
        assert!(contains_critic_markup("{--remove this--}"));
        assert!(contains_critic_markup("Text {--deleted--} more"));
    }

    #[test]
    fn test_critic_substitution() {
        assert!(contains_critic_markup("{~~old~>new~~}"));
        assert!(contains_critic_markup("Replace {~~this~>with that~~} text"));
    }

    #[test]
    fn test_critic_highlight() {
        assert!(contains_critic_markup("{==highlight me==}"));
        assert!(contains_critic_markup("Important {==text==} here"));
    }

    #[test]
    fn test_critic_comment() {
        assert!(contains_critic_markup("{>>This is a comment<<}"));
        assert!(contains_critic_markup("{==text==}{>>comment about it<<}"));
    }

    #[test]
    fn test_multiline_critic() {
        let content = "Here is {++some\ntext that\nspans lines++} ok";
        assert!(contains_critic_markup(content));
    }

    #[test]
    fn test_not_critic() {
        assert!(!contains_critic_markup("Normal {text} here"));
        assert!(!contains_critic_markup("Just ++ symbols"));
        assert!(!contains_critic_markup("{+ incomplete +}"));
        assert!(!contains_critic_markup("{{ template }}"));
    }
}
