use crate::rule::{LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashSet;

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
        let ref_regex = Regex::new(r"^\s*\[([^\]]+)\]:\s*\S+").unwrap();

        for line in content.lines() {
            if let Some(cap) = ref_regex.captures(line) {
                references.insert(cap[1].to_string());
            }
        }

        references
    }

    fn find_undefined_references<'a>(&self, content: &'a str, references: &HashSet<String>) -> Vec<(usize, usize, &'a str)> {
        let mut undefined = Vec::new();
        let link_regex = Regex::new(r"\[([^\]]+)\]\[([^\]]*)\]|\!\[([^\]]+)\]\[([^\]]*)\]").unwrap();

        for (line_num, line) in content.lines().enumerate() {
            for cap in link_regex.captures_iter(line) {
                let reference = if let Some(m) = cap.get(2) {
                    if m.as_str().is_empty() {
                        cap.get(1).map(|m| m.as_str())
                    } else {
                        Some(m.as_str())
                    }
                } else if let Some(m) = cap.get(4) {
                    if m.as_str().is_empty() {
                        cap.get(3).map(|m| m.as_str())
                    } else {
                        Some(m.as_str())
                    }
                } else {
                    None
                };

                if let Some(ref_text) = reference {
                    if !references.contains(ref_text) {
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