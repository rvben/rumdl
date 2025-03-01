use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashSet;
use lazy_static::lazy_static;

lazy_static! {
    static ref REF_REGEX: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s*\S+").unwrap();
    static ref IMAGE_REGEX: Regex = Regex::new(r"!\[([^\]]+)\]\[([^\]]*)\]").unwrap();
    static ref LINK_REGEX: Regex = Regex::new(r"\[([^\]]+)\](?:\[([^\]]*)\]|\[\])").unwrap();
}

/// Rule MD053: Link and image reference definitions should be needed
///
/// This rule is triggered when a link or image reference definition is not used anywhere in the document.
#[derive(Debug)]
pub struct MD053LinkImageReferenceDefinitions {
    ignored_definitions: Vec<String>,
}

impl Default for MD053LinkImageReferenceDefinitions {
    fn default() -> Self {
        Self {
            ignored_definitions: Vec::new(),
        }
    }
}

impl MD053LinkImageReferenceDefinitions {
    pub fn new(ignored_definitions: Vec<String>) -> Self {
        Self {
            ignored_definitions,
        }
    }

    fn extract_references(&self, content: &str) -> Vec<(usize, usize, String)> {
        let mut references = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            if let Some(cap) = REF_REGEX.captures(line) {
                let reference = cap[1].to_string();
                if !self.ignored_definitions.contains(&reference) {
                    references.push((line_num + 1, cap.get(0).unwrap().start(), reference));
                }
            }
        }

        references
    }

    fn find_reference_usages(&self, content: &str) -> HashSet<String> {
        let mut used_refs = HashSet::new();
        
        for line in content.lines() {
            // Check image references: ![alt][ref] or ![alt][]
            for cap in IMAGE_REGEX.captures_iter(line) {
                let reference = if let Some(m) = cap.get(2) {
                    if m.as_str().is_empty() {
                        cap.get(1).map(|m| m.as_str())
                    } else {
                        Some(m.as_str())
                    }
                } else {
                    None
                };

                if let Some(ref_text) = reference {
                    used_refs.insert(ref_text.to_string());
                }
            }

            // Check link references: [text][ref] or [text][] or [text][text]
            for cap in LINK_REGEX.captures_iter(line) {
                let reference = if let Some(m) = cap.get(2) {
                    if m.as_str().is_empty() {
                        cap.get(1).map(|m| m.as_str())
                    } else {
                        Some(m.as_str())
                    }
                } else {
                    cap.get(1).map(|m| m.as_str())
                };

                if let Some(ref_text) = reference {
                    used_refs.insert(ref_text.to_string());
                }
            }
        }

        used_refs
    }
}

impl Rule for MD053LinkImageReferenceDefinitions {
    fn name(&self) -> &'static str {
        "MD053"
    }

    fn description(&self) -> &'static str {
        "Link and image reference definitions should be needed"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let references = self.extract_references(content);
        let used_refs = self.find_reference_usages(content);

        for (line_num, column, reference) in references {
            if !used_refs.contains(&reference) {
                warnings.push(LintWarning {
                    line: line_num,
                    column: column + 1,
                    message: format!("Unused reference definition '{}'", reference),
                    fix: Some(Fix {
                        line: line_num,
                        column: column + 1,
                        replacement: String::new(), // Remove the unused reference definition
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let references = self.extract_references(content);
        let used_refs = self.find_reference_usages(content);
        let mut result = String::new();
        let mut prev_line_empty = false;

        for (line_num, line) in content.lines().enumerate() {
            let current_line_num = line_num + 1;
            let mut skip_line = false;

            for (ref_line, _, reference) in &references {
                if *ref_line == current_line_num && !used_refs.contains(reference) {
                    skip_line = true;
                    break;
                }
            }

            if !skip_line {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(line);
                prev_line_empty = line.trim().is_empty();
            }
        }

        // Ensure no double empty lines
        let result = result.lines().fold(String::new(), |mut acc, line| {
            if !acc.is_empty() {
                acc.push('\n');
            }
            if !(line.trim().is_empty() && prev_line_empty) {
                acc.push_str(line);
                prev_line_empty = line.trim().is_empty();
            }
            acc
        });

        Ok(result)
    }
} 