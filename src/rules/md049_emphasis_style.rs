use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Fancy regex patterns with lookbehind assertions
    static ref UNDERSCORE_PATTERN: FancyRegex = FancyRegex::new(r"(?<!\\)_([^\s_][^\n_]*?[^\s_])(?<!\\)_").unwrap();
    static ref ASTERISK_PATTERN: FancyRegex = FancyRegex::new(r"(?<!\\)\*([^\s\*][^\n\*]*?[^\s\*])(?<!\\)\*").unwrap();
    
    // Code block detection
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(```|~~~)").unwrap();
    static ref CODE_SPAN_PATTERN: Regex = Regex::new(r"`+").unwrap();
    
    // URL detection
    static ref MARKDOWN_LINK_PATTERN: Regex = Regex::new(r"\[.*?\]\(.*?\)").unwrap();
    static ref MARKDOWN_LINK_URL_PART: Regex = Regex::new(r"\[.*?\]\(([^)]+)").unwrap();
    static ref URL_PATTERN: Regex = Regex::new(r"https?://[^\s)]+").unwrap();
}

/// Emphasis style for MD049
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum EmphasisStyle {
    /// Use consistent style based on first occurrence
    Consistent,
    /// Use asterisks for emphasis
    Asterisk,
    /// Use underscores for emphasis
    Underscore,
}

impl Default for EmphasisStyle {
    fn default() -> Self {
        EmphasisStyle::Consistent
    }
}

impl From<&str> for EmphasisStyle {
    fn from(s: &str) -> Self {
        match s {
            "asterisk" => EmphasisStyle::Asterisk,
            "underscore" => EmphasisStyle::Underscore,
            _ => EmphasisStyle::Consistent,
        }
    }
}

/// MD049 - Emphasis style should be consistent
///
/// This rule is triggered when the style for emphasis is inconsistent:
/// - Asterisks: `*text*`
/// - Underscores: `_text_`
///
/// This rule is focused on regular emphasis, not strong emphasis.
#[derive(Debug, Default, Clone)]
pub struct MD049EmphasisStyle {
    style: EmphasisStyle,
}

impl MD049EmphasisStyle {
    /// Create a new instance of MD049EmphasisStyle
    pub fn new(style: EmphasisStyle) -> Self {
        MD049EmphasisStyle { style }
    }

    /// Determine if the content is a URL or part of a Markdown link
    fn is_url(&self, content: &str, full_content: &str, start_pos: usize, end_pos: usize) -> bool {
        // Check for standard URL patterns
        if content.contains("http://") || content.contains("https://") || content.contains("ftp://") {
            return true;
        }

        // Check if this position is inside a Markdown link URL
        let slice_up_to_end = &full_content[..end_pos];
        
        // Look for link patterns that might contain this position
        for captures in MARKDOWN_LINK_URL_PART.captures_iter(slice_up_to_end) {
            if let Some(url_match) = captures.get(1) {
                let url_start = url_match.start();
                let url_end = url_match.end();
                
                // If our emphasized text is within a URL part of a link, skip it
                if start_pos >= url_start && end_pos <= url_end {
                    return true;
                }
            }
        }
        
        // Check if the position is within an explicit URL
        for url_match in URL_PATTERN.find_iter(full_content) {
            if start_pos >= url_match.start() && end_pos <= url_match.end() {
                return true;
            }
        }

        false
    }

    /// Detect all code blocks in the content
    fn detect_code_blocks(&self, content: &str) -> Vec<(usize, usize)> {
        let mut blocks = Vec::new();
        let mut in_code_block = false;
        let mut code_block_start = 0;
        
        // Find fenced code blocks
        for (i, line) in content.lines().enumerate() {
            let line_start = if i == 0 {
                0
            } else {
                content.lines().take(i).map(|l| l.len() + 1).sum()
            };
            
            if CODE_BLOCK_PATTERN.is_match(line.trim()) {
                if !in_code_block {
                    code_block_start = line_start;
                    in_code_block = true;
                } else {
                    let code_block_end = line_start + line.len();
                    blocks.push((code_block_start, code_block_end));
                    in_code_block = false;
                }
            }
        }
        
        // Handle unclosed code blocks
        if in_code_block {
            blocks.push((code_block_start, content.len()));
        }
        
        // Find inline code spans
        let mut i = 0;
        while i < content.len() {
            if let Some(m) = CODE_SPAN_PATTERN.find_at(content, i) {
                let backtick_length = m.end() - m.start();
                let start = m.start();
                
                // Find matching closing backticks
                if let Some(end_pos) = content[m.end()..].find(&"`".repeat(backtick_length)) {
                    let end = m.end() + end_pos + backtick_length;
                    blocks.push((start, end));
                    i = end;
                } else {
                    i = m.end();
                }
            } else {
                break;
            }
        }
        
        blocks.sort_by(|a, b| a.0.cmp(&b.0));
        blocks
    }
    
