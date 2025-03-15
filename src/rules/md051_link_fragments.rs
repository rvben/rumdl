use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashSet;
use lazy_static::lazy_static;

lazy_static! {
    static ref HEADING_REGEX: Regex = Regex::new(r"^#+\s+(.+)$|^(.+)\n[=-]+$").unwrap();
    static ref LINK_REGEX: Regex = Regex::new(r"\[([^\]]*)\]\((?:([^)]+))?#([^)]+)\)").unwrap();
    static ref EXTERNAL_URL_REGEX: Regex = Regex::new(r"^(https?://|ftp://|www\.|[^/]+\.[a-z]{2,})").unwrap();
    static ref CODE_FENCE_REGEX: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();
    static ref MD_FORMAT_REGEX: Regex = Regex::new(r"(\*\*.*?\*\*|\*.*?\*|__.*?__|_.*?_|`.*?`|~~.*?~~|\[.*?\]\(.*?\))").unwrap();
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
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();
        
        // Process ATX headings (# Heading)
        for (i, line) in lines.iter().enumerate() {
            // Check if we're entering/exiting a code block
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
            if line.starts_with('#') {
                if let Some(cap) = HEADING_REGEX.captures(line) {
                    if let Some(m) = cap.get(1) {
                        let heading_text = m.as_str();
                        let fragment = self.heading_to_fragment(heading_text);
                        headings.insert(fragment.to_lowercase());
                    }
                }
            }
            
            // Process Setext headings
            if i < lines.len() - 1 {
                let next_line = lines[i + 1];
                
                if !line.is_empty() && !next_line.is_empty() {
                    let trimmed_next = next_line.trim();
                    if (trimmed_next.starts_with('=') && trimmed_next.chars().all(|c| c == '=')) ||
                       (trimmed_next.starts_with('-') && trimmed_next.chars().all(|c| c == '-')) {
                        let fragment = self.heading_to_fragment(line.trim());
                        headings.insert(fragment.to_lowercase());
                    }
                }
            }
        }

        headings
    }

    /// Convert a heading to a fragment identifier
    /// This follows GitHub's algorithm more closely:
    /// 1. Strip markdown formatting (code, emphasis, etc.)
    /// 2. Convert to lowercase
    /// 3. Replace spaces with hyphens
    /// 4. Remove non-alphanumeric characters except hyphens
    fn heading_to_fragment(&self, heading: &str) -> String {
        // Directly strip formatting patterns
        let mut stripped = heading.to_string();
        
        // Remove bold formatting
        stripped = stripped.replace("**", "").replace("__", "");
        
        // Remove italic formatting
        stripped = stripped.replace("*", "").replace("_", "");
        
        // Remove inline code
        let code_pattern = Regex::new(r"`[^`]+`").unwrap();
        stripped = code_pattern.replace_all(&stripped, "").to_string();
        
        // Remove links
        let link_pattern = Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap();
        stripped = link_pattern.replace_all(&stripped, "$1").to_string();
        
        // Remove strikethrough
        stripped = stripped.replace("~~", "");
        
        // Then process the remaining text
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
        let multiple_hyphens = Regex::new(r"-{2,}").unwrap();
        multiple_hyphens.replace_all(&fragment, "-").to_string()
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
        
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();
        
        for (line_num, line) in content.lines().enumerate() {
            // Check if we're entering/exiting a code block
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
            
            // Check for invalid link fragments
            for cap in LINK_REGEX.captures_iter(line) {
                let url = cap.get(2).map_or("", |m| m.as_str());
                let fragment = &cap[3];

                // Skip validation for external URLs
                if !url.is_empty() && self.is_external_url(url) {
                    continue;
                }

                // Check if the fragment exists (case-insensitive)
                if !headings.contains(&fragment.to_lowercase()) {
                    let full_match = cap.get(0).unwrap();
                    let text = &cap[1];
                    
                    // Create the fix, accounting for shortcut links
                    let replacement = if url.is_empty() {
                        format!("[{}]", text)
                    } else {
                        format!("[{}]({})", text, url)
                    };
                    
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: full_match.start() + 1,
                        message: format!("Link fragment '{}' does not exist", fragment),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: full_match.start() + 1,
                            replacement,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let headings = self.extract_headings(content);
        
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();
        
        for line in content.lines() {
            // Check if we're entering/exiting a code block
            if let Some(cap) = CODE_FENCE_REGEX.captures(line) {
                let marker = cap[0].to_string();
                if !in_code_block {
                    in_code_block = true;
                    code_fence_marker = marker;
                } else if line.trim().starts_with(&code_fence_marker) {
                    in_code_block = false;
                    code_fence_marker.clear();
                }
                
                // Add the line as-is
                result.push_str(line);
                result.push('\n');
                continue;
            }
            
            // Lines in code blocks are left unchanged
            if in_code_block {
                result.push_str(line);
                result.push('\n');
                continue;
            }
            
            // Process links in normal text
            let processed_line = LINK_REGEX.replace_all(line, |caps: &regex::Captures| {
                let url = caps.get(2).map_or("", |m| m.as_str());
                let fragment = &caps[3];
                let text = &caps[1];
                
                // Skip validation for external URLs
                if !url.is_empty() && self.is_external_url(url) {
                    return caps[0].to_string();
                }

                // Check if the fragment exists (case-insensitive)
                if !headings.contains(&fragment.to_lowercase()) {
                    if url.is_empty() {
                        format!("[{}]", text)
                    } else {
                        format!("[{}]({})", text, url)
                    }
                } else {
                    caps[0].to_string()
                }
            });
            
            result.push_str(&processed_line);
            result.push('\n');
        }
        
        // Remove the trailing newline if the original content doesn't end with one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 