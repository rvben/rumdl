/// Rule MD069: No duplicate list markers
///
/// See [docs/md069.md](../../docs/md069.md) for full documentation, configuration, and examples.
///
/// This rule detects duplicate list markers that typically occur from copy-paste
/// with editor auto-list-continuation. For example:
///
/// ```markdown
/// - - duplicate marker (accidental)
/// ```
///
/// Should be:
///
/// ```markdown
/// - duplicate marker
/// ```
///
/// CommonMark parses `- - text` as a valid nested list, but this pattern is
/// never intentionally used in practice. Intentional nested lists use indentation:
///
/// ```markdown
/// - Parent
///   - Child
/// ```
use crate::filtered_lines::FilteredLinesExt;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use regex::Regex;
use std::sync::LazyLock;

/// Pattern: start of line, optional whitespace, list marker, space(s), another list marker, space(s)
/// Captures: (indent)(first_marker)(spaces)(second_marker)(trailing_spaces)
static DUPLICATE_MARKER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)([*+-])(\s+)([*+-])(\s+)").unwrap());

#[derive(Clone, Default)]
pub struct MD069NoDuplicateListMarkers;

impl MD069NoDuplicateListMarkers {
    pub fn new() -> Self {
        Self
    }
}

impl Rule for MD069NoDuplicateListMarkers {
    fn name(&self) -> &'static str {
        "MD069"
    }

    fn description(&self) -> &'static str {
        "Duplicate list markers"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let line_index = &ctx.line_index;

        for filtered_line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_html_blocks()
            .skip_html_comments()
        {
            let line_num = filtered_line.line_num;
            let line = filtered_line.content;

            if let Some(caps) = DUPLICATE_MARKER_REGEX.captures(line) {
                let indent = caps.get(1).map_or("", |m| m.as_str());
                let first_marker = caps.get(2).map_or("", |m| m.as_str());
                let spaces_after_first = caps.get(3).map_or("", |m| m.as_str());
                let second_marker = caps.get(4).map_or("", |m| m.as_str());
                let spaces_after_second = caps.get(5).map_or("", |m| m.as_str());

                // Calculate the full match length
                let match_len = indent.len()
                    + first_marker.len()
                    + spaces_after_first.len()
                    + second_marker.len()
                    + spaces_after_second.len();

                // Get the rest of the line after the duplicate markers
                let rest = &line[match_len..];

                // Build the fixed line: indent + second_marker + single space + rest
                let fixed_line = format!("{indent}{second_marker} {rest}");

                // Calculate byte range for the fix
                let line_start = line_index.get_line_start_byte(line_num).unwrap_or(0);
                let line_end = line_index
                    .get_line_start_byte(line_num + 1)
                    .unwrap_or(ctx.content.len());

                // Clamp line_end to content length
                let line_end = line_end.min(ctx.content.len());

                // Preserve newline if present
                let has_newline = line_end > line_start && ctx.content[line_start..line_end].ends_with('\n');
                let replacement = if has_newline {
                    format!("{fixed_line}\n")
                } else {
                    fixed_line.clone()
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: format!(
                        "Duplicate list marker '{first_marker} {second_marker}' - likely copy-paste error"
                    ),
                    line: line_num,
                    column: indent.len() + 1,
                    end_line: line_num,
                    end_column: match_len + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_start..line_end,
                        replacement,
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut result = String::with_capacity(ctx.content.len());
        let lines: Vec<&str> = ctx.content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            // Check if this line should be skipped
            let should_skip = ctx.lines.get(i).is_some_and(|info| {
                info.in_front_matter || info.in_code_block || info.in_html_block || info.in_html_comment
            });

            if should_skip {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if let Some(caps) = DUPLICATE_MARKER_REGEX.captures(line) {
                let indent = caps.get(1).map_or("", |m| m.as_str());
                let second_marker = caps.get(4).map_or("", |m| m.as_str());
                let spaces_after_first = caps.get(3).map_or("", |m| m.as_str());
                let spaces_after_second = caps.get(5).map_or("", |m| m.as_str());

                // Calculate match length
                let match_len = indent.len()
                    + caps.get(2).map_or(0, |m| m.as_str().len())
                    + spaces_after_first.len()
                    + caps.get(4).map_or(0, |m| m.as_str().len())
                    + spaces_after_second.len();

                let rest = &line[match_len..];

                // Write fixed line
                result.push_str(indent);
                result.push_str(second_marker);
                result.push(' ');
                result.push_str(rest);
                result.push('\n');
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        // Handle trailing newline
        if !ctx.content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Fast check: if no list markers exist at all, skip
        !ctx.content.contains('-') && !ctx.content.contains('*') && !ctx.content.contains('+')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(Self::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    fn check(content: &str) -> Vec<LintWarning> {
        let rule = MD069NoDuplicateListMarkers::new();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        rule.check(&ctx).unwrap()
    }

    fn fix(content: &str) -> String {
        let rule = MD069NoDuplicateListMarkers::new();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        rule.fix(&ctx).unwrap()
    }

    // === Detection tests ===

    #[test]
    fn test_duplicate_dash_markers() {
        let warnings = check("- - duplicate text");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("Duplicate list marker"));
    }

    #[test]
    fn test_duplicate_asterisk_markers() {
        let warnings = check("* * duplicate text");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_duplicate_plus_markers() {
        let warnings = check("+ + duplicate text");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_mixed_markers() {
        let warnings = check("- * mixed markers");
        assert_eq!(warnings.len(), 1);

        let warnings = check("* - mixed markers");
        assert_eq!(warnings.len(), 1);

        let warnings = check("+ - mixed markers");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_indented_duplicate() {
        let warnings = check("  - - indented duplicate");
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].column, 3); // After 2 spaces of indent
    }

    #[test]
    fn test_multiple_duplicates_in_document() {
        let content = "- - first\n- - second\n- normal";
        let warnings = check(content);
        assert_eq!(warnings.len(), 2);
    }

    // === False positive prevention ===

    #[test]
    fn test_cli_flag_no_false_positive() {
        // - --flag should NOT be flagged (second - not followed by space)
        let warnings = check("- --verbose flag");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_short_flag_no_false_positive() {
        // - -n should NOT be flagged (second - not followed by space)
        let warnings = check("- -n option");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_emphasis_no_false_positive() {
        // * *emphasis* should NOT be flagged (second * not followed by space)
        let warnings = check("* *emphasis* text");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_bold_no_false_positive() {
        let warnings = check("- **bold** text");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normal_list_no_false_positive() {
        let warnings = check("- normal list item");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_indented_nested_list_no_false_positive() {
        // Valid nested list structure
        let content = "- Parent\n  - Child";
        let warnings = check(content);
        assert!(warnings.is_empty());
    }

    // === Context skipping ===

    #[test]
    fn test_skip_code_block() {
        let content = "```\n- - in code block\n```";
        let warnings = check(content);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_skip_indented_code_block() {
        let content = "    - - indented code";
        let warnings = check(content);
        // Indented code blocks are treated as code, not lists
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_skip_frontmatter() {
        let content = "---\n- - in frontmatter\n---\n- - actual duplicate";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 4); // Only the one outside frontmatter
    }

    #[test]
    fn test_skip_html_comment() {
        let content = "<!-- - - in comment -->\n- - duplicate";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 2);
    }

    // === Fix tests ===

    #[test]
    fn test_fix_duplicate_dash() {
        let fixed = fix("- - duplicate text");
        assert_eq!(fixed, "- duplicate text");
    }

    #[test]
    fn test_fix_duplicate_asterisk() {
        let fixed = fix("* * duplicate text");
        assert_eq!(fixed, "* duplicate text");
    }

    #[test]
    fn test_fix_mixed_markers() {
        // When mixed, keep the second marker
        let fixed = fix("- * mixed");
        assert_eq!(fixed, "* mixed");
    }

    #[test]
    fn test_fix_preserves_indentation() {
        let fixed = fix("  - - indented");
        assert_eq!(fixed, "  - indented");
    }

    #[test]
    fn test_fix_multiple_duplicates() {
        let content = "- - first\n- - second";
        let fixed = fix(content);
        assert_eq!(fixed, "- first\n- second");
    }

    #[test]
    fn test_fix_preserves_normal_lines() {
        let content = "# Heading\n- - duplicate\n- normal\nParagraph";
        let fixed = fix(content);
        assert_eq!(fixed, "# Heading\n- duplicate\n- normal\nParagraph");
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let fixed = fix("- - duplicate\n");
        assert_eq!(fixed, "- duplicate\n");
    }

    #[test]
    fn test_fix_no_trailing_newline() {
        let fixed = fix("- - duplicate");
        assert_eq!(fixed, "- duplicate");
    }

    // === Edge cases ===

    #[test]
    fn test_multiple_spaces_between_markers() {
        // - -   text (multiple spaces after first marker)
        let warnings = check("-   - text");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_tab_indentation() {
        // Tab at start of line (4 spaces equivalent) creates an indented code block
        // So this should NOT be flagged - it's treated as code, not a list
        let warnings = check("\t- - duplicate");
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_empty_content_after_markers() {
        // Edge case: just markers with trailing space
        let warnings = check("- - ");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_three_markers() {
        // - - - text could be either:
        // 1. HR (---)
        // 2. Triple nested list
        // Our regex will match the first two, leaving "- text"
        let content = "- - - text";
        let warnings = check(content);
        // Should match - - and leave "- text"
        assert_eq!(warnings.len(), 1);
    }
}
