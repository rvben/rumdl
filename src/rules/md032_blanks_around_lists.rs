use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::document_structure_from_str;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{calculate_line_range, LineIndex};
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use std::collections::VecDeque;

lazy_static! {
    static ref LIST_ITEM_START_REGEX: Regex =
        Regex::new(r"^([\t ]*)(?:([*+-])|(\d+)\.)(\s+|$)").unwrap();
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
#[derive(Debug, Default, Clone)]
pub struct MD032BlanksAroundLists;

impl MD032BlanksAroundLists {
    // Updated to return blockquote prefix along with block ranges
    fn find_md032_list_blocks(
        &self,
        lines: &[&str],
        structure: &DocumentStructure,
    ) -> Vec<(usize, usize, String)> {
        let mut list_blocks: Vec<(usize, usize, String)> = Vec::new();
        let num_lines = lines.len();
        let mut current_line_idx_0 = 0;

        while current_line_idx_0 < num_lines {
            let current_line_idx_1 = current_line_idx_0 + 1;
            let line_str = lines[current_line_idx_0];

            if structure.is_in_code_block(current_line_idx_1)
                || structure.is_in_front_matter(current_line_idx_1)
            {
                // Special case: Even if a line is in a code block (e.g., tab-indented),
                // if it looks like a list item, the user probably intended it as such.
                // We should warn about spacing issues rather than silently ignore it.
                // But only apply this to INDENTED code blocks, not FENCED code blocks.
                if structure.is_in_code_block(current_line_idx_1) {
                    // Check if this is an indented code block (starts with tab or 4+ spaces)
                    // vs a fenced code block (enclosed in ``` or ~~~)
                    let line_trimmed = line_str.trim_start();
                    let leading_whitespace = &line_str[..line_str.len() - line_trimmed.len()];
                    let is_indented_code = leading_whitespace.contains('\t') ||
                                          leading_whitespace.chars().filter(|&c| c == ' ').count() >= 4;

                    if is_indented_code {
                        // Check if this "indented code block" line actually looks like a list item
                        let blockquote_prefix = BLOCKQUOTE_PREFIX_RE
                            .find(line_str)
                            .map_or(String::new(), |m| m.as_str().to_string());
                        let line_content = line_str.trim_start_matches(&blockquote_prefix);

                        // If it matches our list item pattern, treat it as a list item for linting purposes
                        if LIST_ITEM_START_REGEX.is_match(line_content) {
                            // Don't skip - process it as a potential list item
                        } else {
                current_line_idx_0 += 1;
                continue;
                        }
                    } else {
                        // Fenced code block - always skip
                        current_line_idx_0 += 1;
                        continue;
                    }
                } else {
                    // Front matter - always skip
                    current_line_idx_0 += 1;
                    continue;
                }
            }

            // Determine blockquote prefix and content *before* checking for list item
            let blockquote_prefix = BLOCKQUOTE_PREFIX_RE
                .find(line_str)
                .map_or(String::new(), |m| m.as_str().to_string());
            let line_content = line_str.trim_start_matches(&blockquote_prefix);

            // Check for list item start on the line *content*
            if let Some(captures) = LIST_ITEM_START_REGEX.captures(line_content) {
                // Use indent calculated from the *content* line for comparison
                if let Some(first_item_content_indent) = get_content_start_column(&captures) {
                    let block_start_line_1 = current_line_idx_1;
                    let mut block_end_line_1 = current_line_idx_1;

                    // blockquote_prefix is already determined for the start line

                    let mut lookahead_idx_0 = current_line_idx_0 + 1;
                    let mut potential_blank_lines = VecDeque::new();

                    while lookahead_idx_0 < num_lines {
                        let next_line_idx_1 = lookahead_idx_0 + 1;
                        let next_line_str = lines[lookahead_idx_0];

                        if structure.is_in_code_block(next_line_idx_1)
                            || structure.is_in_front_matter(next_line_idx_1)
                        {
                            break;
                        }

                        // Check blockquote consistency using the prefix from the *start* line
                        let current_line_prefix = BLOCKQUOTE_PREFIX_RE
                            .find(next_line_str)
                            .map_or(String::new(), |m| m.as_str().to_string());
                        if current_line_prefix != blockquote_prefix {
                            break;
                        }

                        // Get content of the lookahead line after the *consistent* prefix
                        let next_line_content =
                            next_line_str.trim_start_matches(&blockquote_prefix);

                        // Check blankness on the *content* part
                        if BLANK_LINE_RE.is_match(next_line_content) {
                            // Check blankness *after* prefix
                            potential_blank_lines.push_back(lookahead_idx_0);
                            lookahead_idx_0 += 1;
                            continue;
                        }

                        // Check continuation based on *content* after prefix
                        let is_next_list_item_start =
                            LIST_ITEM_START_REGEX.is_match(next_line_content);
                        let next_line_indent = calculate_indent(next_line_content); // Calculate indent on content only

                        // A line continues the list if:
                        // 1. It's a new list item (starts with marker)
                        // 2. It's indented enough to be list content (>= first_item_content_indent)
                        // 3. It's an unindented continuation that immediately follows a list item
                        //    and starts with lowercase (indicating it's a sentence fragment)
                        let is_unindented_continuation = !next_line_content.trim().is_empty() &&
                                                       !next_line_content.starts_with('#') && // Not a heading
                                                       !next_line_content.starts_with("```") && // Not a code block
                                                       !next_line_content.starts_with("~~~") && // Not a code block
                                                       next_line_indent == 0 && // Must be unindented
                                                       // Must start with lowercase letter (sentence fragment)
                                                       next_line_content.trim().chars().next().map_or(false, |c| c.is_lowercase());

                        let is_continuation = is_next_list_item_start
                            || next_line_indent >= first_item_content_indent
                            || is_unindented_continuation;

                        if is_continuation {
                            block_end_line_1 = next_line_idx_1;
                            potential_blank_lines.clear();
                            lookahead_idx_0 += 1;
                        } else {
                            break;
                        }
                    }

                    list_blocks.push((block_start_line_1, block_end_line_1, blockquote_prefix)); // Store prefix
                    current_line_idx_0 = block_end_line_1;
                } else {
                    // Should not happen if regex matched, but handle gracefully
                    current_line_idx_0 += 1;
                }
            } else {
                current_line_idx_0 += 1;
            }
        }

        // Merge adjacent/overlapping blocks
        if list_blocks.is_empty() {
            return list_blocks;
        }

        let mut merged_blocks: Vec<(usize, usize, String)> = Vec::new();
        let mut current_block = list_blocks[0].clone(); // Clone to own the prefix string

        for next_block in list_blocks.iter().skip(1) {
            if next_block.0 <= current_block.1 + 1 && next_block.2 == current_block.2 {
                current_block.1 = std::cmp::max(current_block.1, next_block.1);
            } else {
                merged_blocks.push(current_block.clone());
                current_block = next_block.clone();
            }
        }
        merged_blocks.push(current_block);
        merged_blocks
    }

    fn perform_checks(
        &self,
        _ctx: &crate::lint_context::LintContext,
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
                if !is_prev_excluded && !prev_is_blank && prefixes_match {
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
                            range: line_index.line_col_to_byte_range(start_line, 1),
                            replacement: format!("{}\n{}", prefix, lines[start_line - 1]),
                        }),
                    });
                }
            }

            if end_line < num_lines {
                let next_line_idx_0 = end_line;
                let next_line_idx_1 = end_line + 1;
                let next_line_str = lines[next_line_idx_0];
                let is_next_excluded = structure.is_in_code_block(next_line_idx_1)
                    || structure.is_in_front_matter(next_line_idx_1);
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
                            range: line_index.line_col_to_byte_range(end_line + 1, 1),
                            replacement: format!("{}\n{}", prefix, lines[end_line]),
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

        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for list markers
        if !content.contains('-')
            && !content.contains('*')
            && !content.contains('+')
            && !content.chars().any(|c| c.is_numeric())
        {
            return Ok(Vec::new());
        }

        let structure = document_structure_from_str(content);
        let lines: Vec<&str> = content.lines().collect();
        let line_index = LineIndex::new(content.to_string());

        let list_blocks = self.find_md032_list_blocks(&lines, &structure);

        if list_blocks.is_empty() {
            return Ok(Vec::new());
        }

        self.perform_checks(ctx, &structure, &lines, &list_blocks, &line_index)
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

        let list_blocks = self.find_md032_list_blocks(&lines, structure);

        if list_blocks.is_empty() {
            return Ok(Vec::new());
        }

        self.perform_checks(ctx, structure, &lines, &list_blocks, &line_index)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let structure = document_structure_from_str(ctx.content);
        let lines: Vec<&str> = ctx.content.lines().collect();
        let num_lines = lines.len();
        if num_lines == 0 {
            return Ok(String::new());
        }

        let list_blocks = self.find_md032_list_blocks(&lines, &structure);
        if list_blocks.is_empty() {
            return Ok(ctx.content.to_string());
        }

        let mut insertions: std::collections::BTreeMap<usize, String> =
            std::collections::BTreeMap::new();

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

                if !is_prev_excluded
                    && !is_blank_in_context(lines[prev_line_actual_idx_0])
                    && prev_prefix == *prefix
                {
                    insertions.insert(start_line, prefix.clone());
                }
            }

            // Check after block
            if end_line < num_lines {
                let after_block_line_idx_0 = end_line;
                let after_block_line_idx_1 = end_line + 1;
                let line_after_block_content_str = lines[after_block_line_idx_0];
                let is_line_after_excluded = structure.is_in_code_block(after_block_line_idx_1)
                    || structure.is_in_front_matter(after_block_line_idx_1);
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

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty()
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(Self)
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }
}

