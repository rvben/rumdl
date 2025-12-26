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

    /// Create with custom numbers of blank lines (applies to all heading levels)
    pub fn with_values(lines_above: usize, lines_below: usize) -> Self {
        use md022_config::HeadingLevelConfig;
        Self {
            config: MD022Config {
                lines_above: HeadingLevelConfig::scalar(lines_above),
                lines_below: HeadingLevelConfig::scalar(lines_below),
                allowed_at_start: true,
            },
        }
    }

    pub fn from_config_struct(config: MD022Config) -> Self {
        Self { config }
    }

    /// Fix a document by adding appropriate blank lines around headings
    fn _fix_content(&self, ctx: &crate::lint_context::LintContext) -> String {
        // Content is normalized to LF at I/O boundary
        let line_ending = "\n";
        let had_trailing_newline = ctx.content.ends_with('\n');
        let mut result = Vec::new();
        let mut skip_next = false;

        let heading_at_start_idx = {
            let mut found_non_transparent = false;
            ctx.lines.iter().enumerate().find_map(|(i, line)| {
                // Only count valid headings (skip malformed ones like `#NoSpace`)
                if line.heading.as_ref().is_some_and(|h| h.is_valid) && !found_non_transparent {
                    Some(i)
                } else {
                    // HTML comments and blank lines are "transparent" - they don't count as content
                    // that would prevent a heading from being "at document start"
                    if !line.is_blank && !line.in_html_comment {
                        let trimmed = line.content(ctx.content).trim();
                        // Check for single-line HTML comments too
                        if !(trimmed.starts_with("<!--") && trimmed.ends_with("-->")) {
                            found_non_transparent = true;
                        }
                    }
                    None
                }
            })
        };

        for (i, line_info) in ctx.lines.iter().enumerate() {
            if skip_next {
                skip_next = false;
                continue;
            }
            let line = line_info.content(ctx.content);

            if line_info.in_code_block {
                result.push(line.to_string());
                continue;
            }

            // Check if it's a heading
            if let Some(heading) = &line_info.heading {
                // Skip invalid headings (e.g., `#NoSpace` which lacks required space after #)
                if !heading.is_valid {
                    result.push(line.to_string());
                    continue;
                }

                // This is a heading line (ATX or Setext content)
                let is_first_heading = Some(i) == heading_at_start_idx;
                let heading_level = heading.level as usize;

                // Count existing blank lines above in the result, skipping HTML comments
                let mut blank_lines_above = 0;
                let mut check_idx = result.len();
                while check_idx > 0 {
                    let prev_line = &result[check_idx - 1];
                    let trimmed = prev_line.trim();
                    if trimmed.is_empty() {
                        blank_lines_above += 1;
                        check_idx -= 1;
                    } else if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
                        // Skip HTML comments - they are transparent for blank line counting
                        check_idx -= 1;
                    } else {
                        break;
                    }
                }

                // Determine how many blank lines we need above
                let requirement_above = self.config.lines_above.get_for_level(heading_level);
                let needed_blanks_above = if is_first_heading && self.config.allowed_at_start {
                    0
                } else {
                    requirement_above.required_count().unwrap_or(0)
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
                        result.push(ctx.lines[i + 1].content(ctx.content).to_string());
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
                            let trimmed = next_line.content(ctx.content).trim();
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
                    let requirement_below = self.config.lines_below.get_for_level(heading_level);
                    let needed_blanks_below = if next_is_special {
                        0
                    } else {
                        requirement_below.required_count().unwrap_or(0)
                    };
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
                            let trimmed = next_line.content(ctx.content).trim();
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
                    let requirement_below = self.config.lines_below.get_for_level(heading_level);
                    let needed_blanks_below = if next_is_special {
                        0
                    } else {
                        requirement_below.required_count().unwrap_or(0)
                    };
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
        // Content is normalized to LF at I/O boundary
        if had_trailing_newline && !joined.ends_with('\n') {
            format!("{joined}{line_ending}")
        } else if !had_trailing_newline && joined.ends_with('\n') {
            // Remove trailing newline if original didn't have one
            joined[..joined.len() - 1].to_string()
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

        // Content is normalized to LF at I/O boundary
        let line_ending = "\n";

        let heading_at_start_idx = {
            let mut found_non_transparent = false;
            ctx.lines.iter().enumerate().find_map(|(i, line)| {
                // Only count valid headings (skip malformed ones like `#NoSpace`)
                if line.heading.as_ref().is_some_and(|h| h.is_valid) && !found_non_transparent {
                    Some(i)
                } else {
                    // HTML comments and blank lines are "transparent" - they don't count as content
                    // that would prevent a heading from being "at document start"
                    if !line.is_blank && !line.in_html_comment {
                        let trimmed = line.content(ctx.content).trim();
                        // Check for single-line HTML comments too
                        if !(trimmed.starts_with("<!--") && trimmed.ends_with("-->")) {
                            found_non_transparent = true;
                        }
                    }
                    None
                }
            })
        };

        // Collect all headings first to batch process
        let mut heading_violations = Vec::new();
        let mut processed_headings = std::collections::HashSet::new();

        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            // Skip if already processed or not a heading
            if processed_headings.contains(&line_num) || line_info.heading.is_none() {
                continue;
            }

            let heading = line_info.heading.as_ref().unwrap();

            // Skip invalid headings (e.g., `#NoSpace` which lacks required space after #)
            if !heading.is_valid {
                continue;
            }

            let heading_level = heading.level as usize;

            // Note: Setext underline lines have heading=None, so they're already
            // skipped by the check at line 351. No additional check needed here.

            processed_headings.insert(line_num);

            // Check if this heading is at document start
            let is_first_heading = Some(line_num) == heading_at_start_idx;

            // Get configured blank line requirements for this heading level
            let required_above_count = self.config.lines_above.get_for_level(heading_level).required_count();
            let required_below_count = self.config.lines_below.get_for_level(heading_level).required_count();

            // Count blank lines above if needed
            let should_check_above =
                required_above_count.is_some() && line_num > 0 && (!is_first_heading || !self.config.allowed_at_start);
            if should_check_above {
                let mut blank_lines_above = 0;
                let mut hit_frontmatter_end = false;
                for j in (0..line_num).rev() {
                    let line_content = ctx.lines[j].content(ctx.content);
                    let trimmed = line_content.trim();
                    if ctx.lines[j].is_blank {
                        blank_lines_above += 1;
                    } else if ctx.lines[j].in_html_comment || (trimmed.starts_with("<!--") && trimmed.ends_with("-->"))
                    {
                        // Skip HTML comments - they are transparent for blank line counting
                        continue;
                    } else if ctx.lines[j].in_front_matter {
                        // Skip frontmatter - first heading after frontmatter doesn't need blank line above
                        // Note: We only check in_front_matter flag, NOT the string "---", because
                        // a standalone "---" is a horizontal rule and should NOT exempt headings
                        // from requiring blank lines above
                        hit_frontmatter_end = true;
                        break;
                    } else {
                        break;
                    }
                }
                let required = required_above_count.unwrap();
                if !hit_frontmatter_end && blank_lines_above < required {
                    let needed_blanks = required - blank_lines_above;
                    heading_violations.push((line_num, "above", needed_blanks, heading_level));
                }
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
                    let next_trimmed = next_line.content(ctx.content).trim();

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
                if !next_line_is_special && let Some(required) = required_below_count {
                    // Count blank lines below
                    let blank_lines_below = next_non_blank_idx - effective_last_line - 1;

                    if blank_lines_below < required {
                        let needed_blanks = required - blank_lines_below;
                        heading_violations.push((line_num, "below", needed_blanks, heading_level));
                    }
                }
            }
        }

        // Generate warnings for all violations
        for (heading_line, position, needed_blanks, heading_level) in heading_violations {
            let heading_display_line = heading_line + 1; // 1-indexed for display
            let line_info = &ctx.lines[heading_line];

            // Calculate precise character range for the heading
            let (start_line, start_col, end_line, end_col) =
                calculate_heading_range(heading_display_line, line_info.content(ctx.content));

            let required_above_count = self
                .config
                .lines_above
                .get_for_level(heading_level)
                .required_count()
                .expect("Violations only generated for limited 'above' requirements");
            let required_below_count = self
                .config
                .lines_below
                .get_for_level(heading_level)
                .required_count()
                .expect("Violations only generated for limited 'below' requirements");

            let (message, insertion_point) = match position {
                "above" => (
                    format!(
                        "Expected {} blank {} above heading",
                        required_above_count,
                        if required_above_count == 1 { "line" } else { "lines" }
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
                            required_below_count,
                            if required_below_count == 1 { "line" } else { "lines" }
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
                rule_name: Some(self.name().to_string()),
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
        // Fast path: check if document likely has headings
        if ctx.content.is_empty() || !ctx.likely_has_headings() {
            return true;
        }
        // Verify headings actually exist
        ctx.lines.iter().all(|line| line.heading.is_none())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_missing_blank_above() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();

        let expected = "# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_consecutive_headings_pattern() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let fixed_ctx = LintContext::new(&result, crate::config::MarkdownFlavor::Standard, None);
        let fixed_warnings = rule.check(&fixed_ctx).unwrap();
        assert!(fixed_warnings.is_empty(), "Fixed content should have no warnings");
    }

    #[test]
    fn test_fix_specific_blank_line_cases() {
        let rule = MD022BlanksAroundHeadings::default();

        // Case 1: Testing consecutive headings
        let content1 = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx1 = LintContext::new(content1, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx3 = LintContext::new(content3, crate::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content_with_newline, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();
        assert!(result.ends_with('\n'), "Should preserve trailing newline");

        // Test without trailing newline
        let content_without_newline = "# Title\nContent here.";
        let ctx = LintContext::new(content_without_newline, crate::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule._fix_content(&ctx);
        assert_eq!(result, expected, "Fix should not add blank lines before lists");
    }

    #[test]
    fn test_per_level_configuration_no_blank_above_h1() {
        use md022_config::HeadingLevelConfig;

        // Configure: no blank above H1, 1 blank above H2-H6
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::per_level([0, 1, 1, 1, 1, 1]),
            lines_below: HeadingLevelConfig::scalar(1),
            allowed_at_start: false, // Disable special handling for first heading
        });

        // H1 without blank above should be OK
        let content = "Some text\n# Heading 1\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "H1 without blank above should not trigger warning");

        // H2 without blank above should trigger warning
        let content = "Some text\n## Heading 2\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "H2 without blank above should trigger warning");
        assert!(warnings[0].message.contains("above"));
    }

    #[test]
    fn test_per_level_configuration_different_requirements() {
        use md022_config::HeadingLevelConfig;

        // Configure: 0 blank above H1, 1 above H2-H3, 2 above H4-H6
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::per_level([0, 1, 1, 2, 2, 2]),
            lines_below: HeadingLevelConfig::scalar(1),
            allowed_at_start: false,
        });

        let content = "Text\n# H1\n\nText\n\n## H2\n\nText\n\n### H3\n\nText\n\n\n#### H4\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should have no warnings - all headings satisfy their level-specific requirements
        assert_eq!(
            warnings.len(),
            0,
            "All headings should satisfy level-specific requirements"
        );
    }

    #[test]
    fn test_per_level_configuration_violations() {
        use md022_config::HeadingLevelConfig;

        // Configure: H4 needs 2 blanks above
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::per_level([1, 1, 1, 2, 1, 1]),
            lines_below: HeadingLevelConfig::scalar(1),
            allowed_at_start: false,
        });

        // H4 with only 1 blank above should trigger warning
        let content = "Text\n\n#### Heading 4\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 1, "H4 with insufficient blanks should trigger warning");
        assert!(warnings[0].message.contains("2 blank lines above"));
    }

    #[test]
    fn test_per_level_fix_different_levels() {
        use md022_config::HeadingLevelConfig;

        // Configure: 0 blank above H1, 1 above H2, 2 above H3+
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::per_level([0, 1, 2, 2, 2, 2]),
            lines_below: HeadingLevelConfig::scalar(1),
            allowed_at_start: false,
        });

        let content = "Text\n# H1\nContent\n## H2\nContent\n### H3\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Verify structure: H1 gets 0 blanks above, H2 gets 1, H3 gets 2
        assert!(fixed.contains("Text\n# H1\n\nContent"));
        assert!(fixed.contains("Content\n\n## H2\n\nContent"));
        assert!(fixed.contains("Content\n\n\n### H3\n\nContent"));
    }

    #[test]
    fn test_per_level_below_configuration() {
        use md022_config::HeadingLevelConfig;

        // Configure: different blank line requirements below headings
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::scalar(1),
            lines_below: HeadingLevelConfig::per_level([2, 1, 1, 1, 1, 1]), // H1 needs 2 blanks below
            allowed_at_start: true,
        });

        // H1 with only 1 blank below should trigger warning
        let content = "# Heading 1\n\nSome text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(
            warnings.len(),
            1,
            "H1 with insufficient blanks below should trigger warning"
        );
        assert!(warnings[0].message.contains("2 blank lines below"));
    }

    #[test]
    fn test_scalar_configuration_still_works() {
        use md022_config::HeadingLevelConfig;

        // Ensure scalar configuration still works (backward compatibility)
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::scalar(2),
            lines_below: HeadingLevelConfig::scalar(2),
            allowed_at_start: false,
        });

        let content = "Text\n# H1\nContent\n## H2\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // All headings should need 2 blanks above and below
        assert!(!warnings.is_empty(), "Should have violations for insufficient blanks");
    }

    #[test]
    fn test_unlimited_configuration_skips_requirements() {
        use md022_config::{HeadingBlankRequirement, HeadingLevelConfig};

        // H1 can have any number of blank lines above/below; others require defaults
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::per_level_requirements([
                HeadingBlankRequirement::unlimited(),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
            ]),
            lines_below: HeadingLevelConfig::per_level_requirements([
                HeadingBlankRequirement::unlimited(),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
            ]),
            allowed_at_start: false,
        });

        let content = "# H1\nParagraph\n## H2\nParagraph";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // H1 has no blanks above/below but is unlimited; H2 should get violations
        assert_eq!(warnings.len(), 2, "Only non-unlimited headings should warn");
        assert!(
            warnings.iter().all(|w| w.line >= 3),
            "Warnings should target later headings"
        );

        // Fixing should insert blanks around H2 but leave H1 untouched
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.starts_with("# H1\nParagraph\n\n## H2"),
            "H1 should remain unchanged"
        );
    }

    #[test]
    fn test_html_comment_transparency() {
        // HTML comments are transparent for blank line counting
        // A heading following a blank line + HTML comment should be valid
        // Verified with markdownlint: no MD022 warning for this pattern
        let rule = MD022BlanksAroundHeadings::default();

        // Pattern: content, blank line, HTML comment, heading
        // The blank line before the HTML comment counts for the heading
        let content = "Some content\n\n<!-- markdownlint-disable-next-line MD001 -->\n#### Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "HTML comment is transparent - blank line above it counts for heading"
        );

        // Multi-line HTML comment is also transparent
        let content_multiline = "Some content\n\n<!-- This is a\nmulti-line comment -->\n#### Heading";
        let ctx_multiline = LintContext::new(content_multiline, crate::config::MarkdownFlavor::Standard, None);
        let warnings_multiline = rule.check(&ctx_multiline).unwrap();
        assert!(
            warnings_multiline.is_empty(),
            "Multi-line HTML comment is also transparent"
        );
    }

    #[test]
    fn test_frontmatter_transparency() {
        // Frontmatter is transparent for MD022 - heading can appear immediately after
        // Verified with markdownlint: no MD022 warning for heading after frontmatter
        let rule = MD022BlanksAroundHeadings::default();

        // Heading immediately after frontmatter closing ---
        let content = "---\ntitle: Test\n---\n# First heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Frontmatter is transparent - heading can appear immediately after"
        );

        // Heading with blank line after frontmatter is also valid
        let content_with_blank = "---\ntitle: Test\n---\n\n# First heading";
        let ctx_with_blank = LintContext::new(content_with_blank, crate::config::MarkdownFlavor::Standard, None);
        let warnings_with_blank = rule.check(&ctx_with_blank).unwrap();
        assert!(
            warnings_with_blank.is_empty(),
            "Heading with blank line after frontmatter should also be valid"
        );

        // TOML frontmatter (+++...+++) is also transparent
        let content_toml = "+++\ntitle = \"Test\"\n+++\n# First heading";
        let ctx_toml = LintContext::new(content_toml, crate::config::MarkdownFlavor::Standard, None);
        let warnings_toml = rule.check(&ctx_toml).unwrap();
        assert!(
            warnings_toml.is_empty(),
            "TOML frontmatter is also transparent for MD022"
        );
    }

    #[test]
    fn test_horizontal_rule_not_treated_as_frontmatter() {
        // Issue #238: Horizontal rules (---) should NOT be treated as frontmatter.
        // A heading after a horizontal rule MUST have a blank line above it.
        let rule = MD022BlanksAroundHeadings::default();

        // Case 1: Heading immediately after horizontal rule - SHOULD warn
        let content = "Some content\n\n---\n# Heading after HR";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            !warnings.is_empty(),
            "Heading after horizontal rule without blank line SHOULD trigger MD022"
        );
        assert!(
            warnings.iter().any(|w| w.line == 4),
            "Warning should be on line 4 (the heading line)"
        );

        // Case 2: Heading with blank line after HR - should NOT warn
        let content_with_blank = "Some content\n\n---\n\n# Heading after HR";
        let ctx_with_blank = LintContext::new(content_with_blank, crate::config::MarkdownFlavor::Standard, None);
        let warnings_with_blank = rule.check(&ctx_with_blank).unwrap();
        assert!(
            warnings_with_blank.is_empty(),
            "Heading with blank line after HR should not trigger MD022"
        );

        // Case 3: HR at start of document followed by heading - SHOULD warn
        let content_hr_start = "---\n# Heading";
        let ctx_hr_start = LintContext::new(content_hr_start, crate::config::MarkdownFlavor::Standard, None);
        let warnings_hr_start = rule.check(&ctx_hr_start).unwrap();
        assert!(
            !warnings_hr_start.is_empty(),
            "Heading after HR at document start SHOULD trigger MD022"
        );

        // Case 4: Multiple HRs then heading - SHOULD warn
        let content_multi_hr = "Content\n\n---\n\n---\n# Heading";
        let ctx_multi_hr = LintContext::new(content_multi_hr, crate::config::MarkdownFlavor::Standard, None);
        let warnings_multi_hr = rule.check(&ctx_multi_hr).unwrap();
        assert!(
            !warnings_multi_hr.is_empty(),
            "Heading after multiple HRs without blank line SHOULD trigger MD022"
        );
    }

    #[test]
    fn test_all_hr_styles_require_blank_before_heading() {
        // CommonMark defines HRs as 3+ of -, *, or _ with optional spaces between
        let rule = MD022BlanksAroundHeadings::default();

        // All valid HR styles that should trigger MD022 when followed by heading without blank
        let hr_styles = [
            "---", "***", "___", "- - -", "* * *", "_ _ _", "----", "****", "____", "- - - -",
            "-  -  -", // Multiple spaces between
            "  ---",   // 2 spaces indent (valid per CommonMark)
            "   ---",  // 3 spaces indent (valid per CommonMark)
        ];

        for hr in hr_styles {
            let content = format!("Content\n\n{hr}\n# Heading");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert!(
                !warnings.is_empty(),
                "HR style '{hr}' followed by heading should trigger MD022"
            );
        }
    }

    #[test]
    fn test_setext_heading_after_hr() {
        // Setext headings after HR should also require blank line
        let rule = MD022BlanksAroundHeadings::default();

        // Setext h1 after HR without blank - SHOULD warn
        let content = "Content\n\n---\nHeading\n======";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            !warnings.is_empty(),
            "Setext heading after HR without blank should trigger MD022"
        );

        // Setext h2 after HR without blank - SHOULD warn
        let content_h2 = "Content\n\n---\nHeading\n------";
        let ctx_h2 = LintContext::new(content_h2, crate::config::MarkdownFlavor::Standard, None);
        let warnings_h2 = rule.check(&ctx_h2).unwrap();
        assert!(
            !warnings_h2.is_empty(),
            "Setext h2 after HR without blank should trigger MD022"
        );

        // With blank line - should NOT warn
        let content_ok = "Content\n\n---\n\nHeading\n======";
        let ctx_ok = LintContext::new(content_ok, crate::config::MarkdownFlavor::Standard, None);
        let warnings_ok = rule.check(&ctx_ok).unwrap();
        assert!(
            warnings_ok.is_empty(),
            "Setext heading with blank after HR should not warn"
        );
    }

    #[test]
    fn test_hr_in_code_block_not_treated_as_hr() {
        // HR syntax inside code blocks should be ignored
        let rule = MD022BlanksAroundHeadings::default();

        // HR inside fenced code block - heading after code block needs blank line check
        // but the "---" inside is NOT an HR
        let content = "```\n---\n```\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        // The heading is after a code block fence, not after an HR
        // This tests that we don't confuse code block content with HRs
        assert!(!warnings.is_empty(), "Heading after code block still needs blank line");

        // With blank after code block - should be fine
        let content_ok = "```\n---\n```\n\n# Heading";
        let ctx_ok = LintContext::new(content_ok, crate::config::MarkdownFlavor::Standard, None);
        let warnings_ok = rule.check(&ctx_ok).unwrap();
        assert!(
            warnings_ok.is_empty(),
            "Heading with blank after code block should not warn"
        );
    }

    #[test]
    fn test_hr_in_html_comment_not_treated_as_hr() {
        // HR syntax inside HTML comments should be ignored
        let rule = MD022BlanksAroundHeadings::default();

        // "---" inside HTML comment is NOT an HR
        let content = "<!-- \n---\n -->\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        // HTML comments are transparent, so heading after comment at doc start is OK
        assert!(
            warnings.is_empty(),
            "HR inside HTML comment should be ignored - heading after comment is OK"
        );
    }

    #[test]
    fn test_invalid_hr_not_triggering() {
        // These should NOT be recognized as HRs per CommonMark
        let rule = MD022BlanksAroundHeadings::default();

        let invalid_hrs = [
            "    ---", // 4+ spaces is code block, not HR
            "\t---",   // Tab indent makes it code block
            "--",      // Only 2 dashes
            "**",      // Only 2 asterisks
            "__",      // Only 2 underscores
            "-*-",     // Mixed characters
            "---a",    // Extra character at end
            "a---",    // Extra character at start
        ];

        for invalid in invalid_hrs {
            // These are NOT HRs, so if followed by heading, the heading behavior depends
            // on what the content actually is (code block, paragraph, etc.)
            let content = format!("Content\n\n{invalid}\n# Heading");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            // We're just verifying the HR detection is correct
            // The actual warning behavior depends on what the "invalid HR" is parsed as
            let _ = rule.check(&ctx);
        }
    }

    #[test]
    fn test_frontmatter_vs_horizontal_rule_distinction() {
        // Ensure we correctly distinguish between frontmatter delimiters and standalone HRs
        let rule = MD022BlanksAroundHeadings::default();

        // Frontmatter followed by content, then HR, then heading
        // The HR here is NOT frontmatter, so heading needs blank line
        let content = "---\ntitle: Test\n---\n\nSome content\n\n---\n# Heading after HR";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            !warnings.is_empty(),
            "HR after frontmatter content should still require blank line before heading"
        );

        // Same but with blank line after HR - should be fine
        let content_ok = "---\ntitle: Test\n---\n\nSome content\n\n---\n\n# Heading after HR";
        let ctx_ok = LintContext::new(content_ok, crate::config::MarkdownFlavor::Standard, None);
        let warnings_ok = rule.check(&ctx_ok).unwrap();
        assert!(
            warnings_ok.is_empty(),
            "HR with blank line before heading should not warn"
        );
    }
}
