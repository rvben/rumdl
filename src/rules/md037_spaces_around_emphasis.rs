/// Rule MD037: No spaces around emphasis markers
///
/// See [docs/md037.md](../../docs/md037.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::regex_cache::*;
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Front matter detection
    static ref FRONT_MATTER_DELIM: Regex = Regex::new(r"^---\s*$").unwrap();

    // Detection patterns for valid content that should not be treated as emphasis errors
    static ref UNBALANCED_ASTERISK: Regex = Regex::new(r"\*([^*]+)$|^([^*]*)\*").unwrap();
    static ref UNBALANCED_DOUBLE_ASTERISK: Regex = Regex::new(r"\*\*([^*]+)$|^([^*]*)\*\*").unwrap();
    static ref UNBALANCED_UNDERSCORE: Regex = Regex::new(r"_([^_]+)$|^([^_]*)_").unwrap();
    static ref UNBALANCED_DOUBLE_UNDERSCORE: Regex = Regex::new(r"__([^_]+)$|^([^_]*)__").unwrap();

    // Better detection of inline code with support for multiple backticks
    static ref INLINE_CODE: Regex = Regex::new(r"(`+)([^`]|[^`].*?[^`])(`+)").unwrap();

    // List markers pattern - used to avoid confusion with emphasis
    static ref LIST_MARKER: Regex = Regex::new(r"^\s*[*+-]\s+").unwrap();

    // Valid emphasis at start of line that should not be treated as lists
    static ref VALID_START_EMPHASIS: Regex = Regex::new(r"^(\*\*[^*\s]|\*[^*\s]|__[^_\s]|_[^_\s])").unwrap();

    // Documentation style patterns
    static ref DOC_METADATA_PATTERN: Regex = Regex::new(r"^\s*\*?\s*\*\*[^*]+\*\*\s*:").unwrap();

    // Bold text pattern (for preserving bold text in documentation)
    static ref BOLD_TEXT_PATTERN: Regex = Regex::new(r"\*\*[^*]+\*\*").unwrap();

    // Multi-line emphasis detection (for potential future use)
    static ref MULTI_LINE_EMPHASIS_START: Regex = Regex::new(r"(\*\*|\*|__|_)([^*_\s].*?)$").unwrap();
    static ref MULTI_LINE_EMPHASIS_END: Regex = Regex::new(r"^(.*?)(\*\*|\*|__|_)").unwrap();
}


// Enhanced inline code replacement to handle nested backticks
fn replace_inline_code(line: &str) -> String {
    let mut result = line.to_string();
    let mut offset = 0;

    for cap in INLINE_CODE.captures_iter(line) {
        if let (Some(full_match), Some(_opening), Some(_content), Some(_closing)) =
            (cap.get(0), cap.get(1), cap.get(2), cap.get(3))
        {
            let match_start = full_match.start();
            let match_end = full_match.end();
            let placeholder = " ".repeat(match_end - match_start);

            result.replace_range(match_start + offset..match_end + offset, &placeholder);
            offset += placeholder.len() - (match_end - match_start);
        }
    }

    result
}

/// Rule MD037: Spaces inside emphasis markers
#[derive(Clone)]
pub struct MD037NoSpaceInEmphasis;