    /// Check if a position is within a code block or code span
    fn is_in_code_block_or_span(&self, blocks: &[(usize, usize)], pos: usize) -> bool {
        blocks.iter().any(|&(start, end)| pos >= start && pos < end)
    }

    /// Determine the target emphasis style based on the content and configured style
    fn get_target_style(&self, content: &str) -> EmphasisStyle {
        match self.style {
            EmphasisStyle::Consistent => {
                // Find the first emphasis marker to determine the style
                if let Ok(asterisk_matches) = ASTERISK_PATTERN.find_iter(content).collect::<Result<Vec<_>, _>>() {
                    if let Ok(underscore_matches) = UNDERSCORE_PATTERN.find_iter(content).collect::<Result<Vec<_>, _>>() {
                        if !asterisk_matches.is_empty() && !underscore_matches.is_empty() {
                            // Compare positions of first matches
                            let first_asterisk = asterisk_matches[0].start();
                            let first_underscore = underscore_matches[0].start();
                            
                            if first_asterisk < first_underscore {
                                EmphasisStyle::Asterisk
                            } else {
                                EmphasisStyle::Underscore
                            }
                        } else if !asterisk_matches.is_empty() {
                            EmphasisStyle::Asterisk
                        } else if !underscore_matches.is_empty() {
                            EmphasisStyle::Underscore
                        } else {
                            // No emphasis found, default to asterisk
                            EmphasisStyle::Asterisk
                        }
                    } else {
                        // If underscore regex fails, default to asterisks
                        EmphasisStyle::Asterisk
                    }
                } else {
                    // If asterisk regex fails, check underscores
                    if let Ok(underscore_matches) = UNDERSCORE_PATTERN.find_iter(content).collect::<Result<Vec<_>, _>>() {
                        if !underscore_matches.is_empty() {
                            EmphasisStyle::Underscore
                        } else {
                            // No emphasis found, default to asterisk
                            EmphasisStyle::Asterisk
                        }
                    } else {
                        // Both regexes failed, default to asterisk
                        EmphasisStyle::Asterisk
                    }
                }
            }
            style => style,
        }
    }
}

