use regex::Regex;
/// MkDocs Critic Markup detection utilities
///
/// Critic Markup is a PyMdown Extensions feature for tracking changes in documents.
/// It uses special syntax to represent insertions, deletions, substitutions, highlights, and comments.
///
/// Patterns:
/// - `{++addition++}` - Insert text
/// - `{--deletion--}` - Delete text
/// - `{~~old~>new~~}` - Substitution
/// - `{==highlight==}` - Highlight
/// - `{>>comment<<}` - Comment
///
/// These patterns should be skipped from processing by most rules to avoid false positives.
use std::sync::LazyLock;

/// Pattern to match Critic Markup syntax
/// Matches: {++...++}, {--...--}, {~~...~~}, {==...==}, {>>...<<}
/// Simplified without lookahead/lookbehind for compatibility
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

/// Simple pattern to quickly check if a line might contain Critic Markup
static CRITIC_QUICK_CHECK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{(?:\+\+|--|~~|==|>>)").unwrap());

/// Check if a line contains Critic Markup
pub fn contains_critic_markup(line: &str) -> bool {
    // Quick check first for performance
    if !CRITIC_QUICK_CHECK.is_match(line) {
        return false;
    }

    CRITIC_PATTERN.is_match(line)
}

/// Check if a byte position is within Critic Markup
pub fn is_within_critic_markup(content: &str, byte_pos: usize) -> bool {
    // Find all Critic Markup spans
    for m in CRITIC_PATTERN.find_iter(content) {
        if m.start() <= byte_pos && byte_pos < m.end() {
            return true;
        }
    }
    false
}

/// Get all Critic Markup spans in content
pub fn get_critic_spans(content: &str) -> Vec<(usize, usize)> {
    CRITIC_PATTERN
        .find_iter(content)
        .map(|m| (m.start(), m.end()))
        .collect()
}

/// Check if a specific pattern might be Critic Markup
pub fn is_critic_pattern(text: &str) -> bool {
    // Check if the text matches a complete Critic Markup pattern
    CRITIC_PATTERN.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critic_addition() {
        assert!(contains_critic_markup("{++add this++}"));
        assert!(contains_critic_markup("Text {++inserted here++} more text"));
        assert!(is_critic_pattern("{++new content++}"));
    }

    #[test]
    fn test_critic_deletion() {
        assert!(contains_critic_markup("{--remove this--}"));
        assert!(contains_critic_markup("Text {--deleted--} more"));
        assert!(is_critic_pattern("{--old content--}"));
    }

    #[test]
    fn test_critic_substitution() {
        assert!(contains_critic_markup("{~~old~>new~~}"));
        assert!(contains_critic_markup("Replace {~~this~>with that~~} text"));
        assert!(is_critic_pattern("{~~original~>replacement~~}"));
    }

    #[test]
    fn test_critic_highlight() {
        assert!(contains_critic_markup("{==highlight me==}"));
        assert!(contains_critic_markup("Important {==text==} here"));
        assert!(is_critic_pattern("{==emphasized==}"));
    }

    #[test]
    fn test_critic_comment() {
        assert!(contains_critic_markup("{>>This is a comment<<}"));
        assert!(contains_critic_markup("{==text==}{>>comment about it<<}"));
        assert!(is_critic_pattern("{>>note<<}"));
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

    #[test]
    fn test_within_critic_markup() {
        let content = "Text {++added++} here";
        let add_start = content.find("{++").unwrap();
        let add_end = content.find("++}").unwrap() + 3;

        assert!(is_within_critic_markup(content, add_start + 3));
        assert!(is_within_critic_markup(content, add_end - 1));
        assert!(!is_within_critic_markup(content, 0));
        assert!(!is_within_critic_markup(content, content.len() - 1));
    }

    #[test]
    fn test_get_spans() {
        let content = "{++add++} text {--del--} more {==hi==}";
        let spans = get_critic_spans(content);

        assert_eq!(spans.len(), 3);
        assert_eq!(&content[spans[0].0..spans[0].1], "{++add++}");
        assert_eq!(&content[spans[1].0..spans[1].1], "{--del--}");
        assert_eq!(&content[spans[2].0..spans[2].1], "{==hi==}");
    }
}
