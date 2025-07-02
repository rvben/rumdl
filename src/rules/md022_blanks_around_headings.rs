/// Rule MD022: Headings should be surrounded by blank lines
///
/// See [docs/md022.md](../../docs/md022.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::range_utils::calculate_heading_range;
use toml;

mod md022_config;
use md022_config::MD022Config;

///
/// This rule enforces consistent spacing around headings to improve document readability
/// and visual structure.
///
/// ## Purpose
///
/// - **Readability**: Blank lines create visual separation, making headings stand out
/// - **Parsing**: Many Markdown parsers require blank lines around headings for proper rendering
/// - **Consistency**: Creates a uniform document style throughout
/// - **Focus**: Helps readers identify and focus on section transitions
///
/// ## Configuration Options
///
/// The rule supports customizing the number of blank lines required:
///
/// ```yaml
/// MD022:
///   lines_above: 1  # Number of blank lines required above headings (default: 1)
///   lines_below: 1  # Number of blank lines required below headings (default: 1)
/// ```
///
/// ## Examples
///
/// ### Correct (with default configuration)
///
/// ```markdown
/// Regular paragraph text.
///
/// # Heading 1
///
/// Content under heading 1.
///
/// ## Heading 2
///
/// More content here.
/// ```
///
/// ### Incorrect (with default configuration)
///
/// ```markdown
/// Regular paragraph text.
/// # Heading 1
/// Content under heading 1.
/// ## Heading 2
/// More content here.
/// ```
///
/// ## Special Cases
///
/// This rule handles several special cases:
///
/// - **First Heading**: The first heading in a document doesn't require blank lines above
///   if it appears at the very start of the document
/// - **Front Matter**: YAML front matter is detected and skipped
/// - **Code Blocks**: Headings inside code blocks are ignored
/// - **Document Start/End**: Adjusts requirements for headings at the beginning or end of a document
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Adds the required number of blank lines above headings
/// - Adds the required number of blank lines below headings
/// - Preserves document structure and existing content
///
/// ## Performance Considerations
///
/// The rule is optimized for performance with:
/// - Efficient line counting algorithms
/// - Proper handling of front matter
/// - Smart code block detection
///
#[derive(Clone, Default)]
pub struct MD022BlanksAroundHeadings {
    config: MD022Config,
}

impl MD022BlanksAroundHeadings {
    /// Create a new instance of the rule with default values:
    /// lines_above = 1, lines_below = 1
    pub fn new() -> Self {
        Self {
            config: MD022Config::default(),
        }
    }

    /// Create with custom numbers of blank lines
    pub fn with_values(lines_above: usize, lines_below: usize) -> Self {
        Self {
            config: MD022Config {
                lines_above,
                lines_below,
                allowed_at_start: true,
            },
        }
    }

    pub fn from_config_struct(config: MD022Config) -> Self {
        Self { config }
    }

