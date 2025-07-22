use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_match_range;
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::{HashMap, HashSet};

lazy_static! {
    // Pattern to match reference definitions [ref]: url (standard regex is fine)
    // Note: \S* instead of \S+ to allow empty definitions like [ref]:
    static ref REF_REGEX: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s*.*").unwrap();

    // Pattern to match reference links and images ONLY: [text][reference] or ![text][reference]
    // These need lookbehind for escaped brackets
    // Use a more sophisticated pattern that handles nested brackets
    static ref REF_LINK_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)\[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]\[([^\]]*)\]").unwrap();
    static ref REF_IMAGE_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)!\[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]\[([^\]]*)\]").unwrap();

    // Pattern for shortcut reference links [reference]
    // Must not be preceded by ] (to avoid matching second part of [text][ref])
    // Must not be followed by [ or ( (to avoid matching first part of [text][ref] or [text](url))
    static ref SHORTCUT_REF_REGEX: FancyRegex = FancyRegex::new(r"(?<![\\])\[([^\]]+)\](?!\s*[\[\(])").unwrap();

    // Pattern to match inline links and images (to exclude them)
    static ref INLINE_LINK_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)\[([^\]]+)\]\(([^)]+)\)").unwrap();
    static ref INLINE_IMAGE_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)!\[([^\]]+)\]\(([^)]+)\)").unwrap();

    // Pattern for list items to exclude from reference checks (standard regex is fine)
    static ref LIST_ITEM_REGEX: Regex = Regex::new(r"^\s*[-*+]\s+(?:\[[xX\s]\]\s+)?").unwrap();

    // Pattern for code blocks (standard regex is fine)
    static ref FENCED_CODE_START: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();

    // Pattern for output example sections (standard regex is fine)
    static ref OUTPUT_EXAMPLE_START: Regex = Regex::new(r"^#+\s*(?:Output|Example|Output Style|Output Format)\s*$").unwrap();
}

/// Rule MD052: Reference links and images should use reference style
///
/// See [docs/md052.md](../../docs/md052.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when a reference link or image uses a reference that isn't defined.
#[derive(Clone)]
pub struct MD052ReferenceLinkImages;

impl Default for MD052ReferenceLinkImages {
    fn default() -> Self {
        Self::new()
    }
}

impl MD052ReferenceLinkImages {
    pub fn new() -> Self {
        Self
    }

    /// Check if a position is inside any code span
    fn is_in_code_span(line: usize, col: usize, code_spans: &[crate::lint_context::CodeSpan]) -> bool {
        code_spans
            .iter()
            .any(|span| span.line == line && col >= span.start_col && col < span.end_col)
    }

    fn extract_references(&self, content: &str) -> HashSet<String> {
        let mut references = HashSet::new();
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();

        for line in content.lines() {
            // Handle code block boundaries
            if let Some(cap) = FENCED_CODE_START.captures(line) {
                if let Some(marker) = cap.get(0) {
                    let marker_str = marker.as_str().to_string();
                    if !in_code_block {
                        in_code_block = true;
                        code_fence_marker = marker_str;
                    } else if line.trim().starts_with(&code_fence_marker) {
                        in_code_block = false;
                        code_fence_marker.clear();
                    }
                }
                continue;
            }

            // Skip lines in code blocks
            if in_code_block {
                continue;
            }

            if let Some(cap) = REF_REGEX.captures(line) {
                // Store references in lowercase for case-insensitive comparison
                if let Some(reference) = cap.get(1) {
                    references.insert(reference.as_str().to_lowercase());
                }
            }
        }

        references
    }

