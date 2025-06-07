use crate::utils::range_utils::{calculate_match_range, LineIndex};
use crate::utils::code_block_utils::CodeBlockUtils;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref IMAGE_REGEX: Regex = Regex::new(r"!\[([^\]]*)\](\([^)]+\))").unwrap();
}

/// Rule MD045: Images should have alt text
///
/// See [docs/md045.md](../../docs/md045.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when an image is missing alternate text (alt text).
#[derive(Clone)]
pub struct MD045NoAltText;

impl Default for MD045NoAltText {
    fn default() -> Self {
        Self::new()
    }
}

impl MD045NoAltText {
    pub fn new() -> Self {
        Self
    }
}

impl Rule for MD045NoAltText {
    fn name(&self) -> &'static str {
        "MD045"
    }

    fn description(&self) -> &'static str {
        "Images should have alternate text (alt text)"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();
        
        // Detect all code blocks and code spans
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);
        
        // Track byte positions for each line
        let mut byte_pos = 0;

        for (line_num, line) in content.lines().enumerate() {
            for cap in IMAGE_REGEX.captures_iter(line) {
                let alt_text = cap.get(1).map_or("", |m| m.as_str());
                if alt_text.trim().is_empty() {
                    let full_match = cap.get(0).unwrap();
                    let url_part = cap.get(2).unwrap();
                    
                    // Calculate the byte position of this match in the document
                    let match_byte_pos = byte_pos + full_match.start();
                    
                    // Skip if this image is inside a code block or code span
                    if CodeBlockUtils::is_in_code_block_or_span(&code_blocks, match_byte_pos) {
                        continue;
                    }

                    // Calculate precise character range for the entire image syntax
                    let (start_line, start_col, end_line, end_col) = calculate_match_range(
                        line_num + 1,
                        line,
                        full_match.start(),
                        full_match.len(),
                    );

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Image missing alt text (add description for accessibility: ![description](url))".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index
                                .line_col_to_byte_range_with_length(line_num + 1, full_match.start() + 1, full_match.len()),
                            replacement: format!(
                                "![TODO: Add image description]{}",
                                url_part.as_str()
                            ),
                        }),
                    });
                }
            }
            
            // Update byte position for next line
            byte_pos += line.len() + 1; // +1 for newline
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        
        // Detect all code blocks and code spans
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);
        
        let mut result = String::new();
        let mut last_end = 0;

        for caps in IMAGE_REGEX.captures_iter(content) {
            let full_match = caps.get(0).unwrap();
            let alt_text = caps.get(1).map_or("", |m| m.as_str());
            let url_part = caps.get(2).map_or("", |m| m.as_str());
            
            // Add text before this match
            result.push_str(&content[last_end..full_match.start()]);
            
            // Check if this image is inside a code block
            if CodeBlockUtils::is_in_code_block_or_span(&code_blocks, full_match.start()) {
                // Keep the original image if it's in a code block
                result.push_str(&caps[0]);
            } else if alt_text.trim().is_empty() {
                // Fix the image if it's not in a code block and has empty alt text
                result.push_str(&format!("![TODO: Add image description]{}", url_part));
            } else {
                // Keep the original if alt text is not empty
                result.push_str(&caps[0]);
            }
            
            last_end = full_match.end();
        }
        
        // Add any remaining text
        result.push_str(&content[last_end..]);

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD045NoAltText::new())
    }
}