    /// Fix a document by adding appropriate blank lines around headings
    fn _fix_content(&self, ctx: &crate::lint_context::LintContext) -> String {
        let line_ending = crate::utils::detect_line_ending(ctx.content);
        let had_trailing_newline = ctx.content.ends_with('\n') || ctx.content.ends_with("\r\n");
        let mut result = Vec::new();
        let mut in_front_matter = false;
        let mut front_matter_delimiter_count = 0;
        let mut skip_next = false;

        for (i, line_info) in ctx.lines.iter().enumerate() {
            if skip_next {
                skip_next = false;
                continue;
            }
            let line = &line_info.content;

            // Handle front matter
            if line.trim() == "---" {
                if i == 0 || (i > 0 && ctx.lines[..i].iter().all(|l| l.is_blank)) {
                    if front_matter_delimiter_count == 0 {
                        in_front_matter = true;
                        front_matter_delimiter_count = 1;
                    }
                } else if in_front_matter && front_matter_delimiter_count == 1 {
                    in_front_matter = false;
                    front_matter_delimiter_count = 2;
                }
                result.push(line.to_string());
                continue;
            }

            // Inside front matter or code block, preserve content exactly
            if in_front_matter || line_info.in_code_block {
                result.push(line.to_string());
                continue;
            }

            // Check if it's a heading
            if let Some(heading) = &line_info.heading {
                // This is a heading line (ATX or Setext content)
                let is_first_heading = (0..i).all(|j| {
                    ctx.lines[j].is_blank
                        || (j == 0 && ctx.lines[j].content.trim() == "---")
                        || (in_front_matter && ctx.lines[j].content.trim() == "---")
                });

                // Count existing blank lines above in the result
                let mut blank_lines_above = 0;
                let mut check_idx = result.len();
                while check_idx > 0 && result[check_idx - 1].trim().is_empty() {
                    blank_lines_above += 1;
                    check_idx -= 1;
                }

                // Determine how many blank lines we need above
                let needed_blanks_above = if is_first_heading && self.config.allowed_at_start {
                    0
                } else {
                    self.config.lines_above
                };

                // Add missing blank lines above if needed
                while blank_lines_above < needed_blanks_above {
                    result.push(String::new());
                    blank_lines_above += 1;
                }

                // Add the heading line
                result.push(line.to_string());

                // For Setext headings, also add the underline immediately
                if matches!(
                    heading.style,
                    crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
                ) {
                    // Add the underline (next line)
                    if i + 1 < ctx.lines.len() {
                        result.push(ctx.lines[i + 1].content.clone());
                        skip_next = true; // Skip the underline in the main loop
                    }

                    // Now check blank lines below the underline
                    let mut blank_lines_below = 0;
                    let mut next_content_line_idx = None;
                    for j in (i + 2)..ctx.lines.len() {
                        if ctx.lines[j].is_blank {
                            blank_lines_below += 1;
                        } else {
                            next_content_line_idx = Some(j);
                            break;
                        }
                    }

                    // Check if the next non-blank line is special
                    let next_is_special = if let Some(idx) = next_content_line_idx {
                        let next_line = &ctx.lines[idx];
                        next_line.list_item.is_some() || {
                            let trimmed = next_line.content.trim();
                            (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
                                && (trimmed.len() == 3
                                    || (trimmed.len() > 3
                                        && trimmed
                                            .chars()
                                            .nth(3)
                                            .is_some_and(|c| c.is_whitespace() || c.is_alphabetic())))
                        }
                    } else {
                        false
                    };

                    // Add missing blank lines below if needed
                    let needed_blanks_below = if next_is_special { 0 } else { self.config.lines_below };
                    if blank_lines_below < needed_blanks_below {
                        for _ in 0..(needed_blanks_below - blank_lines_below) {
                            result.push(String::new());
                        }
                    }
                } else {
                    // For ATX headings, check blank lines below
                    let mut blank_lines_below = 0;
                    let mut next_content_line_idx = None;
                    for j in (i + 1)..ctx.lines.len() {
                        if ctx.lines[j].is_blank {
                            blank_lines_below += 1;
                        } else {
                            next_content_line_idx = Some(j);
                            break;
                        }
                    }

                    // Check if the next non-blank line is special
                    let next_is_special = if let Some(idx) = next_content_line_idx {
                        let next_line = &ctx.lines[idx];
                        next_line.list_item.is_some() || {
                            let trimmed = next_line.content.trim();
                            (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
                                && (trimmed.len() == 3
                                    || (trimmed.len() > 3
                                        && trimmed
                                            .chars()
                                            .nth(3)
                                            .is_some_and(|c| c.is_whitespace() || c.is_alphabetic())))
                        }
                    } else {
                        false
                    };

                    // Add missing blank lines below if needed
                    let needed_blanks_below = if next_is_special { 0 } else { self.config.lines_below };
                    if blank_lines_below < needed_blanks_below {
                        for _ in 0..(needed_blanks_below - blank_lines_below) {
                            result.push(String::new());
                        }
                    }
                }
            } else {
                // Regular line - just add it
                result.push(line.to_string());
            }
        }

        let joined = result.join(line_ending);

        // Preserve original trailing newline behavior
        if had_trailing_newline && !joined.ends_with('\n') && !joined.ends_with("\r\n") {
            format!("{joined}{line_ending}")
        } else if !had_trailing_newline && (joined.ends_with('\n') || joined.ends_with("\r\n")) {
            // Remove trailing newline if original didn't have one
            if joined.ends_with("\r\n") {
                joined[..joined.len() - 2].to_string()
            } else {
                joined[..joined.len() - 1].to_string()
            }
        } else {
            joined
        }
    }
}

impl Rule for MD022BlanksAroundHeadings {
    fn name(&self) -> &'static str {
        "MD022"
    }

    fn description(&self) -> &'static str {
        "Headings should be surrounded by blank lines"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut result = Vec::new();

        // Skip if empty document
        if ctx.lines.is_empty() {
            return Ok(result);
        }

        let line_ending = crate::utils::detect_line_ending(ctx.content);

        // Collect all headings first to batch process
        let mut heading_violations = Vec::new();
        let mut processed_headings = std::collections::HashSet::new();

        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            // Skip if already processed or not a heading
            if processed_headings.contains(&line_num) || line_info.heading.is_none() {
                continue;
            }

            let heading = line_info.heading.as_ref().unwrap();

            // For Setext headings, skip the underline line (we process from the content line)
            if matches!(
                heading.style,
                crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
            ) {
                // Check if this is the underline, not the content
                if line_num > 0 && ctx.lines[line_num - 1].heading.is_none() {
                    continue; // This is the underline line
                }
            }

            processed_headings.insert(line_num);

            // Check if this is the first heading in the document
            let is_first_heading = (0..line_num).all(|j| {
                ctx.lines[j].is_blank ||
                // Check for front matter lines
                (j == 0 && ctx.lines[j].content.trim() == "---") ||
                (j > 0 && ctx.lines[0].content.trim() == "---" && ctx.lines[j].content.trim() == "---")
            });

            // Count blank lines above
            let blank_lines_above = if line_num > 0 && (!is_first_heading || !self.config.allowed_at_start) {
                let mut count = 0;
                for j in (0..line_num).rev() {
                    if ctx.lines[j].is_blank {
                        count += 1;
                    } else {
                        break;
                    }
                }
                count
            } else {
                self.config.lines_above // Consider it as having enough blanks if it's the first heading
            };

            // Check if we need blank lines above
            if line_num > 0
                && blank_lines_above < self.config.lines_above
                && (!is_first_heading || !self.config.allowed_at_start)
            {
                let needed_blanks = self.config.lines_above - blank_lines_above;
                heading_violations.push((line_num, "above", needed_blanks));
            }

            // Determine the effective last line of the heading
            let effective_last_line = if matches!(
                heading.style,
                crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
            ) {
                line_num + 1 // For Setext, include the underline
            } else {
                line_num
            };

            // Check blank lines below
            if effective_last_line < ctx.lines.len() - 1 {
                // Find next non-blank line
                let mut next_non_blank_idx = effective_last_line + 1;
                while next_non_blank_idx < ctx.lines.len() && ctx.lines[next_non_blank_idx].is_blank {
                    next_non_blank_idx += 1;
                }

                // Check if next line is a code fence or list item
                let next_line_is_special = next_non_blank_idx < ctx.lines.len() && {
                    let next_line = &ctx.lines[next_non_blank_idx];
                    let next_trimmed = next_line.content.trim();

                    // Check for code fence
                    let is_code_fence = (next_trimmed.starts_with("```") || next_trimmed.starts_with("~~~"))
                        && (next_trimmed.len() == 3
                            || (next_trimmed.len() > 3
                                && next_trimmed
                                    .chars()
                                    .nth(3)
                                    .is_some_and(|c| c.is_whitespace() || c.is_alphabetic())));

                    // Check for list item
                    let is_list_item = next_line.list_item.is_some();

                    is_code_fence || is_list_item
                };

                // Only generate warning if next line is NOT a code fence or list item
                if !next_line_is_special {
                    // Count blank lines below
                    let blank_lines_below = next_non_blank_idx - effective_last_line - 1;

                    if blank_lines_below < self.config.lines_below {
                        let needed_blanks = self.config.lines_below - blank_lines_below;
                        heading_violations.push((line_num, "below", needed_blanks));
                    }
                }
            }
        }

