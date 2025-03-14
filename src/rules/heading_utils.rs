use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    // Optimized regex patterns with more efficient non-capturing groups
    static ref ATX_HEADING: Regex = Regex::new(r"^(\s*)(#{1,6})(\s+)(.*)$").unwrap();
    static ref CLOSED_ATX_HEADING: Regex = Regex::new(r"^(\s*)(#{1,6})(\s+)(.+?)(\s+#{1,6}\s*)?$").unwrap();
    static ref SETEXT_HEADING_1: Regex = Regex::new(r"^(\s*)(=+)(\s*)$").unwrap();
    static ref SETEXT_HEADING_2: Regex = Regex::new(r"^(\s*)(-+)(\s*)$").unwrap();
    static ref FENCED_CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,}).*$").unwrap();
    static ref FENCED_CODE_BLOCK_END: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,})\s*$").unwrap();
    static ref FRONT_MATTER_DELIMITER: Regex = Regex::new(r"^---\s*$").unwrap();
    static ref INDENTED_CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s{4,})").unwrap();
    
    // Valid emphasis patterns at start of line that should not be confused with headings or lists
    static ref VALID_START_EMPHASIS: Regex = Regex::new(r"^(\s*)(\*\*[^*\s]|\*[^*\s]|__[^_\s]|_[^_\s])").unwrap();
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
}

/// Utility functions for working with Markdown headings
pub struct HeadingUtils;

impl HeadingUtils {
    /// Check if a line is an ATX heading (starts with #)
    pub fn is_atx_heading(line: &str) -> bool {
        let re = Regex::new(r"^#{1,6}(?:\s+.+|\s*$)").unwrap();
        re.is_match(line)
    }
    
    /// Check if a line is inside a code block
    pub fn is_in_code_block(content: &str, line_num: usize) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        if line_num >= lines.len() {
            return false;
        }
        
        let mut in_code_block = false;
        let mut in_alternate_code_block = false;
        
        for (i, line) in lines.iter().enumerate() {
            if i > line_num {
                break;
            }
            
            if FENCED_CODE_BLOCK_START.is_match(line) {
                in_code_block = true;
            } else if FENCED_CODE_BLOCK_END.is_match(line) && in_code_block {
                in_code_block = false;
            } else if FENCED_CODE_BLOCK_START.is_match(line) {
                in_alternate_code_block = true;
            } else if FENCED_CODE_BLOCK_END.is_match(line) && in_alternate_code_block {
                in_alternate_code_block = false;
            }
        }
        
        // Check if the current line is indented as code block
        if line_num < lines.len() && INDENTED_CODE_BLOCK_PATTERN.is_match(lines[line_num]) {
            return true;
        }
        
