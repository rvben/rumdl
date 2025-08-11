use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
/// Rule MD010: No tabs
///
/// See [docs/md010.md](../../docs/md010.md) for full documentation, configuration, and examples.
use crate::utils::range_utils::{LineIndex, calculate_match_range};
use crate::utils::regex_cache::{HTML_COMMENT_END, HTML_COMMENT_START};

mod md010_config;
use md010_config::MD010Config;

// HTML comment patterns are now imported from regex_cache

/// Rule MD010: Hard tabs
#[derive(Clone, Default)]
pub struct MD010NoHardTabs {
    config: MD010Config,
}

impl MD010NoHardTabs {
    pub fn new(spaces_per_tab: usize) -> Self {
        Self {
            config: MD010Config { spaces_per_tab },
        }
    }

    pub fn from_config_struct(config: MD010Config) -> Self {
        Self { config }
    }

    // Identify lines that are part of HTML comments
    fn find_html_comment_lines(lines: &[&str]) -> Vec<bool> {
        let mut in_html_comment = false;
        let mut html_comment_lines = vec![false; lines.len()];

        for (i, line) in lines.iter().enumerate() {
            // Check if this line has a comment start
            let has_comment_start = HTML_COMMENT_START.is_match(line);
            // Check if this line has a comment end
            let has_comment_end = HTML_COMMENT_END.is_match(line);

            if has_comment_start && !has_comment_end && !in_html_comment {
                // Comment starts on this line and doesn't end
                in_html_comment = true;
                html_comment_lines[i] = true;
            } else if has_comment_end && in_html_comment {
                // Comment ends on this line
                html_comment_lines[i] = true;
                in_html_comment = false;
            } else if has_comment_start && has_comment_end {
                // Both start and end on the same line
                html_comment_lines[i] = true;
            } else if in_html_comment {
                // We're inside a multi-line comment
                html_comment_lines[i] = true;
            }
        }

        html_comment_lines
    }

