use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    // Standard front matter delimiter (three dashes)
    static ref STANDARD_FRONT_MATTER_START: Regex = Regex::new(r"^---\s*$").unwrap();
    static ref STANDARD_FRONT_MATTER_END: Regex = Regex::new(r"^---\s*$").unwrap();
    
    // Common malformed front matter (dash space dash dash)
    static ref MALFORMED_FRONT_MATTER_START1: Regex = Regex::new(r"^- --\s*$").unwrap();
    static ref MALFORMED_FRONT_MATTER_END1: Regex = Regex::new(r"^- --\s*$").unwrap();
    
    // Alternate malformed front matter (dash dash space dash)
    static ref MALFORMED_FRONT_MATTER_START2: Regex = Regex::new(r"^-- -\s*$").unwrap();
    static ref MALFORMED_FRONT_MATTER_END2: Regex = Regex::new(r"^-- -\s*$").unwrap();
}

/// Utility functions for detecting and handling front matter in Markdown documents
pub struct FrontMatterUtils;

impl FrontMatterUtils {
    /// Check if a line is inside front matter content
    pub fn is_in_front_matter(content: &str, line_num: usize) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        if line_num >= lines.len() {
            return false;
        }
        
        let mut in_standard_front_matter = false;
        let mut in_malformed_front_matter1 = false;
        let mut in_malformed_front_matter2 = false;
        
        for (i, line) in lines.iter().enumerate() {
            if i > line_num {
                break;
            }
            
            // Standard front matter handling
            if i == 0 && STANDARD_FRONT_MATTER_START.is_match(line) {
                in_standard_front_matter = true;
            } else if STANDARD_FRONT_MATTER_END.is_match(line) && in_standard_front_matter && i > 0 {
                in_standard_front_matter = false;
            }
            
            // Malformed front matter type 1 (- --)
            else if i == 0 && MALFORMED_FRONT_MATTER_START1.is_match(line) {
                in_malformed_front_matter1 = true;
            } else if MALFORMED_FRONT_MATTER_END1.is_match(line) && in_malformed_front_matter1 && i > 0 {
                in_malformed_front_matter1 = false;
            }
            
            // Malformed front matter type 2 (-- -)
            else if i == 0 && MALFORMED_FRONT_MATTER_START2.is_match(line) {
                in_malformed_front_matter2 = true;
            } else if MALFORMED_FRONT_MATTER_END2.is_match(line) && in_malformed_front_matter2 && i > 0 {
                in_malformed_front_matter2 = false;
            }
        }
        
        // Return true if we're in any type of front matter
        in_standard_front_matter || in_malformed_front_matter1 || in_malformed_front_matter2
    }
    
    /// Check if a content contains front matter with a specific field
    pub fn has_front_matter_field(content: &str, field_prefix: &str) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 3 {
            return false;
        }
        
        // Check for standard front matter
        if STANDARD_FRONT_MATTER_START.is_match(lines[0]) {
            let mut found_end = false;
            
            for i in 1..lines.len() {
                if STANDARD_FRONT_MATTER_END.is_match(lines[i]) {
                    found_end = true;
                    break;
                }
                
                if lines[i].trim().starts_with(field_prefix) {
                    return true;
                }
            }
            
            // Only count as front matter if it has both start and end markers
            if !found_end {
                return false;
            }
        }
        
        // Check for malformed front matter type 1 (- --)
        else if MALFORMED_FRONT_MATTER_START1.is_match(lines[0]) {
            let mut found_end = false;
            
            for i in 1..lines.len() {
                if MALFORMED_FRONT_MATTER_END1.is_match(lines[i]) {
                    found_end = true;
                    break;
                }
                
                if lines[i].trim().starts_with(field_prefix) {
                    return true;
                }
            }
            
            // Only count as front matter if it has both start and end markers
            if !found_end {
                return false;
            }
        }
        
        // Check for malformed front matter type 2 (-- -)
        else if MALFORMED_FRONT_MATTER_START2.is_match(lines[0]) {
            let mut found_end = false;
            
            for i in 1..lines.len() {
                if MALFORMED_FRONT_MATTER_END2.is_match(lines[i]) {
                    found_end = true;
                    break;
                }
                
                if lines[i].trim().starts_with(field_prefix) {
                    return true;
                }
            }
            
            // Only count as front matter if it has both start and end markers
            if !found_end {
                return false;
            }
        }
        
        false
    }
    
    /// Fix malformed front matter
    pub fn fix_malformed_front_matter(content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 3 {
            return content.to_string();
        }
        
        let mut result = Vec::new();
        let mut in_front_matter = false;
        let mut is_malformed = false;
        
        for (i, line) in lines.iter().enumerate() {
            // Handle front matter start
            if i == 0 {
                if STANDARD_FRONT_MATTER_START.is_match(line) {
                    // Standard front matter - keep as is
                    in_front_matter = true;
                    result.push(line.to_string());
                } else if MALFORMED_FRONT_MATTER_START1.is_match(line) || MALFORMED_FRONT_MATTER_START2.is_match(line) {
                    // Malformed front matter - fix it
                    in_front_matter = true;
                    is_malformed = true;
                    result.push("---".to_string());
                } else {
                    // Regular line
                    result.push(line.to_string());
                }
                continue;
            }
            
            // Handle front matter end
            if in_front_matter {
                if STANDARD_FRONT_MATTER_END.is_match(line) {
                    // Standard front matter end - keep as is
                    in_front_matter = false;
                    result.push(line.to_string());
                } else if (MALFORMED_FRONT_MATTER_END1.is_match(line) || MALFORMED_FRONT_MATTER_END2.is_match(line)) && is_malformed {
                    // Malformed front matter end - fix it
                    in_front_matter = false;
                    result.push("---".to_string());
                } else {
                    // Content inside front matter
                    result.push(line.to_string());
                }
                continue;
            }
            
            // Regular line
            result.push(line.to_string());
        }
        
        result.join("\n")
    }
} 