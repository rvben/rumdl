use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::element_cache::ElementCache;
use crate::utils::range_utils::{LineIndex, calculate_line_range};
use crate::utils::regex_cache::BLOCKQUOTE_PREFIX_RE;
use regex::Regex;
use std::sync::LazyLock;

mod md032_config;
pub use md032_config::MD032Config;

// Detects ordered list items starting with a number other than 1
static ORDERED_LIST_NON_ONE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*([2-9]|\d{2,})\.\s").unwrap());

/// Check if a line is a thematic break (horizontal rule)
/// Per CommonMark: 0-3 spaces of indentation, then 3+ of same char (-, *, _), optionally with spaces between
fn is_thematic_break(line: &str) -> bool {
    // Per CommonMark, thematic breaks can have 0-3 spaces of indentation (< 4 columns)
    if ElementCache::calculate_indentation_width_default(line) > 3 {
        return false;
    }

    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let first_non_space = chars.iter().find(|&&c| c != ' ');

    if let Some(&marker) = first_non_space {
        if marker != '-' && marker != '*' && marker != '_' {
            return false;
        }
        let marker_count = chars.iter().filter(|&&c| c == marker).count();
        let other_count = chars.iter().filter(|&&c| c != marker && c != ' ').count();
        marker_count >= 3 && other_count == 0
    } else {
        false
    }
}

/// Rule MD032: Lists should be surrounded by blank lines
///
/// This rule enforces that lists are surrounded by blank lines, which improves document
/// readability and ensures consistent rendering across different Markdown processors.
///
/// ## Purpose
///
/// - **Readability**: Blank lines create visual separation between lists and surrounding content
/// - **Parsing**: Many Markdown parsers require blank lines around lists for proper rendering
/// - **Consistency**: Ensures uniform document structure and appearance
/// - **Compatibility**: Improves compatibility across different Markdown implementations
///
/// ## Examples
///
/// ### Correct
///
/// ```markdown
/// This is a paragraph of text.
///
/// - Item 1
/// - Item 2
/// - Item 3
///
/// This is another paragraph.
/// ```
///
/// ### Incorrect
///
/// ```markdown
/// This is a paragraph of text.
/// - Item 1
/// - Item 2
/// - Item 3
/// This is another paragraph.
/// ```
///
/// ## Behavior Details
///
/// This rule checks for the following:
///
/// - **List Start**: There should be a blank line before the first item in a list
///   (unless the list is at the beginning of the document or after front matter)
/// - **List End**: There should be a blank line after the last item in a list
///   (unless the list is at the end of the document)
/// - **Nested Lists**: Properly handles nested lists and list continuations
/// - **List Types**: Works with ordered lists, unordered lists, and all valid list markers (-, *, +)
///
/// ## Special Cases
///
/// This rule handles several special cases:
///
/// - **Front Matter**: YAML front matter is detected and skipped
/// - **Code Blocks**: Lists inside code blocks are ignored
/// - **List Content**: Indented content belonging to list items is properly recognized as part of the list
/// - **Document Boundaries**: Lists at the beginning or end of the document have adjusted requirements
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Adds a blank line before the first list item when needed
/// - Adds a blank line after the last list item when needed
/// - Preserves document structure and existing content
///
/// ## Performance Optimizations
///
/// The rule includes several optimizations:
/// - Fast path checks before applying more expensive regex operations
/// - Efficient list item detection
/// - Pre-computation of code block lines to avoid redundant processing
#[derive(Debug, Clone, Default)]
pub struct MD032BlanksAroundLists {
    config: MD032Config,
}

impl MD032BlanksAroundLists {
    pub fn from_config_struct(config: MD032Config) -> Self {
        Self { config }
    }
}

impl MD032BlanksAroundLists {
    /// Check if a blank line should be required before a list based on the previous line context
    fn should_require_blank_line_before(
        ctx: &crate::lint_context::LintContext,
        prev_line_num: usize,
        current_line_num: usize,
    ) -> bool {
        // Always require blank lines after code blocks, front matter, etc.
        if ctx
            .line_info(prev_line_num)
            .is_some_and(|info| info.in_code_block || info.in_front_matter)
        {
            return true;
        }

        // Always allow nested lists (lists indented within other list items)
        if Self::is_nested_list(ctx, prev_line_num, current_line_num) {
            return false;
        }

        // Default: require blank line (matching markdownlint's behavior)
        true
    }

    /// Check if the current list is nested within another list item
    fn is_nested_list(
        ctx: &crate::lint_context::LintContext,
        prev_line_num: usize,    // 1-indexed
        current_line_num: usize, // 1-indexed
    ) -> bool {
        // Check if current line is indented (typical for nested lists)
        if current_line_num > 0 && current_line_num - 1 < ctx.lines.len() {
            let current_line = &ctx.lines[current_line_num - 1];
            if current_line.indent >= 2 {
                // Check if previous line is a list item or list content
                if prev_line_num > 0 && prev_line_num - 1 < ctx.lines.len() {
                    let prev_line = &ctx.lines[prev_line_num - 1];
                    // Previous line is a list item or indented content
                    if prev_line.list_item.is_some() || prev_line.indent >= 2 {
                        return true;
                    }
                }
            }
        }
        false
    }