        // Return true if we're in any type of code block
        in_code_block || in_alternate_code_block
    }
    
    /// Check if a line starts with valid emphasis markers rather than list markers
    pub fn is_start_emphasis(line: &str) -> bool {
        VALID_START_EMPHASIS.is_match(line)
    }

    /// Parse a line into a Heading struct if it's a valid heading
    pub fn parse_heading(content: &str, line_num: usize) -> Option<Heading> {
        let lines: Vec<&str> = content.lines().collect();
        if line_num >= lines.len() {
            return None;
        }

        // Skip processing if we're in a code block
        if Self::is_in_code_block(content, line_num) {
            return None;
        }

        let line = lines[line_num];
        
        // ATX style (#)
        if let Some(atx_heading) = Self::parse_atx_heading(line) {
            return Some(atx_heading);
        }

        // Check for setext style (=== or ---)
        if line_num + 1 < lines.len() {
            let next_line = lines[line_num + 1];
            let next_trimmed = next_line.trim();

            // Check if next line is a valid setext underline
            if !next_trimmed.is_empty() && next_trimmed.chars().all(|c| c == '=' || c == '-') {
                let level = if next_trimmed.starts_with('=') { 1 } else { 2 };
                let style = if level == 1 { HeadingStyle::Setext1 } else { HeadingStyle::Setext2 };
                
                // Get the indentation of both lines
                let heading_indent = line.len() - line.trim_start().len();
                let underline_indent = next_line.len() - next_line.trim_start().len();
                
                // For setext headings, we allow any indentation as long as it's consistent
                if heading_indent == underline_indent {
                    return Some(Heading { 
                        level, 
                        text: line.trim_start().to_string(), // Keep any trailing spaces and formatting
                        style 
                    });
                }
            }
        }

        None
    }

    fn parse_atx_heading(line: &str) -> Option<Heading> {
        let re = Regex::new(r"^(#{1,6})(?:\s+(.+?))?(?:\s+#*)?$").unwrap();
        if let Some(cap) = re.captures(line) {
            let level = cap[1].len();
            let text = cap.get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            let style = if line.trim_end().matches('#').count() > level {
                HeadingStyle::AtxClosed
            } else {
                HeadingStyle::Atx
            };
            Some(Heading { level, text, style })
        } else {
            None
        }
    }

    /// Convert a heading to a different style
    pub fn convert_heading_style(heading: &Heading, target_style: &HeadingStyle) -> String {
        match target_style {
            HeadingStyle::Atx => {
                format!("{}{}", "#".repeat(heading.level), 
                    if heading.text.is_empty() { String::new() } else { format!(" {}", heading.text.trim()) })
            },
            HeadingStyle::AtxClosed => {
                if heading.level > 6 {
                    format!("{}{}", "#".repeat(heading.level),
                        if heading.text.is_empty() { String::new() } else { format!(" {}", heading.text.trim()) })
                } else {
                    let hashes = "#".repeat(heading.level);
                    if heading.text.is_empty() {
                        format!("{} {}", hashes, hashes)
                    } else {
                        format!("{} {} {}", hashes, heading.text.trim(), hashes)
                    }
                }
            },
            HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                if heading.level > 2 {
                    // Fall back to ATX style for levels > 2
                    format!("{}{}", "#".repeat(heading.level),
                        if heading.text.is_empty() { String::new() } else { format!(" {}", heading.text.trim()) })
                } else {
                    let text = heading.text.clone(); // Keep original formatting
                    let underline_char = if heading.level == 1 { '=' } else { '-' };
                    let underline = underline_char.to_string().repeat(text.trim().chars().count().max(3));
                    format!("{}\n{}", text, underline)
                }
            }
        }
    }

    pub fn get_indentation(line: &str) -> usize {
        line.len() - line.trim_start().len()
    }

    pub fn get_heading_text(line: &str) -> Option<String> {
        if let Some(heading) = Self::parse_heading(line, 0) {
            Some(heading.text)
        } else {
            None
        }
    }

    /// Convert a heading text to a valid ID for fragment links
    pub fn heading_to_fragment(text: &str) -> String {
        // Remove any HTML tags
        let text = text.replace("<[^>]*>", "");
        
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

/// Checks if a line is a heading
pub fn is_heading(line: &str) -> bool {
    // Fast path checks first
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    
    if trimmed.starts_with('#') {
        // Check for ATX heading
        ATX_HEADING.is_match(line) || CLOSED_ATX_HEADING.is_match(line)
    } else {
        // We can't tell for setext headings without looking at the next line
        false
    }
}

/// Checks if a line is a setext heading marker
pub fn is_setext_heading_marker(line: &str) -> bool {
    // Fast path checks first
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    
    if trimmed.starts_with('=') {
        SETEXT_HEADING_1.is_match(line)
    } else if trimmed.starts_with('-') {
        SETEXT_HEADING_2.is_match(line)
    } else {
        false
    }
}

/// Checks if a heading is a setext heading (using underlines)
pub fn is_setext_heading(lines: &[&str], index: usize) -> bool {
    if index + 1 >= lines.len() {
        return false;
    }
    
    let next_line = lines[index + 1];
    is_setext_heading_marker(next_line)
}

/// Gets the heading level of a heading at the specified line
pub fn get_heading_level(lines: &[&str], index: usize) -> usize {
    let line = lines[index];
    let trimmed = line.trim();
    
    // Fast path checks first
    if trimmed.is_empty() {
        return 0;
    }
    
    // ATX heading
    if trimmed.starts_with('#') {
        if let Some(caps) = ATX_HEADING.captures(line) {
            if let Some(hashes) = caps.get(2) {
                return hashes.as_str().len();
            }
        }
        
        if let Some(caps) = CLOSED_ATX_HEADING.captures(line) {
            if let Some(hashes) = caps.get(2) {
                return hashes.as_str().len();
            }
        }
    }
    
    // Setext heading
    if index + 1 < lines.len() {
        let next_line = lines[index + 1];
        
        if SETEXT_HEADING_1.is_match(next_line) {
            return 1;
        } else if SETEXT_HEADING_2.is_match(next_line) {
            return 2;
        }
    }
    
    0
}

/// Extracts the text of a heading at the specified line
pub fn extract_heading_text(lines: &[&str], index: usize, level: usize) -> String {
    let line = lines[index];
    
    // Fast path for empty line
    if line.trim().is_empty() {
        return String::new();
    }
    
    // ATX heading
    if level >= 1 && level <= 6 {
        if let Some(caps) = ATX_HEADING.captures(line) {
            if let Some(text) = caps.get(4) {
                return text.as_str().trim_end().to_string();
            }
        }
        
        if let Some(caps) = CLOSED_ATX_HEADING.captures(line) {
            if let Some(text) = caps.get(4) {
                return text.as_str().trim_end().to_string();
            }
        }
    }
    
    // Setext heading
    if (level == 1 || level == 2) && index + 1 < lines.len() {
        let next_line = lines[index + 1];
        if (level == 1 && SETEXT_HEADING_1.is_match(next_line)) || 
           (level == 2 && SETEXT_HEADING_2.is_match(next_line)) {
            return line.trim().to_string();
        }
    }
    
    String::new()
}

/// Gets the indentation of a heading at the specified line
pub fn get_heading_indentation(lines: &[&str], index: usize) -> usize {
    let line = lines[index];
    
    // Fast path for empty line
    if line.is_empty() {
        return 0;
    }
    
    // Count leading spaces
    let mut spaces = 0;
    for c in line.chars() {
        if c == ' ' {
            spaces += 1;
        } else if c == '\t' {
            spaces += 4; // Convention: 1 tab = 4 spaces
        } else {
            break;
        }
    }
    
    spaces
}

/// Checks if a line is a code block marker
pub fn is_code_block_delimiter(line: &str) -> bool {
    // Fast path checks first
    let trimmed = line.trim_start();
    if !trimmed.starts_with("```") && !trimmed.starts_with("~~~") {
        return false;
    }
    
    FENCED_CODE_BLOCK_START.is_match(line) || FENCED_CODE_BLOCK_END.is_match(line)
}

/// Checks if a line is a front matter delimiter
pub fn is_front_matter_delimiter(line: &str) -> bool {
    line.trim() == "---" && FRONT_MATTER_DELIMITER.is_match(line)
}

/// Removes trailing heading marker for closed ATX headings
pub fn remove_trailing_hashes(text: &str) -> String {
    // Fast path if no trailing hashes likely present
    if !text.contains(" #") {
        return text.to_string();
    }
    
    // Find the start of potential trailing hashes
    let mut hash_pos = text.len();
    let mut in_trailing_hashes = false;
    
    for (i, c) in text.char_indices().rev() {
        if c == '#' && !in_trailing_hashes {
            in_trailing_hashes = true;
            hash_pos = i;
        } else if c == ' ' && in_trailing_hashes {
            // Keep going, this is a space before the trailing hashes
        } else if in_trailing_hashes {
            // We found a non-space, non-hash character, so set the position after it
            hash_pos = i + 1;
            break;
        }
    }
    
    if in_trailing_hashes && hash_pos < text.len() {
        let result = text[..hash_pos].trim_end();
        return result.to_string();
    }
    
    text.to_string()
}

/// Normalizes a heading line by ensuring proper spacing, etc.
pub fn normalize_heading(line: &str, level: usize) -> String {
    // Fast path for empty line
    if line.trim().is_empty() {
        return line.to_string();
    }
    
    // Handle ATX headings
    if line.trim_start().starts_with('#') {
        if let Some(caps) = ATX_HEADING.captures(line) {
            let indentation = caps.get(1).map_or("", |m| m.as_str());
            let text_match = caps.get(4).map_or("", |m| m.as_str());
            let text = text_match.trim_end();
            
            // Remove trailing hashes for closed ATX headings
            let cleaned_text = if text.ends_with('#') {
                let ths = remove_trailing_hashes(text);
                ths.trim_end().to_string()
            } else {
                text.to_string()
            };
            
            return format!("{}{} {}", indentation, "#".repeat(level), cleaned_text);
        }
        
        if let Some(caps) = CLOSED_ATX_HEADING.captures(line) {
            let indentation = caps.get(1).map_or("", |m| m.as_str());
            let text = caps.get(4).map_or("", |m| m.as_str()).trim_end();
            
            // Normalize to regular ATX style (removing closing hashes)
            return format!("{}{} {}", indentation, "#".repeat(level), text);
        }
    }
    
    // If we get here, just return the original line
    line.to_string()
} 