/// Rule MD025: Document must have a single top-level heading
///
/// See [docs/md025.md](../../docs/md025.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::{calculate_match_range, LineIndex};
use lazy_static::lazy_static;
use regex::Regex;
use toml;

lazy_static! {
    // Pattern for quick check if content has any headings at all
    static ref HEADING_CHECK: Regex = Regex::new(r"(?m)^(?:\s*)#").unwrap();
    
    // Horizontal rule patterns (from MD035)
    static ref HR_DASH: Regex = Regex::new(r"^\-{3,}\s*$").unwrap();
    static ref HR_ASTERISK: Regex = Regex::new(r"^\*{3,}\s*$").unwrap();
    static ref HR_UNDERSCORE: Regex = Regex::new(r"^_{3,}\s*$").unwrap();
    static ref HR_SPACED_DASH: Regex = Regex::new(r"^(\-\s+){2,}\-\s*$").unwrap();
    static ref HR_SPACED_ASTERISK: Regex = Regex::new(r"^(\*\s+){2,}\*\s*$").unwrap();
    static ref HR_SPACED_UNDERSCORE: Regex = Regex::new(r"^(_\s+){2,}_\s*$").unwrap();
}

#[derive(Clone)]
pub struct MD025SingleTitle {
    level: usize,
    front_matter_title: String,
    /// Allow multiple H1s if they appear to be document sections (appendices, references, etc.)
    allow_document_sections: bool,
    /// Allow multiple H1s if separated by horizontal rules
    allow_with_separators: bool,
}

impl Default for MD025SingleTitle {
    fn default() -> Self {
        Self {
            level: 1,
            front_matter_title: "title".to_string(),
            allow_document_sections: true, // More lenient by default
            allow_with_separators: true,
        }
    }
}

impl MD025SingleTitle {
    pub fn new(level: usize, front_matter_title: &str) -> Self {
        Self {
            level,
            front_matter_title: front_matter_title.to_string(),
            allow_document_sections: true,
            allow_with_separators: true,
        }
    }

    pub fn strict() -> Self {
        Self {
            level: 1,
            front_matter_title: "title".to_string(),
            allow_document_sections: false,
            allow_with_separators: false,
        }
    }

    /// Check if a heading text suggests it's a legitimate document section
    fn is_document_section_heading(&self, heading_text: &str) -> bool {
        if !self.allow_document_sections {
            return false;
        }

        let lower_text = heading_text.to_lowercase();
        
        // Common section names that are legitimate as separate H1s
        let section_indicators = [
            "appendix", "appendices",
            "reference", "references", "bibliography",
            "index", "indices",
            "glossary", "glossaries",
            "conclusion", "conclusions",
            "summary", "executive summary",
            "acknowledgment", "acknowledgments", "acknowledgement", "acknowledgements",
            "about", "contact", "license", "legal",
            "changelog", "change log", "history",
            "faq", "frequently asked questions",
            "troubleshooting", "support",
            "installation", "setup", "getting started",
            "api reference", "api documentation",
            "examples", "tutorials", "guides",
        ];

        // Check if the heading starts with these patterns
        section_indicators.iter().any(|&indicator| {
            lower_text.starts_with(indicator) ||
            lower_text.starts_with(&format!("{}:", indicator)) ||
            lower_text.contains(&format!(" {}", indicator)) ||
            // Handle appendix numbering like "Appendix A", "Appendix 1"
            (indicator == "appendix" && (
                lower_text.matches("appendix").count() == 1 && 
                (lower_text.contains(" a") || lower_text.contains(" b") || 
                 lower_text.contains(" 1") || lower_text.contains(" 2") ||
                 lower_text.contains(" i") || lower_text.contains(" ii"))
            ))
        })
    }

