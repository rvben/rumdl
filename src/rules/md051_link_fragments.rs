use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::regex_cache::*;
use std::collections::HashSet;

/// Rule MD051: Link fragments should match document headings
///
/// See [docs/md051.md](../../docs/md051.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when a link fragment (the part after #) doesn't exist in the document.
/// This only applies to internal document links, not to external URLs.
#[derive(Clone)]
pub struct MD051LinkFragments;

impl Default for MD051LinkFragments {
    fn default() -> Self {
        Self::new()
    }
}

impl MD051LinkFragments {
    pub fn new() -> Self {
        Self
    }

    /// Extract headings from pre-computed DocumentStructure data
    fn extract_headings_from_structure(&self, content: &str, structure: &DocumentStructure) -> HashSet<String> {
        let mut headings = HashSet::new();

        // Early return: if no headings at all, skip processing
        if structure.heading_lines.is_empty() {
            return headings;
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut in_toc = false;

        // Process each heading using pre-computed data from DocumentStructure
        for &line_num in &structure.heading_lines {
            let line_idx = line_num - 1; // Convert from 1-indexed to 0-indexed
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];

            // Check for TOC section
            if TOC_SECTION_START.is_match(line) {
                in_toc = true;
                continue;
            }

            // If we were in TOC and hit another heading, we're out of TOC
            if in_toc && line.trim().starts_with('#') {
                in_toc = false;
            }

            // Skip if in TOC
            if in_toc {
                continue;
            }

            // Check for ATX heading
            if let Some(cap) = ATX_HEADING_WITH_CAPTURE.captures(line) {
                if let Some(heading_text) = cap.get(2) {
                    let heading = heading_text.as_str().trim();
                    let fragment = self.heading_to_fragment(heading);
                    headings.insert(fragment);
                }
                continue;
            }

            // Check for setext heading (only check if next line exists)
            if line_idx + 1 < lines.len() {
                let combined = format!("{}\n{}", line, lines[line_idx + 1]);
                if let Ok(Some(cap)) = SETEXT_HEADING_WITH_CAPTURE.captures(&combined) {
                    if let Some(heading_text) = cap.get(1) {
                        let heading = heading_text.as_str().trim();
                        let fragment = self.heading_to_fragment(heading);
                        headings.insert(fragment);
                    }
                }
            }
        }

        headings
    }

    /// Convert a heading to a fragment identifier following GitHub's algorithm:
    /// 1. Strip all formatting (code, emphasis, links, etc.)
    /// 2. Convert to lowercase
    /// 3. Replace spaces and non-alphanumeric chars with hyphens
    /// 4. Remove consecutive hyphens
    fn heading_to_fragment(&self, heading: &str) -> String {
        let mut stripped = heading.to_string();

        // Remove links but keep the link text: [text](url) -> text
        stripped = INLINE_LINK_REGEX.replace_all(&stripped, "$1").to_string();

        // Remove emphasis and bold formatting more comprehensively
        stripped = BOLD_ASTERISK_REGEX.replace_all(&stripped, "$1").to_string();
        stripped = BOLD_UNDERSCORE_REGEX.replace_all(&stripped, "$1").to_string();
        stripped = ITALIC_ASTERISK_REGEX.replace_all(&stripped, "$1").to_string();
        stripped = ITALIC_UNDERSCORE_REGEX.replace_all(&stripped, "$1").to_string();
        stripped = STRIKETHROUGH_REGEX.replace_all(&stripped, "$1").to_string();

        // Remove code spans by replacing with their content (simplified)
        stripped = stripped.replace("`", "");

        // Convert to lowercase and replace spaces/non-alphanumeric chars with hyphens
        let fragment = stripped
            .to_lowercase()
            .chars()
            .map(|c| match c {
                ' ' => '-',
                c if c.is_alphanumeric() => c,
                _ => '-',
            })
            .collect::<String>();

        // Replace multiple consecutive hyphens with a single one
        MULTIPLE_HYPHENS.replace_all(&fragment, "-").to_string()
    }

    fn is_external_url(&self, url: &str) -> bool {
        EXTERNAL_URL_REGEX.is_match(url).unwrap_or(false)
    }
}

impl Rule for MD051LinkFragments {
    fn name(&self) -> &'static str {
        "MD051"
    }

    fn description(&self) -> &'static str {
        "Link fragments should exist"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early return: if no links at all, skip processing
        if !content.contains('[') || !content.contains('#') {
            return Ok(Vec::new());
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    /// Optimized check using pre-computed document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;

        // Early return: if no links at all, skip processing
        if !content.contains('[') || !content.contains('#') {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let headings = self.extract_headings_from_structure(content, structure);
        let mut in_toc_section = false;

        for (line_num, line) in content.lines().enumerate() {
            // Early return: skip lines without links or fragments
            if !line.contains('[') || !line.contains('#') {
                continue;
            }

            // Check if we're entering a TOC section
            if TOC_SECTION_START.is_match(line) {
                in_toc_section = true;
                continue;
            }

            // Check if we're exiting a TOC section (next heading)
            if in_toc_section && line.starts_with('#') && !TOC_SECTION_START.is_match(line) {
                in_toc_section = false;
            }

            // Skip lines in code blocks or TOC section
            if structure.is_in_code_block(line_num + 1) || in_toc_section {
                continue;
            }

            // Use regex to find all links with fragments
            let mut link_iter = LINK_REGEX.captures_iter(line);
            while let Some(cap_result) = link_iter.next() {
                if let Ok(cap) = cap_result {
                    let full_match = cap.get(0).unwrap();
                    let url = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                    let fragment = cap.get(3).map(|m| m.as_str()).unwrap_or("");

                    // Only check internal links (not external URLs)
                    if self.is_external_url(url) {
                        continue;
                    }

                    // Skip if the link is inside a code span
                    if structure.is_in_code_span(line_num + 1, full_match.start() + 1) {
                        continue;
                    }

                    // Check if the fragment exists in headings
                    if !headings.contains(&fragment.to_lowercase()) {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: full_match.start() + 1,
                            message: format!(
                                "Link fragment '#{}' does not exist in document headings.",
                                fragment
                            ),
                            severity: Severity::Warning,
                            fix: None,
                        });
                    }
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // No automatic fix for missing fragments, just return content as-is
        Ok(ctx.content.to_owned())
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
        Box::new(MD051LinkFragments::new())
    }
}

impl DocumentStructureExtensions for MD051LinkFragments {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        // This rule is only relevant if there are both headings and links
        !doc_structure.heading_lines.is_empty() && !doc_structure.links.is_empty()
    }
}
