use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use lazy_static::lazy_static;
use regex::Regex;

/// Rule MD006: Consider starting bulleted lists at the leftmost column
///
/// See [docs/md006.md](../../docs/md006.md) for full documentation, configuration, and examples.
///
/// In standard Markdown:
/// - Top-level bullet items should start at column 0 (no indentation)
/// - Nested bullet items should be indented under their parent
/// - A bullet item following non-list content should start a new list at column 0
#[derive(Clone)]
pub struct MD006StartBullets;

lazy_static! {
    // Pattern to match bullet list items: captures indentation, marker, and space after marker
    static ref BULLET_PATTERN: Regex = Regex::new(r"^(\s*)([*+-])(\s+)").unwrap();

    // Pattern to match code fence markers
    static ref CODE_FENCE_PATTERN: Regex = Regex::new(r"^(\s*)(```|~~~)").unwrap();
}

impl MD006StartBullets {
    /// Optimized check using centralized list blocks
    fn check_optimized(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let line_index = LineIndex::new(content.to_string());
        let mut result = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Track which lines contain valid bullet items
        let mut valid_bullet_lines = vec![false; lines.len()];

        // Process each unordered list block
        for list_block in &ctx.list_blocks {
            // Skip ordered lists
            if list_block.is_ordered {
                continue;
            }

            // Check each list item in this block
            for &item_line in &list_block.item_lines {
                if let Some(line_info) = ctx.line_info(item_line) {
                    if let Some(list_item) = &line_info.list_item {
                        let line_idx = item_line - 1;
                        let indent = list_item.marker_column;
                        let line = &lines[line_idx];

                        let mut is_valid = false;

                        if indent == 0 {
                            // Top-level items are always valid
                            is_valid = true;
                        } else {
                            // Check if this is a valid nested item
                            match Self::find_relevant_previous_bullet(&lines, line_idx) {
                                Some((prev_idx, prev_indent)) => {
                                    match prev_indent.cmp(&indent) {
                                        std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
                                            // Valid nesting or sibling if previous item was valid
                                            is_valid = valid_bullet_lines[prev_idx];
                                        }
                                        std::cmp::Ordering::Greater => {
                                            // remains invalid
                                        }
                                    }
                                }
                                None => {
                                    // Indented item with no previous bullet remains invalid
                                }
                            }
                        }

                        valid_bullet_lines[line_idx] = is_valid;

                        if !is_valid {
                            // Calculate the precise range for the indentation that needs to be removed
                            let start_col = 1;
                            let end_col = indent + 3; // Include marker and space after it

                            // For the fix, we need to replace the highlighted part with just the bullet marker
                            let trimmed = line.trim_start();
                            let bullet_part = if let Some(captures) = BULLET_PATTERN.captures(trimmed) {
                                let marker = captures.get(2).map_or("*", |m| m.as_str());
                                format!("{marker} ")
                            } else {
                                "* ".to_string()
                            };

                            // Calculate the byte range for the fix
                            let fix_range = line_index.line_col_to_byte_range_with_length(
                                item_line,
                                start_col,
                                end_col - start_col,
                            );

                            result.push(LintWarning {
                                line: item_line,
                                column: start_col,
                                end_line: item_line,
                                end_column: end_col,
                                message: format!(
                                    "Consider starting bulleted lists at the beginning of the line (found {indent} leading spaces)"
                                ),
                                severity: Severity::Warning,
                                rule_name: Some(self.name()),
                                fix: Some(Fix {
                                    range: fix_range,
                                    replacement: bullet_part,
                                }),
                            });
                        }
                    }
                }
            }
        }

