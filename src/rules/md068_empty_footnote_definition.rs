//! MD068: Footnote definitions should not be empty
//!
//! This rule flags footnote definitions that have no content,
//! which is almost always a mistake.
//!
//! ## Example
//!
//! ### Incorrect
//! ```markdown
//! Text with [^1] reference.
//!
//! [^1]:
//! ```
//!
//! ### Correct
//! ```markdown
//! Text with [^1] reference.
//!
//! [^1]: This is the footnote content.
//! ```

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::md066_footnote_validation::{FOOTNOTE_DEF_PATTERN, strip_blockquote_prefix};
use crate::utils::element_cache::ElementCache;
use regex::Regex;
use std::sync::LazyLock;

/// Pattern to match a complete footnote definition line and capture its content
/// Group 1: footnote ID
/// Group 2: content after the colon (may be empty or whitespace-only)
static FOOTNOTE_DEF_WITH_CONTENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ ]{0,3}\[\^([^\]]+)\]:(.*)$").unwrap());

#[derive(Debug, Default, Clone)]
pub struct MD068EmptyFootnoteDefinition;

impl MD068EmptyFootnoteDefinition {
    pub fn new() -> Self {
        Self
    }

    /// Check if a footnote definition has continuation content on subsequent lines
    /// Multi-line footnotes have indented continuation paragraphs
    fn has_continuation_content(&self, ctx: &crate::lint_context::LintContext, def_line_idx: usize) -> bool {
        // Look at subsequent lines for indented content
        for next_idx in (def_line_idx + 1)..ctx.lines.len() {
            if let Some(next_line_info) = ctx.lines.get(next_idx) {
                // Skip frontmatter, HTML comments, and HTML blocks
                if next_line_info.in_front_matter || next_line_info.in_html_comment || next_line_info.in_html_block {
                    continue;
                }

                let next_line = next_line_info.content(ctx.content);
                let next_stripped = strip_blockquote_prefix(next_line);

                // NOTE: We intentionally do NOT skip in_code_block blindly because
                // footnote continuation uses 4-space indentation, which LintContext
                // interprets as an indented code block. We check the stripped content
                // to see if it's a legitimate continuation (4+ columns of indentation).
                // If in_code_block but doesn't start with indentation, it's a fenced code block.
                if next_line_info.in_code_block && ElementCache::calculate_indentation_width_default(next_stripped) < 4
                {
                    // This is a fenced code block, not an indented continuation
                    continue;
                }

                // Empty line - could be paragraph break in multi-line footnote
                if next_stripped.trim().is_empty() {
                    continue;
                }

                // If next non-empty line has 4+ columns of indentation, it's a continuation
                if ElementCache::calculate_indentation_width_default(next_stripped) >= 4 {
                    return true;
                }

                // If it's another footnote definition, the current one has no continuation
                if FOOTNOTE_DEF_PATTERN.is_match(next_stripped) {
                    return false;
                }

                // Non-indented, non-footnote content means no continuation
                return false;
            }
        }

        false
    }
}

impl Rule for MD068EmptyFootnoteDefinition {
    fn name(&self) -> &'static str {
        "MD068"
    }

    fn description(&self) -> &'static str {
        "Footnote definitions should not be empty"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            // Skip special contexts
            if line_info.in_code_block
                || line_info.in_front_matter
                || line_info.in_html_comment
                || line_info.in_html_block
            {
                continue;
            }

            let line = line_info.content(ctx.content);
            let line_stripped = strip_blockquote_prefix(line);

            // Check if this is a footnote definition
            if !FOOTNOTE_DEF_PATTERN.is_match(line_stripped) {
                continue;
            }

            // Extract the content after the colon
            if let Some(caps) = FOOTNOTE_DEF_WITH_CONTENT.captures(line_stripped) {
                let id = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let content = caps.get(2).map(|m| m.as_str()).unwrap_or("");

                // Check if content is empty or whitespace-only
                if content.trim().is_empty() {
                    // Check if this is a multi-line footnote (next line is indented continuation)
                    let has_continuation = self.has_continuation_content(ctx, line_idx);

                    if !has_continuation {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: line_idx + 1,
                            column: 1,
                            end_line: line_idx + 1,
                            end_column: line.len() + 1,
                            message: format!("Footnote definition '[^{id}]' is empty"),
                            severity: Severity::Error,
                            fix: None,
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Can't auto-fix - we don't know what content should be
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD068EmptyFootnoteDefinition)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LintContext;

    fn check(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        MD068EmptyFootnoteDefinition::new().check(&ctx).unwrap()
    }

    #[test]
    fn test_non_empty_definition() {
        let content = r#"Text with [^1].

[^1]: This has content.
"#;
        let warnings = check(content);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_empty_definition() {
        let content = r#"Text with [^1].

[^1]:
"#;
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("empty"));
        assert!(warnings[0].message.contains("[^1]"));
    }

    #[test]
    fn test_whitespace_only_definition() {
        let content = "Text with [^1].\n\n[^1]:   \n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("empty"));
    }

    #[test]
    fn test_multi_line_footnote() {
        // Using explicit string to ensure proper spacing
        let content = "Text with [^1].\n\n[^1]:\n    This is the content.\n";
        let warnings = check(content);
        assert!(
            warnings.is_empty(),
            "Multi-line footnotes with continuation are valid: {warnings:?}"
        );
    }

    #[test]
    fn test_multi_paragraph_footnote() {
        let content = "Text with [^1].\n\n[^1]:\n    First paragraph.\n\n    Second paragraph.\n";
        let warnings = check(content);
        assert!(warnings.is_empty(), "Multi-paragraph footnotes: {warnings:?}");
    }

    #[test]
    fn test_multiple_empty_definitions() {
        let content = r#"Text with [^1] and [^2].

[^1]:
[^2]:
"#;
        let warnings = check(content);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_mixed_empty_and_non_empty() {
        let content = r#"Text with [^1] and [^2].

[^1]: Has content
[^2]:
"#;
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("[^2]"));
    }

    #[test]
    fn test_skip_code_blocks() {
        let content = r#"Text.

```
[^1]:
```
"#;
        let warnings = check(content);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_blockquote_empty_definition() {
        let content = r#"> Text with [^1].
>
> [^1]:
"#;
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_blockquote_with_continuation() {
        // Using explicit string for clarity
        let content = "> Text with [^1].\n>\n> [^1]:\n>     Content on next line.\n";
        let warnings = check(content);
        assert!(warnings.is_empty(), "Blockquote with continuation: {warnings:?}");
    }

    #[test]
    fn test_named_footnote_empty() {
        let content = r#"Text with [^note].

[^note]:
"#;
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("[^note]"));
    }

    #[test]
    fn test_content_after_colon_space() {
        let content = r#"Text with [^1].

[^1]: Content here
"#;
        let warnings = check(content);
        assert!(warnings.is_empty());
    }
}