    /// Check if a line is a horizontal rule
    fn is_horizontal_rule(line: &str) -> bool {
        let trimmed = line.trim();
        HR_DASH.is_match(trimmed)
            || HR_ASTERISK.is_match(trimmed)
            || HR_UNDERSCORE.is_match(trimmed)
            || HR_SPACED_DASH.is_match(trimmed)
            || HR_SPACED_ASTERISK.is_match(trimmed)
            || HR_SPACED_UNDERSCORE.is_match(trimmed)
    }
    
    /// Check if a line might be a Setext heading underline
    fn is_potential_setext_heading(ctx: &crate::lint_context::LintContext, line_num: usize) -> bool {
        if line_num == 0 || line_num >= ctx.lines.len() {
            return false;
        }
        
        let line = ctx.lines[line_num].content.trim();
        let prev_line = if line_num > 0 { ctx.lines[line_num - 1].content.trim() } else { "" };
        
        let is_dash_line = !line.is_empty() && line.chars().all(|c| c == '-');
        let is_equals_line = !line.is_empty() && line.chars().all(|c| c == '=');
        let prev_line_has_content = !prev_line.is_empty() && !Self::is_horizontal_rule(prev_line);
        (is_dash_line || is_equals_line) && prev_line_has_content
    }

    /// Check if headings are separated by horizontal rules
    fn has_separator_before_heading(&self, ctx: &crate::lint_context::LintContext, heading_line: usize) -> bool {
        if !self.allow_with_separators || heading_line == 0 {
            return false;
        }

        // Look for horizontal rules in the lines before this heading
        // Check up to 5 lines before the heading for a horizontal rule
        let search_start = heading_line.saturating_sub(5);
        
        for line_num in search_start..heading_line {
            if line_num >= ctx.lines.len() {
                continue;
            }
            
            let line = &ctx.lines[line_num].content;
            if Self::is_horizontal_rule(line) && !Self::is_potential_setext_heading(ctx, line_num) {
                // Found a horizontal rule before this heading
                // Check that there's no other heading between the HR and this heading
                let has_intermediate_heading = ((line_num + 1)..heading_line).any(|idx| {
                    idx < ctx.lines.len() && ctx.lines[idx].heading.is_some()
                });
                
                if !has_intermediate_heading {
                    return true;
                }
            }
        }
        
        false
    }
}