        Ok(result)
    }
    /// Checks if a line is a bullet list item and returns its indentation level
    fn is_bullet_list_item(line: &str) -> Option<usize> {
        if let Some(captures) = BULLET_PATTERN.captures(line) {
            if let Some(indent) = captures.get(1) {
                return Some(indent.as_str().len());
            }
        }
        None
    }

    /// Checks if a line is blank (empty or whitespace only)
    fn is_blank_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    /// Find the most relevant previous bullet item for nesting validation
    fn find_relevant_previous_bullet(lines: &[&str], line_idx: usize) -> Option<(usize, usize)> {
        let current_indent = Self::is_bullet_list_item(lines[line_idx])?;

        let mut i = line_idx;

        while i > 0 {
            i -= 1;
            if Self::is_blank_line(lines[i]) {
                continue;
            }
            if let Some(prev_indent) = Self::is_bullet_list_item(lines[i]) {
                if prev_indent <= current_indent {
                    // Found a potential parent or sibling
                    // Check if there's any non-list content between this potential parent and current item
                    let mut has_breaking_content = false;
                    for check_line in &lines[(i + 1)..line_idx] {
                        if Self::is_blank_line(check_line) {
                            continue;
                        }
                        if Self::is_bullet_list_item(check_line).is_none() {
                            // Found non-list content - check if it breaks the list structure
                            let content_indent = check_line.len() - check_line.trim_start().len();

                            // Content is acceptable if:
                            // 1. It's indented at least as much as the current item (continuation of parent)
                            // 2. OR it's indented more than the previous bullet (continuation of previous item)
                            // 3. AND we have a true parent relationship (prev_indent < current_indent)
                            let is_continuation = content_indent >= prev_indent.max(2); // At least 2 spaces for continuation
                            let is_valid_nesting = prev_indent < current_indent;

                            if !is_continuation || !is_valid_nesting {
                                has_breaking_content = true;
                                break;
                            }
                        }
                    }

                    if !has_breaking_content {
                        return Some((i, prev_indent));
                    } else {
                        // Content breaks the list structure, but continue searching for an earlier valid parent
                        continue;
                    }
                }
                // If prev_indent > current_indent, it's a child of a sibling, ignore it and keep searching.
            } else {
                // Found non-list content - check if it's a continuation line
                let content_indent = lines[i].len() - lines[i].trim_start().len();
                // If it's indented enough to be a continuation, don't break the search
                if content_indent >= 2 {
                    continue;
                }
                // Otherwise, this breaks the search
                return None;
            }
        }
        None
    }
}