impl Rule for MD037NoSpaceInEmphasis {
    fn name(&self) -> &'static str {
        "MD037"
    }

    fn description(&self) -> &'static str {
        "Spaces inside emphasis markers"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _timer = crate::profiling::ScopedTimer::new("MD037_check");

        // Early return: if no emphasis markers at all, skip processing
        if !has_emphasis_markers(content) {
            return Ok(vec![]);
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    /// Enhanced function to check for spaces inside emphasis markers
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let _timer = crate::profiling::ScopedTimer::new("MD037_check_with_structure");

        let content = ctx.content;

        // Early return if the content is empty or has no emphasis characters
        if content.is_empty() || (!content.contains('*') && !content.contains('_')) {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();

        // Process the content line by line using the document structure
        for (line_num, line) in content.lines().enumerate() {
            // Skip if in code block or front matter
            if structure.is_in_code_block(line_num + 1)
                || structure.is_in_front_matter(line_num + 1)
            {
                continue;
            }

            // Skip if the line doesn't contain any emphasis markers
            if !line.contains('*') && !line.contains('_') {
                continue;
            }

            // Replace inline code with placeholders to avoid false positives
            let line_no_code = replace_inline_code(line);

            // Use the correct emphasis pattern checking logic
            check_emphasis_patterns(&line_no_code, line_num + 1, line, &mut warnings);
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _timer = crate::profiling::ScopedTimer::new("MD037_fix");

        // First check for issues and get all warnings with fixes
        let warnings = match self.check(ctx) {
            Ok(warnings) => warnings,
            Err(e) => return Err(e),
        };

        // If no warnings, return original content
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        // Get all line positions to make it easier to apply fixes by warning
        let mut line_positions = Vec::new();
        let mut pos = 0;
        for line in content.lines() {
            line_positions.push(pos);
            pos += line.len() + 1; // +1 for the newline
        }

        // Apply fixes
        let mut result = content.to_string();
        let mut offset: isize = 0;

        // Sort warnings by position to apply fixes in the correct order
        let mut sorted_warnings: Vec<_> = warnings.iter().filter(|w| w.fix.is_some()).collect();
        sorted_warnings.sort_by_key(|w| (w.line, w.column));

        for warning in sorted_warnings {
            if let Some(fix) = &warning.fix {
                // Calculate the absolute position in the file
                let line_start = line_positions.get(warning.line - 1).copied().unwrap_or(0);
                let abs_start = line_start + warning.column - 1;
                let abs_end = abs_start + (fix.range.end - fix.range.start);

                // Apply fix with offset adjustment
                let actual_start = (abs_start as isize + offset) as usize;
                let actual_end = (abs_end as isize + offset) as usize;

                // Make sure we're not out of bounds
                if actual_start < result.len() && actual_end <= result.len() {
                    // Replace the text
                    result.replace_range(actual_start..actual_end, &fix.replacement);
                    // Update offset for future replacements
                    offset +=
                        fix.replacement.len() as isize - (fix.range.end - fix.range.start) as isize;
                }
            }
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Emphasis
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || (!content.contains('*') && !content.contains('_'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD037NoSpaceInEmphasis)
    }
}

impl DocumentStructureExtensions for MD037NoSpaceInEmphasis {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
        let content = ctx.content;
        !content.is_empty() && (content.contains('*') || content.contains('_'))
    }
}

// Add this function to check fancy-regex patterns
fn check_fancy_pattern(
    line: &str,
    line_num: usize,
    pattern: &FancyRegex,
    warnings: &mut Vec<LintWarning>,
    rule_name: &'static str,
) {
    // find_iter returns Matches directly, not a Result
    let matches = pattern.find_iter(line);

    for full_match in matches.flatten() {
        let start = full_match.start();
        let end = full_match.end();
        let match_text = &line[start..end];

        // Determine if this is asterisk or underscore emphasis
        let (marker_type, is_double) = if match_text.contains('*') {
            ('*', match_text.contains("**"))
        } else {
            ('_', match_text.contains("__"))
        };

        let marker = if is_double {
            &format!("{}{}", marker_type, marker_type)
        } else {
            &format!("{}", marker_type)
        };

        // Extract the content without spaces
        let content = match_text
            .trim_start_matches(marker)
            .trim_end_matches(marker)
            .trim();

        // Create the fixed version
        let fixed_text = format!("{}{}{}", marker, content, marker);

        let warning = LintWarning {
            rule_name: Some(rule_name),
            message: format!("Spaces inside emphasis markers: '{}'", match_text),
            line: line_num,
            column: start + 1, // +1 because columns are 1-indexed
            severity: Severity::Warning,
            fix: Some(Fix {
                range: start..end,
                replacement: fixed_text,
            }),
        };

        warnings.push(warning);
    }
}

// Check for spaces inside emphasis markers with enhanced handling
fn check_emphasis_patterns(
    line: &str,
    line_num: usize,
    _original_line: &str,
    warnings: &mut Vec<LintWarning>,
) {
    // Instance of the rule to call the check_pattern method
    let rule = MD037NoSpaceInEmphasis;

    // Skip documentation patterns
    let trimmed = line.trim_start();
    if (trimmed.starts_with("* *") && line.contains("*:"))
        || (trimmed.starts_with("* **") && line.contains("**:"))
        || DOC_METADATA_PATTERN.is_match(line)
        || BOLD_TEXT_PATTERN.is_match(line)
    {
        return;
    }

    // Special handling for list items - only check content after list marker
    if LIST_MARKER.is_match(line) {
        if let Some(caps) = LIST_MARKER.captures(line) {
            if let Some(full_match) = caps.get(0) {
                let list_marker_end = full_match.end();
                if list_marker_end < line.len() {
                    // Process the content after the list marker
                    let list_content = &line[list_marker_end..];
                    if list_content.contains('*') {
                        // Adjust column positions to account for list_marker_end
                        let mut list_warnings = Vec::new();
                        rule.check_pattern(
                            list_content,
                            line_num,
                            &ASTERISK_EMPHASIS,
                            &mut list_warnings,
                        );

                        // Add list_marker_end to column positions
                        for warning in list_warnings {
                            let mut adjusted_warning = warning;
                            adjusted_warning.column += list_marker_end;
                            if let Some(fix) = adjusted_warning.fix {
                                adjusted_warning.fix = Some(Fix {
                                    range: (fix.range.start + list_marker_end)
                                        ..(fix.range.end + list_marker_end),
                                    replacement: fix.replacement,
                                });
                            }
                            warnings.push(adjusted_warning);
                        }

                        // Check double asterisk with fancy-regex
                        if list_content.contains("**") {
                            let mut fancy_warnings = Vec::new();
                            check_fancy_pattern(
                                list_content,
                                line_num,
                                &DOUBLE_ASTERISK_EMPHASIS,
                                &mut fancy_warnings,
                                rule.name(),
                            );
                            check_fancy_pattern(
                                list_content,
                                line_num,
                                &DOUBLE_ASTERISK_SPACE_START,
                                &mut fancy_warnings,
                                rule.name(),
                            );
                            check_fancy_pattern(
                                list_content,
                                line_num,
                                &DOUBLE_ASTERISK_SPACE_END,
                                &mut fancy_warnings,
                                rule.name(),
                            );

                            // Add list_marker_end to column positions
                            for warning in fancy_warnings {
                                let mut adjusted_warning = warning;
                                adjusted_warning.column += list_marker_end;
                                if let Some(fix) = adjusted_warning.fix {
                                    adjusted_warning.fix = Some(Fix {
                                        range: (fix.range.start + list_marker_end)
                                            ..(fix.range.end + list_marker_end),
                                        replacement: fix.replacement,
                                    });
                                }
                                warnings.push(adjusted_warning);
                            }
                        }
                    }
                    if list_content.contains('_') {
                        // Adjust column positions for underscores too
                        let mut underscore_warnings = Vec::new();
                        rule.check_pattern(
                            list_content,
                            line_num,
                            &UNDERSCORE_EMPHASIS,
                            &mut underscore_warnings,
                        );
                        rule.check_pattern(
                            list_content,
                            line_num,
                            &DOUBLE_UNDERSCORE_EMPHASIS,
                            &mut underscore_warnings,
                        );

                        // Add list_marker_end to column positions
                        for warning in underscore_warnings {
                            let mut adjusted_warning = warning;
                            adjusted_warning.column += list_marker_end;
                            if let Some(fix) = adjusted_warning.fix {
                                adjusted_warning.fix = Some(Fix {
                                    range: (fix.range.start + list_marker_end)
                                        ..(fix.range.end + list_marker_end),
                                    replacement: fix.replacement,
                                });
                            }
                            warnings.push(adjusted_warning);
                        }
                    }
                }
            }
        }
        return;
    }

    // Check for double asterisk emphasis using fancy-regex
    if line.contains("**") {
        check_fancy_pattern(
            line,
            line_num,
            &DOUBLE_ASTERISK_EMPHASIS,
            warnings,
            rule.name(),
        );
        check_fancy_pattern(
            line,
            line_num,
            &DOUBLE_ASTERISK_SPACE_START,
            warnings,
            rule.name(),
        );
        check_fancy_pattern(
            line,
            line_num,
            &DOUBLE_ASTERISK_SPACE_END,
            warnings,
            rule.name(),
        );
    }

    // Skip valid emphasis at the start of a line
    if VALID_START_EMPHASIS.is_match(line) {
        // Still check the rest of the line for emphasis issues
        if let Some(emphasis_start) = line.find(' ') {
            let rest_of_line = &line[emphasis_start..];
            if rest_of_line.contains('*') {
                rule.check_pattern(rest_of_line, line_num, &ASTERISK_EMPHASIS, warnings);

                // Check double asterisk with fancy-regex
                if rest_of_line.contains("**") {
                    check_fancy_pattern(
                        rest_of_line,
                        line_num,
                        &DOUBLE_ASTERISK_EMPHASIS,
                        warnings,
                        rule.name(),
                    );
                    check_fancy_pattern(
                        rest_of_line,
                        line_num,
                        &DOUBLE_ASTERISK_SPACE_START,
                        warnings,
                        rule.name(),
                    );
                    check_fancy_pattern(
                        rest_of_line,
                        line_num,
                        &DOUBLE_ASTERISK_SPACE_END,
                        warnings,
                        rule.name(),
                    );
                }
            }
            if rest_of_line.contains('_') {
                rule.check_pattern(rest_of_line, line_num, &UNDERSCORE_EMPHASIS, warnings);
                rule.check_pattern(
                    rest_of_line,
                    line_num,
                    &DOUBLE_UNDERSCORE_EMPHASIS,
                    warnings,
                );
            }
        }
        return;
    }

    // Check emphasis patterns based on marker type
    if line.contains('*') {
        rule.check_pattern(line, line_num, &ASTERISK_EMPHASIS, warnings);
    }

    if line.contains('_') {
        rule.check_pattern(line, line_num, &UNDERSCORE_EMPHASIS, warnings);
        rule.check_pattern(line, line_num, &DOUBLE_UNDERSCORE_EMPHASIS, warnings);
    }
}

impl MD037NoSpaceInEmphasis {
    // Check a specific emphasis pattern and add warnings
    fn check_pattern(
        &self,
        line: &str,
        line_num: usize,
        pattern: &Regex,
        warnings: &mut Vec<LintWarning>,
    ) {
        for caps in pattern.captures_iter(line) {
            let full_match = caps.get(0).unwrap();
            let start_pos = full_match.start();
            let end_pos = full_match.end();
            let match_text = full_match.as_str();

            // Determine emphasis marker type (* or _) and if it's double
            let (marker, _is_double) = if match_text.contains('*') {
                if match_text.contains("**") {
                    ("**", true)
                } else {
                    ("*", false)
                }
            } else if match_text.contains("__") {
                ("__", true)
            } else {
                ("_", false)
            };

            // Extract the content without spaces
            let content = match_text
                .trim_start_matches(marker)
                .trim_end_matches(marker)
                .trim();

            // Create the fixed version
            let fixed_text = format!("{}{}{}", marker, content, marker);

            let warning = LintWarning {
                rule_name: Some(self.name()),
                message: format!("Spaces inside emphasis markers: {:?}", match_text),
                line: line_num,
                column: start_pos + 1, // +1 because columns are 1-indexed
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: start_pos..end_pos,
                    replacement: fixed_text,
                }),
            };

            warnings.push(warning);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::utils::document_structure::DocumentStructure;

    #[test]
    fn test_with_document_structure() {
        let rule = MD037NoSpaceInEmphasis;

        // Test with no spaces inside emphasis - should pass
        let content = "This is *correct* emphasis and **strong emphasis**";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(result.is_empty(), "No warnings expected for correct emphasis");

        // Test with actual spaces inside emphasis - use content that should warn
        let content = "This is * text with spaces * and more content";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(!result.is_empty(), "Expected warnings for spaces in emphasis");

        // Test with code blocks - emphasis in code should be ignored
        let content = "This is *correct* emphasis\n```\n* incorrect * in code block\n```\nOutside block with * spaces in emphasis *";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(!result.is_empty(), "Expected warnings for spaces in emphasis outside code block");
    }
}
