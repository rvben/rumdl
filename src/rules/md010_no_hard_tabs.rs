use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
/// Rule MD010: No tabs
///
/// See [docs/md010.md](../../docs/md010.md) for full documentation, configuration, and examples.
use crate::utils::range_utils::calculate_match_range;
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
            config: MD010Config {
                spaces_per_tab: crate::types::PositiveUsize::from_const(spaces_per_tab),
            },
        }
    }

    pub const fn from_config_struct(config: MD010Config) -> Self {
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

    fn find_and_group_tabs(line: &str) -> Vec<(usize, usize)> {
        let mut groups = Vec::new();
        let mut current_group_start: Option<usize> = None;
        let mut last_tab_pos = 0;

        for (i, c) in line.chars().enumerate() {
            if c == '\t' {
                if let Some(start) = current_group_start {
                    // We're in a group - check if this tab is consecutive
                    if i == last_tab_pos + 1 {
                        // Consecutive tab, continue the group
                        last_tab_pos = i;
                    } else {
                        // Gap found, save current group and start new one
                        groups.push((start, last_tab_pos + 1));
                        current_group_start = Some(i);
                        last_tab_pos = i;
                    }
                } else {
                    // Start a new group
                    current_group_start = Some(i);
                    last_tab_pos = i;
                }
            }
        }

        // Add the last group if there is one
        if let Some(start) = current_group_start {
            groups.push((start, last_tab_pos + 1));
        }

        groups
    }

    /// Find lines that are inside fenced code blocks (``` or ~~~)
    /// Returns a Vec<bool> where index i indicates if line i is inside a fenced code block
    fn find_fenced_code_block_lines(lines: &[&str]) -> Vec<bool> {
        let mut in_fenced_block = false;
        let mut fence_char: Option<char> = None;
        let mut result = vec![false; lines.len()];

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            if !in_fenced_block {
                // Check for opening fence (``` or ~~~)
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    in_fenced_block = true;
                    fence_char = Some(trimmed.chars().next().unwrap());
                    result[i] = true; // Mark the fence line itself as "in fenced block"
                }
            } else {
                result[i] = true;
                // Check for closing fence (must match opening fence char)
                if let Some(fc) = fence_char {
                    let fence_str: String = std::iter::repeat_n(fc, 3).collect();
                    if trimmed.starts_with(&fence_str) && trimmed.trim() == fence_str {
                        in_fenced_block = false;
                        fence_char = None;
                    }
                }
            }
        }

        result
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
        let _line_index = &ctx.line_index;

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Pre-compute which lines are part of HTML comments
        let html_comment_lines = Self::find_html_comment_lines(&lines);

        // Pre-compute which lines are inside fenced code blocks (``` or ~~~)
        // We only skip fenced code blocks - code has its own formatting rules
        // (e.g., Makefiles require tabs, Go uses tabs by convention)
        // We still flag tab-indented content because it might be accidental
        let fenced_code_block_lines = Self::find_fenced_code_block_lines(&lines);

        for (line_num, &line) in lines.iter().enumerate() {
            // Skip if in HTML comment
            if html_comment_lines[line_num] {
                continue;
            }

            // Skip if in fenced code block - code has its own formatting rules
            if fenced_code_block_lines[line_num] {
                continue;
            }

            // Process tabs directly without intermediate collection
            let tab_groups = Self::find_and_group_tabs(line);
            if tab_groups.is_empty() {
                continue;
            }

            let leading_tabs = Self::count_leading_tabs(line);

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
                        format!(
                            "Found leading tab, use {} spaces instead",
                            self.config.spaces_per_tab.get()
                        )
                    } else {
                        format!(
                            "Found {} leading tabs, use {} spaces instead",
                            tab_count,
                            tab_count * self.config.spaces_per_tab.get()
                        )
                    }
                } else if tab_count == 1 {
                    "Found tab for alignment, use spaces instead".to_string()
                } else {
                    format!("Found {tab_count} tabs for alignment, use spaces instead")
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range_with_length(line_num + 1, start_pos + 1, tab_count),
                        replacement: " ".repeat(tab_count * self.config.spaces_per_tab.get()),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        // Pre-compute which lines are part of HTML comments
        let html_comment_lines = Self::find_html_comment_lines(&lines);

        // Pre-compute which lines are inside fenced code blocks
        // Only skip fenced code blocks - code has its own formatting rules
        // (e.g., Makefiles require tabs, Go uses tabs by convention)
        let fenced_code_block_lines = Self::find_fenced_code_block_lines(&lines);

        for (i, line) in lines.iter().enumerate() {
            if html_comment_lines[i] {
                // Preserve HTML comments as they are
                result.push_str(line);
            } else if fenced_code_block_lines[i] {
                // Preserve fenced code blocks as-is - code has its own formatting rules
                result.push_str(line);
            } else {
                // Replace tabs with spaces in regular markdown content
                // (including tab-indented content which might be accidental)
                result.push_str(&line.replace('\t', &" ".repeat(self.config.spaces_per_tab.get())));
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
        ctx.content.is_empty() || !ctx.has_char('\t')
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_tab() {
        let rule = MD010NoHardTabs::default();
        let content = "Line with\ttab";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "    Indented\nNormal    line\nNo tabs");
    }

    #[test]
    fn test_custom_spaces_per_tab() {
        let rule = MD010NoHardTabs::new(4);
        let content = "\tIndented";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "    Indented");
    }

    #[test]
    fn test_code_blocks_always_ignored() {
        let rule = MD010NoHardTabs::default();
        let content = "Normal\tline\n```\nCode\twith\ttab\n```\nAnother\tline";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should only flag tabs outside code blocks - code has its own formatting rules
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should never flag tabs in code blocks - code has its own formatting rules
        // (e.g., Makefiles require tabs, Go uses tabs by convention)
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_html_comments_ignored() {
        let rule = MD010NoHardTabs::default();
        let content = "Normal\tline\n<!-- HTML\twith\ttab -->\nAnother\tline";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should only flag the tab after the comment
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_empty_lines_with_tabs() {
        let rule = MD010NoHardTabs::default();
        let content = "Normal line\n\t\t\n\t\nAnother line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message, "Empty line contains 2 tabs");
        assert_eq!(result[1].message, "Empty line contains tab");
    }

    #[test]
    fn test_mixed_tabs_and_spaces() {
        let rule = MD010NoHardTabs::default();
        let content = " \tMixed indentation\n\t Mixed again";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_consecutive_tabs() {
        let rule = MD010NoHardTabs::default();
        let content = "Text\t\t\tthree tabs\tand\tanother";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should group consecutive tabs
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].message, "Found 3 tabs for alignment, use spaces instead");
    }

    #[test]
    fn test_find_and_group_tabs() {
        // Test finding and grouping tabs in one pass
        let groups = MD010NoHardTabs::find_and_group_tabs("a\tb\tc");
        assert_eq!(groups, vec![(1, 2), (3, 4)]);

        let groups = MD010NoHardTabs::find_and_group_tabs("\t\tabc");
        assert_eq!(groups, vec![(0, 2)]);

        let groups = MD010NoHardTabs::find_and_group_tabs("no tabs");
        assert!(groups.is_empty());

        // Test with consecutive and non-consecutive tabs
        let groups = MD010NoHardTabs::find_and_group_tabs("\t\t\ta\t\tb");
        assert_eq!(groups, vec![(0, 3), (4, 6)]);

        let groups = MD010NoHardTabs::find_and_group_tabs("\ta\tb\tc");
        assert_eq!(groups, vec![(0, 1), (2, 3), (4, 5)]);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "        Tab");

        // Code blocks are always ignored
        let content_with_code = "```\n\tTab in code\n```";
        let ctx = LintContext::new(content_with_code, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2000);
    }

    #[test]
    fn test_preserve_content() {
        let rule = MD010NoHardTabs::default();
        let content = "**Bold**\ttext\n*Italic*\ttext\n[Link](url)\ttab";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "**Bold**    text\n*Italic*    text\n[Link](url)    tab");
    }

    #[test]
    fn test_edge_cases() {
        let rule = MD010NoHardTabs::default();

        // Tab at end of line
        let content = "Text\t";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        // Only tabs
        let content = "\t\t\t";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Empty line contains 3 tabs");
    }

    #[test]
    fn test_code_blocks_always_preserved_in_fix() {
        let rule = MD010NoHardTabs::default();

        let content = "Text\twith\ttab\n```makefile\ntarget:\n\tcommand\n\tanother\n```\nMore\ttabs";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Tabs in code blocks are preserved - code has its own formatting rules
        // (e.g., Makefiles require tabs, Go uses tabs by convention)
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
