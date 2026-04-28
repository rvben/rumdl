/// Rule MD025: Document must have a single top-level heading
///
/// See [docs/md025.md](../../docs/md025.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::types::HeadingLevel;
use crate::utils::range_utils::calculate_match_range;
use crate::utils::thematic_break;
use toml;

mod md025_config;
use md025_config::MD025Config;

#[derive(Clone, Default)]
pub struct MD025SingleTitle {
    config: MD025Config,
}

impl MD025SingleTitle {
    pub fn new(level: usize, front_matter_title: &str) -> Self {
        Self {
            config: MD025Config {
                level: HeadingLevel::new(level as u8).expect("Level must be 1-6"),
                front_matter_title: front_matter_title.to_string(),
                allow_document_sections: true,
                allow_with_separators: true,
            },
        }
    }

    pub fn strict() -> Self {
        Self {
            config: MD025Config {
                level: HeadingLevel::new(1).unwrap(),
                front_matter_title: "title".to_string(),
                allow_document_sections: false,
                allow_with_separators: false,
            },
        }
    }

    pub fn from_config_struct(config: MD025Config) -> Self {
        Self { config }
    }

    /// Check if the document's frontmatter contains a title field matching the configured key
    fn has_front_matter_title(&self, ctx: &crate::lint_context::LintContext) -> bool {
        if self.config.front_matter_title.is_empty() {
            return false;
        }

        let content_lines = ctx.raw_lines();
        if content_lines.first().map(|l| l.trim()) != Some("---") {
            return false;
        }

        for (idx, line) in content_lines.iter().enumerate().skip(1) {
            if line.trim() == "---" {
                let front_matter_content = content_lines[1..idx].join("\n");
                return front_matter_content
                    .lines()
                    .any(|l| l.trim().starts_with(&format!("{}:", self.config.front_matter_title)));
            }
        }

        false
    }

    /// Check if a heading text suggests it's a legitimate document section
    fn is_document_section_heading(&self, heading_text: &str) -> bool {
        if !self.config.allow_document_sections {
            return false;
        }

        let lower_text = heading_text.to_lowercase();

        // Common section names that are legitimate as separate H1s
        let section_indicators = [
            "appendix",
            "appendices",
            "reference",
            "references",
            "bibliography",
            "index",
            "indices",
            "glossary",
            "glossaries",
            "conclusion",
            "conclusions",
            "summary",
            "executive summary",
            "acknowledgment",
            "acknowledgments",
            "acknowledgement",
            "acknowledgements",
            "about",
            "contact",
            "license",
            "legal",
            "changelog",
            "change log",
            "history",
            "faq",
            "frequently asked questions",
            "troubleshooting",
            "support",
            "installation",
            "setup",
            "getting started",
            "api reference",
            "api documentation",
            "examples",
            "tutorials",
            "guides",
        ];

        // Check if the heading matches these patterns using whole-word matching
        let words: Vec<&str> = lower_text.split_whitespace().collect();
        section_indicators.iter().any(|&indicator| {
            // Multi-word indicators need contiguous word matching
            let indicator_words: Vec<&str> = indicator.split_whitespace().collect();
            let starts_with_indicator = if indicator_words.len() == 1 {
                words.first() == Some(&indicator)
            } else {
                words.len() >= indicator_words.len()
                    && words[..indicator_words.len()] == indicator_words[..]
            };

            starts_with_indicator ||
            lower_text.starts_with(&format!("{indicator}:")) ||
            // Whole-word match anywhere in the heading
            words.contains(&indicator) ||
            // Handle multi-word indicators appearing as a contiguous subsequence
            (indicator_words.len() > 1 && words.windows(indicator_words.len()).any(|w| w == indicator_words.as_slice())) ||
            // Handle appendix numbering like "Appendix A", "Appendix 1"
            (indicator == "appendix" && words.contains(&"appendix") && words.len() >= 2 && {
                let after_appendix = words.iter().skip_while(|&&w| w != "appendix").nth(1);
                matches!(after_appendix, Some(&"a" | &"b" | &"c" | &"d" | &"1" | &"2" | &"3" | &"i" | &"ii" | &"iii" | &"iv"))
            })
        })
    }

