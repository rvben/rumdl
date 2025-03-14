use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;

use crate::rule::{LintError, LintResult, LintWarning, Rule};
use crate::rules::heading_utils::{extract_heading_text, get_heading_level};

lazy_static! {
    // Optimized patterns with fast path checks
    static ref ATX_HEADING: Regex = Regex::new(r"^(#+)(\s+)(.*)$").unwrap();
    static ref CLOSED_ATX_HEADING: Regex = Regex::new(r"^(#+)(\s+)(.+?)(\s+#+\s*$)").unwrap();
    static ref SETEXT_HEADING_1: Regex = Regex::new(r"^=+\s*$").unwrap();
    static ref SETEXT_HEADING_2: Regex = Regex::new(r"^-+\s*$").unwrap();
    static ref FENCED_CODE_BLOCK: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,}).*$").unwrap();
    static ref FENCED_CODE_END: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,})\s*$").unwrap();
    static ref YAML_FRONT_MATTER_START: Regex = Regex::new(r"^---\s*$").unwrap();
}

/// Structure to track code blocks and front matter
#[derive(Default)]
struct CodeBlockState {
    in_code_block: bool,
    in_front_matter: bool,
    front_matter_started: bool,
}

impl CodeBlockState {
    fn update(&mut self, line: &str) {
        // Fast path - check for literal markers first
        let trimmed = line.trim_start();
        
        // YAML front matter handling
        if !self.front_matter_started && line.trim() == "---" && YAML_FRONT_MATTER_START.is_match(line) {
            self.front_matter_started = true;
            self.in_front_matter = true;
            return;
        } else if self.in_front_matter && line.trim() == "---" {
            self.in_front_matter = false;
            return;
        }
        
        // Skip the rest if we're in front matter
        if self.in_front_matter {
            return;
        }
        
        // Code block handling
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            if !self.in_code_block && FENCED_CODE_BLOCK.is_match(line) {
                self.in_code_block = true;
            } else if self.in_code_block && FENCED_CODE_END.is_match(line) {
                self.in_code_block = false;
            }
        }
    }
}

/// Checks if a line is a heading
fn is_heading(content_lines: &[&str], index: usize, code_block_state: &CodeBlockState) -> bool {
    // Skip if we're in a code block or front matter
    if code_block_state.in_code_block || code_block_state.in_front_matter {
        return false;
    }
    
    let line = content_lines[index];
    let trimmed = line.trim();
    
    // Fast path checks before regex
    if trimmed.is_empty() {
        return false;
    }
    
    // Check for ATX style headings (# Heading)
    if trimmed.starts_with('#') {
        return ATX_HEADING.is_match(line) || CLOSED_ATX_HEADING.is_match(line);
    }
    
    // Check for setext style headings (followed by ==== or ----)
    if index + 1 < content_lines.len() {
        let next_line = content_lines[index + 1].trim();
        if !next_line.is_empty() && 
           (next_line.starts_with('=') && SETEXT_HEADING_1.is_match(next_line)) || 
           (next_line.starts_with('-') && SETEXT_HEADING_2.is_match(next_line)) {
            return true;
        }
    }
    
    false
}

/// A rule that checks for multiple headings with the same content
#[derive(Default)]
pub struct MD024MultipleHeadings {
    allow_different_nesting: bool,
}

impl MD024MultipleHeadings {
    /// Create a new instance with configuration
    pub fn new(allow_different_nesting: bool) -> Self {
        MD024MultipleHeadings {
            allow_different_nesting,
        }
    }
    
    /// Get the heading signature based on configuration
    fn get_heading_signature(&self, text: &str, level: usize) -> String {
        if self.allow_different_nesting {
            // If different nesting levels are allowed, only track by text
            text.to_string()
        } else {
            // Otherwise track by text AND level
            format!("{}:{}", level, text)
        }
    }
}

impl Rule for MD024MultipleHeadings {
    fn name(&self) -> &'static str {
        "MD024"
    }

    fn description(&self) -> &'static str {
        "Multiple headings with the same content"
    }

    fn check(&self, content: &str) -> LintResult {
        // Early return for empty content
        if content.is_empty() {
            return Ok(vec![]);
        }
        
        let mut warnings = Vec::new();
        let content_lines: Vec<&str> = content.lines().collect();
        let mut code_block_state = CodeBlockState::default();
        
        // Track headings by their signature
        let mut headings = HashMap::new();
        
        // First pass - identify headings and their lines
        let mut i = 0;
        while i < content_lines.len() {
            // Update code block state
            code_block_state.update(content_lines[i]);
            
            // Check if this line is a heading
            if is_heading(&content_lines, i, &code_block_state) {
                let heading_level = get_heading_level(&content_lines, i);
                let heading_text = extract_heading_text(&content_lines, i, heading_level);
                
                // Skip empty headings
                if !heading_text.trim().is_empty() {
                    let signature = self.get_heading_signature(&heading_text, heading_level);
                    
                    // Check if we've seen this heading before
                    if let Some(first_occurrence) = headings.get(&signature) {
                        warnings.push(LintWarning {
                            line: i + 1,
                            column: 1,
                            message: format!("Multiple headings with the same content (first occurrence at line {})", first_occurrence),
                            fix: None,
                        });
                    } else {
                        // First occurrence
                        headings.insert(signature, i + 1);
                    }
                }
                
                // Skip the next line if this is a setext heading
                if heading_level == 1 && i + 1 < content_lines.len() && SETEXT_HEADING_1.is_match(content_lines[i + 1]) {
                    i += 1;
                } else if heading_level == 2 && i + 1 < content_lines.len() && SETEXT_HEADING_2.is_match(content_lines[i + 1]) {
                    i += 1;
                }
            }
            
            i += 1;
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // No automatic fix for multiple headings with the same content
        // The user needs to decide how to make each heading unique
        Ok(content.to_string())
    }
} 