impl Rule for MD049EmphasisStyle {
    fn name(&self) -> &'static str {
        "MD049"
    }

    fn description(&self) -> &'static str {
        "Emphasis style should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        
        // Determine emphasis style
        let target_style = self.get_target_style(content);
        
        // Detect code blocks and spans
        let code_blocks = self.detect_code_blocks(content);
        
        // Process emphasis based on target style
        if target_style == EmphasisStyle::Asterisk {
            // Look for underscores to convert to asterisks
            if let Ok(matches) = UNDERSCORE_PATTERN.find_iter(content).collect::<Result<Vec<_>, _>>() {
                for m in matches {
                    let start_pos = m.start();
                    let end_pos = m.end();
                    
                    // Skip if in code block or URL
                    if self.is_in_code_block_or_span(&code_blocks, start_pos) ||
                       self.is_url(&content[start_pos..end_pos], content, start_pos, end_pos) {
                        continue;
                    }
                    
                    // Get line and column information
                    let mut line_num = 1;
                    let mut col_num = 1;
                    for (i, c) in content.chars().enumerate() {
                        if i == start_pos {
                            break;
                        }
                        if c == '\n' {
                            line_num += 1;
                            col_num = 1;
                        } else {
                            col_num += 1;
                        }
                    }
                    
                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        line: line_num,
                        column: col_num,
                        message: "Emphasis should use asterisks (*) instead of underscores (_)".to_string(),
                        fix: None,
                        severity: Severity::Warning,
                    });
                }
            }
        } else {
            // Look for asterisks to convert to underscores
            if let Ok(matches) = ASTERISK_PATTERN.find_iter(content).collect::<Result<Vec<_>, _>>() {
                for m in matches {
                    let start_pos = m.start();
                    let end_pos = m.end();
                    
                    // Skip if in code block or URL
                    if self.is_in_code_block_or_span(&code_blocks, start_pos) ||
                       self.is_url(&content[start_pos..end_pos], content, start_pos, end_pos) {
                        continue;
                    }
                    
                    // Get line and column information
                    let mut line_num = 1;
                    let mut col_num = 1;
                    for (i, c) in content.chars().enumerate() {
                        if i == start_pos {
                            break;
                        }
                        if c == '\n' {
                            line_num += 1;
                            col_num = 1;
                        } else {
                            col_num += 1;
                        }
                    }
                    
                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        line: line_num,
                        column: col_num,
                        message: "Emphasis should use underscores (_) instead of asterisks (*)".to_string(),
                        fix: None,
                        severity: Severity::Warning,
                    });
                }
            }
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = content.to_string();
        
        // Determine emphasis style
        let target_style = self.get_target_style(content);
        
        // Detect code blocks and spans
        let code_blocks = self.detect_code_blocks(content);
        
        // Process emphasis based on target style
        if target_style == EmphasisStyle::Asterisk {
            // Convert underscores to asterisks
            // Find all matches first
            let underscore_matches = match UNDERSCORE_PATTERN.find_iter(content).collect::<Result<Vec<_>, _>>() {
                Ok(matches) => matches,
                Err(_) => return Ok(result),
            };
            
            // Create a list of replacements to make
            let mut replacements = Vec::new();
            for m in &underscore_matches {
                let start_pos = m.start();
                let end_pos = m.end();
                
                // Skip if in code block or URL
                if self.is_in_code_block_or_span(&code_blocks, start_pos) ||
                   self.is_url(&content[start_pos..end_pos], content, start_pos, end_pos) {
                    continue;
                }
                
                // Extract the content between the emphasis markers
                let content_between = &content[start_pos + 1..end_pos - 1];
                
                // Add to list of replacements
                replacements.push((start_pos, end_pos, format!("*{}*", content_between)));
            }
            
            // Sort replacements in reverse order to avoid index shifts
            replacements.sort_by(|a, b| b.0.cmp(&a.0));
            
            // Apply all replacements
            for (start, end, replacement) in replacements {
                result.replace_range(start..end, &replacement);
            }
        } else {
            // Convert asterisks to underscores
            // Find all matches first
            let asterisk_matches = match ASTERISK_PATTERN.find_iter(content).collect::<Result<Vec<_>, _>>() {
                Ok(matches) => matches,
                Err(_) => return Ok(result),
            };
            
            // Create a list of replacements to make
            let mut replacements = Vec::new();
            for m in &asterisk_matches {
                let start_pos = m.start();
                let end_pos = m.end();
                
                // Skip if in code block or URL
                if self.is_in_code_block_or_span(&code_blocks, start_pos) ||
                   self.is_url(&content[start_pos..end_pos], content, start_pos, end_pos) {
                    continue;
                }
                
                // Extract the content between the emphasis markers
                let content_between = &content[start_pos + 1..end_pos - 1];
                
                // Add to list of replacements
                replacements.push((start_pos, end_pos, format!("_{}_", content_between)));
            }
            
            // Sort replacements in reverse order to avoid index shifts
            replacements.sort_by(|a, b| b.0.cmp(&a.0));
            
            // Apply all replacements
            for (start, end, replacement) in replacements {
                result.replace_range(start..end, &replacement);
            }
        }
        
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        let rule = MD049EmphasisStyle::default();
        assert_eq!(rule.name(), "MD049");
    }

    #[test]
    fn test_style_from_str() {
        assert_eq!(EmphasisStyle::from("asterisk"), EmphasisStyle::Asterisk);
        assert_eq!(EmphasisStyle::from("underscore"), EmphasisStyle::Underscore);
        assert_eq!(EmphasisStyle::from("other"), EmphasisStyle::Consistent);
    }
}

