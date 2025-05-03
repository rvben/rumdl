use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::{HashMap, HashSet};

lazy_static! {
    // Pattern to match reference definitions [ref]: url (standard regex is fine)
    static ref REF_REGEX: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s*\S+").unwrap();

    // Pattern to match reference links and images ONLY: [text][reference] or ![text][reference]
    // These need lookbehind for escaped brackets
    static ref REF_LINK_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)\[([^\]]+)\]\[([^\]]*)\]").unwrap();
    static ref REF_IMAGE_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)!\[([^\]]+)\]\[([^\]]*)\]").unwrap();

    // Pattern for shortcut reference links [reference]
    static ref SHORTCUT_REF_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)\[([^\]]+)\](?!\s*[\[\(])").unwrap();

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

    /// Detect inline code spans in a line and return their ranges
    fn compute_inline_code_spans(&self, line: &str) -> Vec<(usize, usize)> {
        if !line.contains('`') {
            return Vec::new();
        }

        let mut spans = Vec::new();
        let mut in_code = false;
        let mut code_start = 0;

        for (i, c) in line.chars().enumerate() {
            if c == '`' {
                if !in_code {
                    code_start = i;
                    in_code = true;
                } else {
                    spans.push((code_start, i + 1)); // Include the closing backtick
                    in_code = false;
                }
            }
        }

        spans
    }

    /// Check if a position is within an inline code span
    fn is_in_code_span(&self, spans: &[(usize, usize)], pos: usize) -> bool {
        spans.iter().any(|&(start, end)| pos >= start && pos < end)
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
    ) -> Vec<(usize, usize, String)> {
        let mut undefined = Vec::new();
        let mut reported_refs = HashMap::new();

        let mut in_code_block = false;
        let mut code_fence_marker = String::new();
        let mut in_example_section = false;

        for (line_num, line) in content.lines().enumerate() {
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

            // Check if we're entering an example section
            if OUTPUT_EXAMPLE_START.is_match(line) {
                in_example_section = true;
                continue;
            }

            // Check if we're exiting an example section (next heading)
            if in_example_section && line.starts_with('#') && !OUTPUT_EXAMPLE_START.is_match(line) {
                in_example_section = false;
            }

            // Skip lines in code blocks, example sections, or list items
            if in_code_block || in_example_section || LIST_ITEM_REGEX.is_match(line) {
                continue;
            }

            // Detect inline code spans in this line
            let inline_code_spans = self.compute_inline_code_spans(line);

            // Check for undefined references in reference links
            if let Ok(captures) = REF_LINK_REGEX
                .captures_iter(line)
                .collect::<Result<Vec<_>, _>>()
            {
                for cap in captures {
                    if let Some(full_match) = cap.get(0) {
                        // Skip if inside inline code span
                        if self.is_in_code_span(&inline_code_spans, full_match.start()) {
                            continue;
                        }

                        let reference = if let Some(ref_match) = cap.get(2) {
                            if ref_match.as_str().is_empty() {
                                cap.get(1).map(|m| m.as_str().to_string())
                            } else {
                                Some(ref_match.as_str().to_string())
                            }
                        } else {
                            cap.get(1).map(|m| m.as_str().to_string())
                        };

                        if let Some(ref_text) = reference {
                            let reference_lower = ref_text.to_lowercase();
                            if !references.contains(&reference_lower)
                                && !reported_refs.contains_key(&reference_lower)
                            {
                                undefined.push((line_num, full_match.start(), ref_text));
                                reported_refs.insert(reference_lower, true);
                            }
                        }
                    }
                }
            }

            // Check for undefined references in reference images
            if let Ok(captures) = REF_IMAGE_REGEX
                .captures_iter(line)
                .collect::<Result<Vec<_>, _>>()
            {
                for cap in captures {
                    if let Some(full_match) = cap.get(0) {
                        // Skip if inside inline code span
                        if self.is_in_code_span(&inline_code_spans, full_match.start()) {
                            continue;
                        }

                        let reference = if let Some(ref_match) = cap.get(2) {
                            if ref_match.as_str().is_empty() {
                                cap.get(1).map(|m| m.as_str().to_string())
                            } else {
                                Some(ref_match.as_str().to_string())
                            }
                        } else {
                            cap.get(1).map(|m| m.as_str().to_string())
                        };

                        if let Some(ref_text) = reference {
                            let reference_lower = ref_text.to_lowercase();
                            if !references.contains(&reference_lower)
                                && !reported_refs.contains_key(&reference_lower)
                            {
                                undefined.push((line_num, full_match.start(), ref_text));
                                reported_refs.insert(reference_lower, true);
                            }
                        }
                    }
                }
            }

            // Check for undefined shortcut references
            if let Ok(captures) = SHORTCUT_REF_REGEX
                .captures_iter(line)
                .collect::<Result<Vec<_>, _>>()
            {
                for cap in captures {
                    if let Some(full_match) = cap.get(0) {
                        // Skip if inside inline code span
                        if self.is_in_code_span(&inline_code_spans, full_match.start()) {
                            continue;
                        }

                        // Skip if it's part of an inline link/image or a reference definition
                        if let Ok(is_inline_link) = INLINE_LINK_REGEX.is_match(line) {
                            if is_inline_link {
                                continue;
                            }
                        }
                        if let Ok(is_inline_image) = INLINE_IMAGE_REGEX.is_match(line) {
                            if is_inline_image {
                                continue;
                            }
                        }
                        if REF_REGEX.is_match(line) {
                            continue;
                        }

                        if let Some(ref_match) = cap.get(1) {
                            let ref_text = ref_match.as_str().to_string();
                            let reference_lower = ref_text.to_lowercase();
                            if !references.contains(&reference_lower)
                                && !reported_refs.contains_key(&reference_lower)
                            {
                                undefined.push((line_num, full_match.start(), ref_text));
                                reported_refs.insert(reference_lower, true);
                            }
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

        for (line_num, col, reference) in self.find_undefined_references(content, &references) {
            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: line_num + 1,
                column: col + 1,
                message: format!("Reference '{}' not found", reference),
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