    fn count_leading_tabs(line: &str) -> usize {
        let mut count = 0;
        for c in line.chars() {
            if c == '\t' {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    fn find_tab_positions(line: &str) -> Vec<usize> {
        line.chars()
            .enumerate()
            .filter(|(_, c)| *c == '\t')
            .map(|(i, _)| i)
            .collect()
    }

    fn group_consecutive_tabs(tab_positions: &[usize]) -> Vec<(usize, usize)> {
        if tab_positions.is_empty() {
            return Vec::new();
        }

        let mut groups = Vec::new();
        let mut start = tab_positions[0];
        let mut end = tab_positions[0];

        for &pos in tab_positions.iter().skip(1) {
            if pos == end + 1 {
                // Consecutive tab
                end = pos;
            } else {
                // Gap found, save current group and start new one
                groups.push((start, end + 1)); // end + 1 for exclusive end
                start = pos;
                end = pos;
            }
        }

        // Add the last group
        groups.push((start, end + 1));
        groups
    }
}

impl Rule for MD010NoHardTabs {
    fn name(&self) -> &'static str {
        "MD010"
    }

    fn description(&self) -> &'static str {
        "No tabs"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Pre-compute which lines are part of HTML comments
        let html_comment_lines = Self::find_html_comment_lines(&lines);

        for (line_num, &line) in lines.iter().enumerate() {
            // Skip if in HTML comment
            if html_comment_lines[line_num] {
                continue;
            }

            // Always skip code blocks
            if let Some(line_info) = ctx.line_info(line_num + 1)
                && line_info.in_code_block
            {
                continue;
            }

            let tab_positions = Self::find_tab_positions(line);
            if tab_positions.is_empty() {
                continue;
            }

            let leading_tabs = Self::count_leading_tabs(line);
            let tab_groups = Self::group_consecutive_tabs(&tab_positions);

            // Generate warning for each group of consecutive tabs
            for (start_pos, end_pos) in tab_groups {
                let tab_count = end_pos - start_pos;
                let is_leading = start_pos < leading_tabs;

                // Calculate precise character range for the tab group
                let (start_line, start_col, end_line, end_col) =
                    calculate_match_range(line_num + 1, line, start_pos, tab_count);

                let message = if line.trim().is_empty() {
                    if tab_count == 1 {
                        "Empty line contains tab".to_string()
                    } else {
                        format!("Empty line contains {tab_count} tabs")
                    }
                } else if is_leading {
                    if tab_count == 1 {
                        format!("Found leading tab, use {} spaces instead", self.config.spaces_per_tab)
                    } else {
                        format!(
                            "Found {} leading tabs, use {} spaces instead",
                            tab_count,
                            tab_count * self.config.spaces_per_tab
                        )
                    }
                } else if tab_count == 1 {
                    "Found tab for alignment, use spaces instead".to_string()
                } else {
                    format!("Found {tab_count} tabs for alignment, use spaces instead")
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range_with_length(line_num + 1, start_pos + 1, tab_count),
                        replacement: " ".repeat(tab_count * self.config.spaces_per_tab),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        // Pre-compute which lines are part of HTML comments
        let html_comment_lines = Self::find_html_comment_lines(&lines);

        // Pre-compute line positions for code block detection
        let mut line_positions = Vec::with_capacity(lines.len());
        let mut pos = 0;
        for line in &lines {
            line_positions.push(pos);
            pos += line.len() + 1; // +1 for newline
        }

        for (i, line) in lines.iter().enumerate() {
            if html_comment_lines[i] {
                // Preserve HTML comments as they are
                result.push_str(line);
            } else if ctx.is_in_code_block_or_span(line_positions[i]) {
                // Always preserve code blocks as-is
                result.push_str(line);
            } else {
                // Replace tabs with spaces
                result.push_str(&line.replace('\t', &" ".repeat(self.config.spaces_per_tab)));
            }

            // Add newline if not the last line without a newline
            if i < lines.len() - 1 || content.ends_with('\n') {
                result.push('\n');
            }
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no tabs
        ctx.content.is_empty() || !ctx.content.contains('\t')
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD010Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD010Config::RULE_NAME.to_string(), toml::Value::Table(table)))
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD010Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::rule::Rule;

    #[test]
    fn test_no_tabs() {
        let rule = MD010NoHardTabs::default();
        let content = "This is a line\nAnother line\nNo tabs here";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_tab() {
        let rule = MD010NoHardTabs::default();
        let content = "Line with\ttab";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 10);
        assert_eq!(result[0].message, "Found tab for alignment, use spaces instead");
    }

    #[test]
    fn test_leading_tabs() {
        let rule = MD010NoHardTabs::default();
        let content = "\tIndented line\n\t\tDouble indented";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].message, "Found leading tab, use 4 spaces instead");
        assert_eq!(result[1].line, 2);
        assert_eq!(result[1].message, "Found 2 leading tabs, use 8 spaces instead");
    }

    #[test]
    fn test_fix_tabs() {
        let rule = MD010NoHardTabs::default();
        let content = "\tIndented\nNormal\tline\nNo tabs";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "    Indented\nNormal    line\nNo tabs");
    }

    #[test]
    fn test_custom_spaces_per_tab() {
        let rule = MD010NoHardTabs::new(4);
        let content = "\tIndented";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "    Indented");
    }

