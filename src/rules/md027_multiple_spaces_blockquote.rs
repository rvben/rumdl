use crate::utils::range_utils::{calculate_match_range, LineIndex};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pattern to match quote lines with multiple spaces after >
    static ref BLOCKQUOTE_MULTIPLE_SPACES: Regex = Regex::new(r"^(\s*)>(\s{2,})(.*)$").unwrap();

    // New patterns for detecting malformed blockquote attempts where user intent is clear
    static ref MALFORMED_BLOCKQUOTE_PATTERNS: Vec<(Regex, &'static str)> = vec![
        // Double > without space: >>text (looks like nested but missing spaces)
        (Regex::new(r"^(\s*)>>([^\s>].*|$)").unwrap(), "missing spaces in nested quote"),

        // Triple > without space: >>>text
        (Regex::new(r"^(\s*)>>>([^\s>].*|$)").unwrap(), "missing spaces in deeply nested quote"),

        // Space then > then text: > >text (extra > by mistake)
        (Regex::new(r"^(\s*)>\s+>([^\s>].*|$)").unwrap(), "extra quote marker"),

        // Multiple spaces then >: (spaces)>text (indented blockquote without space)
        (Regex::new(r"^(\s{4,})>([^\s].*|$)").unwrap(), "indented quote missing space"),
    ];
}

/// Rule MD027: No multiple spaces after blockquote symbol
///
/// See [docs/md027.md](../../docs/md027.md) for full documentation, configuration, and examples.

#[derive(Debug, Default, Clone)]
pub struct MD027MultipleSpacesBlockquote;

impl Rule for MD027MultipleSpacesBlockquote {
    fn name(&self) -> &'static str {
        "MD027"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after quote marker (>)"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            let line_num = line_idx + 1;
            
            // Skip lines in code blocks
            if line_info.in_code_block {
                continue;
            }
            
            // Check if this line is a blockquote using cached info
            if let Some(blockquote) = &line_info.blockquote {
                // Part 1: Check for multiple spaces after the blockquote marker
                if blockquote.has_multiple_spaces_after_marker {
                    // Calculate the position of the extra spaces
                    let extra_spaces_start = blockquote.marker_column + blockquote.nesting_level + 1; // Position after all '>' markers + 1 for the first space
                    let spaces_in_prefix = blockquote.prefix.chars().skip(blockquote.indent.len() + blockquote.nesting_level).take_while(|&c| c == ' ').count();
                    let extra_spaces_len = spaces_in_prefix - 1; // All spaces except the first one

                    let (start_line, start_col, end_line, end_col) = calculate_match_range(
                        line_num,
                        &line_info.content,
                        extra_spaces_start,
                        extra_spaces_len,
                    );

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Multiple spaces after quote marker (>)".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: {
                                let line_index = LineIndex::new(ctx.content.to_string());
                                let start_byte = line_index.line_col_to_byte_range(line_num, start_col).start;
                                let end_byte = line_index.line_col_to_byte_range(line_num, end_col).start;
                                start_byte..end_byte
                            },
                            replacement: "".to_string(), // Remove the extra spaces
                        }),
                    });
                }
            } else {
                // Part 2: Check for malformed blockquote attempts on non-blockquote lines
                let malformed_attempts = self.detect_malformed_blockquote_attempts(&line_info.content);
                for (start, len, fixed_line, description) in malformed_attempts {
                    let (start_line, start_col, end_line, end_col) = calculate_match_range(
                        line_num,
                        &line_info.content,
                        start,
                        len,
                    );

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: format!("Malformed quote: {}", description),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: {
                                let line_index = LineIndex::new(ctx.content.to_string());
                                line_index.line_col_to_byte_range(line_num, 1)
                            },
                            replacement: fixed_line,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut result = Vec::with_capacity(ctx.lines.len());

        for line_info in &ctx.lines {
            if let Some(blockquote) = &line_info.blockquote {
                // Fix blockquotes with multiple spaces after the marker
                if blockquote.has_multiple_spaces_after_marker {
                    // Rebuild the line with exactly one space after the markers
                    let fixed_line = format!(
                        "{}{} {}",
                        blockquote.indent,
                        ">".repeat(blockquote.nesting_level),
                        blockquote.content
                    );
                    result.push(fixed_line);
                } else {
                    result.push(line_info.content.clone());
                }
            } else {
                // Check for malformed blockquote attempts
                let malformed_attempts = self.detect_malformed_blockquote_attempts(&line_info.content);
                if !malformed_attempts.is_empty() {
                    // Use the first fix (there should only be one per line)
                    let (_, _, fixed_line, _) = &malformed_attempts[0];
                    result.push(fixed_line.clone());
                } else {
                    result.push(line_info.content.clone());
                }
            }
        }

        // Preserve trailing newline if original content had one
        Ok(result.join("\n") + if ctx.content.ends_with('\n') { "\n" } else { "" })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD027MultipleSpacesBlockquote)
    }
}