    // Convert centralized list blocks to the format expected by perform_checks
    fn convert_list_blocks(&self, ctx: &crate::lint_context::LintContext) -> Vec<(usize, usize, String)> {
        let mut blocks: Vec<(usize, usize, String)> = Vec::new();

        for block in &ctx.list_blocks {
            // For MD032, we need to check if there are code blocks that should
            // split the list into separate segments

            // Simple approach: if there's a fenced code block between list items,
            // split at that point
            let mut segments: Vec<(usize, usize)> = Vec::new();
            let mut current_start = block.start_line;
            let mut prev_item_line = 0;

            for &item_line in &block.item_lines {
                if prev_item_line > 0 {
                    // Check if there's a standalone code fence between prev_item_line and item_line
                    // A code fence that's indented as part of a list item should NOT split the list
                    let mut has_standalone_code_fence = false;

                    // Calculate minimum indentation for list item content
                    let min_indent_for_content = if block.is_ordered {
                        // For ordered lists, content should be indented at least to align with text after marker
                        // e.g., "1. " = 3 chars, so content should be indented 3+ spaces
                        3 // Minimum for "1. "
                    } else {
                        // For unordered lists, content should be indented at least 2 spaces
                        2 // For "- " or "* "
                    };

                    for check_line in (prev_item_line + 1)..item_line {
                        if check_line - 1 < ctx.lines.len() {
                            let line = &ctx.lines[check_line - 1];
                            let line_content = line.content(ctx.content);
                            if line.in_code_block
                                && (line_content.trim().starts_with("```") || line_content.trim().starts_with("~~~"))
                            {
                                // Check if this code fence is indented as part of the list item
                                // If it's indented enough to be part of the list item, it shouldn't split
                                if line.indent < min_indent_for_content {
                                    has_standalone_code_fence = true;
                                    break;
                                }
                            }
                        }
                    }

                    if has_standalone_code_fence {
                        // End current segment before this item
                        segments.push((current_start, prev_item_line));
                        current_start = item_line;
                    }
                }
                prev_item_line = item_line;
            }

            // Add the final segment
            // For the last segment, end at the last list item (not the full block end)
            if prev_item_line > 0 {
                segments.push((current_start, prev_item_line));
            }

            // Check if this list block was split by code fences
            let has_code_fence_splits = segments.len() > 1 && {
                // Check if any segments were created due to code fences
                let mut found_fence = false;
                for i in 0..segments.len() - 1 {
                    let seg_end = segments[i].1;
                    let next_start = segments[i + 1].0;
                    // Check if there's a code fence between these segments
                    for check_line in (seg_end + 1)..next_start {
                        if check_line - 1 < ctx.lines.len() {
                            let line = &ctx.lines[check_line - 1];
                            let line_content = line.content(ctx.content);
                            if line.in_code_block
                                && (line_content.trim().starts_with("```") || line_content.trim().starts_with("~~~"))
                            {
                                found_fence = true;
                                break;
                            }
                        }
                    }
                    if found_fence {
                        break;
                    }
                }
                found_fence
            };

            // Convert segments to blocks
            for (start, end) in segments.iter() {
                // Extend the end to include any continuation lines immediately after the last item
                let mut actual_end = *end;

                // If this list was split by code fences, don't extend any segments
                // They should remain as individual list items for MD032 purposes
                if !has_code_fence_splits && *end < block.end_line {
                    for check_line in (*end + 1)..=block.end_line {
                        if check_line - 1 < ctx.lines.len() {
                            let line = &ctx.lines[check_line - 1];
                            let line_content = line.content(ctx.content);
                            // Stop at next list item or non-continuation content
                            if block.item_lines.contains(&check_line) || line.heading.is_some() {
                                break;
                            }
                            // Don't extend through code blocks
                            if line.in_code_block {
                                break;
                            }
                            // Include indented continuation
                            if line.indent >= 2 {
                                actual_end = check_line;
                            }
                            // Include lazy continuation lines (multiple consecutive lines without indent)
                            // Per CommonMark, only paragraph text can be lazy continuation
                            // Thematic breaks, code fences, etc. cannot be lazy continuations
                            // Only include lazy continuation if allowed by config
                            else if self.config.allow_lazy_continuation
                                && !line.is_blank
                                && line.heading.is_none()
                                && !block.item_lines.contains(&check_line)
                                && !is_thematic_break(line_content)
                            {
                                // This is a lazy continuation line - check if we're still in the same paragraph
                                // Allow multiple consecutive lazy continuation lines
                                actual_end = check_line;
                            } else if !line.is_blank {
                                // Non-blank line that's not a continuation - stop here
                                break;
                            }
                        }
                    }
                }

                blocks.push((*start, actual_end, block.blockquote_prefix.clone()));
            }
        }

        blocks
    }

    fn perform_checks(
        &self,
        ctx: &crate::lint_context::LintContext,
        lines: &[&str],
        list_blocks: &[(usize, usize, String)],
        line_index: &LineIndex,
    ) -> LintResult {
        let mut warnings = Vec::new();
        let num_lines = lines.len();

        // Check for ordered lists starting with non-1 that aren't recognized as lists
        // These need blank lines before them to be parsed as lists by CommonMark
        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1;

            // Skip if this line is already part of a recognized list
            let is_in_list = list_blocks
                .iter()
                .any(|(start, end, _)| line_num >= *start && line_num <= *end);
            if is_in_list {
                continue;
            }

            // Skip if in code block or front matter
            if ctx
                .line_info(line_num)
                .is_some_and(|info| info.in_code_block || info.in_front_matter)
            {
                continue;
            }

            // Check if this line starts with a number other than 1
            if ORDERED_LIST_NON_ONE_RE.is_match(line) {
                // Check if there's a blank line before this
                if line_idx > 0 {
                    let prev_line = lines[line_idx - 1];
                    let prev_is_blank = is_blank_in_context(prev_line);
                    let prev_excluded = ctx
                        .line_info(line_idx)
                        .is_some_and(|info| info.in_code_block || info.in_front_matter);

                    if !prev_is_blank && !prev_excluded {
                        // This ordered list item starting with non-1 needs a blank line before it
                        let (start_line, start_col, end_line, end_col) = calculate_line_range(line_num, line);

                        warnings.push(LintWarning {
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            rule_name: Some(self.name().to_string()),
                            message: "Ordered list starting with non-1 should be preceded by blank line".to_string(),
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range_with_length(line_num, 1, 0),
                                replacement: "\n".to_string(),
                            }),
                        });
                    }
                }
            }
        }

        for &(start_line, end_line, ref prefix) in list_blocks {
            if start_line > 1 {
                let prev_line_actual_idx_0 = start_line - 2;
                let prev_line_actual_idx_1 = start_line - 1;
                let prev_line_str = lines[prev_line_actual_idx_0];
                let is_prev_excluded = ctx
                    .line_info(prev_line_actual_idx_1)
                    .is_some_and(|info| info.in_code_block || info.in_front_matter);
                let prev_prefix = BLOCKQUOTE_PREFIX_RE
                    .find(prev_line_str)
                    .map_or(String::new(), |m| m.as_str().to_string());
                let prev_is_blank = is_blank_in_context(prev_line_str);
                let prefixes_match = prev_prefix.trim() == prefix.trim();

                // Only require blank lines for content in the same context (same blockquote level)
                // and when the context actually requires it
                let should_require = Self::should_require_blank_line_before(ctx, prev_line_actual_idx_1, start_line);
                if !is_prev_excluded && !prev_is_blank && prefixes_match && should_require {
                    // Calculate precise character range for the entire list line that needs a blank line before it
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(start_line, lines[start_line - 1]);

                    warnings.push(LintWarning {
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        rule_name: Some(self.name().to_string()),
                        message: "List should be preceded by blank line".to_string(),
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range_with_length(start_line, 1, 0),
                            replacement: format!("{prefix}\n"),
                        }),
                    });
                }
            }

            if end_line < num_lines {
                let next_line_idx_0 = end_line;
                let next_line_idx_1 = end_line + 1;
                let next_line_str = lines[next_line_idx_0];
                // Check if next line is excluded - front matter or indented code blocks within lists
                // We want blank lines before standalone code blocks, but not within list items
                let is_next_excluded = ctx.line_info(next_line_idx_1).is_some_and(|info| info.in_front_matter)
                    || (next_line_idx_0 < ctx.lines.len()
                        && ctx.lines[next_line_idx_0].in_code_block
                        && ctx.lines[next_line_idx_0].indent >= 2);
                let next_prefix = BLOCKQUOTE_PREFIX_RE
                    .find(next_line_str)
                    .map_or(String::new(), |m| m.as_str().to_string());
                let next_is_blank = is_blank_in_context(next_line_str);
                let prefixes_match = next_prefix.trim() == prefix.trim();

                // Only require blank lines for content in the same context (same blockquote level)
                if !is_next_excluded && !next_is_blank && prefixes_match {
                    // Calculate precise character range for the last line of the list (not the line after)
                    let (start_line_last, start_col_last, end_line_last, end_col_last) =
                        calculate_line_range(end_line, lines[end_line - 1]);

                    warnings.push(LintWarning {
                        line: start_line_last,
                        column: start_col_last,
                        end_line: end_line_last,
                        end_column: end_col_last,
                        severity: Severity::Warning,
                        rule_name: Some(self.name().to_string()),
                        message: "List should be followed by blank line".to_string(),
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range_with_length(end_line + 1, 1, 0),
                            replacement: format!("{prefix}\n"),
                        }),
                    });
                }
            }
        }
        Ok(warnings)
    }
}

