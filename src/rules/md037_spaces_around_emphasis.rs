/// Rule MD037: No spaces around emphasis markers
///
/// See [docs/md037.md](../../docs/md037.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Front matter detection
    static ref FRONT_MATTER_DELIM: Regex = Regex::new(r"^---\s*$").unwrap();

    // Better detection of inline code with support for multiple backticks
    static ref INLINE_CODE: Regex = Regex::new(r"(`+)([^`]|[^`].*?[^`])(`+)").unwrap();

    // List markers pattern - used to avoid confusion with emphasis
    static ref LIST_MARKER: Regex = Regex::new(r"^\s*[*+-]\s+").unwrap();

    // Valid emphasis at start of line that should not be treated as lists
    static ref VALID_START_EMPHASIS: Regex = Regex::new(r"^(\*\*[^*\s]|\*[^*\s]|__[^_\s]|_[^_\s])").unwrap();

    // Documentation style patterns
    static ref DOC_METADATA_PATTERN: Regex = Regex::new(r"^\s*\*?\s*\*\*[^*]+\*\*\s*:").unwrap();

    // Bold text pattern (for preserving bold text in documentation) - only match valid bold without spaces
    static ref BOLD_TEXT_PATTERN: Regex = Regex::new(r"\*\*[^*\s][^*]*[^*\s]\*\*|\*\*[^*\s]\*\*").unwrap();

    // Pre-compiled patterns for quick checks
    static ref QUICK_DOC_CHECK: Regex = Regex::new(r"^\s*\*\s+\*").unwrap();
    static ref QUICK_BOLD_CHECK: Regex = Regex::new(r"\*\*[^*\s]").unwrap();
}

/// Enhanced inline code replacement with optimized performance
#[inline]
fn replace_inline_code(line: &str) -> String {
    // Quick check: if no backticks, return original
    if !line.contains('`') {
        return line.to_string();
    }

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

/// Represents an emphasis marker found in text (optimized with smaller size)
#[derive(Debug, Clone, PartialEq)]
struct EmphasisMarker {
    marker_type: u8,    // b'*' or b'_' for faster comparison
    count: u8,          // 1 for single, 2 for double (u8 is sufficient)
    start_pos: usize,   // Position in the line
}

impl EmphasisMarker {
    #[inline]
    fn end_pos(&self) -> usize {
        self.start_pos + self.count as usize
    }

    #[inline]
    fn as_char(&self) -> char {
        self.marker_type as char
    }
}

/// Represents a complete emphasis span (optimized structure)
#[derive(Debug, Clone)]
struct EmphasisSpan {
    opening: EmphasisMarker,
    closing: EmphasisMarker,
    content: String,
    has_leading_space: bool,
    has_trailing_space: bool,
}

/// Optimized emphasis marker parsing using byte iteration
#[inline]
fn find_emphasis_markers(line: &str) -> Vec<EmphasisMarker> {
    // Early return for lines without emphasis markers
    if !line.contains('*') && !line.contains('_') {
        return Vec::new();
    }

    let mut markers = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'*' || byte == b'_' {
            let start_pos = i;
            let mut count = 1u8;

            // Count consecutive markers (limit to avoid overflow)
            while i + (count as usize) < bytes.len()
                && bytes[i + (count as usize)] == byte
                && count < 3
            {
                count += 1;
            }

            // Only consider single (*) and double (**) markers
            if count == 1 || count == 2 {
                markers.push(EmphasisMarker {
                    marker_type: byte,
                    count,
                    start_pos,
                });
            }

            i += count as usize;
        } else {
            i += 1;
        }
    }

    markers
}