impl MD027MultipleSpacesBlockquote {
    /// Detect malformed blockquote attempts where user intent is clear
    fn detect_malformed_blockquote_attempts(&self, line: &str) -> Vec<(usize, usize, String, String)> {
        let mut results = Vec::new();

        for (pattern, issue_type) in MALFORMED_BLOCKQUOTE_PATTERNS.iter() {
            if let Some(cap) = pattern.captures(line) {
                let match_obj = cap.get(0).unwrap();
                let start = match_obj.start();
                let len = match_obj.len();

                // Extract potential blockquote components
                if let Some((fixed_line, description)) = self.extract_blockquote_fix_from_match(&cap, issue_type, line) {
                    // Only proceed if this looks like a genuine blockquote attempt
                    if self.looks_like_blockquote_attempt(line, &fixed_line) {
                        results.push((start, len, fixed_line, description));
                    }
                }
            }
        }

        results
    }

    /// Extract the proper blockquote format from a malformed match
    fn extract_blockquote_fix_from_match(&self, cap: &regex::Captures, issue_type: &str, _original_line: &str) -> Option<(String, String)> {
        match issue_type {
            "missing spaces in nested blockquote" => {
                // >>text -> > > text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((format!("{}> > {}", indent, content.trim()), "Missing spaces in nested blockquote".to_string()))
            },
            "missing spaces in deeply nested blockquote" => {
                // >>>text -> > > > text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((format!("{}> > > {}", indent, content.trim()), "Missing spaces in deeply nested blockquote".to_string()))
            },
            "extra blockquote marker" => {
                // > >text -> > text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((format!("{}> {}", indent, content.trim()), "Extra blockquote marker".to_string()))
            },
            "indented blockquote missing space" => {
                // (spaces)>text -> (spaces)> text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((format!("{}> {}", indent, content.trim()), "Indented blockquote missing space".to_string()))
            },
            _ => None,
        }
    }

    /// Check if the pattern looks like a genuine blockquote attempt
    fn looks_like_blockquote_attempt(&self, original: &str, fixed: &str) -> bool {
        // Basic heuristics to avoid false positives

        // 1. Content should not be too short (avoid flagging things like ">>>" alone)
        let trimmed_original = original.trim();
        if trimmed_original.len() < 5 {  // More restrictive
            return false;
        }

        // 2. Should contain some text content after the markers
        let content_after_markers = trimmed_original.trim_start_matches('>').trim_start_matches(' ');
        if content_after_markers.is_empty() || content_after_markers.len() < 3 {  // More restrictive
            return false;
        }

        // 3. Content should contain some alphabetic characters (not just symbols)
        if !content_after_markers.chars().any(|c| c.is_alphabetic()) {
            return false;
        }

        // 4. Fixed version should actually be a valid blockquote
        // Check if it starts with optional whitespace followed by >
        let blockquote_pattern = regex::Regex::new(r"^\s*>").unwrap();
        if !blockquote_pattern.is_match(fixed) {
            return false;
        }

        // 5. Avoid flagging things that might be code or special syntax
        if content_after_markers.starts_with('#') // Headers
            || content_after_markers.starts_with('[') // Links
            || content_after_markers.starts_with('`') // Code
            || content_after_markers.starts_with("http") // URLs
            || content_after_markers.starts_with("www.") // URLs
            || content_after_markers.starts_with("ftp") // URLs
        {
            return false;
        }

        // 6. Content should look like prose, not code or markup
        let word_count = content_after_markers.split_whitespace().count();
        if word_count < 3 {  // Should be at least 3 words to look like prose
            return false;
        }

        true
    }
}
