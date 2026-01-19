use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::range_utils::calculate_trailing_range;
use crate::utils::regex_cache::{ORDERED_LIST_MARKER_REGEX, UNORDERED_LIST_MARKER_REGEX, get_cached_regex};

mod md009_config;
use md009_config::MD009Config;

// No need for lazy_static, we'll use get_cached_regex directly

#[derive(Debug, Clone, Default)]
pub struct MD009TrailingSpaces {
    config: MD009Config,
}

impl MD009TrailingSpaces {
    pub fn new(br_spaces: usize, strict: bool) -> Self {
        Self {
            config: MD009Config {
                br_spaces: crate::types::BrSpaces::from_const(br_spaces),
                strict,
                list_item_empty_lines: false,
            },
        }
    }

    pub const fn from_config_struct(config: MD009Config) -> Self {
        Self { config }
    }

    fn count_trailing_spaces(line: &str) -> usize {
        line.chars().rev().take_while(|&c| c == ' ').count()
    }

    fn count_trailing_spaces_ascii(line: &str) -> usize {
        line.as_bytes().iter().rev().take_while(|&&b| b == b' ').count()
    }

    fn trimmed_len_ascii_whitespace(line: &str) -> usize {
        line.as_bytes()
            .iter()
            .rposition(|b| !b.is_ascii_whitespace())
            .map(|idx| idx + 1)
            .unwrap_or(0)
    }

    fn calculate_trailing_range_ascii(
        line: usize,
        line_len: usize,
        content_end: usize,
    ) -> (usize, usize, usize, usize) {
        // Return 1-indexed columns to match calculate_trailing_range behavior
        (line, content_end + 1, line, line_len + 1)
    }

    fn is_empty_list_item_line(line: &str, prev_line: Option<&str>) -> bool {
        // A line is an empty list item line if:
        // 1. It's blank or only contains spaces
        // 2. The previous line is a list item
        if !line.trim().is_empty() {
            return false;
        }

        if let Some(prev) = prev_line {
            // Check for unordered list markers (*, -, +) with proper formatting
            UNORDERED_LIST_MARKER_REGEX.is_match(prev) || ORDERED_LIST_MARKER_REGEX.is_match(prev)
        } else {
            false
        }
    }
}