impl Rule for MD025SingleTitle {
    fn name(&self) -> &'static str {
        "MD025"
    }

    fn description(&self) -> &'static str {
        "Multiple top-level headings in the same document"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early return for empty content
        if ctx.lines.is_empty() {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();

        // Check for front matter title if configured
        let mut _found_title_in_front_matter = false;
        if !self.front_matter_title.is_empty() {
            // Detect front matter manually
            let content_lines: Vec<&str> = ctx.content.lines().collect();
            if content_lines.first().map(|l| l.trim()) == Some("---") {
                // Look for the end of front matter
                for (idx, line) in content_lines.iter().enumerate().skip(1) {
                    if line.trim() == "---" {
                        // Extract front matter content
                        let front_matter_content = content_lines[1..idx].join("\n");
                        
                        // Check if it contains a title field
                        _found_title_in_front_matter = front_matter_content.lines().any(|line| {
                            line.trim()
                                .starts_with(&format!("{}:", self.front_matter_title))
                        });
                        break;
                    }
                }
            }
        }

        // Find all headings at the target level using cached information
        let mut target_level_headings = Vec::new();
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                if heading.level as usize == self.level {
                    // Ignore if indented 4+ spaces (code block)
                    if line_info.indent >= 4 {
                        continue;
                    }
                    target_level_headings.push(line_num);
                }
            }
        }

        // If we have multiple target level headings, flag all subsequent ones (not the first)
        // unless they are legitimate document sections
        if target_level_headings.len() > 1 {
            // Skip the first heading, check the rest for legitimacy
            for &line_num in &target_level_headings[1..] {
                if let Some(heading) = &ctx.lines[line_num].heading {
                    let heading_text = &heading.text;

                    // Check if this heading should be allowed
                    let should_allow = self.is_document_section_heading(heading_text) ||
                        self.has_separator_before_heading(ctx, line_num);

                    if should_allow {
                        continue; // Skip flagging this heading
                    }

                    // Calculate precise character range for the heading text content
                    let line_content = &ctx.lines[line_num].content;
                    let text_start_in_line = if let Some(pos) = line_content.find(heading_text) {
                        pos
                    } else {
                        // Fallback: find after hash markers for ATX headings
                        if line_content.trim_start().starts_with('#') {
                            let trimmed = line_content.trim_start();
                            let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                            let after_hashes = &trimmed[hash_count..];
                            let text_start_in_trimmed = after_hashes.find(heading_text).unwrap_or(0);
                            (line_content.len() - trimmed.len()) + hash_count + text_start_in_trimmed
                        } else {
                            0 // Setext headings start at beginning
                        }
                    };

                    let (start_line, start_col, end_line, end_col) = calculate_match_range(
                        line_num + 1, // Convert to 1-indexed
                        line_content,
                        text_start_in_line,
                        heading_text.len(),
                    );

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!(
                            "Multiple top-level headings (level {}) in the same document",
                            self.level
                        ),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_content_range(line_num + 1),
                            replacement: {
                                let leading_spaces = line_content.len() - line_content.trim_start().len();
                                let indentation = " ".repeat(leading_spaces);
                                if heading_text.is_empty() {
                                    format!("{}{}", indentation, "#".repeat(self.level + 1))
                                } else {
                                    format!("{}{} {}", indentation, "#".repeat(self.level + 1), heading_text)
                                }
                            },
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut fixed_lines = Vec::new();
        let mut found_first = false;
        let mut skip_next = false;

        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if skip_next {
                skip_next = false;
                continue;
            }
            
            if let Some(heading) = &line_info.heading {
                if heading.level as usize == self.level {
                    if !found_first {
                        found_first = true;
                        // Keep the first heading as-is
                        fixed_lines.push(line_info.content.clone());
                        
                        // For Setext headings, also add the underline
                        if matches!(heading.style, crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2) {
                            if line_num + 1 < ctx.lines.len() {
                                fixed_lines.push(ctx.lines[line_num + 1].content.clone());
                                skip_next = true;
                            }
                        }
                    } else {
                        // Check if this heading should be allowed
                        let should_allow = self.is_document_section_heading(&heading.text) ||
                            self.has_separator_before_heading(ctx, line_num);

                        if should_allow {
                            // Keep the heading as-is
                            fixed_lines.push(line_info.content.clone());
                            
                            // For Setext headings, also add the underline
                            if matches!(heading.style, crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2) {
                                if line_num + 1 < ctx.lines.len() {
                                    fixed_lines.push(ctx.lines[line_num + 1].content.clone());
                                    skip_next = true;
                                }
                            }
                        } else {
                            // Demote this heading to the next level
                            let style = match heading.style {
                                crate::lint_context::HeadingStyle::ATX => {
                                    if heading.has_closing_sequence {
                                        crate::rules::heading_utils::HeadingStyle::AtxClosed
                                    } else {
                                        crate::rules::heading_utils::HeadingStyle::Atx
                                    }
                                }
                                crate::lint_context::HeadingStyle::Setext1 => {
                                    // When demoting from level 1 to 2, use Setext2
                                    if self.level == 1 {
                                        crate::rules::heading_utils::HeadingStyle::Setext2
                                    } else {
                                        // For higher levels, use ATX
                                        crate::rules::heading_utils::HeadingStyle::Atx
                                    }
                                }
                                crate::lint_context::HeadingStyle::Setext2 => {
                                    // Setext2 can only go to ATX
                                    crate::rules::heading_utils::HeadingStyle::Atx
                                }
                            };
                            
                            let replacement = if heading.text.is_empty() {
                                // For empty headings, manually construct the replacement
                                match style {
                                    crate::rules::heading_utils::HeadingStyle::Atx => {
                                        "#".repeat(self.level + 1)
                                    }
                                    crate::rules::heading_utils::HeadingStyle::AtxClosed => {
                                        format!("{} {}", "#".repeat(self.level + 1), "#".repeat(self.level + 1))
                                    }
                                    crate::rules::heading_utils::HeadingStyle::Setext1 | 
                                    crate::rules::heading_utils::HeadingStyle::Setext2 |
                                    crate::rules::heading_utils::HeadingStyle::Consistent => {
                                        // For empty Setext or Consistent, use ATX style
                                        "#".repeat(self.level + 1)
                                    }
                                }
                            } else {
                                crate::rules::heading_utils::HeadingUtils::convert_heading_style(
                                    &heading.text,
                                    (self.level + 1) as u32,
                                    style,
                                )
                            };
                            
                            // Add indentation
                            let indentation = " ".repeat(line_info.indent);
                            fixed_lines.push(format!("{}{}", indentation, replacement));
                            
                            // For Setext headings, skip the original underline
                            if matches!(heading.style, crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2) {
                                if line_num + 1 < ctx.lines.len() {
                                    skip_next = true;
                                }
                            }
                        }
                    }
                } else {
                    // Not a target level heading, keep as-is
                    fixed_lines.push(line_info.content.clone());
                    
                    // For Setext headings, also add the underline
                    if matches!(heading.style, crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2) {
                        if line_num + 1 < ctx.lines.len() {
                            fixed_lines.push(ctx.lines[line_num + 1].content.clone());
                            skip_next = true;
                        }
                    }
                }
            } else {
                // Not a heading line, keep as-is
                fixed_lines.push(line_info.content.clone());
            }
        }

        let result = fixed_lines.join("\n");
        if ctx.content.ends_with('\n') {
            Ok(result + "\n")
        } else {
            Ok(result)
        }
    }


    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty()
            || (!ctx.content.contains('#')
                && !ctx.content.contains('=')
                && !ctx.content.contains('-'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        None
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert("level".to_string(), toml::Value::Integer(self.level as i64));
        map.insert(
            "front_matter_title".to_string(),
            toml::Value::String(self.front_matter_title.clone()),
        );
        map.insert(
            "allow_document_sections".to_string(),
            toml::Value::Boolean(self.allow_document_sections),
        );
        map.insert(
            "allow_with_separators".to_string(),
            toml::Value::Boolean(self.allow_with_separators),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let level =
            crate::config::get_rule_config_value::<u32>(config, "MD025", "level").unwrap_or(1);
        let front_matter_title =
            crate::config::get_rule_config_value::<String>(config, "MD025", "front_matter_title")
                .unwrap_or_else(|| "title".to_string());
        let allow_document_sections =
            crate::config::get_rule_config_value::<bool>(config, "MD025", "allow_document_sections")
                .unwrap_or(true); // Default to true for better UX
        let allow_with_separators =
            crate::config::get_rule_config_value::<bool>(config, "MD025", "allow_with_separators")
                .unwrap_or(true);

        Box::new(MD025SingleTitle {
            level: level as usize,
            front_matter_title,
            allow_document_sections,
            allow_with_separators,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_cached_headings() {
        let rule = MD025SingleTitle::default();

        // Test with only one level-1 heading
        let content = "# Title\n\n## Section 1\n\n## Section 2";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with multiple level-1 headings (non-section names) - should flag
        let content = "# Title 1\n\n## Section 1\n\n# Another Title\n\n## Section 2";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1); // Should flag the second level-1 heading
        assert_eq!(result[0].line, 5);

        // Test with front matter title and a level-1 heading
        let content = "---\ntitle: Document Title\n---\n\n# Main Heading\n\n## Section 1";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag a single title after front matter"
        );
    }

    #[test]
    fn test_allow_document_sections() {
        let rule = MD025SingleTitle::default(); // Has allow_document_sections = true

        // Test valid document sections that should NOT be flagged
        let valid_cases = vec![
            "# Main Title\n\n## Content\n\n# Appendix A\n\nAppendix content",
            "# Introduction\n\nContent here\n\n# References\n\nRef content",
            "# Guide\n\nMain content\n\n# Bibliography\n\nBib content",
            "# Manual\n\nContent\n\n# Index\n\nIndex content",
            "# Document\n\nContent\n\n# Conclusion\n\nFinal thoughts",
            "# Tutorial\n\nContent\n\n# FAQ\n\nQuestions and answers",
            "# Project\n\nContent\n\n# Acknowledgments\n\nThanks",
        ];

        for case in valid_cases {
            let ctx = crate::lint_context::LintContext::new(case);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Should not flag document sections in: {}",
                case
            );
        }

        // Test invalid cases that should still be flagged
        let invalid_cases = vec![
            "# Main Title\n\n## Content\n\n# Random Other Title\n\nContent",
            "# First\n\nContent\n\n# Second Title\n\nMore content",
        ];

        for case in invalid_cases {
            let ctx = crate::lint_context::LintContext::new(case);
            let result = rule.check(&ctx).unwrap();
            assert!(
                !result.is_empty(),
                "Should flag non-section headings in: {}",
                case
            );
        }
    }

    #[test]
    fn test_strict_mode() {
        let rule = MD025SingleTitle::strict(); // Has allow_document_sections = false

        // Even document sections should be flagged in strict mode
        let content = "# Main Title\n\n## Content\n\n# Appendix A\n\nAppendix content";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Strict mode should flag all multiple H1s");
    }

    #[test]
    fn test_bounds_checking_bug() {
        // Test case that could trigger bounds error in fix generation
        // When col + self.level exceeds line_content.len()
        let rule = MD025SingleTitle::default();

        // Create content with very short second heading
        let content = "# First\n#";
        let ctx = crate::lint_context::LintContext::new(content);

        // This should not panic
        let result = rule.check(&ctx);
        assert!(result.is_ok());

        // Test the fix as well
        let fix_result = rule.fix(&ctx);
        assert!(fix_result.is_ok());
    }

    #[test]
    fn test_bounds_checking_edge_case() {
        // Test case that specifically targets the bounds checking fix
        // Create a heading where col + self.level would exceed line length
        let rule = MD025SingleTitle::default();

        // Create content where the second heading is just "#" (length 1)
        // col will be 0, self.level is 1, so col + self.level = 1
        // This should not exceed bounds for "#" but tests the edge case
        let content = "# First Title\n#";
        let ctx = crate::lint_context::LintContext::new(content);

        // This should not panic and should handle the edge case gracefully
        let result = rule.check(&ctx);
        assert!(result.is_ok());

        if let Ok(warnings) = result {
            if !warnings.is_empty() {
                // Check that the fix doesn't cause a panic
                let fix_result = rule.fix(&ctx);
                assert!(fix_result.is_ok());

                // The fix should produce valid content
                if let Ok(fixed_content) = fix_result {
                    assert!(!fixed_content.is_empty());
                    // Should convert the second "#" to "##" (or "## " if there's content)
                    assert!(fixed_content.contains("##"));
                }
            }
        }
    }

    #[test]
    fn test_horizontal_rule_separators() {
        let rule = MD025SingleTitle::default(); // Has allow_with_separators = true

        // Test that headings separated by horizontal rules are allowed
        let content = "# First Title\n\nContent here.\n\n---\n\n# Second Title\n\nMore content.\n\n***\n\n# Third Title\n\nFinal content.";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag headings separated by horizontal rules");

        // Test that headings without separators are still flagged
        let content = "# First Title\n\nContent here.\n\n---\n\n# Second Title\n\nMore content.\n\n# Third Title\n\nNo separator before this one.";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag the heading without separator");
        assert_eq!(result[0].line, 11); // Third title on line 11

        // Test with allow_with_separators = false
        let strict_rule = MD025SingleTitle::strict();
        let content = "# First Title\n\nContent here.\n\n---\n\n# Second Title\n\nMore content.";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = strict_rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Strict mode should flag all multiple H1s regardless of separators");
    }
}
