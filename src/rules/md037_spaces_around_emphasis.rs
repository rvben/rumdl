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

/// Represents an emphasis marker found in text
#[derive(Debug, Clone, PartialEq)]
struct EmphasisMarker {
    marker_type: char,  // '*' or '_'
    count: usize,       // 1 for single, 2 for double
    start_pos: usize,   // Position in the line
    end_pos: usize,     // End position of the marker
}

/// Represents a complete emphasis span
#[derive(Debug, Clone)]
struct EmphasisSpan {
    opening: EmphasisMarker,
    closing: EmphasisMarker,
    content: String,
    has_leading_space: bool,
    has_trailing_space: bool,
}

/// Parse emphasis markers from a line of text
fn find_emphasis_markers(line: &str) -> Vec<EmphasisMarker> {
    let mut markers = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '*' || chars[i] == '_' {
            let marker_char = chars[i];
            let start_pos = i;
            let mut count = 1;

            // Count consecutive markers
            while i + count < chars.len() && chars[i + count] == marker_char {
                count += 1;
            }

            // Only consider single (*) and double (**) markers
            if count == 1 || count == 2 {
                markers.push(EmphasisMarker {
                    marker_type: marker_char,
                    count,
                    start_pos,
                    end_pos: start_pos + count,
                });
            }

            i += count;
        } else {
            i += 1;
        }
    }

    markers
}

/// Find valid emphasis spans by pairing opening and closing markers
fn find_emphasis_spans(line: &str, markers: Vec<EmphasisMarker>) -> Vec<EmphasisSpan> {
    let mut spans = Vec::new();
    let mut used_markers = vec![false; markers.len()];

    for i in 0..markers.len() {
        if used_markers[i] {
            continue;
        }

        let opening = &markers[i];

        // Look for the nearest matching closing marker (not the farthest)
        // This prevents creating spans that cross multiple emphasis boundaries
        for j in (i + 1)..markers.len() {
            if used_markers[j] {
                continue;
            }

            let closing = &markers[j];

            // Must be same type and count
            if closing.marker_type == opening.marker_type && closing.count == opening.count {
                // Extract content between markers
                let content_start = opening.end_pos;
                let content_end = closing.start_pos;

                if content_end > content_start {
                    let content = line[content_start..content_end].to_string();

                    // Check if this is a valid emphasis span
                    if is_valid_emphasis_content(&content) && is_valid_emphasis_span(line, opening, closing) {
                        // Additional check: make sure we're not crossing other emphasis markers
                        let crosses_markers = markers.iter().enumerate().any(|(idx, marker)| {
                            idx != i && idx != j && !used_markers[idx] &&
                            marker.start_pos > opening.end_pos && marker.end_pos < closing.start_pos &&
                            marker.marker_type == opening.marker_type
                        });

                        if !crosses_markers {
                            let has_leading_space = content.starts_with(' ') || content.starts_with('\t');
                            let has_trailing_space = content.ends_with(' ') || content.ends_with('\t');

                            spans.push(EmphasisSpan {
                                opening: opening.clone(),
                                closing: closing.clone(),
                                content,
                                has_leading_space,
                                has_trailing_space,
                            });

                            // Mark both markers as used
                            used_markers[i] = true;
                            used_markers[j] = true;
                            break; // Take the first valid match, not the farthest
                        }
                    }
                }
            }
        }
    }

    spans
}

/// Check if this is a valid emphasis span by examining the context
fn is_valid_emphasis_span(line: &str, opening: &EmphasisMarker, closing: &EmphasisMarker) -> bool {
    let content_start = opening.end_pos;
    let content_end = closing.start_pos;

    // Content must exist
    if content_end <= content_start {
        return false;
    }

    let content = &line[content_start..content_end];

    // Content cannot be just whitespace
    if content.trim().is_empty() {
        return false;
    }

    // Check for valid emphasis boundaries
    // Opening marker should be preceded by whitespace or start of line
    let before_opening = if opening.start_pos > 0 {
        line.chars().nth(opening.start_pos - 1)
    } else {
        None
    };

    // Closing marker should be followed by whitespace, punctuation, or end of line
    let after_closing = if closing.end_pos < line.len() {
        line.chars().nth(closing.end_pos)
    } else {
        None
    };

    // Opening should be at start of line or after whitespace/punctuation/HTML
    let valid_opening = opening.start_pos == 0
        || before_opening.map_or(false, |c| c.is_whitespace() || "([{\"'>".contains(c));

    // Closing should be at end of line or before whitespace/punctuation/HTML
    let valid_closing = closing.end_pos == line.len()
        || after_closing.map_or(false, |c| c.is_whitespace() || ")]}\"'.,!?;:<".contains(c));

    // Content should not contain line breaks (for single-line emphasis)
    let no_line_breaks = !content.contains('\n');

    // Content should not start and end with the same emphasis marker (avoid nested conflicts)
    let no_marker_conflict = !(content.starts_with(opening.marker_type) && content.ends_with(opening.marker_type));

    valid_opening && valid_closing && no_line_breaks && no_marker_conflict
}