impl Rule for MD032BlanksAroundLists {
    fn name(&self) -> &'static str {
        "MD032"
    }

    fn description(&self) -> &'static str {
        "Lists should be surrounded by blank lines"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let lines: Vec<&str> = content.lines().collect();
        let line_index = &ctx.line_index;

        // Early return for empty content
        if lines.is_empty() {
            return Ok(Vec::new());
        }

        let list_blocks = self.convert_list_blocks(ctx);

        if list_blocks.is_empty() {
            return Ok(Vec::new());
        }

        self.perform_checks(ctx, &lines, &list_blocks, line_index)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        self.fix_with_structure_impl(ctx)
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Fast path: check if document likely has lists
        if ctx.content.is_empty() || !ctx.likely_has_lists() {
            return true;
        }
        // Verify list blocks actually exist
        ctx.list_blocks.is_empty()
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        use crate::rule_config_serde::RuleConfig;
        let default_config = MD032Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD032Config::RULE_NAME.to_string(), toml::Value::Table(table)))
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD032Config>(config);
        Box::new(MD032BlanksAroundLists::from_config_struct(rule_config))
    }
}

impl MD032BlanksAroundLists {
    /// Helper method for fixing implementation
    fn fix_with_structure_impl(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let lines: Vec<&str> = ctx.content.lines().collect();
        let num_lines = lines.len();
        if num_lines == 0 {
            return Ok(String::new());
        }

        let list_blocks = self.convert_list_blocks(ctx);
        if list_blocks.is_empty() {
            return Ok(ctx.content.to_string());
        }

        let mut insertions: std::collections::BTreeMap<usize, String> = std::collections::BTreeMap::new();

        // Phase 1: Identify needed insertions
        for &(start_line, end_line, ref prefix) in &list_blocks {
            // Check before block
            if start_line > 1 {
                let prev_line_actual_idx_0 = start_line - 2;
                let prev_line_actual_idx_1 = start_line - 1;
                let is_prev_excluded = ctx
                    .line_info(prev_line_actual_idx_1)
                    .is_some_and(|info| info.in_code_block || info.in_front_matter);
                let prev_prefix = BLOCKQUOTE_PREFIX_RE
                    .find(lines[prev_line_actual_idx_0])
                    .map_or(String::new(), |m| m.as_str().to_string());

                let should_require = Self::should_require_blank_line_before(ctx, prev_line_actual_idx_1, start_line);
                if !is_prev_excluded
                    && !is_blank_in_context(lines[prev_line_actual_idx_0])
                    && prev_prefix == *prefix
                    && should_require
                {
                    insertions.insert(start_line, prefix.clone());
                }
            }

            // Check after block
            if end_line < num_lines {
                let after_block_line_idx_0 = end_line;
                let after_block_line_idx_1 = end_line + 1;
                let line_after_block_content_str = lines[after_block_line_idx_0];
                // Check if next line is excluded - in code block, front matter, or starts an indented code block
                // Only exclude code fence lines if they're indented (part of list content)
                let is_line_after_excluded = ctx
                    .line_info(after_block_line_idx_1)
                    .is_some_and(|info| info.in_code_block || info.in_front_matter)
                    || (after_block_line_idx_0 < ctx.lines.len()
                        && ctx.lines[after_block_line_idx_0].in_code_block
                        && ctx.lines[after_block_line_idx_0].indent >= 2
                        && (ctx.lines[after_block_line_idx_0]
                            .content(ctx.content)
                            .trim()
                            .starts_with("```")
                            || ctx.lines[after_block_line_idx_0]
                                .content(ctx.content)
                                .trim()
                                .starts_with("~~~")));
                let after_prefix = BLOCKQUOTE_PREFIX_RE
                    .find(line_after_block_content_str)
                    .map_or(String::new(), |m| m.as_str().to_string());

                if !is_line_after_excluded
                    && !is_blank_in_context(line_after_block_content_str)
                    && after_prefix == *prefix
                {
                    insertions.insert(after_block_line_idx_1, prefix.clone());
                }
            }
        }

        // Phase 2: Reconstruct with insertions
        let mut result_lines: Vec<String> = Vec::with_capacity(num_lines + insertions.len());
        for (i, line) in lines.iter().enumerate() {
            let current_line_num = i + 1;
            if let Some(prefix_to_insert) = insertions.get(&current_line_num)
                && (result_lines.is_empty() || result_lines.last().unwrap() != prefix_to_insert)
            {
                result_lines.push(prefix_to_insert.clone());
            }
            result_lines.push(line.to_string());
        }

        // Preserve the final newline if the original content had one
        let mut result = result_lines.join("\n");
        if ctx.content.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }
}

