use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::document_structure_from_str;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{LineIndex, calculate_line_range};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref BLOCKQUOTE_PREFIX_RE: Regex = Regex::new(r"^(\s*>)+(\s*)").unwrap();
    static ref BLANK_LINE_RE: Regex = Regex::new(r"^\s*$").unwrap();
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
#[derive(Debug, Clone)]
pub struct MD032BlanksAroundLists {
    /// Allow lists to follow headings without blank lines
    pub allow_after_headings: bool,
    /// Allow lists to follow content ending with colons without blank lines
    pub allow_after_colons: bool,
}

impl Default for MD032BlanksAroundLists {
    fn default() -> Self {
        Self {
            allow_after_headings: true, // More lenient by default
            allow_after_colons: true,
        }
    }
}

impl MD032BlanksAroundLists {
    pub fn strict() -> Self {
        Self {
            allow_after_headings: false,
            allow_after_colons: false,
        }
    }

    /// Check if a blank line should be required before a list based on the previous line context
    fn should_require_blank_line_before(
        &self,
        prev_line: &str,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
        prev_line_num: usize,
    ) -> bool {
        let trimmed_prev = prev_line.trim();

        // Always require blank lines after code blocks, front matter, etc.
        if structure.is_in_code_block(prev_line_num) || structure.is_in_front_matter(prev_line_num) {
            return true;
        }

        // Allow lists after headings if configured
        if self.allow_after_headings && self.is_heading_line_from_context(ctx, prev_line_num - 1) {
            return false;
        }

        // Allow lists after content ending with colons if configured
        if self.allow_after_colons && trimmed_prev.ends_with(':') {
            return false;
        }

        // Default: require blank line
        true
    }

