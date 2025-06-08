use crate::utils::range_utils::{calculate_line_range, LineIndex};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::front_matter_utils::FrontMatterUtils;

/// Rule MD041: First line in file should be a top-level heading
///
/// See [docs/md041.md](../../docs/md041.md) for full documentation, configuration, and examples.

#[derive(Clone)]
pub struct MD041FirstLineHeading {
    pub level: usize,
    pub front_matter_title: bool,
}

impl Default for MD041FirstLineHeading {
    fn default() -> Self {
        Self {
            level: 1,
            front_matter_title: true,
        }
    }
}

impl MD041FirstLineHeading {
    pub fn new(level: usize, front_matter_title: bool) -> Self {
        Self {
            level,
            front_matter_title,
        }
    }

    fn has_front_matter_title(&self, content: &str) -> bool {
        if !self.front_matter_title {
            return false;
        }

        FrontMatterUtils::has_front_matter_field(content, "title:")
    }
}

impl Rule for MD041FirstLineHeading {
    fn name(&self) -> &'static str {
        "MD041"
    }

    fn description(&self) -> &'static str {
        "First line in file should be a top level heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();
        if content.trim().is_empty() {
            return Ok(warnings);
        }
        if self.has_front_matter_title(content) {
            return Ok(warnings);
        }
        
        // Find the first non-blank line after front matter using cached info
        let mut first_content_line_num = None;
        let mut skip_lines = 0;
        
        // Check for front matter
        if ctx.lines.first().map(|l| l.content.trim()) == Some("---") {
            // Skip front matter
            for (idx, line_info) in ctx.lines.iter().enumerate().skip(1) {
                if line_info.content.trim() == "---" {
                    skip_lines = idx + 1;
                    break;
                }
            }
        }
        
        for (line_num, line_info) in ctx.lines.iter().enumerate().skip(skip_lines) {
            if !line_info.content.trim().is_empty() {
                first_content_line_num = Some(line_num);
                break;
            }
        }
        
        if first_content_line_num.is_none() {
            // No non-blank lines after front matter
            return Ok(warnings);
        }
        
        let first_line_idx = first_content_line_num.unwrap();
        
        // Check if the first non-blank line is a heading of the required level
        let first_line_info = &ctx.lines[first_line_idx];
        let is_correct_heading = if let Some(heading) = &first_line_info.heading {
            heading.level as usize == self.level
        } else {
            false
        };
        
        if !is_correct_heading {
            // Calculate precise character range for the entire first line
            let first_line = first_line_idx + 1; // Convert to 1-indexed
            let first_line_content = &first_line_info.content;
            let (start_line, start_col, end_line, end_col) =
                calculate_line_range(first_line, first_line_content);

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!(
                    "First line in file should be a level {} heading",
                    self.level
                ),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: LineIndex::new(content.to_string())
                        .line_col_to_byte_range_with_length(first_line, 1, 0),
                    replacement: format!(
                        "{} Title\n\n",
                        "#".repeat(self.level)
                    ),
                }),
            });
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let content =
            crate::rules::front_matter_utils::FrontMatterUtils::fix_malformed_front_matter(content);
        if content.trim().is_empty() || self.has_front_matter_title(&content) {
            return Ok(content.to_string());
        }
        
        // Re-create context for the potentially fixed content
        let fixed_ctx = crate::lint_context::LintContext::new(&content);
        
        // Find the first non-blank line after front matter
        let mut first_content_line_num = None;
        let mut skip_lines = 0;
        
        // Check for front matter
        if fixed_ctx.lines.first().map(|l| l.content.trim()) == Some("---") {
            // Skip front matter
            for (idx, line_info) in fixed_ctx.lines.iter().enumerate().skip(1) {
                if line_info.content.trim() == "---" {
                    skip_lines = idx + 1;
                    break;
                }
            }
        }
        
        for (line_num, line_info) in fixed_ctx.lines.iter().enumerate().skip(skip_lines) {
            if !line_info.content.trim().is_empty() {
                first_content_line_num = Some(line_num);
                break;
            }
        }
        
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Check if we have any headings at all
        let has_any_heading = fixed_ctx.lines.iter().any(|line| line.heading.is_some());
        
        if !has_any_heading {
            // Add a new title at the beginning
            result.push_str(&format!("{} Title\n\n{}", "#".repeat(self.level), content));
        } else if let Some(first_line_idx) = first_content_line_num {
            // Check if first content line is a heading of correct level
            let first_line_info = &fixed_ctx.lines[first_line_idx];
            
            if let Some(heading) = &first_line_info.heading {
                if heading.level as usize != self.level {
                    // Fix the existing heading level
                    for (i, line) in lines.iter().enumerate() {
                        if i == first_line_idx {
                            result.push_str(&format!(
                                "{} {}",
                                "#".repeat(self.level),
                                heading.text
                            ));
                        } else {
                            result.push_str(line);
                        }
                        if i < lines.len() - 1 {
                            result.push('\n');
                        }
                    }
                } else {
                    // No fix needed, return original
                    return Ok(content.to_string());
                }
            } else {
                // First line is not a heading, add a new title before it
                for (i, line) in lines.iter().enumerate() {
                    if i == first_line_idx {
                        result.push_str(&format!("{} Title\n\n", "#".repeat(self.level)));
                    }
                    result.push_str(line);
                    if i < lines.len() - 1 {
                        result.push('\n');
                    }
                }
            }
        } else {
            // No content after front matter
            return Ok(content.to_string());
        }
        
        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let level =
            crate::config::get_rule_config_value::<u32>(config, "MD041", "level").unwrap_or(1);
        let front_matter_title =
            crate::config::get_rule_config_value::<String>(config, "MD041", "front_matter_title")
                .unwrap_or_else(|| "title".to_string());
        let level_usize = level as usize;
        let use_front_matter = !front_matter_title.is_empty();
        Box::new(MD041FirstLineHeading::new(level_usize, use_front_matter))
    }
}
