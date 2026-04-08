use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
/// Rule MD010: No tabs
///
/// See [docs/md010.md](../../docs/md010.md) for full documentation, configuration, and examples.
use crate::utils::range_utils::calculate_match_range;

mod md010_config;
use md010_config::MD010Config;

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

    /// Detect which lines are inside fenced code blocks (``` or ~~~).
    /// Only fenced code blocks are skipped — indented code blocks (4+ spaces / tab)
    /// are NOT skipped because the tabs themselves are what MD010 should flag.
    fn find_fenced_code_block_lines(lines: &[&str]) -> Vec<bool> {
        let mut in_fenced_block = false;
        let mut fence_char: Option<char> = None;
        let mut fence_len: usize = 0;
        let mut result = vec![false; lines.len()];

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            if !in_fenced_block {
                // Check for opening fence (3+ backticks or tildes)
                let first_char = trimmed.chars().next();
                if matches!(first_char, Some('`') | Some('~')) {
                    let fc = first_char.unwrap();
                    let count = trimmed.chars().take_while(|&c| c == fc).count();
                    if count >= 3 {
                        in_fenced_block = true;
                        fence_char = Some(fc);
                        fence_len = count;
                        result[i] = true;
                    }
                }
            } else {
                result[i] = true;
                // Check for closing fence (must match opening fence char and be >= opening length)
                if let Some(fc) = fence_char {
                    let first = trimmed.chars().next();
                    if first == Some(fc) {
                        let count = trimmed.chars().take_while(|&c| c == fc).count();
                        // Closing fence must be at least as long as opening, with nothing else on the line
                        if count >= fence_len && trimmed[count..].trim().is_empty() {
                            in_fenced_block = false;
                            fence_char = None;
                            fence_len = 0;
                        }
                    }
                }
            }
        }

        result
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
}

impl Rule for MD010NoHardTabs {
    fn name(&self) -> &'static str {
        "MD010"
    }

    fn description(&self) -> &'static str {
        "No tabs"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let line_index = &ctx.line_index;

        let mut warnings = Vec::new();
        let lines = ctx.raw_lines();

        // Track fenced code blocks separately — we skip FENCED blocks but NOT
        // indented code blocks (since tab indentation IS what MD010 should flag)
        let fenced_lines = Self::find_fenced_code_block_lines(lines);

        for (line_num, &line) in lines.iter().enumerate() {
            // Skip fenced code blocks (code has its own formatting rules)
            if fenced_lines[line_num] {
                continue;
            }

            // Skip HTML comments, HTML blocks, PyMdown blocks, mkdocstrings, ESM blocks
            if ctx.line_info(line_num + 1).is_some_and(|info| {
                info.in_html_comment
                    || info.in_mdx_comment
                    || info.in_html_block
                    || info.in_pymdown_block
                    || info.in_mkdocstrings
                    || info.in_esm_block
            }) {
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
                        range: line_index.line_col_to_byte_range_with_length(line_num + 1, start_pos + 1, tab_count),
                        replacement: " ".repeat(tab_count * self.config.spaces_per_tab.get()),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
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
    fn test_tilde_fence_longer_than_3() {
        let rule = MD010NoHardTabs::default();
        // 5-tilde fenced code block should be recognized and tabs inside should be skipped
        let content = "~~~~~\ncode\twith\ttab\n~~~~~\ntext\twith\ttab";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Only tabs on line 4 (outside the code block) should be flagged
        assert_eq!(
            result.len(),
            2,
            "Expected 2 warnings but got {}: {:?}",
            result.len(),
            result
        );
        assert_eq!(result[0].line, 4);
        assert_eq!(result[1].line, 4);
    }

    #[test]
    fn test_backtick_fence_longer_than_3() {
        let rule = MD010NoHardTabs::default();
        // 5-backtick fenced code block
        let content = "`````\ncode\twith\ttab\n`````\ntext\twith\ttab";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "Expected 2 warnings but got {}: {:?}",
            result.len(),
            result
        );
        assert_eq!(result[0].line, 4);
        assert_eq!(result[1].line, 4);
    }

    #[test]
    fn test_indented_code_block_tabs_flagged() {
        let rule = MD010NoHardTabs::default();
        // Tabs in indented code blocks are flagged because the tab IS the problem
        // (unlike fenced code blocks where tabs are part of the code formatting)
        let content = "    code\twith\ttab\n\nNormal\ttext";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            3,
            "Expected 3 warnings but got {}: {:?}",
            result.len(),
            result
        );
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 1);
        assert_eq!(result[2].line, 3);
    }

    #[test]
    fn test_html_comment_end_then_start_same_line() {
        let rule = MD010NoHardTabs::default();
        // Tabs inside consecutive HTML comments should not be flagged
        let content =
            "<!-- first comment\nend --> text <!-- second comment\n\ttabbed content inside second comment\n-->";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected 0 warnings but got {}: {:?}",
            result.len(),
            result
        );
    }

    #[test]
    fn test_fix_tilde_fence_longer_than_3() {
        let rule = MD010NoHardTabs::default();
        let content = "~~~~~\ncode\twith\ttab\n~~~~~\ntext\twith\ttab";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Tabs inside code block preserved, tabs outside replaced
        assert_eq!(fixed, "~~~~~\ncode\twith\ttab\n~~~~~\ntext    with    tab");
    }

    #[test]
    fn test_fix_indented_code_block_tabs_replaced() {
        let rule = MD010NoHardTabs::default();
        let content = "    code\twith\ttab\n\nNormal\ttext";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // All tabs replaced, including those in indented code blocks
        assert_eq!(fixed, "    code    with    tab\n\nNormal    text");
    }
}
