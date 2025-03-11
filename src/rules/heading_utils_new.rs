use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    // ATX heading patterns
    static ref ATX_HEADING_PATTERN: Regex = Regex::new(r"^(\s*)(#{1,6})(\s*)(.+?)(\s*)(#*)(\s*)$").unwrap();
    static ref SIMPLE_ATX_HEADING_PATTERN: Regex = Regex::new(r"^(\s*)(#{1,6})(\s+)(.+?)(\s*)$").unwrap();
    
    // Setext heading patterns
    static ref SETEXT_HEADING_UNDERLINE_PATTERN: Regex = Regex::new(r"^(\s*)(=+|-+)(\s*)$").unwrap();
}

/// Represents different styles of Markdown headings
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum HeadingStyle {
    Atx,             // # Heading
    AtxClosed,       // # Heading #
    Setext1,         // Heading
                     // =======
    Setext2,         // Heading
                     // -------
}

/// Represents a heading in a Markdown document
#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub level: usize,
    pub text: String,
    pub style: HeadingStyle,
    pub indentation: usize,
}

/// Utility functions for working with Markdown headings
pub struct HeadingUtilsNew;

impl HeadingUtilsNew {
    /// Check if a line is an ATX heading (starts with #)
    pub fn is_atx_heading(line: &str) -> bool {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            return false;
        }
        
        let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
        if hash_count > 6 {
            return false;
        }
        