    fn is_horizontal_rule(line: &str) -> bool {
        thematic_break::is_thematic_break(line)
    }

    /// Check if a line might be a Setext heading underline
    fn is_potential_setext_heading(ctx: &crate::lint_context::LintContext, line_num: usize) -> bool {
        if line_num == 0 || line_num >= ctx.lines.len() {
            return false;
        }

        let line = ctx.lines[line_num].content(ctx.content).trim();
        let prev_line = if line_num > 0 {
            ctx.lines[line_num - 1].content(ctx.content).trim()
        } else {
            ""
        };

        let is_dash_line = !line.is_empty() && line.chars().all(|c| c == '-');
        let is_equals_line = !line.is_empty() && line.chars().all(|c| c == '=');
        let prev_line_has_content = !prev_line.is_empty() && !Self::is_horizontal_rule(prev_line);
        (is_dash_line || is_equals_line) && prev_line_has_content
    }

    /// Check if headings are separated by horizontal rules
    fn has_separator_before_heading(&self, ctx: &crate::lint_context::LintContext, heading_line: usize) -> bool {
        if !self.config.allow_with_separators || heading_line == 0 {
            return false;
        }

        // Look for horizontal rules in the lines before this heading
        // Check up to 5 lines before the heading for a horizontal rule
        let search_start = heading_line.saturating_sub(5);

        for line_num in search_start..heading_line {
            if line_num >= ctx.lines.len() {
                continue;
            }

            let line = &ctx.lines[line_num].content(ctx.content);
            if Self::is_horizontal_rule(line) && !Self::is_potential_setext_heading(ctx, line_num) {
                // Found a horizontal rule before this heading
                // Check that there's no other heading between the HR and this heading
                let has_intermediate_heading =
                    ((line_num + 1)..heading_line).any(|idx| idx < ctx.lines.len() && ctx.lines[idx].heading.is_some());

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

        let mut warnings = Vec::new();

        let found_title_in_front_matter = self.has_front_matter_title(ctx);

        // Find all headings at the target level using cached information
        let mut target_level_headings = Vec::new();
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading
                && heading.level as usize == self.config.level.as_usize()
                && heading.is_valid
            // Skip malformed headings like `#NoSpace`
            {
                // Ignore if indented 4+ spaces (indented code block) or inside fenced code block
                if line_info.visual_indent >= 4 || line_info.in_code_block {
                    continue;
                }
                target_level_headings.push(line_num);
            }
        }

        // Determine which headings to flag as duplicates.
        // If frontmatter has a title, it counts as the first heading,
        // so ALL body headings at the target level are duplicates.
        // Otherwise, skip the first body heading and flag the rest.
        let headings_to_flag: &[usize] = if found_title_in_front_matter {
            &target_level_headings
        } else if target_level_headings.len() > 1 {
            &target_level_headings[1..]
        } else {
            &[]
        };

        if !headings_to_flag.is_empty() {
            for &line_num in headings_to_flag {
                if let Some(heading) = &ctx.lines[line_num].heading {
                    let heading_text = &heading.text;

                    // Check if this heading should be allowed
                    let should_allow = self.is_document_section_heading(heading_text)
                        || self.has_separator_before_heading(ctx, line_num);

                    if should_allow {
                        continue; // Skip flagging this heading
                    }

                    // Calculate precise character range for the heading text content
                    let line_content = &ctx.lines[line_num].content(ctx.content);
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

                    // For Setext headings, the fix range must cover both
                    // the text line and the underline line
                    let is_setext = matches!(
                        heading.style,
                        crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
                    );
                    let fix_range = if is_setext && line_num + 2 <= ctx.lines.len() {
                        // Cover text line + underline line
                        let text_range = ctx.line_index.line_content_range(line_num + 1);
                        let underline_range = ctx.line_index.line_content_range(line_num + 2);
                        text_range.start..underline_range.end
                    } else {
                        ctx.line_index.line_content_range(line_num + 1)
                    };

                    // Demote to one level below the configured top-level heading.
                    // Markdown only supports levels 1-6, so if the configured level
                    // is already 6, the heading cannot be demoted.
                    let demoted_level = self.config.level.as_usize() + 1;
                    let fix = if demoted_level > 6 {
                        None
                    } else {
                        let leading_spaces = line_content.len() - line_content.trim_start().len();
                        let indentation = " ".repeat(leading_spaces);
                        let raw = &heading.raw_text;
                        let hashes = "#".repeat(demoted_level);
                        let closing = if heading.has_closing_sequence {
                            format!(" {}", "#".repeat(demoted_level))
                        } else {
                            String::new()
                        };
                        let replacement = if raw.is_empty() {
                            format!("{indentation}{hashes}{closing}")
                        } else {
                            format!("{indentation}{hashes} {raw}{closing}")
                        };
                        Some(Fix::new(fix_range, replacement))
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        message: format!(
                            "Multiple top-level headings (level {}) in the same document",
                            self.config.level.as_usize()
                        ),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Error,
                        fix,
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());

        // Build the full fix set: each flagged heading plus every subordinate heading
        // in its section, all demoted by the same +1 delta. Wrapping cascade fixes in
        // synthetic LintWarning objects lets apply_warning_fixes handle range sorting
        // and deduplication automatically.
        let mut all_warnings = warnings.clone();

        let target_level = self.config.level.as_usize();

        for warning in &warnings {
            // warning.line is 1-indexed; convert to 0-indexed for ctx.lines access.
            let heading_line = warning.line - 1;

            // Section boundary: the next heading at or above target_level, or end of doc.
            let section_end = ctx
                .lines
                .iter()
                .enumerate()
                .skip(heading_line + 1)
                .find(|(_, li)| {
                    li.heading.as_ref().is_some_and(|h| {
                        h.level as usize <= target_level && h.is_valid && !li.in_code_block && li.visual_indent < 4
                    })
                })
                .map_or(ctx.lines.len(), |(i, _)| i);

            // Emit a cascade Fix for each subordinate heading inside [heading_line+1, section_end).
            for line_num in (heading_line + 1)..section_end {
                let line_info = &ctx.lines[line_num];
                let Some(heading) = &line_info.heading else {
                    continue;
                };
                if !heading.is_valid || line_info.in_code_block || line_info.visual_indent >= 4 {
                    continue;
                }

                let new_level = heading.level as usize + 1;
                if new_level > 6 {
                    // Heading is already at the maximum depth; no fix possible.
                    continue;
                }

                let line_content = line_info.content(ctx.content);

                // For Setext headings the fix range must cover both the text line and its
                // underline so they are replaced atomically with an ATX heading.
                let is_setext = matches!(
                    heading.style,
                    crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
                );
                let fix_range = if is_setext && line_num + 2 <= ctx.lines.len() {
                    let text_range = ctx.line_index.line_content_range(line_num + 1);
                    let underline_range = ctx.line_index.line_content_range(line_num + 2);
                    text_range.start..underline_range.end
                } else {
                    ctx.line_index.line_content_range(line_num + 1)
                };

                let leading_spaces = line_content.len() - line_content.trim_start().len();
                let indentation = " ".repeat(leading_spaces);
                let hashes = "#".repeat(new_level);
                let raw = &heading.raw_text;
                let closing = if heading.has_closing_sequence {
                    format!(" {}", "#".repeat(new_level))
                } else {
                    String::new()
                };
                let replacement = if raw.is_empty() {
                    format!("{indentation}{hashes}{closing}")
                } else {
                    format!("{indentation}{hashes} {raw}{closing}")
                };

                all_warnings.push(crate::rule::LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: String::new(),
                    line: line_num + 1,
                    column: 1,
                    end_line: line_num + 1,
                    end_column: line_content.chars().count(),
                    severity: crate::rule::Severity::Error,
                    fix: Some(Fix::new(fix_range, replacement)),
                });
            }
        }

        // Filter cascade warnings through the same inline-disable logic applied to the
        // original warnings. This ensures that a subordinate heading on a disabled line
        // (e.g., `<!-- markdownlint-disable-line MD025 -->`) is not cascade-demoted.
        let all_warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(all_warnings, ctx.inline_config(), self.name());

        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &all_warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped for performance
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty
        if ctx.content.is_empty() {
            return true;
        }

        // Skip if no heading markers at all
        if !ctx.likely_has_headings() {
            return true;
        }

        let has_fm_title = self.has_front_matter_title(ctx);

        // Fast path: count target level headings efficiently
        let mut target_level_count = 0;
        for line_info in &ctx.lines {
            if let Some(heading) = &line_info.heading
                && heading.level as usize == self.config.level.as_usize()
            {
                // Ignore if indented 4+ spaces (indented code block), inside fenced code block, or PyMdown block
                if line_info.visual_indent >= 4 || line_info.in_code_block || line_info.in_pymdown_block {
                    continue;
                }
                target_level_count += 1;

                // If frontmatter has a title, even 1 body heading is a duplicate
                if has_fm_title {
                    return false;
                }

                // Otherwise, we need more than 1 to have duplicates
                if target_level_count > 1 {
                    return false;
                }
            }
        }

        // If we have 0 or 1 target level headings (without frontmatter title), skip
        target_level_count <= 1
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD025Config>(config);
        Box::new(Self::from_config_struct(rule_config))
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
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with multiple level-1 headings (non-section names) - should flag
        let content = "# Title 1\n\n## Section 1\n\n# Another Title\n\n## Section 2";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1); // Should flag the second level-1 heading
        assert_eq!(result[0].line, 5);

        // Test with front matter title and a level-1 heading - should flag the body H1
        let content = "---\ntitle: Document Title\n---\n\n# Main Heading\n\n## Section 1";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag body H1 when frontmatter has title");
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_allow_document_sections() {
        // Need to create rule with allow_document_sections = true
        let config = md025_config::MD025Config {
            allow_document_sections: true,
            ..Default::default()
        };
        let rule = MD025SingleTitle::from_config_struct(config);

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
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(result.is_empty(), "Should not flag document sections in: {case}");
        }

        // Test invalid cases that should still be flagged
        let invalid_cases = vec![
            "# Main Title\n\n## Content\n\n# Random Other Title\n\nContent",
            "# First\n\nContent\n\n# Second Title\n\nMore content",
        ];

        for case in invalid_cases {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(!result.is_empty(), "Should flag non-section headings in: {case}");
        }
    }

    #[test]
    fn test_strict_mode() {
        let rule = MD025SingleTitle::strict(); // Has allow_document_sections = false

        // Even document sections should be flagged in strict mode
        let content = "# Main Title\n\n## Content\n\n# Appendix A\n\nAppendix content";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Strict mode should flag all multiple H1s");
    }

    #[test]
    fn test_bounds_checking_bug() {
        // Test case that could trigger bounds error in fix generation
        // When col + self.config.level.as_usize() exceeds line_content.len()
        let rule = MD025SingleTitle::default();

        // Create content with very short second heading
        let content = "# First\n#";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

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
        // Create a heading where col + self.config.level.as_usize() would exceed line length
        let rule = MD025SingleTitle::default();

        // Create content where the second heading is just "#" (length 1)
        // col will be 0, self.config.level.as_usize() is 1, so col + self.config.level.as_usize() = 1
        // This should not exceed bounds for "#" but tests the edge case
        let content = "# First Title\n#";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        // This should not panic and should handle the edge case gracefully
        let result = rule.check(&ctx);
        assert!(result.is_ok());

        if let Ok(warnings) = result
            && !warnings.is_empty()
        {
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

    #[test]
    fn test_horizontal_rule_separators() {
        // Need to create rule with allow_with_separators = true
        let config = md025_config::MD025Config {
            allow_with_separators: true,
            ..Default::default()
        };
        let rule = MD025SingleTitle::from_config_struct(config);

        // Test that headings separated by horizontal rules are allowed
        let content = "# First Title\n\nContent here.\n\n---\n\n# Second Title\n\nMore content.\n\n***\n\n# Third Title\n\nFinal content.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag headings separated by horizontal rules"
        );

        // Test that headings without separators are still flagged
        let content = "# First Title\n\nContent here.\n\n---\n\n# Second Title\n\nMore content.\n\n# Third Title\n\nNo separator before this one.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag the heading without separator");
        assert_eq!(result[0].line, 11); // Third title on line 11

        // Test with allow_with_separators = false
        let strict_rule = MD025SingleTitle::strict();
        let content = "# First Title\n\nContent here.\n\n---\n\n# Second Title\n\nMore content.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = strict_rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Strict mode should flag all multiple H1s regardless of separators"
        );
    }

    #[test]
    fn test_python_comments_in_code_blocks() {
        let rule = MD025SingleTitle::default();

        // Test that Python comments in code blocks are not treated as headers
        let content = "# Main Title\n\n```python\n# This is a Python comment, not a heading\nprint('Hello')\n```\n\n## Section\n\nMore content.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag Python comments in code blocks as headings"
        );

        // Test the fix method doesn't modify Python comments
        let content = "# Main Title\n\n```python\n# Python comment\nprint('test')\n```\n\n# Second Title";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("# Python comment"),
            "Fix should preserve Python comments in code blocks"
        );
        assert!(
            fixed.contains("## Second Title"),
            "Fix should demote the actual second heading"
        );
    }

    #[test]
    fn test_fix_preserves_attribute_lists() {
        let rule = MD025SingleTitle::strict();

        // Duplicate H1 with attribute list - fix should demote to H2 while preserving attrs
        let content = "# First Title\n\n# Second Title { #custom-id .special }";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        // Should flag the second H1
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        // Per-warning fix demotes the heading itself (cascade is handled by fix() method)
        assert!(warnings[0].fix.is_some());

        // Verify fix() preserves attribute list
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("## Second Title { #custom-id .special }"),
            "fix() should demote to H2 while preserving attribute list, got: {fixed}"
        );
    }

    #[test]
    fn test_frontmatter_title_counts_as_h1() {
        let rule = MD025SingleTitle::default();

        // Frontmatter with title + one body H1 → should warn on the body H1
        let content = "---\ntitle: Heading in frontmatter\n---\n\n# Heading in document\n\nSome introductory text.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag body H1 when frontmatter has title");
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_frontmatter_title_with_multiple_body_h1s() {
        let config = md025_config::MD025Config {
            front_matter_title: "title".to_string(),
            ..Default::default()
        };
        let rule = MD025SingleTitle::from_config_struct(config);

        // Frontmatter with title + multiple body H1s → should warn on ALL body H1s
        let content = "---\ntitle: FM Title\n---\n\n# First Body H1\n\nContent\n\n# Second Body H1\n\nMore content";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Should flag all body H1s when frontmatter has title");
        assert_eq!(result[0].line, 5);
        assert_eq!(result[1].line, 9);
    }

    #[test]
    fn test_frontmatter_without_title_no_warning() {
        let rule = MD025SingleTitle::default();

        // Frontmatter without title key + one body H1 → no warning
        let content = "---\nauthor: Someone\ndate: 2024-01-01\n---\n\n# Only Heading\n\nContent here.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag when frontmatter has no title");
    }

    #[test]
    fn test_no_frontmatter_single_h1_no_warning() {
        let rule = MD025SingleTitle::default();

        // No frontmatter + single body H1 → no warning
        let content = "# Only Heading\n\nSome content.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag single H1 without frontmatter");
    }

    #[test]
    fn test_frontmatter_custom_title_key() {
        // Custom front_matter_title key
        let config = md025_config::MD025Config {
            front_matter_title: "heading".to_string(),
            ..Default::default()
        };
        let rule = MD025SingleTitle::from_config_struct(config);

        // Frontmatter with "heading:" key → should count as H1
        let content = "---\nheading: My Heading\n---\n\n# Body Heading\n\nContent.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Should flag body H1 when custom frontmatter key matches"
        );
        assert_eq!(result[0].line, 5);

        // Frontmatter with "title:" but configured for "heading:" → should not count
        let content = "---\ntitle: My Title\n---\n\n# Body Heading\n\nContent.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag when frontmatter key doesn't match config"
        );
    }

    #[test]
    fn test_frontmatter_title_empty_config_disables() {
        // Empty front_matter_title disables frontmatter title detection
        let rule = MD025SingleTitle::new(1, "");

        let content = "---\ntitle: My Title\n---\n\n# Body Heading\n\nContent.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag when front_matter_title is empty");
    }

    #[test]
    fn test_frontmatter_title_with_level_config() {
        // When level is set to 2, frontmatter title counts as the first heading at that level
        let config = md025_config::MD025Config {
            level: HeadingLevel::new(2).unwrap(),
            front_matter_title: "title".to_string(),
            ..Default::default()
        };
        let rule = MD025SingleTitle::from_config_struct(config);

        // Frontmatter with title + body H2 → should flag body H2
        let content = "---\ntitle: FM Title\n---\n\n# Body H1\n\n## Body H2\n\nContent.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Should flag body H2 when level=2 and frontmatter has title"
        );
        assert_eq!(result[0].line, 7);
    }

    #[test]
    fn test_frontmatter_title_fix_demotes_body_heading() {
        let config = md025_config::MD025Config {
            front_matter_title: "title".to_string(),
            ..Default::default()
        };
        let rule = MD025SingleTitle::from_config_struct(config);

        let content = "---\ntitle: FM Title\n---\n\n# Body Heading\n\nContent.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("## Body Heading"),
            "Fix should demote body H1 to H2 when frontmatter has title, got: {fixed}"
        );
        // Frontmatter should be preserved
        assert!(fixed.contains("---\ntitle: FM Title\n---"));
    }

    #[test]
    fn test_frontmatter_title_should_skip_respects_frontmatter() {
        let rule = MD025SingleTitle::default();

        // With frontmatter title + 1 body H1, should_skip should return false
        let content = "---\ntitle: FM Title\n---\n\n# Body Heading\n\nContent.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !rule.should_skip(&ctx),
            "should_skip must return false when frontmatter has title and body has H1"
        );

        // Without frontmatter title + 1 body H1, should_skip should return true
        let content = "---\nauthor: Someone\n---\n\n# Body Heading\n\nContent.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            rule.should_skip(&ctx),
            "should_skip should return true with no frontmatter title and single H1"
        );
    }

    #[test]
    fn test_fix_cascades_subheadings_after_demoting_duplicate_h1() {
        let rule = MD025SingleTitle::default();

        // Exact reproduction from issue #573
        let content = "abcd\n\n# 1_1\n\n# 1_2\n\n## 1_2-2_1\n\n# 1_3\n\n## 1_3-2_1\n\n### 1_3-2_1-3_1\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("# 1_1"), "First H1 must be preserved: {fixed}");
        assert!(
            fixed.contains("## 1_2\n"),
            "Duplicate H1 must be demoted to H2: {fixed}"
        );
        assert!(
            fixed.contains("### 1_2-2_1"),
            "H2 under demoted H1 must cascade to H3: {fixed}"
        );
        assert!(fixed.contains("## 1_3\n"), "Third H1 must be demoted to H2: {fixed}");
        assert!(
            fixed.contains("### 1_3-2_1"),
            "H2 under third demoted H1 must cascade to H3: {fixed}"
        );
        assert!(
            fixed.contains("#### 1_3-2_1-3_1"),
            "H3 under third demoted H1 must cascade to H4: {fixed}"
        );
    }

    #[test]
    fn test_fix_cascades_single_section_only() {
        let rule = MD025SingleTitle::default();

        // Sub-headings of a demoted section must not affect sub-headings of other sections
        let content = "# Main\n\n# Alpha\n\n## Alpha Sub\n\n# Beta\n\n## Beta Sub\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("# Main\n"), "First H1 preserved: {fixed}");
        assert!(fixed.contains("## Alpha\n"), "Alpha H1 demoted to H2: {fixed}");
        assert!(fixed.contains("### Alpha Sub"), "Alpha Sub cascades to H3: {fixed}");
        assert!(fixed.contains("## Beta\n"), "Beta H1 demoted to H2: {fixed}");
        assert!(fixed.contains("### Beta Sub"), "Beta Sub cascades to H3: {fixed}");
    }

    #[test]
    fn test_fix_cascade_stops_at_next_same_level() {
        let rule = MD025SingleTitle::default();

        // H2 under first demoted section must not bleed into content after the next H1
        // (which is itself demoted). The cascade boundary is the next heading at or above
        // the original target level.
        let content = "# Main\n\n# A\n\n## A1\n\n# B\n\n## B1\n\n### B1a\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("## A\n"), "A demoted to H2: {fixed}");
        assert!(fixed.contains("### A1"), "A1 cascades to H3: {fixed}");
        assert!(fixed.contains("## B\n"), "B demoted to H2: {fixed}");
        assert!(fixed.contains("### B1"), "B1 cascades to H3: {fixed}");
        assert!(fixed.contains("#### B1a"), "B1a cascades to H4: {fixed}");
        // Original first H1 still at level 1
        assert!(fixed.contains("# Main"), "Main preserved at H1: {fixed}");
    }

    #[test]
    fn test_fix_cascade_does_not_exceed_level_6() {
        // A heading at level 6 under a demoted section cannot go deeper; it stays at 6.
        let rule = MD025SingleTitle::default();

        // Build a chain: H1, H1, H2, H3, H4, H5, H6 under the second H1
        let content = "# Title\n\n# Section\n\n## L2\n\n### L3\n\n#### L4\n\n##### L5\n\n###### L6\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("# Title"), "First H1 preserved: {fixed}");
        assert!(fixed.contains("## Section"), "Section demoted to H2: {fixed}");
        assert!(fixed.contains("### L2"), "L2 cascades to H3: {fixed}");
        assert!(fixed.contains("#### L3"), "L3 cascades to H4: {fixed}");
        assert!(fixed.contains("##### L4"), "L4 cascades to H5: {fixed}");
        assert!(fixed.contains("###### L5"), "L5 cascades to H6: {fixed}");
        // L6 cannot go to H7 — stays at H6
        assert!(fixed.contains("###### L6"), "L6 at max depth stays at H6: {fixed}");
    }

    #[test]
    fn test_fix_cascade_respects_inline_disable_on_subordinate() {
        // A subordinate heading on a markdownlint-disable-line MD025 line must not
        // be cascade-fixed: the inline disable explicitly opts that line out.
        let rule = MD025SingleTitle::default();

        let content = "# Title\n# Demote\n## Skip <!-- markdownlint-disable-line MD025 -->\n## Cascade\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("## Demote"), "Duplicate H1 should be demoted: {fixed}");
        // ## Skip has an inline disable — cascade must not touch it.
        // Use exact-prefix matching to avoid "## Skip" matching inside "### Skip".
        let skip_line = fixed.lines().find(|l| l.contains("Skip")).unwrap_or("");
        assert!(
            skip_line.starts_with("## Skip"),
            "Inline-disabled subordinate should stay at level 2, got line: {skip_line:?}"
        );
        // ## Cascade has no disable — it falls in the section and must cascade
        assert!(
            fixed.contains("### Cascade"),
            "Non-disabled subordinate should cascade to level 3: {fixed}"
        );
    }

    #[test]
    fn test_section_indicator_whole_word_matching() {
        // Bug: substring matching causes false matches (e.g., "reindex" matches " index")
        let config = md025_config::MD025Config {
            allow_document_sections: true,
            ..Default::default()
        };
        let rule = MD025SingleTitle::from_config_struct(config);

        // These should NOT match section indicators (they contain indicators as substrings)
        let false_positive_cases = vec![
            "# Main Title\n\n# Understanding Reindex Operations",
            "# Main Title\n\n# The Summarization Pipeline",
            "# Main Title\n\n# Data Indexing Strategy",
            "# Main Title\n\n# Unsupported Browsers",
        ];

        for case in false_positive_cases {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.len(),
                1,
                "Should flag duplicate H1 (not a section indicator): {case}"
            );
        }

        // These SHOULD still match as legitimate section indicators
        let true_positive_cases = vec![
            "# Main Title\n\n# Index",
            "# Main Title\n\n# Summary",
            "# Main Title\n\n# About",
            "# Main Title\n\n# References",
        ];

        for case in true_positive_cases {
            let ctx = crate::lint_context::LintContext::new(case, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(result.is_empty(), "Should allow section indicator heading: {case}");
        }
    }
}