    fn find_undefined_references(
        &self,
        content: &str,
        references: &HashSet<String>,
        ctx: &crate::lint_context::LintContext,
    ) -> Vec<(usize, usize, usize, String)> {
        let mut undefined = Vec::new();
        let mut reported_refs = HashMap::new();
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();
        let mut in_example_section = false;

        // Get code spans once for the entire function
        let code_spans = ctx.code_spans();

        // Use cached data for reference links and images
        for link in &ctx.links {
            if !link.is_reference {
                continue; // Skip inline links
            }

            // Skip links inside code spans
            if Self::is_in_code_span(link.line, link.start_col, &code_spans) {
                continue;
            }

            if let Some(ref_id) = &link.reference_id {
                let reference_lower = ref_id.to_lowercase();

                // Check if reference is defined
                if !references.contains(&reference_lower) && !reported_refs.contains_key(&reference_lower) {
                    // Check if the line is in an example section or list item
                    if let Some(line_info) = ctx.line_info(link.line) {
                        if OUTPUT_EXAMPLE_START.is_match(&line_info.content) {
                            in_example_section = true;
                            continue;
                        }

                        if in_example_section {
                            continue;
                        }

                        // Skip list items
                        if LIST_ITEM_REGEX.is_match(&line_info.content) {
                            continue;
                        }
                    }

                    let match_len = link.byte_end - link.byte_offset;
                    undefined.push((link.line - 1, link.start_col, match_len, ref_id.clone()));
                    reported_refs.insert(reference_lower, true);
                }
            }
        }

        // Use cached data for reference images
        for image in &ctx.images {
            if !image.is_reference {
                continue; // Skip inline images
            }

            // Skip images inside code spans
            if Self::is_in_code_span(image.line, image.start_col, &code_spans) {
                continue;
            }

            if let Some(ref_id) = &image.reference_id {
                let reference_lower = ref_id.to_lowercase();

                // Check if reference is defined
                if !references.contains(&reference_lower) && !reported_refs.contains_key(&reference_lower) {
                    // Check if the line is in an example section or list item
                    if let Some(line_info) = ctx.line_info(image.line) {
                        if OUTPUT_EXAMPLE_START.is_match(&line_info.content) {
                            in_example_section = true;
                            continue;
                        }

                        if in_example_section {
                            continue;
                        }

                        // Skip list items
                        if LIST_ITEM_REGEX.is_match(&line_info.content) {
                            continue;
                        }
                    }

                    let match_len = image.byte_end - image.byte_offset;
                    undefined.push((image.line - 1, image.start_col, match_len, ref_id.clone()));
                    reported_refs.insert(reference_lower, true);
                }
            }
        }

        // Build a set of byte ranges that are already covered by parsed links/images
        let mut covered_ranges: Vec<(usize, usize)> = Vec::new();

        // Add ranges from parsed links
        for link in &ctx.links {
            covered_ranges.push((link.byte_offset, link.byte_end));
        }

        // Add ranges from parsed images
        for image in &ctx.images {
            covered_ranges.push((image.byte_offset, image.byte_end));
        }

        // Sort ranges by start position
        covered_ranges.sort_by_key(|&(start, _)| start);

        // Handle shortcut references [text] which aren't captured in ctx.links
        // Need to use regex for these
        let lines: Vec<&str> = content.lines().collect();
        in_example_section = false; // Reset for line-by-line processing

        for (line_num, line) in lines.iter().enumerate() {
            // Handle code blocks
            if let Some(cap) = FENCED_CODE_START.captures(line) {
                if let Some(marker) = cap.get(0) {
                    let marker_str = marker.as_str().to_string();
                    if !in_code_block {
                        in_code_block = true;
                        code_fence_marker = marker_str;
                    } else if line.trim().starts_with(&code_fence_marker) {
                        in_code_block = false;
                        code_fence_marker.clear();
                    }
                }
                continue;
            }

            if in_code_block {
                continue;
            }

            // Check for example sections
            if OUTPUT_EXAMPLE_START.is_match(line) {
                in_example_section = true;
                continue;
            }

            if in_example_section {
                // Check if we're exiting the example section (another heading)
                if line.starts_with('#') && !OUTPUT_EXAMPLE_START.is_match(line) {
                    in_example_section = false;
                } else {
                    continue;
                }
            }

            // Skip list items
            if LIST_ITEM_REGEX.is_match(line) {
                continue;
            }

            // Check shortcut references: [reference]
            if let Ok(captures) = SHORTCUT_REF_REGEX.captures_iter(line).collect::<Result<Vec<_>, _>>() {
                for cap in captures {
                    if let Some(ref_match) = cap.get(1) {
                        let reference = ref_match.as_str();
                        let reference_lower = reference.to_lowercase();

                        if !references.contains(&reference_lower) && !reported_refs.contains_key(&reference_lower) {
                            let full_match = cap.get(0).unwrap();
                            let col = full_match.start();

                            // Skip if inside code span
                            let code_spans = ctx.code_spans();
                            if Self::is_in_code_span(line_num + 1, col, &code_spans) {
                                continue;
                            }

                            // Check if this position is within a covered range
                            let line_start_byte = ctx.line_offsets[line_num];
                            let byte_pos = line_start_byte + col;
                            let byte_end = byte_pos + (full_match.end() - full_match.start());

                            // Check if this shortcut ref overlaps with any parsed link/image
                            let mut is_covered = false;
                            for &(range_start, range_end) in &covered_ranges {
                                if range_start <= byte_pos && byte_end <= range_end {
                                    // This shortcut ref is completely within a parsed link/image
                                    is_covered = true;
                                    break;
                                }
                                if range_start > byte_end {
                                    // No need to check further (ranges are sorted)
                                    break;
                                }
                            }

                            if is_covered {
                                continue;
                            }

                            // More sophisticated checks to avoid false positives

                            // Check 1: If preceded by ], this might be part of [text][ref]
                            // Look for the pattern ...][ref] and check if there's a matching [ before
                            if col > 0 && line.chars().nth(col.saturating_sub(1)) == Some(']') {
                                // Look backwards for a [ that would make this [text][ref]
                                let mut bracket_count = 1; // We already saw one ]
                                let mut check_pos = col.saturating_sub(2);
                                let mut found_opening = false;

                                while check_pos > 0 {
                                    match line.chars().nth(check_pos) {
                                        Some(']') => bracket_count += 1,
                                        Some('[') => {
                                            bracket_count -= 1;
                                            if bracket_count == 0 {
                                                // Check if this [ is escaped
                                                if check_pos == 0 || line.chars().nth(check_pos - 1) != Some('\\') {
                                                    found_opening = true;
                                                }
                                                break;
                                            }
                                        }
                                        _ => {}
                                    }
                                    if check_pos == 0 {
                                        break;
                                    }
                                    check_pos = check_pos.saturating_sub(1);
                                }

                                if found_opening {
                                    // This is part of [text][ref], skip it
                                    continue;
                                }
                            }

                            // Check 2: If there's an escaped bracket pattern before this
                            // e.g., \[text\][ref], the [ref] shouldn't be treated as a shortcut
                            let before_text = &line[..col];
                            if before_text.contains("\\]") {
                                // Check if there's a \[ before the \]
                                if let Some(escaped_close_pos) = before_text.rfind("\\]") {
                                    let search_text = &before_text[..escaped_close_pos];
                                    if search_text.contains("\\[") {
                                        // This looks like \[...\][ref], skip it
                                        continue;
                                    }
                                }
                            }

                            let match_len = full_match.end() - full_match.start();
                            undefined.push((line_num, col, match_len, reference.to_string()));
                            reported_refs.insert(reference_lower, true);
                        }
                    }
                }
            }
        }

        undefined
    }
}