        // Check if there's a space after the hash marks or if it's an empty heading
        let after_hash = &trimmed[hash_count..];
        after_hash.is_empty() || after_hash.starts_with(' ')
    }
    
    /// Check if a line is a closed ATX heading (# Heading #)
    pub fn is_closed_atx_heading(line: &str) -> bool {
        let trimmed = line.trim();
        if !Self::is_atx_heading(line) {
            return false;
        }
        
        trimmed.ends_with('#')
    }
    
    /// Check if a line is a Setext heading underline (==== or ----)
    pub fn is_setext_heading_underline(line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return false;
        }
        
        let first_char = trimmed.chars().next().unwrap();
        if first_char != '=' && first_char != '-' {
            return false;
        }
        
        trimmed.chars().all(|c| c == first_char)
    }
    
    /// Check if a line is a Setext heading (considering the next line)
    pub fn is_setext_heading(line: &str, next_line: Option<&str>) -> bool {
        if let Some(next) = next_line {
            !line.trim().is_empty() && Self::is_setext_heading_underline(next)
        } else {
            false
        }
    }
    
    /// Parse a line into a Heading struct if it's a valid ATX heading
    pub fn parse_atx_heading(line: &str) -> Option<Heading> {
        if let Some(caps) = ATX_HEADING_PATTERN.captures(line) {
            let indentation = caps.get(1).map_or("", |m| m.as_str()).len();
            let level = caps.get(2).map_or("", |m| m.as_str()).len();
            let text = caps.get(4).map_or("", |m| m.as_str()).to_string();
            let trailing_hashes = !caps.get(6).map_or("", |m| m.as_str()).is_empty();
            
            let style = if trailing_hashes {
                HeadingStyle::AtxClosed
            } else {
                HeadingStyle::Atx
            };
            
            return Some(Heading { 
                level, 
                text, 
                style,
                indentation
            });
        }
        
        // Try with simpler pattern for basic headings
        if let Some(caps) = SIMPLE_ATX_HEADING_PATTERN.captures(line) {
            let indentation = caps.get(1).map_or("", |m| m.as_str()).len();
            let level = caps.get(2).map_or("", |m| m.as_str()).len();
            let text = caps.get(4).map_or("", |m| m.as_str()).to_string();
            
            return Some(Heading { 
                level, 
                text, 
                style: HeadingStyle::Atx,
                indentation
            });
        }
        
        None
    }
    
    /// Parse a Setext heading (considering the current line and next line)
    pub fn parse_setext_heading(line: &str, next_line: Option<&str>) -> Option<Heading> {
        if let Some(next) = next_line {
            if Self::is_setext_heading_underline(next) {
                let indentation = line.len() - line.trim_start().len();
                let text = line.trim().to_string();
                let first_char = next.trim().chars().next().unwrap();
                
                let style = if first_char == '=' {
                    HeadingStyle::Setext1
                } else {
                    HeadingStyle::Setext2
                };
                
                let level = if style == HeadingStyle::Setext1 { 1 } else { 2 };
                
                return Some(Heading {
                    level,
                    text,
                    style,
                    indentation
                });
            }
        }
        
        None
    }
    
    /// Find the first heading in a document
    pub fn find_first_heading(content: &str) -> Option<(Heading, usize)> {
        let lines: Vec<&str> = content.lines().collect();
        let mut line_num = 0;
        
        while line_num < lines.len() {
            // Check for ATX heading
            if Self::is_atx_heading(lines[line_num]) {
                if let Some(heading) = Self::parse_atx_heading(lines[line_num]) {
                    return Some((heading, line_num));
                }
            }
            
            // Check for Setext heading
            if line_num + 1 < lines.len() {
                if Self::is_setext_heading(lines[line_num], Some(lines[line_num + 1])) {
                    if let Some(heading) = Self::parse_setext_heading(lines[line_num], Some(lines[line_num + 1])) {
                        return Some((heading, line_num));
                    }
                }
            }
            
            line_num += 1;
        }
        
        None
    }
    
    /// Extract all headings from a document
    pub fn extract_all_headings(content: &str) -> Vec<(Heading, usize)> {
        let lines: Vec<&str> = content.lines().collect();
        let mut headings = Vec::new();
        let mut line_num = 0;
        
        while line_num < lines.len() {
            // Check for ATX heading
            if Self::is_atx_heading(lines[line_num]) {
                if let Some(heading) = Self::parse_atx_heading(lines[line_num]) {
                    headings.push((heading, line_num));
                }
            }
            
            // Check for Setext heading
            if line_num + 1 < lines.len() {
                if Self::is_setext_heading(lines[line_num], Some(lines[line_num + 1])) {
                    if let Some(heading) = Self::parse_setext_heading(lines[line_num], Some(lines[line_num + 1])) {
                        headings.push((heading, line_num));
                        // Skip the underline
                        line_num += 1;
                    }
                }
            }
            
            line_num += 1;
        }
        
        headings
    }
    
    /// Convert a heading to ATX style
    pub fn to_atx_style(heading: &Heading) -> String {
        let indent = " ".repeat(heading.indentation);
        let hashes = "#".repeat(heading.level);
        format!("{}{} {}", indent, hashes, heading.text)
    }
    
    /// Convert a heading to closed ATX style
    pub fn to_closed_atx_style(heading: &Heading) -> String {
        let indent = " ".repeat(heading.indentation);
        let hashes = "#".repeat(heading.level);
        format!("{}{} {} {}", indent, hashes, heading.text, hashes)
    }
    
    /// Convert a heading to Setext style (only works for level 1 and 2)
    pub fn to_setext_style(heading: &Heading) -> (String, String) {
        let indent = " ".repeat(heading.indentation);
        let text_line = format!("{}{}", indent, heading.text);
        
        let underline_char = if heading.level == 1 { '=' } else { '-' };
        // Use the same length as the text for the underline
        let underline_length = heading.text.len();
        let underline_line = format!("{}{}", indent, underline_char.to_string().repeat(underline_length));
        
        (text_line, underline_line)
    }
    
    /// Convert a heading text to a valid ID for fragment links
    pub fn heading_to_fragment(text: &str) -> String {
        // Remove any HTML tags
        let html_tag_pattern = Regex::new(r"<[^>]*>").unwrap();
        let text = html_tag_pattern.replace_all(text, "").to_string();
        
        // Convert to lowercase
        let text = text.to_lowercase();
        
        // Replace spaces with hyphens
        let text = text.replace(" ", "-");
        
        // Remove any non-alphanumeric characters except hyphens
        let text = text.chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>();
        
        // Remove leading and trailing hyphens
        let text = text.trim_matches('-').to_string();
        
        text
    }
} 