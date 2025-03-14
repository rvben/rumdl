use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashSet;
use lazy_static::lazy_static;

lazy_static! {
    static ref HEADING_REGEX: Regex = Regex::new(r"^#+\s+(.+)$|^(.+)\n[=-]+$").unwrap();
    static ref LINK_REGEX: Regex = Regex::new(r"\[([^\]]*)\]\(([^)]+)#([^)]+)\)").unwrap();
    static ref EXTERNAL_URL_REGEX: Regex = Regex::new(r"^(https?://|ftp://|www\.|[^/]+\.[a-z]{2,})").unwrap();
}

/// Rule MD051: Link fragments should exist
///
/// This rule is triggered when a link fragment (the part after #) doesn't exist in the document.
/// This only applies to internal document links, not to external URLs.
pub struct MD051LinkFragments;

impl MD051LinkFragments {
    pub fn new() -> Self {
        Self
    }

    fn extract_headings(&self, content: &str) -> HashSet<String> {
        let mut headings = HashSet::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Process ATX headings (# Heading)
        for line in &lines {
            if line.starts_with('#') {
                if let Some(cap) = HEADING_REGEX.captures(line) {
                    if let Some(m) = cap.get(1) {
                        headings.insert(self.heading_to_fragment(m.as_str()));
                    }
                }
            }
        }
        
        // Process Setext headings (Heading\n===== or Heading\n-----)
        for i in 0..lines.len().saturating_sub(1) {
            let line = lines[i];
            let next_line = lines[i + 1];
            
            if !line.is_empty() && !next_line.is_empty() {
                let trimmed_next = next_line.trim();
                if (trimmed_next.starts_with('=') && trimmed_next.chars().all(|c| c == '=')) ||
                   (trimmed_next.starts_with('-') && trimmed_next.chars().all(|c| c == '-')) {
                    headings.insert(self.heading_to_fragment(line.trim()));
                }
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

    /// Check if a URL is external (has a protocol or domain)
    fn is_external_url(&self, url: &str) -> bool {
        EXTERNAL_URL_REGEX.is_match(url)
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

        for (line_num, line) in content.lines().enumerate() {
            for cap in LINK_REGEX.captures_iter(line) {
                let url = &cap[2];
                let fragment = &cap[3];

                // Skip validation for external URLs
                if self.is_external_url(url) {
                    continue;
                }

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

        let result = LINK_REGEX.replace_all(content, |caps: &regex::Captures| {
            let url = &caps[2];
            let fragment = &caps[3];
            
            // Skip validation for external URLs
            if self.is_external_url(url) {
                return caps[0].to_string();
            }

            if !headings.contains(fragment) {
                format!("[{}]({})", &caps[1], &caps[2])
            } else {
                caps[0].to_string()
            }
        });

        Ok(result.to_string())
    }
} 