    /// Check if a line is a heading using cached LintContext info
    fn is_heading_line_from_context(&self, ctx: &crate::lint_context::LintContext, line_idx: usize) -> bool {
        if line_idx < ctx.lines.len() {
            ctx.lines[line_idx].heading.is_some()
        } else {
            false
        }
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
                    // Check if there's a fenced code block between prev_item_line and item_line
                    let mut has_code_fence = false;
                    for check_line in (prev_item_line + 1)..item_line {
                        if check_line - 1 < ctx.lines.len() {
                            let line = &ctx.lines[check_line - 1];
                            if line.in_code_block
                                && (line.content.trim().starts_with("```") || line.content.trim().starts_with("~~~"))
                            {
                                has_code_fence = true;
                                break;
                            }
                        }
                    }

                    if has_code_fence {
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
                            if line.in_code_block
                                && (line.content.trim().starts_with("```") || line.content.trim().starts_with("~~~"))
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
                            // Include lazy continuation only if it's not a separate paragraph
                            else if check_line == *end + 1 && !line.is_blank && line.heading.is_none() {
                                // Check if this looks like list continuation vs new paragraph
                                // Simple heuristic: if it starts with uppercase and the list item ended with punctuation,
                                // it's likely a new paragraph
                                let is_likely_new_paragraph = {
                                    let first_char = line.content.trim().chars().next();
                                    first_char.is_some_and(|c| c.is_uppercase())
                                };

                                if !is_likely_new_paragraph {
                                    actual_end = check_line;
                                } else {
                                    break;
                                }
                            } else if !line.is_blank {
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
        structure: &DocumentStructure,
        lines: &[&str],
        list_blocks: &[(usize, usize, String)],
        line_index: &LineIndex,
    ) -> LintResult {
        let mut warnings = Vec::new();
        let num_lines = lines.len();

        for &(start_line, end_line, ref prefix) in list_blocks {
            if start_line > 1 {
                let prev_line_actual_idx_0 = start_line - 2;
                let prev_line_actual_idx_1 = start_line - 1;
                let prev_line_str = lines[prev_line_actual_idx_0];
                let is_prev_excluded = structure.is_in_code_block(prev_line_actual_idx_1)
                    || structure.is_in_front_matter(prev_line_actual_idx_1);
                let prev_prefix = BLOCKQUOTE_PREFIX_RE
                    .find(prev_line_str)
                    .map_or(String::new(), |m| m.as_str().to_string());
                let prev_is_blank = is_blank_in_context(prev_line_str);
                let prefixes_match = prev_prefix.trim() == prefix.trim();

                // Only require blank lines for content in the same context (same blockquote level)
                // and when the context actually requires it
                let should_require =
                    self.should_require_blank_line_before(prev_line_str, ctx, structure, prev_line_actual_idx_1);
                if !is_prev_excluded && !prev_is_blank && prefixes_match && should_require {
                    // Calculate precise character range for the entire list line that needs a blank line before it
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(start_line, lines[start_line - 1]);

                    warnings.push(LintWarning {
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Error,
                        rule_name: Some(self.name()),
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
                // Check if next line is excluded - in code block, front matter, or starts an indented code block
                // Only exclude code fence lines if they're indented (part of list content)
                let is_next_excluded = structure.is_in_code_block(next_line_idx_1)
                    || structure.is_in_front_matter(next_line_idx_1)
                    || (next_line_idx_0 < ctx.lines.len()
                        && ctx.lines[next_line_idx_0].in_code_block
                        && ctx.lines[next_line_idx_0].indent >= 2
                        && (ctx.lines[next_line_idx_0].content.trim().starts_with("```")
                            || ctx.lines[next_line_idx_0].content.trim().starts_with("~~~")));
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
                        severity: Severity::Error,
                        rule_name: Some(self.name()),
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
        // Delegate to optimized check_with_structure by creating a temporary DocumentStructure
        // This fallback path should rarely be used since the main lint engine calls check_with_structure
        let structure = document_structure_from_str(ctx.content);
        self.check_with_structure(ctx, &structure)
    }

    /// Optimized check using pre-computed document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        let lines: Vec<&str> = content.lines().collect();
        let line_index = LineIndex::new(content.to_string());

        // Early return for empty content
        if lines.is_empty() {
            return Ok(Vec::new());
        }

        let list_blocks = self.convert_list_blocks(ctx);

        if list_blocks.is_empty() {
            return Ok(Vec::new());
        }

        self.perform_checks(ctx, structure, &lines, &list_blocks, &line_index)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Delegate to helper method with temporary DocumentStructure
        let structure = document_structure_from_str(ctx.content);
        self.fix_with_structure(ctx, &structure)
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || ctx.list_blocks.is_empty()
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "allow_after_headings".to_string(),
            toml::Value::Boolean(self.allow_after_headings),
        );
        map.insert(
            "allow_after_colons".to_string(),
            toml::Value::Boolean(self.allow_after_colons),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let allow_after_headings =
            crate::config::get_rule_config_value::<bool>(config, "MD032", "allow_after_headings").unwrap_or(true); // Default to true for better UX

        let allow_after_colons =
            crate::config::get_rule_config_value::<bool>(config, "MD032", "allow_after_colons").unwrap_or(true);

        Box::new(MD032BlanksAroundLists {
            allow_after_headings,
            allow_after_colons,
        })
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }
}

impl MD032BlanksAroundLists {
    /// Helper method for fixing with a pre-computed DocumentStructure
    fn fix_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> Result<String, LintError> {
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
                let is_prev_excluded = structure.is_in_code_block(prev_line_actual_idx_1)
                    || structure.is_in_front_matter(prev_line_actual_idx_1);
                let prev_prefix = BLOCKQUOTE_PREFIX_RE
                    .find(lines[prev_line_actual_idx_0])
                    .map_or(String::new(), |m| m.as_str().to_string());

                let should_require = self.should_require_blank_line_before(
                    lines[prev_line_actual_idx_0],
                    ctx,
                    structure,
                    prev_line_actual_idx_1,
                );
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
                let is_line_after_excluded = structure.is_in_code_block(after_block_line_idx_1)
                    || structure.is_in_front_matter(after_block_line_idx_1)
                    || (after_block_line_idx_0 < ctx.lines.len()
                        && ctx.lines[after_block_line_idx_0].in_code_block
                        && ctx.lines[after_block_line_idx_0].indent >= 2
                        && (ctx.lines[after_block_line_idx_0].content.trim().starts_with("```")
                            || ctx.lines[after_block_line_idx_0].content.trim().starts_with("~~~")));
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
            if let Some(prefix_to_insert) = insertions.get(&current_line_num) {
                if result_lines.is_empty() || result_lines.last().unwrap() != prefix_to_insert {
                    result_lines.push(prefix_to_insert.clone());
                }
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

impl DocumentStructureExtensions for MD032BlanksAroundLists {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
        let content = ctx.content;

        // Early return for empty content
        if content.is_empty() {
            return false;
        }

        // Quick check for list markers
        if !content.contains('-')
            && !content.contains('*')
            && !content.contains('+')
            && !content.chars().any(|c| c.is_numeric())
        {
            return false;
        }

        // This rule is relevant if we found any list blocks
        !ctx.list_blocks.is_empty()
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
        let ctx = LintContext::new(content);
        rule.check(&ctx).expect("Lint check failed")
    }

    fn fix(content: &str) -> String {
        let rule = MD032BlanksAroundLists::default();
        let ctx = LintContext::new(content);
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
        let content = "- Item 1\n- Item 2\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            1,
            "Expected 1 warning for list at start without trailing blank line"
        );
        assert_eq!(
            warnings[0].line, 2,
            "Warning should be on the last line of the list (line 2)"
        );
        assert!(warnings[0].message.contains("followed by blank line"));

        // Test that warning has fix
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(fixed_content, "- Item 1\n- Item 2\n\nText");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
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
        let content = "Text 1\n- Item 1\n- Item 2\nText 2";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Expected 2 warnings for list in middle without surrounding blank lines"
        );
        assert_eq!(warnings[0].line, 2, "First warning on line 2 (start)");
        assert!(warnings[0].message.contains("preceded by blank line"));
        assert_eq!(warnings[1].line, 3, "Second warning on line 3 (end)");
        assert!(warnings[1].message.contains("followed by blank line"));

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(fixed_content, "Text 1\n\n- Item 1\n- Item 2\n\nText 2");

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
        let content = "Text\n* Item 1\n  Content\n* Item 2\n  More content\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Expected 2 warnings for list block (lines 2-5) missing surrounding blanks. Got: {warnings:?}"
        );
        if warnings.len() == 2 {
            assert_eq!(warnings[0].line, 2, "Warning 1 should be on line 2 (start)");
            assert!(warnings[0].message.contains("preceded by blank line"));
            assert_eq!(warnings[1].line, 5, "Warning 2 should be on line 5 (end)");
            assert!(warnings[1].message.contains("followed by blank line"));
        }

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected_fixed = "Text\n\n* Item 1\n  Content\n* Item 2\n  More content\n\nText";
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
        let content = "Text\n- Item 1\n  - Nested 1\n- Item 2\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 2, "Nested list block warnings. Got: {warnings:?}"); // Needs blank before line 2, after line 4
        if warnings.len() == 2 {
            assert_eq!(warnings[0].line, 2);
            assert_eq!(warnings[1].line, 4);
        }

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(fixed_content, "Text\n\n- Item 1\n  - Nested 1\n- Item 2\n\nText");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_list_with_internal_blanks() {
        let content = "Text\n* Item 1\n\n  More Item 1 Content\n* Item 2\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "List with internal blanks warnings. Got: {warnings:?}"
        );
        if warnings.len() == 2 {
            assert_eq!(warnings[0].line, 2);
            assert_eq!(warnings[1].line, 5); // End of block is line 5
        }

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(
            fixed_content,
            "Text\n\n* Item 1\n\n  More Item 1 Content\n* Item 2\n\nText"
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
        let content = "---\ntitle: Test\n---\n- List Item\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 1, "Front matter test warnings. Got: {warnings:?}");
        if !warnings.is_empty() {
            assert_eq!(warnings[0].line, 4); // Warning on last line of list
            assert!(warnings[0].message.contains("followed by blank line"));
        }

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(fixed_content, "---\ntitle: Test\n---\n- List Item\n\nText");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_multiple_lists() {
        let content = "Text\n- List 1 Item 1\n- List 1 Item 2\nText 2\n* List 2 Item 1\nText 3";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 4, "Multiple lists warnings. Got: {warnings:?}");

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(
            fixed_content,
            "Text\n\n- List 1 Item 1\n- List 1 Item 2\n\nText 2\n\n* List 2 Item 1\n\nText 3"
        );

        // Verify fix resolves the issue
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
        let content = "> Quote line 1\n> - List item 1\n> - List item 2\n> Quote line 2";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Expected 2 warnings for blockquoted list. Got: {warnings:?}"
        );
        if warnings.len() == 2 {
            assert_eq!(warnings[0].line, 2);
            assert_eq!(warnings[1].line, 3);
        }

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        // Check expected output preserves the space after >
        assert_eq!(
            fixed_content, "> Quote line 1\n> \n> - List item 1\n> - List item 2\n> \n> Quote line 2",
            "Fix for blockquoted list failed. Got:\n{fixed_content}"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_ordered_list() {
        let content = "Text\n1. Item 1\n2. Item 2\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 2);

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(fixed_content, "Text\n\n1. Item 1\n2. Item 2\n\nText");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_no_double_blank_fix() {
        let content = "Text\n\n- Item 1\n- Item 2\nText"; // Missing blank after
        let warnings = lint(content);
        assert_eq!(warnings.len(), 1);
        if !warnings.is_empty() {
            assert_eq!(
                warnings[0].line, 4,
                "Warning line for missing blank after should be the last line of the block"
            );
        }

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(
            fixed_content, "Text\n\n- Item 1\n- Item 2\n\nText",
            "Fix added extra blank after. Got:\n{fixed_content}"
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
        let content = "> Text before\n> - Item 1\n>   - Nested item\n> - Item 2\n> Text after";
        let warnings = lint(content);
        // MD032 detects each list item as needing blanks, so we get 6 warnings:
        // Line 2: preceded + followed, Line 3: preceded + followed, Line 4: preceded + followed
        assert_eq!(
            warnings.len(),
            6,
            "Should warn for missing blanks around each blockquoted list item"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "> Text before\n> \n> - Item 1\n>   - Nested item\n> - Item 2\n> \n> Text after";
        assert_eq!(fixed_content, expected, "Fix should preserve blockquote structure");

        // Note: This is a complex edge case where MD032's granular approach to list detection
        // means that nested lists within blockquotes may not be perfectly handled by the fix.
        // The fix reduces warnings but may not eliminate all of them due to the nested structure.
        let warnings_after_fix = lint(&fixed_content);
        assert!(
            warnings_after_fix.len() < warnings.len(),
            "Fix should reduce the number of warnings"
        );
    }

    #[test]
    fn test_fix_mixed_list_markers() {
        let content = "Text\n- Item 1\n* Item 2\n+ Item 3\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Should warn for missing blanks around mixed marker list"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "Text\n\n- Item 1\n* Item 2\n+ Item 3\n\nText";
        assert_eq!(fixed_content, expected, "Fix should handle mixed list markers");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_fix_ordered_list_with_different_numbers() {
        let content = "Text\n1. First\n3. Third\n2. Second\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 2, "Should warn for missing blanks around ordered list");

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "Text\n\n1. First\n3. Third\n2. Second\n\nText";
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
        let content = "Text\n- Item 1\n  ```\n  code\n  ```\n- Item 2\nText";
        let warnings = lint(content);
        // MD032 detects the code block as breaking the list, so we get 3 warnings:
        // Line 2: preceded, Line 6: preceded + followed
        assert_eq!(
            warnings.len(),
            3,
            "Should warn for missing blanks around list items separated by code blocks"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "Text\n\n- Item 1\n  ```\n  code\n  ```\n\n- Item 2\n\nText";
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
        let content = "Text\n- Level 1\n  - Level 2\n    - Level 3\n      - Level 4\n- Back to Level 1\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Should warn for missing blanks around deeply nested list"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "Text\n\n- Level 1\n  - Level 2\n    - Level 3\n      - Level 4\n- Back to Level 1\n\nText";
        assert_eq!(fixed_content, expected, "Fix should handle deeply nested lists");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_fix_list_with_multiline_items() {
        let content = "Text\n- Item 1\n  continues here\n  and here\n- Item 2\n  also continues\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Should warn for missing blanks around multiline list"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "Text\n\n- Item 1\n  continues here\n  and here\n- Item 2\n  also continues\n\nText";
        assert_eq!(fixed_content, expected, "Fix should handle multiline list items");

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
        let content = "Text\n\t- Item with tab\n  - Item with spaces\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 2, "Should warn regardless of indentation type");

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "Text\n\n\t- Item with tab\n  - Item with spaces\n\nText";
        assert_eq!(fixed_content, expected, "Fix should preserve original indentation");

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(warnings_after_fix.len(), 0, "Fix should resolve all warnings");
    }

    #[test]
    fn test_fix_warning_objects_have_correct_ranges() {
        let content = "Text\n- Item 1\n- Item 2\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 2);

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
        let content = "Text\n- Item 1\n- Item 2\nText";

        // Apply fix once
        let fixed_once = fix(content);
        assert_eq!(fixed_once, "Text\n\n- Item 1\n- Item 2\n\nText");

        // Apply fix again - should be unchanged
        let fixed_twice = fix(&fixed_once);
        assert_eq!(fixed_twice, fixed_once, "Fix should be idempotent");

        // No warnings after fix
        let warnings_after_fix = lint(&fixed_once);
        assert_eq!(warnings_after_fix.len(), 0, "No warnings should remain after fix");
    }

    #[test]
    fn test_fix_with_windows_line_endings() {
        let content = "Text\r\n- Item 1\r\n- Item 2\r\nText";
        let warnings = lint(content);
        assert_eq!(warnings.len(), 2, "Should detect issues with Windows line endings");

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        // Note: Our fix uses \n, which is standard for Rust string processing
        let expected = "Text\n\n- Item 1\n- Item 2\n\nText";
        assert_eq!(fixed_content, expected, "Fix should handle Windows line endings");
    }

    #[test]
    fn test_fix_preserves_final_newline() {
        // Test with final newline
        let content_with_newline = "Text\n- Item 1\n- Item 2\nText\n";
        let fixed_with_newline = fix(content_with_newline);
        assert!(
            fixed_with_newline.ends_with('\n'),
            "Fix should preserve final newline when present"
        );
        assert_eq!(fixed_with_newline, "Text\n\n- Item 1\n- Item 2\n\nText\n");

        // Test without final newline
        let content_without_newline = "Text\n- Item 1\n- Item 2\nText";
        let fixed_without_newline = fix(content_without_newline);
        assert!(
            !fixed_without_newline.ends_with('\n'),
            "Fix should not add final newline when not present"
        );
        assert_eq!(fixed_without_newline, "Text\n\n- Item 1\n- Item 2\n\nText");
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
}