impl Rule for MD052ReferenceLinkImages {
    fn name(&self) -> &'static str {
        "MD052"
    }

    fn description(&self) -> &'static str {
        "Reference links and images should use a reference that exists"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();
        let references = self.extract_references(content);

        // Use optimized detection method with cached link/image data
        for (line_num, col, match_len, reference) in self.find_undefined_references(content, &references, ctx) {
            let lines: Vec<&str> = content.lines().collect();
            let line_content = lines.get(line_num).unwrap_or(&"");

            // Calculate precise character range for the entire undefined reference
            let (start_line, start_col, end_line, end_col) =
                calculate_match_range(line_num + 1, line_content, col, match_len);

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!("Reference '{reference}' not found"),
                severity: Severity::Warning,
                fix: None,
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        // No automatic fix available for undefined references
        Ok(content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD052ReferenceLinkImages::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_valid_reference_link() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[text][ref]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_undefined_reference_link() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[text][undefined]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Reference 'undefined' not found"));
    }

    #[test]
    fn test_valid_reference_image() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "![alt][img]\n\n[img]: image.jpg";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_undefined_reference_image() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "![alt][missing]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Reference 'missing' not found"));
    }

    #[test]
    fn test_case_insensitive_references() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[Text][REF]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_shortcut_reference_valid() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[ref]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_shortcut_reference_undefined() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[undefined]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Reference 'undefined' not found"));
    }

    #[test]
    fn test_inline_links_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[text](https://example.com)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_inline_images_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "![alt](image.jpg)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_references_in_code_blocks_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "```\n[undefined]\n```\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_references_in_inline_code_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "`[undefined]`";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // References inside inline code spans should be ignored
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_comprehensive_inline_code_detection() {
        let rule = MD052ReferenceLinkImages::new();
        let content = r#"# Test

This `[inside]` should be ignored.
This [outside] should be flagged.
Reference links `[text][ref]` in code are ignored.
Regular reference [text][missing] should be flagged.
Images `![alt][img]` in code are ignored.
Regular image ![alt][badimg] should be flagged.

Multiple `[one]` and `[two]` in code ignored, but [three] is not.

```
[code block content] should be ignored
```

`Multiple [refs] in [same] code span` ignored."#;

        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should only flag: outside, missing, badimg, three (4 total)
        assert_eq!(result.len(), 4);

        let messages: Vec<&str> = result.iter().map(|w| &*w.message).collect();
        assert!(messages.iter().any(|m| m.contains("outside")));
        assert!(messages.iter().any(|m| m.contains("missing")));
        assert!(messages.iter().any(|m| m.contains("badimg")));
        assert!(messages.iter().any(|m| m.contains("three")));

        // Should NOT flag any references inside code spans
        assert!(!messages.iter().any(|m| m.contains("inside")));
        assert!(!messages.iter().any(|m| m.contains("one")));
        assert!(!messages.iter().any(|m| m.contains("two")));
        assert!(!messages.iter().any(|m| m.contains("refs")));
        assert!(!messages.iter().any(|m| m.contains("same")));
    }

    #[test]
    fn test_multiple_undefined_references() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[link1][ref1] [link2][ref2] [link3][ref3]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3);
        assert!(result[0].message.contains("ref1"));
        assert!(result[1].message.contains("ref2"));
        assert!(result[2].message.contains("ref3"));
    }

    #[test]
    fn test_mixed_valid_and_undefined() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[valid][ref] [invalid][missing]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing"));
    }

    #[test]
    fn test_empty_reference() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[text][]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Empty reference should use the link text as reference
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_escaped_brackets_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "\\[not a link\\]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_list_items_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "- [undefined]\n* [another]\n+ [third]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // List items that look like shortcut references should be ignored
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_output_example_section_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "## Output\n\n[undefined]\n\n## Normal Section\n\n[missing]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only the reference outside the Output section should be flagged
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing"));
    }

    #[test]
    fn test_reference_definitions_in_code_blocks_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[link][ref]\n\n```\n[ref]: https://example.com\n```";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Reference defined in code block should not count
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("ref"));
    }

    #[test]
    fn test_multiple_references_to_same_undefined() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[first][missing] [second][missing] [third][missing]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should only report once per unique reference
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing"));
    }

    #[test]
    fn test_reference_with_special_characters() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[text][ref-with-hyphens]\n\n[ref-with-hyphens]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_extract_references() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[ref1]: url1\n[Ref2]: url2\n[REF3]: url3";
        let refs = rule.extract_references(content);

        assert_eq!(refs.len(), 3);
        assert!(refs.contains("ref1"));
        assert!(refs.contains("ref2"));
        assert!(refs.contains("ref3"));
    }

    #[test]
    fn test_inline_code_not_flagged() {
        let rule = MD052ReferenceLinkImages::new();

        // Test that arrays in inline code are not flagged as references
        let content = r#"# Test

Configure with `["JavaScript", "GitHub", "Node.js"]` in your settings.

Also, `[todo]` is not a reference link.

But this [reference] should be flagged.

And this `[inline code]` should not be flagged.
"#;

        let ctx = LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();

        // Should only flag [reference], not the ones in backticks
        assert_eq!(warnings.len(), 1, "Should only flag one undefined reference");
        assert!(warnings[0].message.contains("'reference'"));
    }

    #[test]
    fn test_code_block_references_ignored() {
        let rule = MD052ReferenceLinkImages::new();

        let content = r#"# Test

```markdown
[undefined] reference in code block
![undefined] image in code block
```

[real-undefined] reference outside
"#;

        let ctx = LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();

        // Should only flag [real-undefined], not the ones in code block
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("'real-undefined'"));
    }
}