impl Rule for MD009TrailingSpaces {
    fn name(&self) -> &'static str {
        "MD009"
    }

    fn description(&self) -> &'static str {
        "Trailing spaces should be removed"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = &ctx.line_index;

        let mut warnings = Vec::new();

        // Store lines in a Vec only for accessing previous line (for list item check)
        // This is still necessary due to the need to look back at prev_line
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, &line) in lines.iter().enumerate() {
            let line_is_ascii = line.is_ascii();
            let trailing_spaces = if line_is_ascii {
                Self::count_trailing_spaces_ascii(line)
            } else {
                Self::count_trailing_spaces(line)
            };

            // Skip if no trailing spaces
            if trailing_spaces == 0 {
                continue;
            }

            // Handle empty lines
            let trimmed_len = if line_is_ascii {
                Self::trimmed_len_ascii_whitespace(line)
            } else {
                line.trim_end().len()
            };
            if trimmed_len == 0 {
                if trailing_spaces > 0 {
                    // Check if this is an empty list item line and config allows it
                    let prev_line = if line_num > 0 { Some(lines[line_num - 1]) } else { None };
                    if self.config.list_item_empty_lines && Self::is_empty_list_item_line(line, prev_line) {
                        continue;
                    }

                    // Calculate precise character range for all trailing spaces on empty line
                    let (start_line, start_col, end_line, end_col) = if line_is_ascii {
                        Self::calculate_trailing_range_ascii(line_num + 1, line.len(), 0)
                    } else {
                        calculate_trailing_range(line_num + 1, line, 0)
                    };
                    let line_start = *ctx.line_offsets.get(line_num).unwrap_or(&0);
                    let fix_range = if line_is_ascii {
                        line_start..line_start + line.len()
                    } else {
                        _line_index.line_col_to_byte_range_with_length(line_num + 1, 1, line.len())
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Empty line has trailing spaces".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: fix_range,
                            replacement: String::new(),
                        }),
                    });
                }
                continue;
            }

            // Handle code blocks if not in strict mode
            if !self.config.strict {
                // Use pre-computed line info
                if let Some(line_info) = ctx.line_info(line_num + 1)
                    && line_info.in_code_block
                {
                    continue;
                }
            }

            // Check if it's a valid line break
            // Special handling: if the content ends with a newline, the last line from .lines()
            // is not really the "last line" in terms of trailing spaces rules
            let is_truly_last_line = line_num == lines.len() - 1 && !content.ends_with('\n');
            if !self.config.strict && !is_truly_last_line && trailing_spaces == self.config.br_spaces.get() {
                continue;
            }

            // Check if this is an empty blockquote line ("> " or ">> " etc)
            // These are allowed by MD028 to have a single trailing space
            let trimmed = if line_is_ascii {
                &line[..trimmed_len]
            } else {
                line.trim_end()
            };
            let is_empty_blockquote_with_space = trimmed.chars().all(|c| c == '>' || c == ' ' || c == '\t')
                && trimmed.contains('>')
                && trailing_spaces == 1;

            if is_empty_blockquote_with_space {
                continue; // Allow single trailing space for empty blockquote lines
            }
            // Calculate precise character range for all trailing spaces
            let (start_line, start_col, end_line, end_col) = if line_is_ascii {
                Self::calculate_trailing_range_ascii(line_num + 1, line.len(), trimmed.len())
            } else {
                calculate_trailing_range(line_num + 1, line, trimmed.len())
            };
            let line_start = *ctx.line_offsets.get(line_num).unwrap_or(&0);
            let fix_range = if line_is_ascii {
                let start = line_start + trimmed.len();
                let end = start + trailing_spaces;
                start..end
            } else {
                _line_index.line_col_to_byte_range_with_length(
                    line_num + 1,
                    trimmed.chars().count() + 1,
                    trailing_spaces,
                )
            };

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: if trailing_spaces == 1 {
                    "Trailing space found".to_string()
                } else {
                    format!("{trailing_spaces} trailing spaces found")
                },
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: fix_range,
                    replacement: if !self.config.strict
                        && !is_truly_last_line
                        && trailing_spaces == self.config.br_spaces.get()
                    {
                        " ".repeat(self.config.br_spaces.get())
                    } else {
                        String::new()
                    },
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // For simple cases (strict mode), use fast regex approach
        if self.config.strict {
            // In strict mode, remove ALL trailing spaces everywhere
            return Ok(get_cached_regex(r"(?m) +$")
                .unwrap()
                .replace_all(content, "")
                .to_string());
        }

        // For complex cases, we need line-by-line processing but with optimizations
        // We need to collect lines since we need to look at previous lines for list item checks
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::with_capacity(content.len()); // Pre-allocate capacity

        for (i, line) in lines.iter().enumerate() {
            // Fast path: if no trailing spaces, just add the line
            if !line.ends_with(' ') {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            let trimmed = line.trim_end();
            let trailing_spaces = Self::count_trailing_spaces(line);

            // Handle empty lines - fast regex replacement
            if trimmed.is_empty() {
                // Check if this is an empty list item line and config allows it
                let prev_line = if i > 0 { Some(lines[i - 1]) } else { None };
                if self.config.list_item_empty_lines && Self::is_empty_list_item_line(line, prev_line) {
                    result.push_str(line);
                } else {
                    // Remove all trailing spaces - line is empty so don't add anything
                }
                result.push('\n');
                continue;
            }

            // Handle code blocks if not in strict mode
            if let Some(line_info) = ctx.line_info(i + 1)
                && line_info.in_code_block
            {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            // No special handling for empty blockquote lines - treat them like regular lines

            // Handle lines with trailing spaces
            let is_truly_last_line = i == lines.len() - 1 && !content.ends_with('\n');

            result.push_str(trimmed);

            // Check if this line is a heading - headings should never have trailing spaces
            let is_heading = if let Some(line_info) = ctx.line_info(i + 1) {
                line_info.heading.is_some()
            } else {
                // Fallback: check if line starts with #
                trimmed.starts_with('#')
            };

            // Check if this is an empty blockquote line (just ">")
            let is_empty_blockquote = if let Some(line_info) = ctx.line_info(i + 1) {
                line_info.blockquote.as_ref().is_some_and(|bq| bq.content.is_empty())
            } else {
                false
            };

            // In non-strict mode, preserve line breaks ONLY if they have exactly br_spaces
            // BUT: Never preserve trailing spaces in headings or empty blockquotes as they serve no purpose
            if !self.config.strict
                && !is_truly_last_line
                && trailing_spaces == self.config.br_spaces.get()
                && !is_heading
                && !is_empty_blockquote
            {
                // Preserve the exact number of spaces for hard line breaks
                match self.config.br_spaces.get() {
                    0 => {}
                    1 => result.push(' '),
                    2 => result.push_str("  "),
                    n => result.push_str(&" ".repeat(n)),
                }
            }
            result.push('\n');
        }

        // Preserve original ending (with or without final newline)
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no spaces at all
        ctx.content.is_empty() || !ctx.content.contains(' ')
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD009Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD009Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD009Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::rule::Rule;

    #[test]
    fn test_no_trailing_spaces() {
        let rule = MD009TrailingSpaces::default();
        let content = "This is a line\nAnother line\nNo trailing spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_basic_trailing_spaces() {
        let rule = MD009TrailingSpaces::default();
        let content = "Line with spaces   \nAnother line  \nClean line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Default br_spaces=2, so line with 2 spaces is OK
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].message, "3 trailing spaces found");
    }

    #[test]
    fn test_fix_basic_trailing_spaces() {
        let rule = MD009TrailingSpaces::default();
        let content = "Line with spaces   \nAnother line  \nClean line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Line 1: 3 spaces -> removed (doesn't match br_spaces=2)
        // Line 2: 2 spaces -> kept (matches br_spaces=2)
        // Line 3: no spaces -> unchanged
        assert_eq!(fixed, "Line with spaces\nAnother line  \nClean line");
    }

    #[test]
    fn test_strict_mode() {
        let rule = MD009TrailingSpaces::new(2, true);
        let content = "Line with spaces  \nCode block:  \n```  \nCode with spaces  \n```  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // In strict mode, all trailing spaces are flagged
        assert_eq!(result.len(), 5);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line with spaces\nCode block:\n```\nCode with spaces\n```");
    }

    #[test]
    fn test_non_strict_mode_with_code_blocks() {
        let rule = MD009TrailingSpaces::new(2, false);
        let content = "Line with spaces  \n```\nCode with spaces  \n```\nOutside code  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // In non-strict mode, code blocks are not checked
        // Line 1 has 2 spaces (= br_spaces), so it's OK
        // Line 5 is last line without newline, so trailing spaces are flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_br_spaces_preservation() {
        let rule = MD009TrailingSpaces::new(2, false);
        let content = "Line with two spaces  \nLine with three spaces   \nLine with one space ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // br_spaces=2, so lines with exactly 2 spaces are OK
        // Line 2 has 3 spaces (should be removed, not normalized)
        // Line 3 has 1 space and is last line without newline (will be removed)
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);

        let fixed = rule.fix(&ctx).unwrap();
        // Line 1: keeps 2 spaces (exact match with br_spaces)
        // Line 2: removes all 3 spaces (doesn't match br_spaces)
        // Line 3: last line without newline, spaces removed
        assert_eq!(
            fixed,
            "Line with two spaces  \nLine with three spaces\nLine with one space"
        );
    }

    #[test]
    fn test_empty_lines_with_spaces() {
        let rule = MD009TrailingSpaces::default();
        let content = "Normal line\n   \n  \nAnother line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message, "Empty line has trailing spaces");
        assert_eq!(result[1].message, "Empty line has trailing spaces");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Normal line\n\n\nAnother line");
    }

    #[test]
    fn test_empty_blockquote_lines() {
        let rule = MD009TrailingSpaces::default();
        let content = "> Quote\n>   \n> More quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[0].message, "3 trailing spaces found");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> Quote\n>\n> More quote"); // All trailing spaces removed
    }

    #[test]
    fn test_last_line_handling() {
        let rule = MD009TrailingSpaces::new(2, false);

        // Content without final newline
        let content = "First line  \nLast line  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Last line without newline should have trailing spaces removed
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "First line  \nLast line");

        // Content with final newline
        let content_with_newline = "First line  \nLast line  \n";
        let ctx = LintContext::new(content_with_newline, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Both lines should preserve br_spaces
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_trailing_space() {
        let rule = MD009TrailingSpaces::new(2, false);
        let content = "Line with one space ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Trailing space found");
    }

    #[test]
    fn test_tabs_not_spaces() {
        let rule = MD009TrailingSpaces::default();
        let content = "Line with tab\t\nLine with spaces  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Only spaces are checked, not tabs
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_mixed_content() {
        let rule = MD009TrailingSpaces::new(2, false);
        // Construct content with actual trailing spaces using string concatenation
        let mut content = String::new();
        content.push_str("# Heading");
        content.push_str("   "); // Add 3 trailing spaces (more than br_spaces=2)
        content.push('\n');
        content.push_str("Normal paragraph\n> Blockquote\n>\n```\nCode block\n```\n- List item\n");

        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should flag the line with trailing spaces
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].message.contains("trailing spaces"));
    }

    #[test]
    fn test_column_positions() {
        let rule = MD009TrailingSpaces::default();
        let content = "Text   ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 5); // After "Text"
        assert_eq!(result[0].end_column, 8); // After all spaces
    }

    #[test]
    fn test_default_config() {
        let rule = MD009TrailingSpaces::default();
        let config = rule.default_config_section();
        assert!(config.is_some());
        let (name, _value) = config.unwrap();
        assert_eq!(name, "MD009");
    }

    #[test]
    fn test_from_config() {
        let mut config = crate::config::Config::default();
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config
            .values
            .insert("br_spaces".to_string(), toml::Value::Integer(3));
        rule_config
            .values
            .insert("strict".to_string(), toml::Value::Boolean(true));
        config.rules.insert("MD009".to_string(), rule_config);

        let rule = MD009TrailingSpaces::from_config(&config);
        let content = "Line   ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        // In strict mode, should remove all spaces
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line");
    }

    #[test]
    fn test_list_item_empty_lines() {
        // Create rule with list_item_empty_lines enabled
        let config = MD009Config {
            list_item_empty_lines: true,
            ..Default::default()
        };
        let rule = MD009TrailingSpaces::from_config_struct(config);

        // Test unordered list with empty line
        let content = "- First item\n  \n- Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should not flag the empty line with spaces after list item
        assert!(result.is_empty());

        // Test ordered list with empty line
        let content = "1. First item\n  \n2. Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test that non-list empty lines are still flagged
        let content = "Normal paragraph\n  \nAnother paragraph";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_list_item_empty_lines_disabled() {
        // Default config has list_item_empty_lines disabled
        let rule = MD009TrailingSpaces::default();

        let content = "- First item\n  \n- Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should flag the empty line with spaces
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_performance_large_document() {
        let rule = MD009TrailingSpaces::default();
        let mut content = String::new();
        for i in 0..1000 {
            content.push_str(&format!("Line {i} with spaces  \n"));
        }
        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Default br_spaces=2, so all lines with 2 spaces are OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_preserve_content_after_fix() {
        let rule = MD009TrailingSpaces::new(2, false);
        let content = "**Bold** text  \n*Italic* text  \n[Link](url)  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "**Bold** text  \n*Italic* text  \n[Link](url)");
    }

    #[test]
    fn test_nested_blockquotes() {
        let rule = MD009TrailingSpaces::default();
        let content = "> > Nested  \n> >   \n> Normal  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Line 2 has empty blockquote with 3 spaces, line 3 is last line without newline
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);

        let fixed = rule.fix(&ctx).unwrap();
        // Line 1: Keeps 2 spaces (exact match with br_spaces)
        // Line 2: Empty blockquote with 3 spaces -> removes all (doesn't match br_spaces)
        // Line 3: Last line without newline -> removes all spaces
        assert_eq!(fixed, "> > Nested  \n> >\n> Normal");
    }

    #[test]
    fn test_normalized_line_endings() {
        let rule = MD009TrailingSpaces::default();
        // In production, content is normalized to LF at I/O boundary
        let content = "Line with spaces  \nAnother line  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Line 1 has 2 spaces (= br_spaces) so it's OK
        // Line 2 is last line without newline, so it's flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_issue_80_no_space_normalization() {
        // Test for GitHub issue #80 - MD009 should not add spaces when removing trailing spaces
        let rule = MD009TrailingSpaces::new(2, false); // br_spaces=2

        // Test that 1 trailing space is removed, not normalized to 2
        let content = "Line with one space \nNext line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].message, "Trailing space found");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line with one space\nNext line");

        // Test that 3 trailing spaces are removed, not normalized to 2
        let content = "Line with three spaces   \nNext line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].message, "3 trailing spaces found");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line with three spaces\nNext line");

        // Test that exactly 2 trailing spaces are preserved
        let content = "Line with two spaces  \nNext line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0); // Should not flag lines with exact br_spaces

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Line with two spaces  \nNext line");
    }
}
