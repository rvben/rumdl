//!
//! Rule MD030: Spaces after list markers
//!
//! See [docs/md030.md](../../docs/md030.md) for full documentation, configuration, and examples.

use crate::rule::{LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::rules::list_utils::ListType;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::calculate_match_range;
use toml;

mod md030_config;
use md030_config::MD030Config;

#[derive(Clone, Default)]
pub struct MD030ListMarkerSpace {
    config: MD030Config,
}

impl MD030ListMarkerSpace {
    pub fn new(ul_single: usize, ul_multi: usize, ol_single: usize, ol_multi: usize) -> Self {
        Self {
            config: MD030Config {
                ul_single,
                ul_multi,
                ol_single,
                ol_multi,
            },
        }
    }

    pub fn from_config_struct(config: MD030Config) -> Self {
        Self { config }
    }

    pub fn get_expected_spaces(&self, list_type: ListType, is_multi: bool) -> usize {
        match (list_type, is_multi) {
            (ListType::Unordered, false) => self.config.ul_single,
            (ListType::Unordered, true) => self.config.ul_multi,
            (ListType::Ordered, false) => self.config.ol_single,
            (ListType::Ordered, true) => self.config.ol_multi,
        }
    }
}

impl Rule for MD030ListMarkerSpace {
    fn name(&self) -> &'static str {
        "MD030"
    }

    fn description(&self) -> &'static str {
        "Spaces after list markers should be consistent"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<String> = ctx.content.lines().map(|l| l.to_string()).collect();
        let mut in_blockquote = false;
        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;

            // Skip if in code block
            if let Some(line_info) = ctx.line_info(line_num)
                && line_info.in_code_block
            {
                continue;
            }
            // Skip indented code blocks (4+ spaces or tab)
            if line.starts_with("    ") || line.starts_with("\t") {
                continue;
            }
            // Track blockquotes (for now, just skip lines starting with >)
            let mut l = line.as_str();
            while l.trim_start().starts_with('>') {
                l = l.trim_start().trim_start_matches('>').trim_start();
                in_blockquote = true;
            }
            if in_blockquote {
                in_blockquote = false;
                continue;
            }
            // Use pre-computed list item information
            if let Some(line_info) = ctx.line_info(line_num)
                && let Some(list_info) = &line_info.list_item
            {
                let list_type = if list_info.is_ordered {
                    ListType::Ordered
                } else {
                    ListType::Unordered
                };

                // Calculate actual spacing after marker
                let marker_end = list_info.marker_column + list_info.marker.len();
                let actual_spaces = list_info.content_column.saturating_sub(marker_end);

                let expected_spaces = self.get_expected_spaces(list_type, false);

                // Check for tabs in the spacing
                let line_content = &line[list_info.marker_column..];
                let spacing_content = if line_content.len() > list_info.marker.len() {
                    let after_marker_start = list_info.marker.len();
                    let after_marker_end = after_marker_start + actual_spaces;
                    &line_content[after_marker_start..after_marker_end.min(line_content.len())]
                } else {
                    ""
                };
                let has_tabs = spacing_content.contains('\t');

                // Check if spacing is incorrect or contains tabs
                if actual_spaces != expected_spaces || has_tabs {
                    // Calculate precise character range for the problematic spacing
                    let whitespace_start_pos = marker_end;
                    let whitespace_len = actual_spaces;

                    // Calculate the range that needs to be replaced (the entire whitespace after marker)
                    let (start_line, start_col, end_line, end_col) =
                        calculate_match_range(line_num, line, whitespace_start_pos, whitespace_len);

                    // Generate the correct replacement text (just the correct spacing)
                    let correct_spaces = " ".repeat(expected_spaces);

                    // Calculate byte positions for the fix range
                    let line_start_byte = ctx.line_offsets.get(line_num - 1).copied().unwrap_or(0);
                    let whitespace_start_byte = line_start_byte + whitespace_start_pos;
                    let whitespace_end_byte = whitespace_start_byte + whitespace_len;

                    let fix = Some(crate::rule::Fix {
                        range: whitespace_start_byte..whitespace_end_byte,
                        replacement: correct_spaces,
                    });

                    // Generate appropriate message
                    let message =
                        format!("Spaces after list markers (Expected: {expected_spaces}; Actual: {actual_spaces})");

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        severity: Severity::Warning,
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message,
                        fix,
                    });
                }
            }
        }
        Ok(warnings)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty()
            || (!ctx.content.contains('*')
                && !ctx.content.contains('-')
                && !ctx.content.contains('+')
                && !ctx.content.contains(|c: char| c.is_ascii_digit()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD030Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD030Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule> {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD030Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, crate::rule::LintError> {
        let content = ctx.content;
        let structure = crate::utils::document_structure::DocumentStructure::new(content);
        let lines: Vec<&str> = content.lines().collect();
        let mut result_lines = Vec::new();

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1;

            // Skip if in code block
            if let Some(line_info) = ctx.line_info(line_num)
                && line_info.in_code_block
            {
                result_lines.push(line.to_string());
                continue;
            }

            // Skip if in front matter
            if structure.is_in_front_matter(line_num) {
                result_lines.push(line.to_string());
                continue;
            }

            // Skip if this is an indented code block (4+ spaces with blank line before)
            if self.is_indented_code_block(line, line_idx, &lines) {
                result_lines.push(line.to_string());
                continue;
            }

            // Skip blockquotes for now (conservative approach)
            if line.trim_start().starts_with('>') {
                result_lines.push(line.to_string());
                continue;
            }

            // Try to fix list marker spacing
            if let Some(fixed_line) = self.try_fix_list_marker_spacing(line) {
                result_lines.push(fixed_line);
            } else {
                result_lines.push(line.to_string());
            }
        }

        // Preserve trailing newline if original content had one
        let result = result_lines.join("\n");
        if content.ends_with('\n') && !result.ends_with('\n') {
            Ok(result + "\n")
        } else {
            Ok(result)
        }
    }
}

impl MD030ListMarkerSpace {
    /// Fix list marker spacing - handles tabs, multiple spaces, and mixed whitespace
    fn try_fix_list_marker_spacing(&self, line: &str) -> Option<String> {
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];

        // Check for unordered list markers
        for marker in &["*", "-", "+"] {
            if let Some(after_marker) = trimmed.strip_prefix(marker) {
                // Fix if there are tabs, multiple spaces, or mixed whitespace
                if after_marker.starts_with('\t')
                    || after_marker.starts_with("  ")
                    || (after_marker.starts_with(' ') && after_marker.chars().nth(1) == Some('\t'))
                {
                    let content = after_marker.trim_start();
                    if !content.is_empty() {
                        // Use the configured number of spaces for unordered lists
                        let spaces = " ".repeat(self.config.ul_single);
                        return Some(format!("{indent}{marker}{spaces}{content}"));
                    }
                }
                break; // Found a marker, don't check others
            }
        }

        // Check for ordered list markers
        if let Some(dot_pos) = trimmed.find('.') {
            let before_dot = &trimmed[..dot_pos];
            if before_dot.chars().all(|c| c.is_ascii_digit()) && !before_dot.is_empty() {
                let after_dot = &trimmed[dot_pos + 1..];
                // Fix if there are tabs, multiple spaces, or mixed whitespace
                if after_dot.starts_with('\t')
                    || after_dot.starts_with("  ")
                    || (after_dot.starts_with(' ') && after_dot.chars().nth(1) == Some('\t'))
                {
                    let content = after_dot.trim_start();
                    if !content.is_empty() {
                        // Use the configured number of spaces for ordered lists
                        let spaces = " ".repeat(self.config.ol_single);
                        return Some(format!("{indent}{before_dot}.{spaces}{content}"));
                    }
                }
            }
        }

        None
    }

    /// Check if a line is part of an indented code block (4+ spaces with blank line before)
    fn is_indented_code_block(&self, line: &str, line_idx: usize, lines: &[&str]) -> bool {
        // Must start with 4+ spaces or tab
        if !line.starts_with("    ") && !line.starts_with('\t') {
            return false;
        }

        // If it's the first line, it's not an indented code block
        if line_idx == 0 {
            return false;
        }

        // Check if there's a blank line before this line or before the start of the indented block
        if self.has_blank_line_before_indented_block(line_idx, lines) {
            return true;
        }

        false
    }

    /// Check if there's a blank line before the start of an indented block
    fn has_blank_line_before_indented_block(&self, line_idx: usize, lines: &[&str]) -> bool {
        // Walk backwards to find the start of the indented block
        let mut current_idx = line_idx;

        // Find the first line in this indented block
        while current_idx > 0 {
            let current_line = lines[current_idx];
            let prev_line = lines[current_idx - 1];

            // If current line is not indented, we've gone too far
            if !current_line.starts_with("    ") && !current_line.starts_with('\t') {
                break;
            }

            // If previous line is not indented, check if it's blank
            if !prev_line.starts_with("    ") && !prev_line.starts_with('\t') {
                return prev_line.trim().is_empty();
            }

            current_idx -= 1;
        }

        false
    }
}

impl DocumentStructureExtensions for MD030ListMarkerSpace {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_with_document_structure() {
        let rule = MD030ListMarkerSpace::default();
        let content = "* Item 1\n* Item 2\n  * Nested item\n1. Ordered item";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Correctly spaced list markers should not generate warnings"
        );
        let content = "*  Item 1 (too many spaces)\n* Item 2\n1.   Ordered item (too many spaces)";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        // Expect warnings for lines with too many spaces after the marker
        assert_eq!(
            result.len(),
            2,
            "Should flag lines with too many spaces after list marker"
        );
        for warning in result {
            assert!(
                warning.message.starts_with("Spaces after list markers (Expected:")
                    && warning.message.contains("Actual:"),
                "Warning message should include expected and actual values, got: '{}'",
                warning.message
            );
        }
    }
}
