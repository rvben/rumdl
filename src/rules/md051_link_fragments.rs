use crate::utils::document_structure::DocumentStructure;

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

lazy_static! {
    static ref ATX_HEADING_REGEX: Regex = Regex::new(r"^(#{1,6})\s+(.+?)(?:\s+#*\s*)?$").unwrap();
    static ref SETEXT_HEADING_REGEX: Regex = Regex::new(r"^([^\n]+)\n([=\-])\2+\s*$").unwrap();
    static ref CODE_FENCE_REGEX: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();
    static ref TOC_SECTION_START: Regex =
        Regex::new(r"^#+\s*(?:Table of Contents|Contents|TOC)\s*$").unwrap();
    static ref MULTIPLE_HYPHENS: Regex = Regex::new(r"-{2,}").unwrap();
    static ref LINK_REGEX: FancyRegex =
        FancyRegex::new(r"(?<!\\)\[([^\]]*)\]\((?:([^)]+))?#([^)]+)\)").unwrap();
    static ref EXTERNAL_URL_REGEX: FancyRegex =
        FancyRegex::new(r"^(https?://|ftp://|www\.|[^/]+\.[a-z]{2,})").unwrap();
    static ref INLINE_CODE_REGEX: FancyRegex = FancyRegex::new(r"`[^`]+`").unwrap();
    static ref BOLD_ASTERISK_REGEX: Regex = Regex::new(r"\*\*(.+?)\*\*").unwrap();
    static ref BOLD_UNDERSCORE_REGEX: Regex = Regex::new(r"__(.+?)__").unwrap();
    static ref ITALIC_ASTERISK_REGEX: Regex = Regex::new(r"\*([^*]+?)\*").unwrap();
    static ref ITALIC_UNDERSCORE_REGEX: Regex = Regex::new(r"_([^_]+?)_").unwrap();
    static ref LINK_TEXT_REGEX: FancyRegex = FancyRegex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap();
    static ref STRIKETHROUGH_REGEX: Regex = Regex::new(r"~~(.+?)~~").unwrap();
}

/// Rule MD051: Link fragments should exist
///
/// This rule is triggered when a link fragment (the part after #) doesn't exist in the document.
/// This only applies to internal document links, not to external URLs.
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

    fn extract_headings(&self, content: &str) -> HashSet<String> {
        let mut headings = HashSet::new();
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();

        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            // Handle code block boundaries
            if let Some(cap) = CODE_FENCE_REGEX.captures(line) {
                let marker = cap[0].to_string();
                if !in_code_block {
                    in_code_block = true;
                    code_fence_marker = marker;
                } else if line.trim().starts_with(&code_fence_marker) {
                    in_code_block = false;
                    code_fence_marker.clear();
                }
                continue;
            }

            // Skip lines in code blocks
            if in_code_block {
                continue;
            }

            // Process ATX headings
            if let Some(cap) = ATX_HEADING_REGEX.captures(line) {
                let heading_text = cap.get(2).unwrap().as_str();
                let fragment = self.heading_to_fragment(heading_text);
                headings.insert(fragment.to_lowercase());
                continue;
            }

            // Process Setext headings
            if i < lines.len() - 1 {
                let next_line = lines[i + 1];
                if !line.is_empty() && !next_line.is_empty() {
                    let trimmed_next = next_line.trim();
                    if (trimmed_next.starts_with('=') && trimmed_next.chars().all(|c| c == '='))
                        || (trimmed_next.starts_with('-') && trimmed_next.chars().all(|c| c == '-'))
                    {
                        let fragment = self.heading_to_fragment(line.trim());
                        headings.insert(fragment.to_lowercase());
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

        // Handle code spans more thoroughly
        if let Ok(captures) = INLINE_CODE_REGEX
            .captures_iter(&stripped.clone())
            .collect::<Result<Vec<_>, _>>()
        {
            for cap in captures {
                if let Some(code_match) = cap.get(0) {
                    // Extract the code content (without the backticks)
                    let code_text = &code_match.as_str()[1..code_match.as_str().len() - 1];
                    // Replace the entire code span with just the text
                    stripped = stripped.replace(code_match.as_str(), code_text);
                }
            }
        }

        // Manual approach for nested formatting
        // First, handle bold formatting with capture groups
        stripped = BOLD_ASTERISK_REGEX.replace_all(&stripped, "$1").to_string();
        stripped = BOLD_UNDERSCORE_REGEX
            .replace_all(&stripped, "$1")
            .to_string();

        // Then handle italic formatting
        stripped = ITALIC_ASTERISK_REGEX
            .replace_all(&stripped, "$1")
            .to_string();
        stripped = ITALIC_UNDERSCORE_REGEX
            .replace_all(&stripped, "$1")
            .to_string();

        // Handle links more thoroughly
        if let Ok(captures) = LINK_TEXT_REGEX
            .captures_iter(&stripped.clone())
            .collect::<Result<Vec<_>, _>>()
        {
            for cap in captures {
                if let (Some(full_match), Some(text_match)) = (cap.get(0), cap.get(1)) {
                    // Replace the entire link with just the link text
                    stripped = stripped.replace(full_match.as_str(), text_match.as_str());
                }
            }
        }

        // Remove strikethrough
        stripped = STRIKETHROUGH_REGEX.replace_all(&stripped, "$1").to_string();

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

    fn check(&self, content: &str) -> LintResult {
        let structure = DocumentStructure::new(content);
        let mut warnings = Vec::new();
        let headings = self.extract_headings(content);
        let mut in_toc_section = false;

        for (line_num, line) in content.lines().enumerate() {
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

            // Only check for links if line contains a link
            if !line.contains('[') || !line.contains(')') {
                continue;
            }

            // Use regex to find all links with fragments
            if let Ok(caps) = LINK_REGEX.captures_iter(line).collect::<Result<Vec<_>, _>>() {
                for cap in caps {
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
                            message: format!("Link fragment '#{}' does not exist in document headings.", fragment),
                            severity: Severity::Warning,
                            fix: None,
                        });
                    }
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // No automatic fix for missing fragments, just return content as-is
        Ok(content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}
