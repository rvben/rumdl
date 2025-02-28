use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashSet;

/// Rule MD051: Link fragments should exist
///
/// This rule is triggered when a link fragment (the part after #) doesn't exist in the document.
pub struct MD051LinkFragments;

impl MD051LinkFragments {
    pub fn new() -> Self {
        Self
    }

    fn extract_headings(&self, content: &str) -> HashSet<String> {
        let mut headings = HashSet::new();
        let heading_regex = Regex::new(r"^#+\s+(.+)$|^(.+)\n[=-]+$").unwrap();

        for line in content.lines() {
            if let Some(cap) = heading_regex.captures(line) {
                let heading = cap.get(1).or_else(|| cap.get(2)).unwrap().as_str();
                headings.insert(self.heading_to_fragment(heading));
            }
        }

        headings
    }

    fn heading_to_fragment(&self, heading: &str) -> String {
        heading
            .to_lowercase()
            .chars()
            .map(|c| match c {
                ' ' => '-',
                c if c.is_alphanumeric() => c,
                _ => '-',
            })
            .collect()
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
        let mut warnings = Vec::new();
        let headings = self.extract_headings(content);
        let link_regex = Regex::new(r"\[([^\]]*)\]\(([^)]+)#([^)]+)\)").unwrap();

        for (line_num, line) in content.lines().enumerate() {
            for cap in link_regex.captures_iter(line) {
                let fragment = &cap[3];
                if !headings.contains(fragment) {
                    let full_match = cap.get(0).unwrap();
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: full_match.start() + 1,
                        message: format!("Link fragment '{}' does not exist", fragment),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: full_match.start() + 1,
                            replacement: format!("[{}]({})", &cap[1], &cap[2]),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let headings = self.extract_headings(content);
        let link_regex = Regex::new(r"\[([^\]]*)\]\(([^)]+)#([^)]+)\)").unwrap();

        let result = link_regex.replace_all(content, |caps: &regex::Captures| {
            let fragment = &caps[3];
            if !headings.contains(fragment) {
                format!("[{}]({})", &caps[1], &caps[2])
            } else {
                caps[0].to_string()
            }
        });

        Ok(result.to_string())
    }
} 