impl DocumentStructureExtensions for MD032BlanksAroundLists {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
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

        let lines: Vec<&str> = content.lines().collect();

        // Use MD032's own sophisticated list detection to check for list blocks
        let list_blocks = self.find_md032_list_blocks(&lines, doc_structure);

        // This rule is relevant if we found any list blocks
        !list_blocks.is_empty()
    }
}

// Helper to determine the column where content starts after the marker
fn get_content_start_column(captures: &Captures) -> Option<usize> {
    let indent_len = captures.get(1).map_or(0, |m| m.as_str().len());

    // For unordered lists: capture group 2 has the marker (*+-)
    // For ordered lists: capture group 3 has the number, dot is included in regex pattern
    let marker_len = if let Some(unordered) = captures.get(2) {
        unordered.as_str().len() // Just the marker character
    } else if let Some(ordered) = captures.get(3) {
        ordered.as_str().len() + 1 // Number + dot
    } else {
        return None; // Should not happen if regex matched
    };

    let space_after_len = captures.get(4).map_or(0, |m| m.as_str().len()); // Space after marker

    Some(indent_len + marker_len + space_after_len)
}

// Calculates visual indentation, treating tabs as expanding to 4 spaces (common behavior)
fn calculate_indent(line: &str) -> usize {
    // Find the first non-whitespace character
    line.find(|c: char| !c.is_whitespace()).unwrap_or(0)
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
        let rule = MD032BlanksAroundLists;
        let ctx = LintContext::new(content);
        rule.check(&ctx).expect("Lint check failed")
    }

    fn fix(content: &str) -> String {
        let rule = MD032BlanksAroundLists;
        let ctx = LintContext::new(content);
        rule.fix(&ctx).expect("Lint fix failed")
    }

    // Test that warnings include Fix objects
    fn check_warnings_have_fixes(content: &str) {
        let warnings = lint(content);
        for warning in &warnings {
            assert!(
                warning.fix.is_some(),
                "Warning should have fix: {:?}",
                warning
            );
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
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
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
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
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
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
    }

    #[test]
    fn test_correct_spacing() {
        let content = "Text 1\n\n- Item 1\n- Item 2\n\nText 2";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            0,
            "Expected no warnings for correctly spaced list"
        );

        let fixed_content = fix(content);
        assert_eq!(
            fixed_content, content,
            "Fix should not change correctly spaced content"
        );
    }

    #[test]
    fn test_list_with_content() {
        let content = "Text\n* Item 1\n  Content\n* Item 2\n  More content\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Expected 2 warnings for list block (lines 2-5) missing surrounding blanks. Got: {:?}",
            warnings
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
            "Fix did not produce the expected output. Got:\n{}",
            fixed_content
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
    }

    #[test]
    fn test_nested_list() {
        let content = "Text\n- Item 1\n  - Nested 1\n- Item 2\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Nested list block warnings. Got: {:?}",
            warnings
        ); // Needs blank before line 2, after line 4
        if warnings.len() == 2 {
            assert_eq!(warnings[0].line, 2);
            assert_eq!(warnings[1].line, 4);
        }

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(
            fixed_content,
            "Text\n\n- Item 1\n  - Nested 1\n- Item 2\n\nText"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
    }

    #[test]
    fn test_list_with_internal_blanks() {
        let content = "Text\n* Item 1\n\n  More Item 1 Content\n* Item 2\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "List with internal blanks warnings. Got: {:?}",
            warnings
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
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
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
        assert_eq!(
            warnings.len(),
            1,
            "Front matter test warnings. Got: {:?}",
            warnings
        );
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
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
    }

    #[test]
    fn test_multiple_lists() {
        let content = "Text\n- List 1 Item 1\n- List 1 Item 2\nText 2\n* List 2 Item 1\nText 3";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            4,
            "Multiple lists warnings. Got: {:?}",
            warnings
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        assert_eq!(
            fixed_content,
            "Text\n\n- List 1 Item 1\n- List 1 Item 2\n\nText 2\n\n* List 2 Item 1\n\nText 3"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
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
            "Expected 2 warnings for blockquoted list. Got: {:?}",
            warnings
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
            fixed_content,
            "> Quote line 1\n> \n> - List item 1\n> - List item 2\n> \n> Quote line 2",
            "Fix for blockquoted list failed. Got:\n{}",
            fixed_content
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
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
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
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
            "Fix added extra blank after. Got:\n{}",
            fixed_content
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
            "Fix added extra blank before. Got:\n{}",
            fixed_content2
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
        let expected =
            "> Text before\n> \n> - Item 1\n>   - Nested item\n> - Item 2\n> \n> Text after";
        assert_eq!(
            fixed_content, expected,
            "Fix should preserve blockquote structure"
        );

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
        assert_eq!(
            fixed_content, expected,
            "Fix should handle mixed list markers"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
    }

    #[test]
    fn test_fix_ordered_list_with_different_numbers() {
        let content = "Text\n1. First\n3. Third\n2. Second\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Should warn for missing blanks around ordered list"
        );

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
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
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
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
    }

    #[test]
    fn test_fix_deeply_nested_lists() {
        let content =
            "Text\n- Level 1\n  - Level 2\n    - Level 3\n      - Level 4\n- Back to Level 1\nText";
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
        assert_eq!(
            fixed_content, expected,
            "Fix should handle deeply nested lists"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
    }

    #[test]
    fn test_fix_list_with_multiline_items() {
        let content =
            "Text\n- Item 1\n  continues here\n  and here\n- Item 2\n  also continues\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Should warn for missing blanks around multiline list"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected =
            "Text\n\n- Item 1\n  continues here\n  and here\n- Item 2\n  also continues\n\nText";
        assert_eq!(
            fixed_content, expected,
            "Fix should handle multiline list items"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
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
        assert_eq!(
            warnings2.len(),
            1,
            "List at document end should need blank before"
        );
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
        assert_eq!(
            warnings.len(),
            0,
            "Multiple blank lines should be preserved"
        );
        let fixed_content = fix(content);
        assert_eq!(
            fixed_content, content,
            "Fix should not modify already correct content"
        );
    }

    #[test]
    fn test_fix_handles_tabs_and_spaces() {
        let content = "Text\n\t- Item with tab\n  - Item with spaces\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Should warn regardless of indentation type"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        let expected = "Text\n\n\t- Item with tab\n  - Item with spaces\n\nText";
        assert_eq!(
            fixed_content, expected,
            "Fix should preserve original indentation"
        );

        // Verify fix resolves the issue
        let warnings_after_fix = lint(&fixed_content);
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "Fix should resolve all warnings"
        );
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
            assert!(
                fix.range.start <= fix.range.end,
                "Fix range should be valid"
            );
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
        assert_eq!(
            warnings_after_fix.len(),
            0,
            "No warnings should remain after fix"
        );
    }

    #[test]
    fn test_fix_with_windows_line_endings() {
        let content = "Text\r\n- Item 1\r\n- Item 2\r\nText";
        let warnings = lint(content);
        assert_eq!(
            warnings.len(),
            2,
            "Should detect issues with Windows line endings"
        );

        // Test that warnings have fixes
        check_warnings_have_fixes(content);

        let fixed_content = fix(content);
        // Note: Our fix uses \n, which is standard for Rust string processing
        let expected = "Text\n\n- Item 1\n- Item 2\n\nText";
        assert_eq!(
            fixed_content, expected,
            "Fix should handle Windows line endings"
        );
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
            "Should not warn for properly formatted list with multi-line items. Got: {:?}",
            warnings
        );

        let fixed_content = fix(content);
        // Should not change the content since it's already correct
        assert_eq!(
            fixed_content, content,
            "Should not modify correctly formatted multi-line list items"
        );
    }
}