impl Rule for MD006StartBullets {
    fn name(&self) -> &'static str {
        "MD006"
    }

    fn description(&self) -> &'static str {
        "Consider starting bulleted lists at the beginning of the line"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early returns for performance
        if content.is_empty() || ctx.list_blocks.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any list markers before processing
        if !content.contains('*') && !content.contains('-') && !content.contains('+') {
            return Ok(Vec::new());
        }

        // Use centralized list blocks for better performance and consistency
        self.check_optimized(ctx)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();

        let mut fixed_lines: Vec<String> = Vec::with_capacity(lines.len());

        // Create a map of line numbers to replacements

        let mut line_replacements = std::collections::HashMap::new();
        for warning in warnings {
            if let Some(fix) = warning.fix {
                // Line number is 1-based in warnings but we need 0-based for indexing
                let line_idx = warning.line - 1;
                line_replacements.insert(line_idx, fix.replacement);
            }
        }

        // Apply replacements line by line

        let mut i = 0;
        while i < lines.len() {
            if let Some(_replacement) = line_replacements.get(&i) {
                let prev_line_is_blank = i > 0 && Self::is_blank_line(lines[i - 1]);
                let prev_line_is_list = i > 0 && Self::is_bullet_list_item(lines[i - 1]).is_some();
                // Only insert a blank line if previous line is not blank and not a list
                if !prev_line_is_blank && !prev_line_is_list && i > 0 {
                    fixed_lines.push(String::new());
                }
                // The replacement is the fixed line (unindented list item)
                // Use the original line, trimmed of leading whitespace
                let fixed_line = lines[i].trim_start();
                fixed_lines.push(fixed_line.to_string());
            } else {
                fixed_lines.push(lines[i].to_string());
            }
            i += 1;
        }

        // Join the lines with newlines

        let result = fixed_lines.join("\n");
        if content.ends_with('\n') {
            Ok(result + "\n")
        } else {
            Ok(result)
        }
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> LintResult {
        let content = _ctx.content;
        if doc_structure.list_lines.is_empty() {
            return Ok(Vec::new());
        }
        if !content.contains('*') && !content.contains('-') && !content.contains('+') {
            return Ok(Vec::new());
        }
        let line_index = LineIndex::new(content.to_string());
        let mut result = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut valid_bullet_lines = vec![false; lines.len()];
        for &line_num in &doc_structure.list_lines {
            let line_idx = line_num - 1;
            if line_idx >= lines.len() {
                continue;
            }
            let line = lines[line_idx];
            if doc_structure.is_in_code_block(line_num) {
                continue;
            }
            if let Some(indent) = Self::is_bullet_list_item(line) {
                let mut is_valid = false; // Assume invalid initially
                if indent == 0 {
                    is_valid = true;
                } else {
                    match Self::find_relevant_previous_bullet(&lines, line_idx) {
                        Some((prev_idx, prev_indent)) => {
                            match prev_indent.cmp(&indent) {
                                std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
                                    // Valid nesting or sibling if previous item was valid
                                    is_valid = valid_bullet_lines[prev_idx];
                                }
                                std::cmp::Ordering::Greater => {
                                    // remains invalid
                                }
                            }
                        }
                        None => {
                            // Indented item with no previous bullet remains invalid
                        }
                    }
                }
                valid_bullet_lines[line_idx] = is_valid;

                if !is_valid {
                    // Calculate the precise range for the indentation that needs to be removed
                    // For "  * Indented bullet", we want to highlight the indentation, marker, and space after marker "  * " (columns 1-4)
                    let start_col = 1; // Start from beginning of line
                    let end_col = indent + 3; // Include marker and space after it (indent + 1 for marker + 1 for space + 1 for inclusive range)

                    // For the fix, we need to replace the highlighted part ("  *") with just the bullet marker ("* ")
                    let line = lines[line_idx];
                    let trimmed = line.trim_start();
                    // Extract just the bullet marker and normalize to single space
                    let bullet_part = if let Some(captures) = BULLET_PATTERN.captures(trimmed) {
                        format!("{} ", captures.get(2).unwrap().as_str()) // Always use single space
                    } else {
                        "* ".to_string() // fallback
                    };
                    let replacement = bullet_part;

                    result.push(LintWarning {
                        rule_name: Some(self.name()),
                        severity: Severity::Warning,
                        line: line_num,
                        column: start_col,
                        end_line: line_num,
                        end_column: end_col,
                        message: "List item indentation".to_string(),
                        fix: Some(Fix {
                            range: {
                                let start_byte = line_index.line_col_to_byte_range(line_num, start_col).start;
                                let end_byte = line_index.line_col_to_byte_range(line_num, end_col).start;
                                start_byte..end_byte
                            },
                            replacement,
                        }),
                    });
                }
            }
        }
        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || (!content.contains('*') && !content.contains('-') && !content.contains('+'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        None
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD006StartBullets)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        None
    }
}

impl DocumentStructureExtensions for MD006StartBullets {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
        // This rule is only relevant if there are unordered list items
        ctx.list_blocks.iter().any(|block| !block.is_ordered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        let rule = MD006StartBullets;

        // Test with properly formatted lists
        let content_valid = "* Item 1\n* Item 2\n  * Nested item\n  * Another nested item";
        let structure_valid = DocumentStructure::new(content_valid);
        let ctx_valid = crate::lint_context::LintContext::new(content_valid);
        let result_valid = rule.check_with_structure(&ctx_valid, &structure_valid).unwrap();
        assert!(
            result_valid.is_empty(),
            "Properly formatted lists should not generate warnings, found: {result_valid:?}"
        );

        // Test with improperly indented list - adjust expectations based on actual implementation
        let content_invalid = "  * Item 1\n  * Item 2\n    * Nested item";
        let structure = DocumentStructure::new(content_invalid);
        let ctx_invalid = crate::lint_context::LintContext::new(content_invalid);
        let result = rule.check_with_structure(&ctx_invalid, &structure).unwrap();

        // If no warnings are generated, the test should be updated to match implementation behavior
        assert!(!result.is_empty(), "Improperly indented lists should generate warnings");
        assert_eq!(
            result.len(),
            2,
            "Should generate warnings for both improperly indented top-level items"
        );

        // Test with mixed indentation - standard nesting is VALID
        let content = "* Item 1\n  * Item 2 (standard nesting is valid)";
        let structure = DocumentStructure::new(content);
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        // Assert that standard nesting does NOT generate warnings
        assert!(
            result.is_empty(),
            "Standard nesting (* Item ->   * Item) should NOT generate warnings, found: {result:?}"
        );
    }
}