/// Check if content is valid for emphasis (not empty, not just whitespace)
fn is_valid_emphasis_content(content: &str) -> bool {
    let trimmed = content.trim();
    !trimmed.is_empty() && trimmed.len() > 0
}

/// Check if an emphasis span has spacing issues that should be flagged
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

            // Use the new emphasis parsing logic
            self.check_line_for_emphasis_issues(&line_no_code, line_num + 1, &mut warnings);
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
    /// Check a line for emphasis spacing issues using the new parsing logic
    fn check_line_for_emphasis_issues(
        &self,
        line: &str,
        line_num: usize,
        warnings: &mut Vec<LintWarning>,
    ) {
        // Skip documentation patterns
        let trimmed = line.trim_start();
        if (trimmed.starts_with("* *") && line.contains("*:"))
            || (trimmed.starts_with("* **") && line.contains("**:"))
            || DOC_METADATA_PATTERN.is_match(line)
            || BOLD_TEXT_PATTERN.is_match(line)
        {
            return;
        }

        // Improved list detection: only treat as list if it's actually a list item
        // A list item should have content after the marker that doesn't look like emphasis
        if LIST_MARKER.is_match(line) {
            if let Some(caps) = LIST_MARKER.captures(line) {
                if let Some(full_match) = caps.get(0) {
                    let list_marker_end = full_match.end();
                    if list_marker_end < line.len() {
                        let remaining_content = &line[list_marker_end..];

                        // Check if this is actually a list item or emphasis with spacing issues
                        // A real list item should have substantial content that doesn't end with emphasis markers
                        // and shouldn't look like "word *" or "word * word"
                        let is_likely_list = self.is_likely_list_item(remaining_content);

                        if is_likely_list {
                            // Process the content after the list marker for emphasis
                            self.check_line_content_for_emphasis(remaining_content, line_num, list_marker_end, warnings);
                        } else {
                            // This looks like emphasis with spacing issues, not a list item
                            // Process the entire line as potential emphasis
                            self.check_line_content_for_emphasis(line, line_num, 0, warnings);
                        }
                    }
                }
            }
            return;
        }

        // Check the entire line
        self.check_line_content_for_emphasis(line, line_num, 0, warnings);
    }

    /// Determine if content after a "* " marker is likely a list item or emphasis
    fn is_likely_list_item(&self, content: &str) -> bool {
        let trimmed = content.trim();

        // Empty content is not a list item
        if trimmed.is_empty() {
            return false;
        }

        // If it's very short (1-2 words) and ends with *, it's likely emphasis
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        if words.len() <= 2 && trimmed.ends_with('*') && !trimmed.ends_with("**") {
            return false;
        }

        // Check for valid emphasis patterns within the content first
        let markers = find_emphasis_markers(content);
        let spans = find_emphasis_spans(content, markers.clone());

        // If we find valid emphasis spans (without spacing issues), it's likely a list item
        // Example: "List item with *emphasis*" should be treated as a list
        for span in &spans {
            if !has_spacing_issues(&span) {
                // This contains valid emphasis, so it's a list item
                return true;
            }
        }

        // If all spans have spacing issues, check the pattern more carefully
        if !spans.is_empty() {
            // If the content looks like "text * and * text * here" (multiple emphasis with issues),
            // it's likely emphasis, not a list
            let emphasis_count = spans.len();
            let total_markers = markers.len();

            // If we have multiple emphasis spans with issues and many markers, it's likely emphasis
            if emphasis_count >= 2 && total_markers >= 4 {
                return false;
            }

            // If it's a single span with issues and short content, it's likely emphasis
            if emphasis_count == 1 && words.len() <= 3 {
                return false;
            }
        }

        // If it contains multiple words (4+) and no emphasis issues, it's likely a list
        if words.len() >= 4 && spans.is_empty() {
            return true;
        }

        // Default to treating as emphasis for ambiguous cases
        false
    }

    /// Check line content for emphasis issues with proper context awareness
    fn check_line_content_for_emphasis(
        &self,
        content: &str,
        line_num: usize,
        offset: usize,
        warnings: &mut Vec<LintWarning>,
    ) {
        // Find all emphasis markers
        let markers = find_emphasis_markers(content);
        if markers.is_empty() {
            return;
        }

        // Find valid emphasis spans
        let spans = find_emphasis_spans(content, markers.clone());

        // Check each span for spacing issues
        for span in spans {
            if has_spacing_issues(&span) {
                // Calculate the full span including markers
                let full_start = span.opening.start_pos;
                let full_end = span.closing.end_pos;
                let full_text = &content[full_start..full_end];

                // Create the marker string
                let marker_str = span.opening.marker_type.to_string().repeat(span.opening.count);

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
