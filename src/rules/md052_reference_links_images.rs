use crate::rule::{LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashSet;
use lazy_static::lazy_static;

lazy_static! {
    static ref REF_REGEX: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s*\S+").unwrap();
    static ref LINK_REGEX: Regex = Regex::new(r"\[([^\]]+)\](?:\[([^\]]*)\])?|\!\[([^\]]+)\](?:\[([^\]]*)\])?").unwrap();
    static ref REF_DEF_PATTERN: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s*\S+").unwrap();
}

/// Rule MD052: Reference links and images should use a reference that exists
///
/// This rule is triggered when a reference link or image uses a reference that isn't defined.
pub struct MD052ReferenceLinkImages;

impl MD052ReferenceLinkImages {
    pub fn new() -> Self {
        Self
    }

    fn extract_references(&self, content: &str) -> HashSet<String> {
        let mut references = HashSet::new();

        for line in content.lines() {
            if let Some(cap) = REF_REGEX.captures(line) {
                // Store references in lowercase for case-insensitive comparison
                references.insert(cap[1].to_lowercase());
            }
        }

        references
    }

    fn find_undefined_references<'a>(&self, content: &'a str, references: &HashSet<String>) -> Vec<(usize, usize, &'a str)> {
        let mut undefined = Vec::new();
        
        for (line_num, line) in content.lines().enumerate() {
            // Skip reference definitions to avoid false positives
            if REF_DEF_PATTERN.is_match(line) {
                continue;
            }

            // Handle regular reference links/images with [text][id] format
            for cap in LINK_REGEX.captures_iter(line) {
                let reference = if let Some(m) = cap.get(2) {
                    // [text][id] format
                    if m.as_str().is_empty() {
                        // [text][] format - use text as reference
                        cap.get(1).map(|m| m.as_str())
                    } else {
                        Some(m.as_str())
                    }
                } else if let Some(m) = cap.get(4) {
                    // ![text][id] format
                    if m.as_str().is_empty() {
                        // ![text][] format - use text as reference
                        cap.get(3).map(|m| m.as_str())
                    } else {
                        Some(m.as_str())
                    }
                } else if let Some(m1) = cap.get(1) {
                    let text = m1.as_str();
                    // Handle [text] shortcut reference format - check that it's not part of [text][id] or [text]: definition
                    if !line.contains(&format!("[{}][", text)) && !line.contains(&format!("[{}]:", text)) {
                        Some(text)
                    } else {
                        None
                    }
                } else if let Some(m3) = cap.get(3) {
                    let text = m3.as_str();
                    // Handle ![text] shortcut reference format
                    if !line.contains(&format!("![{}][", text)) {
                        Some(text)
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(ref_text) = reference {
                    // Compare in lowercase for case-insensitive matching
                    if !references.contains(&ref_text.to_lowercase()) {
                        undefined.push((line_num + 1, cap.get(0).unwrap().start(), ref_text));
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

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let references = self.extract_references(content);
        let undefined = self.find_undefined_references(content, &references);

        for (line_num, column, ref_text) in undefined {
            warnings.push(LintWarning {
                line: line_num,
                column: column + 1,
                message: format!("Reference '{}' not found", ref_text),
                fix: None, // Cannot automatically fix undefined references
            });
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Cannot automatically fix undefined references as we don't know the intended URLs
        Ok(content.to_string())
    }
} 