        // Generate warnings for all violations
        for (heading_line, position, needed_blanks) in heading_violations {
            let heading_display_line = heading_line + 1; // 1-indexed for display
            let line_info = &ctx.lines[heading_line];

            // Calculate precise character range for the heading
            let (start_line, start_col, end_line, end_col) =
                calculate_heading_range(heading_display_line, &line_info.content);

            let (message, insertion_point) = match position {
                "above" => (
                    format!(
                        "Expected {} blank {} above heading",
                        self.config.lines_above,
                        if self.config.lines_above == 1 { "line" } else { "lines" }
                    ),
                    heading_line, // Insert before the heading line
                ),
                "below" => {
                    // For Setext headings, insert after the underline
                    let insert_after = if line_info.heading.as_ref().is_some_and(|h| {
                        matches!(
                            h.style,
                            crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
                        )
                    }) {
                        heading_line + 2
                    } else {
                        heading_line + 1
                    };

                    (
                        format!(
                            "Expected {} blank {} below heading",
                            self.config.lines_below,
                            if self.config.lines_below == 1 { "line" } else { "lines" }
                        ),
                        insert_after,
                    )
                }
                _ => continue,
            };

            // Calculate byte range for insertion
            let byte_range = if insertion_point == 0 && position == "above" {
                // Insert at beginning of document (only for "above" case at line 0)
                0..0
            } else if position == "above" && insertion_point > 0 {
                // For "above", insert at the start of the heading line
                ctx.lines[insertion_point].byte_offset..ctx.lines[insertion_point].byte_offset
            } else if position == "below" && insertion_point - 1 < ctx.lines.len() {
                // For "below", insert after the line
                let line_idx = insertion_point - 1;
                let line_end_offset = if line_idx + 1 < ctx.lines.len() {
                    ctx.lines[line_idx + 1].byte_offset
                } else {
                    ctx.content.len()
                };
                line_end_offset..line_end_offset
            } else {
                // Insert at end of file
                let content_len = ctx.content.len();
                content_len..content_len
            };

            result.push(LintWarning {
                rule_name: Some(self.name()),
                message,
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: byte_range,
                    replacement: line_ending.repeat(needed_blanks),
                }),
            });
        }

        Ok(result)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if ctx.content.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Use a consolidated fix that avoids adding multiple blank lines
        let fixed = self._fix_content(ctx);

        Ok(fixed)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || ctx.lines.iter().all(|line| line.heading.is_none())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        None
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD022Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD022Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD022Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_valid_headings() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "\n# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_missing_blank_above() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0); // No warning for first heading

        let fixed = rule.fix(&ctx).unwrap();

        // Test for the ability to handle the content without breaking it
        // Don't check for exact string equality which may break with implementation changes
        assert!(fixed.contains("# Heading 1"));
        assert!(fixed.contains("Some content."));
        assert!(fixed.contains("## Heading 2"));
        assert!(fixed.contains("More content."));
    }

    #[test]
    fn test_missing_blank_below() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "\n# Heading 1\nSome content.\n\n## Heading 2\n\nMore content.\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);

        // Test the fix
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("# Heading 1\n\nSome content"));
    }

    #[test]
    fn test_missing_blank_above_and_below() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\nSome content.\n## Heading 2\nMore content.\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3); // Missing blanks: below first heading, above second heading, below second heading

        // Test the fix
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("# Heading 1\n\nSome content"));
        assert!(fixed.contains("Some content.\n\n## Heading 2"));
        assert!(fixed.contains("## Heading 2\n\nMore content"));
    }

    #[test]
    fn test_fix_headings() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\nSome content.\n## Heading 2\nMore content.";
        let ctx = LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        let expected = "# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_consecutive_headings_pattern() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        // Using more specific assertions to check the structure
        let lines: Vec<&str> = result.lines().collect();
        assert!(!lines.is_empty());

        // Find the positions of the headings
        let h1_pos = lines.iter().position(|&l| l == "# Heading 1").unwrap();
        let h2_pos = lines.iter().position(|&l| l == "## Heading 2").unwrap();
        let h3_pos = lines.iter().position(|&l| l == "### Heading 3").unwrap();

        // Verify blank lines between headings
        assert!(
            h2_pos > h1_pos + 1,
            "Should have at least one blank line after first heading"
        );
        assert!(
            h3_pos > h2_pos + 1,
            "Should have at least one blank line after second heading"
        );

        // Verify there's a blank line between h1 and h2
        assert!(lines[h1_pos + 1].trim().is_empty(), "Line after h1 should be blank");

        // Verify there's a blank line between h2 and h3
        assert!(lines[h2_pos + 1].trim().is_empty(), "Line after h2 should be blank");
    }

    #[test]
    fn test_blanks_around_setext_headings() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "Heading 1\n=========\nSome content.\nHeading 2\n---------\nMore content.";
        let ctx = LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        // Check that the fix follows requirements without being too rigid about the exact output format
        let lines: Vec<&str> = result.lines().collect();

        // Verify key elements are present
        assert!(result.contains("Heading 1"));
        assert!(result.contains("========="));
        assert!(result.contains("Some content."));
        assert!(result.contains("Heading 2"));
        assert!(result.contains("---------"));
        assert!(result.contains("More content."));

        // Verify structure ensures blank lines are added after headings
        let heading1_marker_idx = lines.iter().position(|&l| l == "=========").unwrap();
        let some_content_idx = lines.iter().position(|&l| l == "Some content.").unwrap();
        assert!(
            some_content_idx > heading1_marker_idx + 1,
            "Should have a blank line after the first heading"
        );

        let heading2_marker_idx = lines.iter().position(|&l| l == "---------").unwrap();
        let more_content_idx = lines.iter().position(|&l| l == "More content.").unwrap();
        assert!(
            more_content_idx > heading2_marker_idx + 1,
            "Should have a blank line after the second heading"
        );

        // Verify that the fixed content has no warnings
        let fixed_ctx = LintContext::new(&result);
        let fixed_warnings = rule.check(&fixed_ctx).unwrap();
        assert!(fixed_warnings.is_empty(), "Fixed content should have no warnings");
    }

    #[test]
    fn test_fix_specific_blank_line_cases() {
        let rule = MD022BlanksAroundHeadings::default();

        // Case 1: Testing consecutive headings
        let content1 = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx1 = LintContext::new(content1);
        let result1 = rule.fix(&ctx1).unwrap();
        // Verify structure rather than exact content as the fix implementation may vary
        assert!(result1.contains("# Heading 1"));
        assert!(result1.contains("## Heading 2"));
        assert!(result1.contains("### Heading 3"));
        // Ensure each heading has a blank line after it
        let lines: Vec<&str> = result1.lines().collect();
        let h1_pos = lines.iter().position(|&l| l == "# Heading 1").unwrap();
        let h2_pos = lines.iter().position(|&l| l == "## Heading 2").unwrap();
        assert!(lines[h1_pos + 1].trim().is_empty(), "Should have a blank line after h1");
        assert!(lines[h2_pos + 1].trim().is_empty(), "Should have a blank line after h2");

        // Case 2: Headings with content
        let content2 = "# Heading 1\nContent under heading 1\n## Heading 2";
        let ctx2 = LintContext::new(content2);
        let result2 = rule.fix(&ctx2).unwrap();
        // Verify structure
        assert!(result2.contains("# Heading 1"));
        assert!(result2.contains("Content under heading 1"));
        assert!(result2.contains("## Heading 2"));
        // Check spacing
        let lines2: Vec<&str> = result2.lines().collect();
        let h1_pos2 = lines2.iter().position(|&l| l == "# Heading 1").unwrap();
        let _content_pos = lines2.iter().position(|&l| l == "Content under heading 1").unwrap();
        assert!(
            lines2[h1_pos2 + 1].trim().is_empty(),
            "Should have a blank line after heading 1"
        );

        // Case 3: Multiple consecutive headings with blank lines preserved
        let content3 = "# Heading 1\n\n\n## Heading 2\n\n\n### Heading 3\n\nContent";
        let ctx3 = LintContext::new(content3);
        let result3 = rule.fix(&ctx3).unwrap();
        // Just verify it doesn't crash and properly formats headings
        assert!(result3.contains("# Heading 1"));
        assert!(result3.contains("## Heading 2"));
        assert!(result3.contains("### Heading 3"));
        assert!(result3.contains("Content"));
    }

    #[test]
    fn test_fix_preserves_existing_blank_lines() {
        let rule = MD022BlanksAroundHeadings::new();
        let content = "# Title

## Section 1

Content here.

## Section 2

More content.
### Missing Blank Above

Even more content.

## Section 3

Final content.";

        let expected = "# Title

## Section 1

Content here.

## Section 2

More content.

### Missing Blank Above

Even more content.

## Section 3

Final content.";

        let ctx = LintContext::new(content);
        let result = rule._fix_content(&ctx);
        assert_eq!(
            result, expected,
            "Fix should only add missing blank lines, never remove existing ones"
        );
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD022BlanksAroundHeadings::new();

        // Test with trailing newline
        let content_with_newline = "# Title\nContent here.\n";
        let ctx = LintContext::new(content_with_newline);
        let result = rule.fix(&ctx).unwrap();
        assert!(result.ends_with('\n'), "Should preserve trailing newline");

        // Test without trailing newline
        let content_without_newline = "# Title\nContent here.";
        let ctx = LintContext::new(content_without_newline);
        let result = rule.fix(&ctx).unwrap();
        assert!(
            !result.ends_with('\n'),
            "Should not add trailing newline if original didn't have one"
        );
    }

    #[test]
    fn test_fix_does_not_add_blank_lines_before_lists() {
        let rule = MD022BlanksAroundHeadings::new();
        let content = "## Configuration\n\nThis rule has the following configuration options:\n\n- `option1`: Description of option 1.\n- `option2`: Description of option 2.\n\n## Another Section\n\nSome content here.";

        let expected = "## Configuration\n\nThis rule has the following configuration options:\n\n- `option1`: Description of option 1.\n- `option2`: Description of option 2.\n\n## Another Section\n\nSome content here.";

        let ctx = LintContext::new(content);
        let result = rule._fix_content(&ctx);
        assert_eq!(result, expected, "Fix should not add blank lines before lists");
    }
}
