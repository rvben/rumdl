use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

lazy_static! {
    // Simplified HR patterns - more efficient
    static ref HR_PATTERN: Regex = Regex::new(r"^\s*[-*_]{3,}\s*$").unwrap();
    // Single pattern for quick list check - more specific to avoid false positives
    static ref QUICK_LIST_CHECK: Regex = Regex::new(r"(?:^|\n)\s*(?:[-*+]|\d+[.)])\S").unwrap();
    // Optimized list item regex with better performance characteristics
    static ref LIST_ITEM_RE: Regex = Regex::new(r"^(\s*)([-*+]|\d+[.)])(\S.*)").unwrap();
}

#[derive(Debug)]
pub struct MD015NoMissingSpaceAfterListMarker {
    pub require_space: bool,
}

impl Default for MD015NoMissingSpaceAfterListMarker {
    fn default() -> Self {
        Self {
            require_space: true,
        }
    }
}

impl MD015NoMissingSpaceAfterListMarker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_require_space(require_space: bool) -> Self {
        Self { require_space }
    }

    /// Check if a line is a horizontal rule - optimized to use a single regex
    #[inline(always)]
    fn is_horizontal_rule(line: &str) -> bool {
        HR_PATTERN.is_match(line)
    }

    /// Check if line contains a list marker without space
    #[inline(always)]
    fn is_list_item_without_space(line: &str) -> bool {
        if line.is_empty() || line.trim().is_empty() {
            return false;
        }

        if LIST_ITEM_RE.captures(line).is_some() {
            // Match found, now check if there's no space after the marker
            return true;
        }

        false
    }

    /// Fix a list item without space for MD015 rule
    #[inline(always)]
    fn fix_list_item(line: &str) -> String {
        if let Some(caps) = LIST_ITEM_RE.captures(line) {
            format!("{}{} {}", &caps[1], &caps[2], &caps[3])
        } else {
            line.to_string()
        }
    }

    /// Pre-compute which lines are in code blocks or front matter for better performance
    #[inline]
    fn get_special_lines(&self, content: &str) -> (HashSet<usize>, HashSet<usize>) {
        let lines: Vec<&str> = content.lines().collect();
        let mut code_block_lines = HashSet::with_capacity(lines.len() / 4);
        let mut front_matter_lines = HashSet::with_capacity(10); // Usually small

        let mut in_code_block = false;
        let mut code_fence = String::new();
        let mut in_front_matter = false;

        for (i, line) in lines.iter().enumerate() {
            // Track front matter
            if i == 0 && line.trim() == "---" {
                in_front_matter = true;
                front_matter_lines.insert(i);
                continue;
            }

            if in_front_matter {
                front_matter_lines.insert(i);
                if line.trim() == "---" {
                    in_front_matter = false;
                }
                continue;
            }

            // Track code blocks more efficiently
            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    in_code_block = true;
                    code_fence = if trimmed.starts_with("```") {
                        "```".to_string()
                    } else {
                        "~~~".to_string()
                    };
                } else if trimmed.starts_with(&code_fence) {
                    in_code_block = false;
                }
            }

            if in_code_block {
                code_block_lines.insert(i);
            }
        }

        (code_block_lines, front_matter_lines)
    }
}

impl Rule for MD015NoMissingSpaceAfterListMarker {
    fn name(&self) -> &'static str {
        "MD015"
    }

    fn description(&self) -> &'static str {
        "List markers must be followed by a space"
    }

    fn check(&self, content: &str) -> LintResult {
        let _timer = crate::profiling::ScopedTimer::new("MD015_check");

        // Quick returns for common cases
        if content.is_empty() || !self.require_space {
            return Ok(Vec::new());
        }

        // Early return if no list markers found
        if !content.contains('-')
            && !content.contains('*')
            && !content.contains('+')
            && !content.contains(|c: char| c.is_ascii_digit())
        {
            return Ok(Vec::new());
        }

        // Quick check for potential list items without spaces
        if !QUICK_LIST_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Pre-compute special lines efficiently
        let (code_block_lines, front_matter_lines) = self.get_special_lines(content);

        // Pre-allocate warnings with estimated capacity
        let estimated_warnings = content.lines().count() / 10; // Rough estimate: 10% of lines might be warnings
        warnings.reserve(estimated_warnings);

        for (line_num, line) in lines.iter().enumerate() {
            // Fast checks using HashSet lookups
            if code_block_lines.contains(&line_num) || front_matter_lines.contains(&line_num) {
                continue;
            }

            // Skip if this is a horizontal rule
            if Self::is_horizontal_rule(line) {
                continue;
            }

            // Use our optimized check for list items without space
            if Self::is_list_item_without_space(line) {
                let is_unordered = line.trim_start().starts_with(['*', '+', '-']);
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    severity: Severity::Warning,
                    line: line_num + 1,
                    column: 1,
                    message: if is_unordered {
                        "Missing space after unordered list marker".to_string()
                    } else {
                        "Missing space after ordered list marker".to_string()
                    },
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line_num + 1, 1),
                        replacement: Self::fix_list_item(line),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _timer = crate::profiling::ScopedTimer::new("MD015_fix");

        // Quick returns for common cases
        if content.is_empty() || !self.require_space {
            return Ok(content.to_string());
        }

        // Early return if no list markers found
        if !content.contains('-')
            && !content.contains('*')
            && !content.contains('+')
            && !content.contains(|c: char| c.is_ascii_digit())
        {
            return Ok(content.to_string());
        }

        // Quick check for potential list items without spaces
        if !QUICK_LIST_CHECK.is_match(content) {
            return Ok(content.to_string());
        }

        // Pre-compute special lines efficiently
        let (code_block_lines, front_matter_lines) = self.get_special_lines(content);

        // Process the content more efficiently
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::with_capacity(content.len() + 100); // Pre-allocate with extra space

        for (i, line) in lines.iter().enumerate() {
            // Fast checks using HashSet lookups
            if code_block_lines.contains(&i)
                || front_matter_lines.contains(&i)
                || Self::is_horizontal_rule(line)
            {
                result.push_str(line);
            }
            // Skip if this is a horizontal rule
            else if Self::is_list_item_without_space(line) {
                result.push_str(&Self::fix_list_item(line));
            } else {
                result.push_str(line);
            }

            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Remove trailing newline if original didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
