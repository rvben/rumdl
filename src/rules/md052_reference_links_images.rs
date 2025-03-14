use crate::rule::{LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::{HashSet, HashMap};
use lazy_static::lazy_static;

lazy_static! {
    // Pattern to match reference definitions [ref]: url
    static ref REF_REGEX: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s*\S+").unwrap();
    
    // Pattern to match reference links and images ONLY: [text][reference] or ![text][reference]
    static ref REF_LINK_REGEX: Regex = Regex::new(r"\[([^\]]+)\]\[([^\]]*)\]").unwrap();
    static ref REF_IMAGE_REGEX: Regex = Regex::new(r"!\[([^\]]+)\]\[([^\]]*)\]").unwrap();
    
    // Pattern for shortcut reference links [reference]
    static ref SHORTCUT_REF_REGEX: Regex = Regex::new(r"\[([^\]]+)\]").unwrap();
    
    // Pattern to match inline links and images (to exclude them)
    static ref INLINE_LINK_REGEX: Regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    static ref INLINE_IMAGE_REGEX: Regex = Regex::new(r"!\[([^\]]+)\]\(([^)]+)\)").unwrap();
    
    // Pattern for reference definitions (same as REF_REGEX)
    static ref REF_DEF_PATTERN: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s*\S+").unwrap();
    
    // Patterns for code blocks
    static ref FENCED_CODE_START: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();
    
    // Pattern to match task list items/checklists
    static ref TASK_LIST_REGEX: Regex = Regex::new(r"^\s*[-*+]\s+\[[xX\s]\]\s+").unwrap();
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
        // Use a HashMap to track references already reported to avoid duplicates
        let mut reported_refs = HashMap::new();
        
        // Track if we're inside a code block
        let mut in_code_block = false;
        let mut code_fence_marker: Option<&str> = None;
        
        for (line_num, line) in content.lines().enumerate() {
            // Skip reference definitions to avoid false positives
            if REF_DEF_PATTERN.is_match(line) {
                continue;
            }
            
            // Handle code blocks
            if FENCED_CODE_START.is_match(line) && !in_code_block {
                if let Some(marker) = FENCED_CODE_START.captures(line).and_then(|c| c.get(1)).map(|m| m.as_str()) {
                    // We've entered a code block
                    in_code_block = true;
                    code_fence_marker = Some(marker);
                    continue;
                }
            }
            
            // Check if we're exiting a code block
            if in_code_block {
                if let Some(marker) = code_fence_marker {
                    if line.starts_with(marker) {
                        in_code_block = false;
                        code_fence_marker = None;
                    }
                    // Skip this line and all lines in the code block
                    continue;
                }
            }
            
            // Skip checking lines in code blocks
            if in_code_block {
                continue;
            }

            // First, identify all inline links/images with their positions
            let mut inline_elements = Vec::new();
            
            // Capture all inline link positions
            for cap in INLINE_LINK_REGEX.captures_iter(line) {
                if let Some(m) = cap.get(0) {
                    inline_elements.push((m.start(), m.end()));
                    
                    // Also check if there are any references inside this inline link
                    let link_text = cap.get(1).map_or("", |m| m.as_str());
                    let inside_refs = self.extract_references_inside_text(link_text);
                    for &(ref_start, ref_end) in &inside_refs {
                        // Adjust the positions to be relative to the line
                        inline_elements.push((m.start() + 1 + ref_start, m.start() + 1 + ref_end));
                    }
                }
            }
            
            // Capture all inline image positions
            for cap in INLINE_IMAGE_REGEX.captures_iter(line) {
                if let Some(m) = cap.get(0) {
                    inline_elements.push((m.start(), m.end()));
                    
                    // Also check if there are any references inside this inline image
                    let img_text = cap.get(1).map_or("", |m| m.as_str());
                    let inside_refs = self.extract_references_inside_text(img_text);
                    for &(ref_start, ref_end) in &inside_refs {
                        // Adjust the positions to be relative to the line
                        inline_elements.push((m.start() + 2 + ref_start, m.start() + 2 + ref_end));
                    }
                }
            }
            
            // Then check for reference links
            for cap in REF_LINK_REGEX.captures_iter(line) {
                let full_match = cap.get(0).unwrap();
                let match_start = full_match.start();
                let match_end = full_match.end();
                
                // Skip if this match overlaps with an inline element
                if is_position_overlapping(match_start, match_end, &inline_elements) {
                    continue;
                }
                
                // Extract the reference
                if let Some(ref_match) = cap.get(2) {
                    let ref_text = ref_match.as_str();
                    
                    if ref_text.is_empty() {
                        // [text][] format - use text as reference
                        if let Some(text_match) = cap.get(1) {
                            let text = text_match.as_str();
                            let lowercase_text = text.to_lowercase();
                            if !references.contains(&lowercase_text) {
                                // Check if we've already reported this reference on this line
                                let key = format!("{}:{}", line_num + 1, lowercase_text);
                                if !reported_refs.contains_key(&key) {
                                    undefined.push((line_num + 1, match_start, text));
                                    reported_refs.insert(key, true);
                                }
                            }
                        }
                    } else {
                        // [text][reference] format
                        let lowercase_ref = ref_text.to_lowercase();
                        if !references.contains(&lowercase_ref) {
                            // Check if we've already reported this reference on this line
                            let key = format!("{}:{}", line_num + 1, lowercase_ref);
                            if !reported_refs.contains_key(&key) {
                                undefined.push((line_num + 1, match_start, ref_text));
                                reported_refs.insert(key, true);
                            }
                        }
                    }
                }
            }
            
            // Check for shortcut reference links [reference]
            for cap in SHORTCUT_REF_REGEX.captures_iter(line) {
                let full_match = cap.get(0).unwrap();
                let match_start = full_match.start();
                let match_end = full_match.end();
                
                // Skip if this match overlaps with an inline element
                if is_position_overlapping(match_start, match_end, &inline_elements) {
                    continue;
                }
                
                // Skip checklist/task list items: '- [ ] Task description'
                if TASK_LIST_REGEX.is_match(line) && (match_start > 0 && match_start <= 5) {
                    continue;
                }
                
                // Check if this is followed by a [ or ( which would make it part of a reference or inline link
                if match_end < line.len() {
                    let next_char = line[match_end..].chars().next();
                    if let Some(c) = next_char {
                        if c == '[' || c == '(' {
                            continue;
                        }
                    }
                }
                
                // Check if this is preceded by a ! which would make it an image
                if match_start > 0 {
                    let prev_char = line[..match_start].chars().last();
                    if let Some(c) = prev_char {
                        if c == '!' {
                            continue;
                        }
                    }
                }
                
                // Extract the reference
                if let Some(ref_match) = cap.get(1) {
                    let ref_text = ref_match.as_str();
                    let lowercase_ref = ref_text.to_lowercase();
                    
                    if !references.contains(&lowercase_ref) {
                        // Check if we've already reported this reference on this line
                        let key = format!("{}:{}", line_num + 1, lowercase_ref);
                        if !reported_refs.contains_key(&key) {
                            undefined.push((line_num + 1, match_start, ref_text));
                            reported_refs.insert(key, true);
                        }
                    }
                }
            }
            
            // Finally check for reference images
            for cap in REF_IMAGE_REGEX.captures_iter(line) {
                let full_match = cap.get(0).unwrap();
                let match_start = full_match.start();
                let match_end = full_match.end();
                
                // Skip if this match overlaps with an inline element
                if is_position_overlapping(match_start, match_end, &inline_elements) {
                    continue;
                }
                
                // Extract the reference
                if let Some(ref_match) = cap.get(2) {
                    let ref_text = ref_match.as_str();
                    
                    if ref_text.is_empty() {
                        // ![text][] format - use text as reference
                        if let Some(text_match) = cap.get(1) {
                            let text = text_match.as_str();
                            let lowercase_text = text.to_lowercase();
                            if !references.contains(&lowercase_text) {
                                // Check if we've already reported this reference on this line
                                let key = format!("{}:{}", line_num + 1, lowercase_text);
                                if !reported_refs.contains_key(&key) {
                                    undefined.push((line_num + 1, match_start, text));
                                    reported_refs.insert(key, true);
                                }
                            }
                        }
                    } else {
                        // ![text][reference] format
                        let lowercase_ref = ref_text.to_lowercase();
                        if !references.contains(&lowercase_ref) {
                            // Check if we've already reported this reference on this line
                            let key = format!("{}:{}", line_num + 1, lowercase_ref);
                            if !reported_refs.contains_key(&key) {
                                undefined.push((line_num + 1, match_start, ref_text));
                                reported_refs.insert(key, true);
                            }
                        }
                    }
                }
            }
        }

        undefined
    }
    
    // Helper method to extract reference positions inside text (for nested references)
    fn extract_references_inside_text(&self, text: &str) -> Vec<(usize, usize)> {
        let mut positions = Vec::new();
        
        // Find reference links inside the text
        for cap in REF_LINK_REGEX.captures_iter(text) {
            if let Some(m) = cap.get(0) {
                positions.push((m.start(), m.end()));
            }
        }
        
        // Find reference images inside the text
        for cap in REF_IMAGE_REGEX.captures_iter(text) {
            if let Some(m) = cap.get(0) {
                positions.push((m.start(), m.end()));
            }
        }
        
        positions
    }
}

// Helper function to check if a position overlaps with any of the excluded positions
fn is_position_overlapping(start: usize, end: usize, excluded_positions: &[(usize, usize)]) -> bool {
    for &(excl_start, excl_end) in excluded_positions {
        // Check for any overlap between positions
        if start <= excl_end && end >= excl_start {
            return true;
        }
    }
    false
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