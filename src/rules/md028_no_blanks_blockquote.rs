use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug, Default)]
pub struct MD028NoBlanksBlockquote;

impl MD028NoBlanksBlockquote {
    fn is_blockquote_line(line: &str) -> bool {
        line.trim_start().starts_with('>')
    }

    fn is_empty_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    fn find_blank_line_range(&self, lines: &[&str], start_idx: usize) -> Option<(usize, usize)> {
        let mut end_idx = start_idx;
        while end_idx < lines.len() && Self::is_empty_line(lines[end_idx]) {
            end_idx += 1;
        }

        if end_idx < lines.len() && Self::is_blockquote_line(lines[end_idx]) {
            Some((start_idx, end_idx))
        } else {
            None
        }
    }

    fn get_indentation(line: &str) -> String {
        let chars: Vec<char> = line.chars().collect();
        let indent_len = chars.iter()
            .take_while(|&&c| c.is_whitespace())
            .count();
        chars[..indent_len].iter().collect()
    }
}

impl Rule for MD028NoBlanksBlockquote {
    fn name(&self) -> &'static str {
        "MD028"
    }

    fn description(&self) -> &'static str {
        "Blank line inside blockquote"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            if Self::is_blockquote_line(lines[i]) {
                // Find the end of this blockquote section
                let mut j = i + 1;
                while j < lines.len() && (Self::is_blockquote_line(lines[j]) || Self::is_empty_line(lines[j])) {
                    j += 1;
                }

                // Check for blank lines between blockquote lines
                let mut k = i;
                while k < j {
                    if Self::is_empty_line(lines[k]) {
                        if let Some((start, end)) = self.find_blank_line_range(&lines, k) {
                            let blank_lines = end - start;
                            warnings.push(LintWarning {
                                message: if blank_lines == 1 {
                                    "Blank line inside blockquote".to_string()
                                } else {
                                    format!("{} blank lines inside blockquote", blank_lines)
                                },
                                line: k + 1,
                                column: 1,
                                fix: Some(Fix {
                                    line: start + 1,
                                    column: 1,
                                    replacement: (start..end)
                                        .map(|idx| {
                                            let indent = Self::get_indentation(
                                                if idx == 0 { lines[0] }
                                                else { lines[idx - 1] }
                                            );
                                            format!("{}> ", indent)
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n"),
                                }),
                            });
                            k = end;
                            continue;
                        }
                    }
                    k += 1;
                }
                i = j;
            } else {
                i += 1;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            if Self::is_blockquote_line(lines[i]) {
                // Add the current blockquote line
                result.push_str(lines[i]);
                result.push('\n');

                // Find the end of this blockquote section
                let mut j = i + 1;
                while j < lines.len() && (Self::is_blockquote_line(lines[j]) || Self::is_empty_line(lines[j])) {
                    if Self::is_empty_line(lines[j]) {
                        // Add blockquote marker for empty lines
                        let indent = Self::get_indentation(lines[i]);
                        result.push_str(&format!("{}> \n", indent));
                    } else {
                        result.push_str(lines[j]);
                        result.push('\n');
                    }
                    j += 1;
                }
                i = j;
            } else {
                result.push_str(lines[i]);
                result.push('\n');
                i += 1;
            }
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 