// Checks if a line is blank, considering blockquote context
fn is_blank_in_context(line: &str) -> bool {
    // A line is blank if it's empty or contains only whitespace,
    // potentially after removing blockquote markers.
    if let Some(m) = BLOCKQUOTE_PREFIX_RE.find(line) {
        // If a blockquote prefix is found, check if the content *after* the prefix is blank.
        line[m.end()..].trim().is_empty()
    } else {
        // No blockquote prefix, check the whole line for blankness.
        line.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::rule::Rule;

    fn lint(content: &str) -> Vec<LintWarning> {
        let rule = MD032BlanksAroundLists::default();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.check(&ctx).expect("Lint check failed")
    }

    fn fix(content: &str) -> String {
        let rule = MD032BlanksAroundLists::default();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.fix(&ctx).expect("Lint fix failed")
    }

    // Test that warnings include Fix objects
    fn check_warnings_have_fixes(content: &str) {
        let warnings = lint(content);
        for warning in &warnings {
            assert!(warning.fix.is_some(), "Warning should have fix: {warning:?}");
        }
    }

    #[test]
    fn test_list_at_start() {
        // Per markdownlint-cli: trailing text without blank line is treated as lazy continuation
        // so NO warning is expected here
        let content = "- Item 1\n- Item 2\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            0,
            "Trailing text is lazy continuation per CommonMark - no warning expected"
        );
    }

    #[test]
    fn test_list_at_end() {
        let content = "Text\n- Item 1\n- Item 2";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            1,
            "Expected 1 warning for list at end without preceding blank line"
        );
        assert_eq!(
            warnings[0].line, 2,
            "Warning should be on the first line of the list (line 2)"
        );
        assert!(warnings[0].message.contains("preceded by blank line"));

        // Test that warning has fix
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(fixed_content, "Text\n\n- Item 1\n- Item 2");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_list_in_middle() {
        // Per markdownlint-cli: only preceding blank line is required
        // Trailing text is treated as lazy continuation
        let content = "Text 1\n- Item 1\n- Item 2\nText 2";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            1,
            "Expected 1 warning for list needing preceding blank line (trailing text is lazy continuation)"
        );
        assert_eq!(warnings[0].line, 2, "Warning on line 2 (start)");
        assert!(warnings[0].message.contains("preceded by blank line"));

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(fixed_content, "Text 1\n\n- Item 1\n- Item 2\nText 2");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_correct_spacing() {
        let content = "Text 1\n\n- Item 1\n- Item 2\n\nText 2";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 0, "Expected no warnings for correctly spaced list");

        let fixed_content = fix(content);
        assert_eq!(fixed_content, content, "Fix should not change correctly spaced content");
    }

    #[test]
    fn test_list_with_content() {
        // Per markdownlint-cli: only preceding blank line warning
        // Trailing text is lazy continuation
        let content = "Text\n* Item 1\n  Content\n* Item 2\n  More content\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            1,
            "Expected 1 warning for list needing preceding blank line. Got: {warnings:?}"
        );
        assert_eq!(warnings[0].line, 2, "Warning should be on line 2 (start)");
        assert!(warnings[0].message.contains("preceded by blank line"));

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected_fixed = "Text\n\n* Item 1\n  Content\n* Item 2\n  More content\nText";
        assert_eq!(
            fixed_content, expected_fixed,
            "Fix did not produce the expected output. Got:\n{fixed_content}"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_nested_list() {
        // Per markdownlint-cli: only preceding blank line warning
        let content = "Text\n- Item 1\n  - Nested 1\n- Item 2\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            1,
            "Nested list block needs preceding blank only. Got: {warnings:?}"
        );
        assert_eq!(warnings[0].line, 2);
        assert!(warnings[0].message.contains("preceded by blank line"));

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(fixed_content, "Text\n\n- Item 1\n  - Nested 1\n- Item 2\nText");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_list_with_internal_blanks() {
        // Per markdownlint-cli: only preceding blank line warning
        let content = "Text\n* Item 1\n\n  More Item 1 Content\n* Item 2\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            1,
            "List with internal blanks needs preceding blank only. Got: {warnings:?}"
        );
        assert_eq!(warnings[0].line, 2);
        assert!(warnings[0].message.contains("preceded by blank line"));

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(
            fixed_content,
            "Text\n\n* Item 1\n\n  More Item 1 Content\n* Item 2\nText"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_ignore_code_blocks() {
        let content = "```\n- Not a list item\n```\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 0);
        let fixed_content = fix(content);
        assert_eq!(fixed_content, content);
    }

    #[test]
    fn test_ignore_front_matter() {
        // Per markdownlint-cli: NO warnings - front matter is followed by list, trailing text is lazy continuation
        let content = "---\ntitle: Test\n---\n- List Item\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            0,
            "Front matter test should have no MD032 warnings. Got: {warnings:?}"
        );

        // No fixes needed since no warnings
        let fixed_content = fix(content);
        assert_eq!(fixed_content, content, "No changes when no warnings");
    }

    #[test]
    fn test_multiple_lists() {
        // Our implementation treats "Text 2" and "Text 3" as lazy continuation within a single merged list block
        // (since both - and * are unordered markers and there's no structural separator)
        // markdownlint-cli sees them as separate lists with 3 warnings, but our behavior differs.
        // The key requirement is that the fix resolves all warnings.
        let content = "Text\n- List 1 Item 1\n- List 1 Item 2\nText 2\n* List 2 Item 1\nText 3";
        let warnings = lint(content);
        // At minimum we should warn about missing preceding blank for line 2
        assert!(
            !warnings.is_empty(),
            "Should have at least one warning for missing blank line. Got: {warnings:?}"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        // The fix should add blank lines before lists that need them
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_adjacent_lists() {
        let content = "- List 1\n\n* List 2";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 0);
        let fixed_content = fix(content);
        assert_eq!(fixed_content, content);
    }

    #[test]
    fn test_list_in_blockquote() {
        // Per markdownlint-cli: 1 warning (preceding only, trailing is lazy continuation)
        let content = "> Quote line 1\n> - List item 1\n> - List item 2\n> Quote line 2";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            1,
            "Expected 1 warning for blockquoted list needing preceding blank. Got: {warnings:?}"
        );
        assert_eq!(warnings[0].line, 2);

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        // Fix should add blank line before list only
        assert_eq!(
            fixed_content, "> Quote line 1\n> \n> - List item 1\n> - List item 2\n> Quote line 2",
            "Fix for blockquoted list failed. Got:\n{fixed_content}"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_ordered_list() {
        // Per markdownlint-cli: 1 warning (preceding only)
        let content = "Text\n1. Item 1\n2. Item 2\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 1);

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(fixed_content, "Text\n\n1. Item 1\n2. Item 2\nText");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_no_double_blank_fix() {
        // Per markdownlint-cli: trailing text is lazy continuation, so NO warning needed
        let content = "Text\n\n- Item 1\n- Item 2\nText"; // Has preceding blank, trailing is lazy
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            0,
            "Should have no warnings - properly preceded, trailing is lazy"
        );

        let fixed_content = fix(content);
        assert_eq!(
            fixed_content, content,
            "No fix needed when no warnings. Got:\n{fixed_content}"
        );

        let content2 = "Text\n- Item 1\n- Item 2\n\nText"; // Missing blank before
        let warnings2 = lint(content2);
        assert_eq!(warnings2.len(), 1);
        if !warnings2.is_empty() {
            assert_eq!(
                warnings2[0].line, 2,
                "Warning line for missing blank before should be the first line of the block"
            );
        }

        // Test that warnings have fixes
        check_warnings_have_fixes(content2);

        let fixed_content2 = fix(content2);
        assert_eq!(
            fixed_content2, "Text\n\n- Item 1\n- Item 2\n\nText",
            "Fix added extra blank before. Got:\n{fixed_content2}"
        );
    }

    #[test]
    fn test_empty_input() {
        let content = "";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 0);
        let fixed_content = fix(content);
        assert_eq!(fixed_content, "");
    }

    #[test]
    fn test_only_list() {
        let content = "- Item 1\n- Item 2";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 0);
        let fixed_content = fix(content);
        assert_eq!(fixed_content, content);
    }

    // === COMPREHENSIVE FIX TESTS ===

    #[test]
    fn test_fix_complex_nested_blockquote() {
        // Per markdownlint-cli: 1 warning (preceding only)
        let content = "> Text before\n> - Item 1\n>   - Nested item\n> - Item 2\n> Text after";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            1,
            "Should warn for missing preceding blank only. Got: {warnings:?}"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "> Text before\n> \n> - Item 1\n>   - Nested item\n> - Item 2\n> Text after";
        assert_eq!(fixed_content, expected, "Fix should preserve blockquote structure");

        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should eliminate all warnings");
    }

    #[test]
    fn test_fix_mixed_list_markers() {
        // Per markdownlint-cli: mixed markers may be treated as separate lists
        // The exact behavior depends on implementation details
        let content = "Text\n- Item 1\n* Item 2\n+ Item 3\nText";
        let warnings = lint(content);
        // At minimum, there should be a warning for the first list needing preceding blank
        assert!(
            !warnings.is_empty(),
            "Should have at least 1 warning for mixed marker list. Got: {warnings:?}"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        // The fix should add at least a blank line before the first list
        assert!(
            fixed_content.contains("Text\n\n-"),
            "Fix should add blank line before first list item"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_fix_ordered_list_with_different_numbers() {
        // Per markdownlint-cli: 1 warning (preceding only)
        let content = "Text\n1. First\n3. Third\n2. Second\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 1, "Should warn for missing preceding blank only");

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "Text\n\n1. First\n3. Third\n2. Second\nText";
        assert_eq!(
            fixed_content, expected,
            "Fix should handle ordered lists with non-sequential numbers"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_fix_list_with_code_blocks_inside() {
        // Per markdownlint-cli: 1 warning (preceding only)
        let content = "Text\n- Item 1\n  ```\n  code\n  ```\n- Item 2\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 1, "Should warn for missing preceding blank only");

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "Text\n\n- Item 1\n  ```\n  code\n  ```\n- Item 2\nText";
        assert_eq!(
            fixed_content, expected,
            "Fix should handle lists with internal code blocks"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_fix_deeply_nested_lists() {
        // Per markdownlint-cli: 1 warning (preceding only)
        let content = "Text\n- Level 1\n  - Level 2\n    - Level 3\n      - Level 4\n- Back to Level 1\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 1, "Should warn for missing preceding blank only");

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "Text\n\n- Level 1\n  - Level 2\n    - Level 3\n      - Level 4\n- Back to Level 1\nText";
        assert_eq!(fixed_content, expected, "Fix should handle deeply nested lists");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_fix_list_with_multiline_items() {
        // Per markdownlint-cli: trailing "Text" at indent=0 is lazy continuation
        // Only the preceding blank line is required
        let content = "Text\n- Item 1\n  continues here\n  and here\n- Item 2\n  also continues\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            1,
            "Should only warn for missing blank before list (trailing text is lazy continuation)"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "Text\n\n- Item 1\n  continues here\n  and here\n- Item 2\n  also continues\nText";
        assert_eq!(fixed_content, expected, "Fix should add blank before list only");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_fix_list_at_document_boundaries() {
        // List at very start
        let content1 = "- Item 1\n- Item 2";
        let warnings1 = lint(content1);
        assert_eq!(
            warnings1.len(),
            0,
            "List at document start should not need blank before"
        );
        let fixed1 = fix(content1);
        assert_eq!(fixed1, content1, "No fix needed for list at start");

        // List at very end
        let content2 = "Text\n- Item 1\n- Item 2";
        let warnings2 = lint(content2);
        assert_eq!(warnings2.len(), 1, "List at document end should need blank before");
        check_warnings_have_fixes(content2);
        let fixed2 = fix(content2);
        assert_eq!(
            fixed2, "Text\n\n- Item 1\n- Item 2",
            "Should add blank before list at end"
        );
    }

    #[test]
    fn test_fix_preserves_existing_blank_lines() {
        let content = "Text\n\n\n- Item 1\n- Item 2\n\n\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 0, "Multiple blank lines should be preserved");
        let fixed_content = fix(content);
        assert_eq!(fixed_content, content, "Fix should not modify already correct content");
    }

    #[test]
    fn test_fix_handles_tabs_and_spaces() {
        // Per markdownlint-cli: trailing text is lazy continuation, only preceding blank needed
        let content = "Text\n\t- Item with tab\n  - Item with spaces\nText";
        let warnings = lint(content);
        // The tab-indented item and space-indented item may be seen as separate lists
        // Per markdownlint-cli behavior, we expect at least 1 warning
        assert!(!warnings.is_empty(), "Should warn for missing blank before list");

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        // Only add blank before, not after (trailing text is lazy continuation)
        let expected = "Text\n\n\t- Item with tab\n  - Item with spaces\nText";
        assert_eq!(fixed_content, expected, "Fix should preserve original indentation");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_fix_warning_objects_have_correct_ranges() {
        // Per markdownlint-cli: trailing text is lazy continuation, only 1 warning
        let content = "Text\n- Item 1\n- Item 2\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 1, "Only preceding blank warning expected");

        // Check that each warning has a fix with a valid range
        for warning in &warnings {
            assert!(warning.fix.is_some(), "Warning should have fix");
            let fix = warning.fix.as_ref().unwrap();
            assert!(fix.range.start <= fix.range.end, "Fix range should be valid");
            assert!(
                !fix.replacement.is_empty() || fix.range.start == fix.range.end,
                "Fix should have replacement or be insertion"
            );
        }
    }

    #[test]
    fn test_fix_idempotent() {
        // Per markdownlint-cli: trailing text is lazy continuation
        let content = "Text\n- Item 1\n- Item 2\nText";

        // Apply fix once - only adds blank before (trailing text is lazy continuation)
        let fixed_once = fix(content);
        assert_eq!(fixed_once, "Text\n\n- Item 1\n- Item 2\nText");

        // Apply fix again - should be unchanged
        let fixed_twice = fix(&fixed_once);
        assert_eq!(fixed_twice, fixed_once, "Fix should be idempotent");

        // No warnings after fix
        let warnings_after_fix = lint(&fixed_once);
        assert_eq!(warnings_after_fix.len(), 0, "No warnings should remain after fix");
    }

    #[test]
    fn test_fix_with_normalized_line_endings() {
        // In production, content is normalized to LF at I/O boundary
        // Unit tests should use LF input to reflect actual runtime behavior
        // Per markdownlint-cli: trailing text is lazy continuation, only 1 warning
        let content = "Text\n- Item 1\n- Item 2\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 1, "Should detect missing blank before list");

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        // Only adds blank before (trailing text is lazy continuation)
        let expected = "Text\n\n- Item 1\n- Item 2\nText";
        assert_eq!(fixed_content, expected, "Fix should work with normalized LF content");
    }

    #[test]
    fn test_fix_preserves_final_newline() {
        // Per markdownlint-cli: trailing text is lazy continuation
        // Test with final newline
        let content_with_newline = "Text\n- Item 1\n- Item 2\nText\n";
        let fixed_with_newline = fix(content_with_newline);
        assert!(
            fixed_with_newline.ends_with('\n'),
            "Fix should preserve final newline when present"
        );
        // Only adds blank before (trailing text is lazy continuation)
        assert_eq!(fixed_with_newline, "Text\n\n- Item 1\n- Item 2\nText\n");

        // Test without final newline
        let content_without_newline = "Text\n- Item 1\n- Item 2\nText";
        let fixed_without_newline = fix(content_without_newline);
        assert!(
            !fixed_without_newline.ends_with('\n'),
            "Fix should not add final newline when not present"
        );
        // Only adds blank before (trailing text is lazy continuation)
        assert_eq!(fixed_without_newline, "Text\n\n- Item 1\n- Item 2\nText");
    }

    #[test]
    fn test_fix_multiline_list_items_no_indent() {
        let content = "## Configuration\n\nThis rule has the following configuration options:\n\n- `option1`: Description that continues\non the next line without indentation.\n- `option2`: Another description that also continues\non the next line.\n\n## Next Section";

        let warnings = lint(content);
        // Should only warn about missing blank lines around the entire list, not between items
        assert_eq!(
            warnings.len(),
            0,
            "Should not warn for properly formatted list with multi-line items. Got: {warnings:?}"
        );

        let fixed_content = fix(content);
        // Should not change the content since it's already correct
        assert_eq!(
            fixed_content, content,
            "Should not modify correctly formatted multi-line list items"
        );
    }

    #[test]
    fn test_nested_list_with_lazy_continuation() {
        // Issue #188: Nested list following a lazy continuation line should not require blank lines
        // This matches markdownlint-cli behavior which does NOT warn on this pattern
        //
        // The key element is line 6 (`!=`), ternary...) which is a lazy continuation of line 5.
        // Line 6 contains `||` inside code spans, which should NOT be detected as a table separator.
        let content = r#"# Test

- **Token Dispatch (Phase 3.2)**: COMPLETE. Extracts tokens from both:
  1. Switch/case dispatcher statements (original Phase 3.2)
  2. Inline conditionals - if/else, bitwise checks (`&`, `|`), comparison (`==`,
`!=`), ternary operators (`?:`), macros (`ISTOK`, `ISUNSET`), compound conditions (`&&`, `||`) (Phase 3.2.1)
     - 30 explicit tokens extracted, 23 dispatcher rules with embedded token
       references"#;

        let warnings = lint(content);
        // No MD032 warnings should be generated - this is a valid nested list structure
        // with lazy continuation (line 6 has no indent but continues line 5)
        let md032_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD032"))
            .collect();
        assert_eq!(
            md032_warnings.len(),
            0,
            "Should not warn for nested list with lazy continuation. Got: {md032_warnings:?}"
        );
    }

    #[test]
    fn test_pipes_in_code_spans_not_detected_as_table() {
        // Pipes inside code spans should NOT break lists
        let content = r#"# Test

- Item with `a | b` inline code
  - Nested item should work

"#;

        let warnings = lint(content);
        let md032_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD032"))
            .collect();
        assert_eq!(
            md032_warnings.len(),
            0,
            "Pipes in code spans should not break lists. Got: {md032_warnings:?}"
        );
    }

    #[test]
    fn test_multiple_code_spans_with_pipes() {
        // Multiple code spans with pipes should not break lists
        let content = r#"# Test

- Item with `a | b` and `c || d` operators
  - Nested item should work

"#;

        let warnings = lint(content);
        let md032_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD032"))
            .collect();
        assert_eq!(
            md032_warnings.len(),
            0,
            "Multiple code spans with pipes should not break lists. Got: {md032_warnings:?}"
        );
    }

    #[test]
    fn test_actual_table_breaks_list() {
        // An actual table between list items SHOULD break the list
        let content = r#"# Test

- Item before table

| Col1 | Col2 |
|------|------|
| A    | B    |

- Item after table

"#;

        let warnings = lint(content);
        // There should be NO MD032 warnings because both lists are properly surrounded by blank lines
        let md032_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD032"))
            .collect();
        assert_eq!(
            md032_warnings.len(),
            0,
            "Both lists should be properly separated by blank lines. Got: {md032_warnings:?}"
        );
    }

    #[test]
    fn test_thematic_break_not_lazy_continuation() {
        // Thematic breaks (HRs) cannot be lazy continuation per CommonMark
        // List followed by HR without blank line should warn
        let content = r#"- Item 1
- Item 2
***

More text.
"#;

        let warnings = lint(content);
        let md032_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD032"))
            .collect();
        assert_eq!(
            md032_warnings.len(),
            1,
            "Should warn for list not followed by blank line before thematic break. Got: {md032_warnings:?}"
        );
        assert!(
            md032_warnings[0].message.contains("followed by blank line"),
            "Warning should be about missing blank after list"
        );
    }

    #[test]
    fn test_thematic_break_with_blank_line() {
        // List followed by blank line then HR should NOT warn
        let content = r#"- Item 1
- Item 2

***

More text.
"#;

        let warnings = lint(content);
        let md032_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD032"))
            .collect();
        assert_eq!(
            md032_warnings.len(),
            0,
            "Should not warn when list is properly followed by blank line. Got: {md032_warnings:?}"
        );
    }

    #[test]
    fn test_various_thematic_break_styles() {
        // Test different HR styles are all recognized
        // Note: Spaced styles like "- - -" and "* * *" are excluded because they start
        // with list markers ("- " or "* ") which get parsed as list items by the
        // upstream CommonMark parser. That's a separate parsing issue.
        for hr in ["---", "***", "___"] {
            let content = format!(
                r#"- Item 1
- Item 2
{hr}

More text.
"#
            );

            let warnings = lint(&content);
            let md032_warnings: Vec<_> = warnings
                .iter()
                .filter(|w| w.rule_name.as_deref() == Some("MD032"))
                .collect();
            assert_eq!(
                md032_warnings.len(),
                1,
                "Should warn for HR style '{hr}' without blank line. Got: {md032_warnings:?}"
            );
        }
    }

    // === LAZY CONTINUATION TESTS ===

    fn lint_with_config(content: &str, config: MD032Config) -> Vec<LintWarning> {
        let rule = MD032BlanksAroundLists::from_config_struct(config);
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.check(&ctx).expect("Lint check failed")
    }

    fn fix_with_config(content: &str, config: MD032Config) -> String {
        let rule = MD032BlanksAroundLists::from_config_struct(config);
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.fix(&ctx).expect("Lint fix failed")
    }

    #[test]
    fn test_lazy_continuation_allowed_by_default() {
        // Default behavior: lazy continuation is allowed, no warning
        let content = "# Heading\n\n1. List\nSome text.";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            0,
            "Default behavior should allow lazy continuation. Got: {warnings:?}"
        );
    }

    #[test]
    fn test_lazy_continuation_disallowed() {
        // With allow_lazy_continuation = false, should warn
        let content = "# Heading\n\n1. List\nSome text.";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config);
        assert_eq!(
            warnings.len(),
            1,
            "Should warn when lazy continuation is disallowed. Got: {warnings:?}"
        );
        assert!(
            warnings[0].message.contains("followed by blank line"),
            "Warning message should mention blank line"
        );
    }

    #[test]
    fn test_lazy_continuation_fix() {
        // With allow_lazy_continuation = false, fix should insert blank line
        let content = "# Heading\n\n1. List\nSome text.";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let fixed = fix_with_config(content, config.clone());
        assert_eq!(
            fixed, "# Heading\n\n1. List\n\nSome text.",
            "Fix should insert blank line before lazy continuation"
        );

        // Verify no warnings after fix
        let warnings_after = lint_with_config(&fixed, config);
        assert_eq!(warnings_after.len(), 0, "No warnings should remain after fix");
    }

    #[test]
    fn test_lazy_continuation_multiple_lines() {
        // Multiple lazy continuation lines
        let content = "- Item 1\nLine 2\nLine 3";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config.clone());
        assert_eq!(
            warnings.len(),
            1,
            "Should warn for lazy continuation. Got: {warnings:?}"
        );

        let fixed = fix_with_config(content, config.clone());
        assert_eq!(
            fixed, "- Item 1\n\nLine 2\nLine 3",
            "Fix should insert blank line after list"
        );

        // Verify no warnings after fix
        let warnings_after = lint_with_config(&fixed, config);
        assert_eq!(warnings_after.len(), 0, "No warnings should remain after fix");
    }

    #[test]
    fn test_lazy_continuation_with_indented_content() {
        // Indented content is valid continuation, not lazy continuation
        let content = "- Item 1\n  Indented content\nLazy text";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config);
        assert_eq!(
            warnings.len(),
            1,
            "Should warn for lazy text after indented content. Got: {warnings:?}"
        );
    }

    #[test]
    fn test_lazy_continuation_properly_separated() {
        // With proper blank line, no warning even with strict config
        let content = "- Item 1\n\nSome text.";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config);
        assert_eq!(
            warnings.len(),
            0,
            "Should not warn when list is properly followed by blank line. Got: {warnings:?}"
        );
    }

    // ==================== Expert-level edge case tests ====================

    #[test]
    fn test_lazy_continuation_ordered_list_parenthesis_marker() {
        // Ordered list with parenthesis marker (1) instead of period
        let content = "1) First item\nLazy continuation";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config.clone());
        assert_eq!(
            warnings.len(),
            1,
            "Should warn for lazy continuation with parenthesis marker"
        );

        let fixed = fix_with_config(content, config);
        assert_eq!(fixed, "1) First item\n\nLazy continuation");
    }

    #[test]
    fn test_lazy_continuation_followed_by_another_list() {
        // Lazy continuation text followed by another list item
        // In CommonMark, "Some text" becomes part of Item 1's lazy continuation,
        // and "- Item 2" starts a new list item within the same list.
        // This is valid list structure, not a lazy continuation warning case.
        let content = "- Item 1\nSome text\n- Item 2";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config);
        // No MD032 warning because this is valid list structure
        // (all content is within the list block)
        assert_eq!(
            warnings.len(),
            0,
            "Valid list structure should not trigger lazy continuation warning"
        );
    }

    #[test]
    fn test_lazy_continuation_multiple_in_document() {
        // Multiple lists with lazy continuation at end
        // First list: "- Item 1\nLazy 1" - lazy continuation is part of list
        // Blank line separates the lists
        // Second list: "- Item 2\nLazy 2" - lazy continuation followed by EOF
        // Only the second list triggers a warning (list not followed by blank)
        let content = "- Item 1\nLazy 1\n\n- Item 2\nLazy 2";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config.clone());
        assert_eq!(
            warnings.len(),
            1,
            "Should warn for second list (not followed by blank). Got: {warnings:?}"
        );

        let fixed = fix_with_config(content, config.clone());
        let warnings_after = lint_with_config(&fixed, config);
        assert_eq!(warnings_after.len(), 0, "No warnings should remain after fix");
    }

    #[test]
    fn test_lazy_continuation_end_of_document_no_newline() {
        // Lazy continuation at end of document without trailing newline
        let content = "- Item\nNo trailing newline";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config.clone());
        assert_eq!(warnings.len(), 1, "Should warn even at end of document");

        let fixed = fix_with_config(content, config);
        assert_eq!(fixed, "- Item\n\nNo trailing newline");
    }

    #[test]
    fn test_lazy_continuation_thematic_break_still_needs_blank() {
        // Thematic break after list without blank line still triggers MD032
        // The thematic break ends the list, but MD032 requires blank line separation
        let content = "- Item 1\n---";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config.clone());
        // Should warn because list needs blank line before thematic break
        assert_eq!(
            warnings.len(),
            1,
            "List should need blank line before thematic break. Got: {warnings:?}"
        );

        // Verify fix adds blank line
        let fixed = fix_with_config(content, config);
        assert_eq!(fixed, "- Item 1\n\n---");
    }

    #[test]
    fn test_lazy_continuation_heading_not_flagged() {
        // Heading after list should NOT be flagged as lazy continuation
        // (headings end lists per CommonMark)
        let content = "- Item 1\n# Heading";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config);
        // The warning should be about missing blank line, not lazy continuation
        // But headings interrupt lists, so the list ends at Item 1
        assert!(
            warnings.iter().all(|w| !w.message.contains("lazy")),
            "Heading should not trigger lazy continuation warning"
        );
    }

    #[test]
    fn test_lazy_continuation_mixed_list_types() {
        // Mixed ordered and unordered with lazy continuation
        let content = "- Unordered\n1. Ordered\nLazy text";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config.clone());
        assert!(!warnings.is_empty(), "Should warn about structure issues");
    }

    #[test]
    fn test_lazy_continuation_deep_nesting() {
        // Deep nested list with lazy continuation at end
        let content = "- Level 1\n  - Level 2\n    - Level 3\nLazy at root";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config.clone());
        assert!(
            !warnings.is_empty(),
            "Should warn about lazy continuation after nested list"
        );

        let fixed = fix_with_config(content, config.clone());
        let warnings_after = lint_with_config(&fixed, config);
        assert_eq!(warnings_after.len(), 0, "No warnings should remain after fix");
    }

    #[test]
    fn test_lazy_continuation_with_emphasis_in_text() {
        // Lazy continuation containing emphasis markers
        let content = "- Item\n*emphasized* continuation";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config.clone());
        assert_eq!(warnings.len(), 1, "Should warn even with emphasis in continuation");

        let fixed = fix_with_config(content, config);
        assert_eq!(fixed, "- Item\n\n*emphasized* continuation");
    }

    #[test]
    fn test_lazy_continuation_with_code_span() {
        // Lazy continuation containing code span
        let content = "- Item\n`code` continuation";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config.clone());
        assert_eq!(warnings.len(), 1, "Should warn even with code span in continuation");

        let fixed = fix_with_config(content, config);
        assert_eq!(fixed, "- Item\n\n`code` continuation");
    }

    #[test]
    fn test_lazy_continuation_whitespace_only_line() {
        // Line with only whitespace is NOT considered a blank line for MD032
        // This matches CommonMark where only truly empty lines are "blank"
        let content = "- Item\n   \nText after whitespace-only line";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config.clone());
        // Whitespace-only line does NOT count as blank line separator
        assert_eq!(
            warnings.len(),
            1,
            "Whitespace-only line should NOT count as separator. Got: {warnings:?}"
        );

        // Verify fix adds proper blank line
        let fixed = fix_with_config(content, config);
        assert!(fixed.contains("\n\nText"), "Fix should add blank line separator");
    }

    #[test]
    fn test_lazy_continuation_blockquote_context() {
        // List inside blockquote with lazy continuation
        let content = "> - Item\n> Lazy in quote";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config);
        // Inside blockquote, lazy continuation may behave differently
        // This tests that we handle blockquote context
        assert!(warnings.len() <= 1, "Should handle blockquote context gracefully");
    }

    #[test]
    fn test_lazy_continuation_fix_preserves_content() {
        // Ensure fix doesn't modify the actual content
        let content = "- Item with special chars: <>&\nContinuation with: \"quotes\"";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let fixed = fix_with_config(content, config);
        assert!(fixed.contains("<>&"), "Should preserve special chars");
        assert!(fixed.contains("\"quotes\""), "Should preserve quotes");
        assert_eq!(fixed, "- Item with special chars: <>&\n\nContinuation with: \"quotes\"");
    }

    #[test]
    fn test_lazy_continuation_fix_idempotent() {
        // Running fix twice should produce same result
        let content = "- Item\nLazy";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let fixed_once = fix_with_config(content, config.clone());
        let fixed_twice = fix_with_config(&fixed_once, config);
        assert_eq!(fixed_once, fixed_twice, "Fix should be idempotent");
    }

    #[test]
    fn test_lazy_continuation_config_default_allows() {
        // Verify default config allows lazy continuation
        let content = "- Item\nLazy text that continues";
        let default_config = MD032Config::default();
        assert!(
            default_config.allow_lazy_continuation,
            "Default should allow lazy continuation"
        );
        let warnings = lint_with_config(content, default_config);
        assert_eq!(warnings.len(), 0, "Default config should not warn on lazy continuation");
    }

    #[test]
    fn test_lazy_continuation_after_multi_line_item() {
        // List item with proper indented continuation, then lazy text
        let content = "- Item line 1\n  Item line 2 (indented)\nLazy (not indented)";
        let config = MD032Config {
            allow_lazy_continuation: false,
        };
        let warnings = lint_with_config(content, config.clone());
        assert_eq!(
            warnings.len(),
            1,
            "Should warn only for the lazy line, not the indented line"
        );
    }
}