/// Optimized emphasis span finding with reduced complexity
fn find_emphasis_spans(line: &str, markers: Vec<EmphasisMarker>) -> Vec<EmphasisSpan> {
    // Early return for insufficient markers
    if markers.len() < 2 {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut used_markers = vec![false; markers.len()];

    // Process markers in pairs more efficiently
    for i in 0..markers.len() {
        if used_markers[i] {
            continue;
        }

        let opening = &markers[i];

        // Look for the nearest matching closing marker using optimized search
        for j in (i + 1)..markers.len() {
            if used_markers[j] {
                continue;
            }

            let closing = &markers[j];

            // Quick type and count check
            if closing.marker_type == opening.marker_type && closing.count == opening.count {
                let content_start = opening.end_pos();
                let content_end = closing.start_pos;

                if content_end > content_start {
                    let content = &line[content_start..content_end];

                    // Optimized validation checks
                    if is_valid_emphasis_content_fast(content)
                        && is_valid_emphasis_span_fast(line, opening, closing)
                    {
                        // Quick check for crossing markers
                        let crosses_markers = markers[i+1..j].iter().any(|marker| {
                            marker.marker_type == opening.marker_type
                        });

                        if !crosses_markers {
                            let has_leading_space = content.starts_with(' ') || content.starts_with('\t');
                            let has_trailing_space = content.ends_with(' ') || content.ends_with('\t');

                            spans.push(EmphasisSpan {
                                opening: opening.clone(),
                                closing: closing.clone(),
                                content: content.to_string(),
                                has_leading_space,
                                has_trailing_space,
                            });

                            // Mark both markers as used
                            used_markers[i] = true;
                            used_markers[j] = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    spans
}

/// Fast validation of emphasis span context
#[inline]
fn is_valid_emphasis_span_fast(line: &str, opening: &EmphasisMarker, closing: &EmphasisMarker) -> bool {
    let content_start = opening.end_pos();
    let content_end = closing.start_pos;

    // Content must exist and not be just whitespace
    if content_end <= content_start {
        return false;
    }

    let content = &line[content_start..content_end];
    if content.trim().is_empty() {
        return false;
    }

    // Quick boundary checks using byte indexing
    let bytes = line.as_bytes();

    // Opening should be at start or after valid character
    let valid_opening = opening.start_pos == 0
        || matches!(bytes.get(opening.start_pos.saturating_sub(1)),
                   Some(&b' ') | Some(&b'\t') | Some(&b'(') | Some(&b'[') | Some(&b'{') | Some(&b'"') | Some(&b'\'') | Some(&b'>'));

    // Closing should be at end or before valid character
    let valid_closing = closing.end_pos() >= bytes.len()
        || matches!(bytes.get(closing.end_pos()),
                   Some(&b' ') | Some(&b'\t') | Some(&b')') | Some(&b']') | Some(&b'}') | Some(&b'"') | Some(&b'\'') | Some(&b'.') | Some(&b',') | Some(&b'!') | Some(&b'?') | Some(&b';') | Some(&b':') | Some(&b'<'));

    valid_opening && valid_closing && !content.contains('\n')
}

/// Fast validation of emphasis content
#[inline]
fn is_valid_emphasis_content_fast(content: &str) -> bool {
    !content.trim().is_empty()
}

/// Check if an emphasis span has spacing issues that should be flagged
#[inline]
fn has_spacing_issues(span: &EmphasisSpan) -> bool {
    span.has_leading_space || span.has_trailing_space
}

/// Rule MD037: Spaces inside emphasis markers
#[derive(Clone)]
pub struct MD037NoSpaceInEmphasis;

impl Default for MD037NoSpaceInEmphasis {
    fn default() -> Self {
        Self
    }
}

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
        if !content.contains('*') && !content.contains('_') {
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

            // Use the optimized emphasis parsing logic
            self.check_line_for_emphasis_issues_fast(&line_no_code, line_num + 1, &mut warnings);
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _timer = crate::profiling::ScopedTimer::new("MD037_fix");

        // Fast path: if no emphasis markers, return unchanged
        if !content.contains('*') && !content.contains('_') {
            return Ok(content.to_string());
        }

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
        content.contains('*') || content.contains('_')
    }
}

impl MD037NoSpaceInEmphasis {
    /// Optimized line checking for emphasis spacing issues
    #[inline]
    fn check_line_for_emphasis_issues_fast(
        &self,
        line: &str,
        line_num: usize,
        warnings: &mut Vec<LintWarning>,
    ) {
        // Quick documentation pattern checks using pre-compiled regex
        if QUICK_DOC_CHECK.is_match(line) || QUICK_BOLD_CHECK.is_match(line) {
            if DOC_METADATA_PATTERN.is_match(line) || BOLD_TEXT_PATTERN.is_match(line) {
                return;
            }
        }

        // Optimized list detection with fast path
        if line.starts_with(' ') || line.starts_with('*') || line.starts_with('+') || line.starts_with('-') {
            if LIST_MARKER.is_match(line) {
                if let Some(caps) = LIST_MARKER.captures(line) {
                    if let Some(full_match) = caps.get(0) {
                        let list_marker_end = full_match.end();
                        if list_marker_end < line.len() {
                            let remaining_content = &line[list_marker_end..];

                            if self.is_likely_list_item_fast(remaining_content) {
                                self.check_line_content_for_emphasis_fast(remaining_content, line_num, list_marker_end, warnings);
                            } else {
                                self.check_line_content_for_emphasis_fast(line, line_num, 0, warnings);
                            }
                        }
                    }
                }
                return;
            }
        }

        // Check the entire line
        self.check_line_content_for_emphasis_fast(line, line_num, 0, warnings);
    }

    /// Fast list item detection with optimized logic
    #[inline]
    fn is_likely_list_item_fast(&self, content: &str) -> bool {
        let trimmed = content.trim();

        // Early returns for obvious cases
        if trimmed.is_empty() || trimmed.len() < 3 {
            return false;
        }

        // Quick word count using bytes
        let word_count = trimmed.split_whitespace().count();

        // Short content ending with * is likely emphasis
        if word_count <= 2 && trimmed.ends_with('*') && !trimmed.ends_with("**") {
            return false;
        }

        // Long content (4+ words) without emphasis is likely a list
        if word_count >= 4 {
            // Quick check: if no emphasis markers, it's a list
            if !trimmed.contains('*') && !trimmed.contains('_') {
                return true;
            }
        }

        // For ambiguous cases, default to emphasis (more conservative)
        false
    }

    /// Optimized line content checking for emphasis issues
    fn check_line_content_for_emphasis_fast(
        &self,
        content: &str,
        line_num: usize,
        offset: usize,
        warnings: &mut Vec<LintWarning>,
    ) {
        // Find all emphasis markers using optimized parsing
        let markers = find_emphasis_markers(content);
        if markers.is_empty() {
            return;
        }

        // Find valid emphasis spans
        let spans = find_emphasis_spans(content, markers);

        // Check each span for spacing issues
        for span in spans {
            if has_spacing_issues(&span) {
                // Calculate the full span including markers
                let full_start = span.opening.start_pos;
                let full_end = span.closing.end_pos();
                let full_text = &content[full_start..full_end];

                // Create the marker string efficiently
                let marker_char = span.opening.as_char();
                let marker_str = if span.opening.count == 1 {
                    marker_char.to_string()
                } else {
                    format!("{}{}", marker_char, marker_char)
                };

                // Create the fixed version by trimming spaces from content
                let trimmed_content = span.content.trim();
                let fixed_text = format!("{}{}{}", marker_str, trimmed_content, marker_str);

                let warning = LintWarning {
                    rule_name: Some(self.name()),
                    message: format!("Spaces inside emphasis markers: {:?}", full_text),
                    line: line_num,
                    column: offset + full_start + 1, // +1 because columns are 1-indexed
                    end_line: line_num,
                    end_column: offset + full_end + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: (offset + full_start)..(offset + full_end),
                        replacement: fixed_text,
                    }),
                };

                warnings.push(warning);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::utils::document_structure::DocumentStructure;

    #[test]
    fn test_emphasis_marker_parsing() {
        let markers = find_emphasis_markers("This has *single* and **double** emphasis");
        assert_eq!(markers.len(), 4); // *, *, **, **

        let markers = find_emphasis_markers("*start* and *end*");
        assert_eq!(markers.len(), 4); // *, *, *, *
    }

    #[test]
    fn test_emphasis_span_detection() {
        let markers = find_emphasis_markers("This has *valid* emphasis");
        let spans = find_emphasis_spans("This has *valid* emphasis", markers);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "valid");
        assert!(!spans[0].has_leading_space);
        assert!(!spans[0].has_trailing_space);

        let markers = find_emphasis_markers("This has * invalid * emphasis");
        let spans = find_emphasis_spans("This has * invalid * emphasis", markers);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, " invalid ");
        assert!(spans[0].has_leading_space);
        assert!(spans[0].has_trailing_space);
    }

    #[test]
    fn test_with_document_structure() {
        let rule = MD037NoSpaceInEmphasis;

        // Test with no spaces inside emphasis - should pass
        let content = "This is *correct* emphasis and **strong emphasis**";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            result.is_empty(),
            "No warnings expected for correct emphasis"
        );

        // Test with actual spaces inside emphasis - use content that should warn
        let content = "This is * text with spaces * and more content";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            !result.is_empty(),
            "Expected warnings for spaces in emphasis"
        );

        // Test with code blocks - emphasis in code should be ignored
        let content = "This is *correct* emphasis\n```\n* incorrect * in code block\n```\nOutside block with * spaces in emphasis *";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            !result.is_empty(),
            "Expected warnings for spaces in emphasis outside code block"
        );
    }
}
