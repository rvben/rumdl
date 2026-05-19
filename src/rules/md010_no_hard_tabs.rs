use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
/// Rule MD010: No tabs
///
/// See [docs/md010.md](../../docs/md010.md) for full documentation, configuration, and examples.
use crate::utils::range_utils::calculate_match_range;

pub mod md010_config;
pub use md010_config::MD010Config;

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
                code_blocks: false,
            },
        }
    }

    pub const fn from_config_struct(config: MD010Config) -> Self {
        Self { config }
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

        // When `code_blocks` is false (the default), skip tabs inside ANY code block -
        // fenced and indented alike - using the shared spec-compliant flag.
        let skip_code_blocks = !self.config.code_blocks;

        for (line_num, &line) in lines.iter().enumerate() {
            if skip_code_blocks && ctx.line_info(line_num + 1).is_some_and(|info| info.in_code_block) {
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
                    fix: Some(Fix::new(
                        line_index.line_col_to_byte_range_with_length(line_num + 1, start_pos + 1, tab_count),
                        " ".repeat(tab_count * self.config.spaces_per_tab.get()),
                    )),
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
    fn test_leading_tabs_skipped_in_indented_code_by_default() {
        // Both lines start with a tab at column 0: parsed as an indented code block.
        // Default code_blocks=false skips tabs in indented code blocks.
        let content = "\tIndented line\n\t\tDouble indented";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::default();
        let result_off = rule_off.check(&ctx).unwrap();
        assert!(
            result_off.is_empty(),
            "indented code block skipped by default, got {result_off:?}"
        );
        assert_eq!(
            rule_off.fix(&ctx).unwrap(),
            "\tIndented line\n\t\tDouble indented",
            "fix must preserve indented code block content"
        );

        // code_blocks=true: tabs inside indented code blocks are flagged.
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
        });
        let result_on = rule_on.check(&ctx).unwrap();
        assert_eq!(result_on.len(), 2, "got {result_on:?}");
        assert_eq!(result_on[0].line, 1);
        assert_eq!(result_on[0].message, "Found leading tab, use 4 spaces instead");
        assert_eq!(result_on[1].line, 2);
        assert_eq!(result_on[1].message, "Found 2 leading tabs, use 8 spaces instead");
        assert_eq!(rule_on.fix(&ctx).unwrap(), "    Indented line\n        Double indented");
    }

    #[test]
    fn test_fix_tabs() {
        // Line 1 starts with a tab at column 0 -> indented code block, skipped by default.
        // Line 2 has a mid-line tab (alignment) -> flagged and fixed.
        let content = "\tIndented\nNormal\tline\nNo tabs";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::default();
        let warnings_off = rule_off.check(&ctx).unwrap();
        assert_eq!(warnings_off.len(), 1, "got {warnings_off:?}");
        assert_eq!(warnings_off[0].line, 2);
        assert_eq!(warnings_off[0].message, "Found tab for alignment, use spaces instead");
        assert_eq!(
            rule_off.fix(&ctx).unwrap(),
            "\tIndented\nNormal    line\nNo tabs",
            "indented code block line preserved; alignment tab fixed"
        );

        // code_blocks=true: line 1 is also flagged.
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
        });
        let warnings_on = rule_on.check(&ctx).unwrap();
        assert_eq!(warnings_on.len(), 2, "got {warnings_on:?}");
        assert_eq!(warnings_on[0].line, 1);
        assert_eq!(warnings_on[1].line, 2);
        assert_eq!(rule_on.fix(&ctx).unwrap(), "    Indented\nNormal    line\nNo tabs");
    }

    #[test]
    fn test_custom_spaces_per_tab() {
        // Single tab at column 0 -> indented code block, skipped by default.
        let content = "\tIndented";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::new(4);
        assert!(
            rule_off.check(&ctx).unwrap().is_empty(),
            "indented code block skipped by default"
        );
        assert_eq!(
            rule_off.fix(&ctx).unwrap(),
            "\tIndented",
            "indented code block preserved by default"
        );

        // code_blocks=true: tab is flagged and fixed.
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
        });
        assert_eq!(rule_on.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule_on.fix(&ctx).unwrap(), "    Indented");
    }

    #[test]
    fn test_fenced_code_block_tabs_skipped_by_default() {
        let rule = MD010NoHardTabs::default();
        let content = "Normal\tline\n```\nCode\twith\ttab\n```\nAnother\tline";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // By default (code_blocks=false) tabs inside code blocks are skipped
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 5);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Normal    line\n```\nCode\twith\ttab\n```\nAnother    line");
    }

    #[test]
    fn test_fenced_only_content_skipped_by_default() {
        let rule = MD010NoHardTabs::default();
        let content = "```\nCode\twith\ttab\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // By default (code_blocks=false) tabs in fenced code blocks are skipped
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
        // " \t..." (space then tab) and "\t ..." (tab then space): both parsed as
        // indented code blocks by the shared spec-compliant flag.
        // Default code_blocks=false skips them.
        let content = " \tMixed indentation\n\t Mixed again";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::default();
        let result_off = rule_off.check(&ctx).unwrap();
        assert!(
            result_off.is_empty(),
            "indented code block lines skipped, got {result_off:?}"
        );
        assert_eq!(
            rule_off.fix(&ctx).unwrap(),
            " \tMixed indentation\n\t Mixed again",
            "content preserved unchanged"
        );

        // code_blocks=true: both lines flagged.
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
        });
        let result_on = rule_on.check(&ctx).unwrap();
        assert_eq!(result_on.len(), 2, "got {result_on:?}");
        assert_eq!(rule_on.fix(&ctx).unwrap(), "     Mixed indentation\n     Mixed again");
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
        // "\tTab" at column 0 -> indented code block, skipped by default (code_blocks=false).
        let content_plain = "\tTab";
        let ctx_plain = LintContext::new(content_plain, crate::config::MarkdownFlavor::Standard, None);
        let rule_8_off = MD010NoHardTabs::new(8); // spaces_per_tab=8, code_blocks=false
        assert!(
            rule_8_off.check(&ctx_plain).unwrap().is_empty(),
            "indented code block skipped"
        );
        assert_eq!(
            rule_8_off.fix(&ctx_plain).unwrap(),
            "\tTab",
            "content preserved unchanged"
        );

        // code_blocks=true: the tab is flagged and replaced with 8 spaces.
        let rule_8_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(8),
            code_blocks: true,
        });
        assert_eq!(rule_8_on.check(&ctx_plain).unwrap().len(), 1);
        assert_eq!(rule_8_on.fix(&ctx_plain).unwrap(), "        Tab");

        // Fenced code block: tab skipped by default.
        let content_fenced = "```\n\tTab in code\n```";
        let ctx_fenced = LintContext::new(content_fenced, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            rule_8_off.check(&ctx_fenced).unwrap().is_empty(),
            "fenced code block skipped"
        );
        assert_eq!(rule_8_off.fix(&ctx_fenced).unwrap(), "```\n\tTab in code\n```");

        // code_blocks=true: tab inside fence is flagged.
        let result_on = rule_8_on.check(&ctx_fenced).unwrap();
        assert_eq!(result_on.len(), 1, "got {result_on:?}");
        assert_eq!(result_on[0].line, 2);
        assert_eq!(rule_8_on.fix(&ctx_fenced).unwrap(), "```\n        Tab in code\n```");
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
    fn test_fenced_code_block_tabs_preserved_in_fix_by_default() {
        let rule = MD010NoHardTabs::default();

        let content = "Text\twith\ttab\n```makefile\ntarget:\n\tcommand\n\tanother\n```\nMore\ttabs";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // By default (code_blocks=false) tabs in fenced code blocks are preserved
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
    fn test_indented_code_block_tabs_skipped_by_default() {
        // "    code\twith\ttab" is indented with 4 spaces -> indented code block.
        // Default code_blocks=false skips it; only the tab on the normal line is flagged.
        let content = "    code\twith\ttab\n\nNormal\ttext";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::default();
        let result_off = rule_off.check(&ctx).unwrap();
        assert_eq!(
            result_off.len(),
            1,
            "expected 1 warning (only normal-text tab), got {}: {:?}",
            result_off.len(),
            result_off
        );
        assert_eq!(result_off[0].line, 3);
        assert_eq!(result_off[0].message, "Found tab for alignment, use spaces instead");

        // code_blocks=true: all 3 tabs flagged (2 on line 1, 1 on line 3).
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
        });
        let result_on = rule_on.check(&ctx).unwrap();
        assert_eq!(
            result_on.len(),
            3,
            "expected 3 warnings with code_blocks=true, got {}: {:?}",
            result_on.len(),
            result_on
        );
        assert_eq!(result_on[0].line, 1);
        assert_eq!(result_on[1].line, 1);
        assert_eq!(result_on[2].line, 3);
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
        // Default code_blocks=false: indented code block tabs preserved, normal-text tab fixed.
        let content = "    code\twith\ttab\n\nNormal\ttext";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let rule_off = MD010NoHardTabs::default();
        assert_eq!(
            rule_off.fix(&ctx).unwrap(),
            "    code\twith\ttab\n\nNormal    text",
            "indented code block preserved; only normal-text tab fixed"
        );

        // code_blocks=true: all tabs replaced including those in the indented code block.
        let rule_on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
        });
        assert_eq!(
            rule_on.fix(&ctx).unwrap(),
            "    code    with    tab\n\nNormal    text",
            "all tabs replaced with code_blocks=true"
        );
    }

    #[test]
    fn test_issue_630_default_skips_both_code_blocks() {
        // Default code_blocks = false: tabs skipped in BOTH block types.
        let rule = MD010NoHardTabs::default();
        let content = "Foo bar\n\n    for range 100 {\n    \tfoo()\n    }\n\nThis is a fenced\n\n```\nfor range 100 {\n\tfoo()\n}\n```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "both code blocks skipped, got {result:?}");
    }

    #[test]
    fn test_issue_630_code_blocks_true_flags_both() {
        // code_blocks = true: tabs flagged in BOTH block types.
        let rule = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
        });
        let content = "Foo bar\n\n    for range 100 {\n    \tfoo()\n    }\n\nThis is a fenced\n\n```\nfor range 100 {\n\tfoo()\n}\n```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Line 4 "    \tfoo()": one alignment tab group inside the indented block.
        // Line 11 "\tfoo()": one leading tab group inside the fenced block.
        assert_eq!(result.len(), 2, "got {result:?}");
        assert_eq!(result[0].line, 4);
        assert_eq!(result[1].line, 11);
    }

    #[test]
    fn test_code_blocks_toggle_fenced() {
        let content = "Normal\tline\n```\nCode\twith\ttab\n```\nAnother\tline";

        // Default false: only the two tab groups outside the fence.
        let off = MD010NoHardTabs::default();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let r_off = off.check(&ctx).unwrap();
        assert_eq!(r_off.len(), 2, "got {r_off:?}");
        assert_eq!(r_off[0].line, 1);
        assert_eq!(r_off[1].line, 5);
        assert_eq!(
            off.fix(&ctx).unwrap(),
            "Normal    line\n```\nCode\twith\ttab\n```\nAnother    line"
        );

        // true: also the two groups on the fenced content line.
        let on = MD010NoHardTabs::from_config_struct(MD010Config {
            spaces_per_tab: crate::types::PositiveUsize::from_const(4),
            code_blocks: true,
        });
        let r_on = on.check(&ctx).unwrap();
        assert_eq!(r_on.len(), 4, "got {r_on:?}");
        assert_eq!(r_on[0].line, 1);
        assert_eq!(r_on[1].line, 3);
        assert_eq!(r_on[2].line, 3);
        assert_eq!(r_on[3].line, 5);
        assert_eq!(
            on.fix(&ctx).unwrap(),
            "Normal    line\n```\nCode    with    tab\n```\nAnother    line"
        );
    }

    #[test]
    fn test_code_blocks_toggle_makefile_fence_preserved_by_default() {
        let content = "Text\twith\ttab\n```makefile\ntarget:\n\tcommand\n```\nMore\ttabs";
        let off = MD010NoHardTabs::default();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        // Default preserves the Makefile recipe tab; only prose tabs fixed.
        assert_eq!(
            off.fix(&ctx).unwrap(),
            "Text    with    tab\n```makefile\ntarget:\n\tcommand\n```\nMore    tabs"
        );
    }
}