    #[test]
    fn test_code_blocks_always_ignored() {
        let rule = MD010NoHardTabs::default();
        let content = "Normal\tline\n```\nCode\twith\ttab\n```\nAnother\tline";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Should only flag tabs outside code blocks
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 5);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Normal    line\n```\nCode\twith\ttab\n```\nAnother    line");
    }

    #[test]
    fn test_code_blocks_never_checked() {
        let rule = MD010NoHardTabs::default();
        let content = "```\nCode\twith\ttab\n```";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Should never flag tabs in code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_html_comments_ignored() {
        let rule = MD010NoHardTabs::default();
        let content = "Normal\tline\n<!-- HTML\twith\ttab -->\nAnother\tline";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Should not flag tabs in HTML comments
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_multiline_html_comments() {
        let rule = MD010NoHardTabs::default();
        let content = "Before\n<!--\nMultiline\twith\ttabs\ncomment\t-->\nAfter\ttab";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Should only flag the tab after the comment
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_empty_lines_with_tabs() {
        let rule = MD010NoHardTabs::default();
        let content = "Normal line\n\t\t\n\t\nAnother line";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message, "Empty line contains 2 tabs");
        assert_eq!(result[1].message, "Empty line contains tab");
    }

    #[test]
    fn test_mixed_tabs_and_spaces() {
        let rule = MD010NoHardTabs::default();
        let content = " \tMixed indentation\n\t Mixed again";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_consecutive_tabs() {
        let rule = MD010NoHardTabs::default();
        let content = "Text\t\t\tthree tabs\tand\tanother";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        // Should group consecutive tabs
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].message, "Found 3 tabs for alignment, use spaces instead");
    }

    #[test]
    fn test_tab_positions() {
        let tabs = MD010NoHardTabs::find_tab_positions("a\tb\tc");
        assert_eq!(tabs, vec![1, 3]);

        let tabs = MD010NoHardTabs::find_tab_positions("\t\tabc");
        assert_eq!(tabs, vec![0, 1]);

        let tabs = MD010NoHardTabs::find_tab_positions("no tabs");
        assert!(tabs.is_empty());
    }

    #[test]
    fn test_group_consecutive_tabs() {
        let groups = MD010NoHardTabs::group_consecutive_tabs(&[0, 1, 2, 5, 6]);
        assert_eq!(groups, vec![(0, 3), (5, 7)]);

        let groups = MD010NoHardTabs::group_consecutive_tabs(&[1, 3, 5]);
        assert_eq!(groups, vec![(1, 2), (3, 4), (5, 6)]);

        let groups = MD010NoHardTabs::group_consecutive_tabs(&[]);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_count_leading_tabs() {
        assert_eq!(MD010NoHardTabs::count_leading_tabs("\t\tcode"), 2);
        assert_eq!(MD010NoHardTabs::count_leading_tabs(" \tcode"), 0);
        assert_eq!(MD010NoHardTabs::count_leading_tabs("no tabs"), 0);
        assert_eq!(MD010NoHardTabs::count_leading_tabs("\t"), 1);
    }

    #[test]
    fn test_default_config() {
        let rule = MD010NoHardTabs::default();
        let config = rule.default_config_section();
        assert!(config.is_some());
        let (name, _value) = config.unwrap();
        assert_eq!(name, "MD010");
    }

    #[test]
    fn test_from_config() {
        // Test that custom config values are properly loaded
        let custom_spaces = 8;
        let rule = MD010NoHardTabs::new(custom_spaces);
        let content = "\tTab";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "        Tab");

        // Code blocks are always ignored
        let content_with_code = "```\n\tTab in code\n```";
        let ctx = LintContext::new(content_with_code);
        let result = rule.check(&ctx).unwrap();
        // Tabs in code blocks are never flagged
        assert!(result.is_empty());
    }

    #[test]
    fn test_performance_large_document() {
        let rule = MD010NoHardTabs::default();
        let mut content = String::new();
        for i in 0..1000 {
            content.push_str(&format!("Line {i}\twith\ttabs\n"));
        }
        let ctx = LintContext::new(&content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2000);
    }

    #[test]
    fn test_preserve_content() {
        let rule = MD010NoHardTabs::default();
        let content = "**Bold**\ttext\n*Italic*\ttext\n[Link](url)\ttab";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "**Bold**    text\n*Italic*    text\n[Link](url)    tab");
    }

    #[test]
    fn test_edge_cases() {
        let rule = MD010NoHardTabs::default();

        // Tab at end of line
        let content = "Text\t";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        // Only tabs
        let content = "\t\t\t";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty line contains 3 tabs");
    }

    #[test]
    fn test_code_blocks_always_preserved_in_fix() {
        let rule = MD010NoHardTabs::default();

        let content = "Text\twith\ttab\n```makefile\ntarget:\n\tcommand\n\tanother\n```\nMore\ttabs";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Should always preserve tabs in all code blocks
        let expected = "Text    with    tab\n```makefile\ntarget:\n\tcommand\n\tanother\n```\nMore    tabs";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_find_html_comment_lines() {
        let lines = vec!["Normal", "<!-- Start", "Middle", "End -->", "After"];
        let result = MD010NoHardTabs::find_html_comment_lines(&lines);
        assert_eq!(result, vec![false, true, true, true, false]);

        let lines = vec!["<!-- Single line comment -->", "Normal"];
        let result = MD010NoHardTabs::find_html_comment_lines(&lines);
        assert_eq!(result, vec![true, false]);
    }
}
