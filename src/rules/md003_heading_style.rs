//!
//! Rule MD003: Heading style
//!
//! See [docs/md003.md](../../docs/md003.md) for full documentation, configuration, and examples.

use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::heading_utils::HeadingStyle;
use crate::utils::document_structure::DocumentStructure;
use crate::utils::range_utils::calculate_heading_range;
use lazy_static::lazy_static;
use regex::Regex;
use std::str::FromStr;
use toml;

lazy_static! {
    static ref FRONT_MATTER_DELIMITER: Regex = Regex::new(r"^---\s*$").unwrap();
    static ref QUICK_HEADING_CHECK: Regex =
        Regex::new(r"(?m)^(\s*)#|^(\s*)[^\s].*\n(\s*)(=+|-+)\s*$").unwrap();
}

/// Rule MD003: Heading style
#[derive(Clone)]
pub struct MD003HeadingStyle {
    style: HeadingStyle,
}

impl Default for MD003HeadingStyle {
    fn default() -> Self {
        Self {
            style: HeadingStyle::Consistent,
        }
    }
}

impl MD003HeadingStyle {
    pub fn new(style: HeadingStyle) -> Self {
        Self { style }
    }

    /// Detects the first heading style in the document for "consistent" mode
    /// Note: This is only used as a fallback if DocumentStructure is not available
    fn detect_first_heading_style(&self, content: &str) -> Option<HeadingStyle> {
        lazy_static! {
            static ref ATX_PATTERN: Regex =
                Regex::new(r"^(#{1,6})(\s+)([^#\n]+?)(?:\s+(#{1,6}))?\s*$").unwrap();
        }

        let lines: Vec<&str> = content.lines().collect();

        // Check for front matter first
        let mut in_front_matter = false;
        let mut line_idx = 0;

        // Skip front matter if present
        if !lines.is_empty() && lines[0].trim() == "---" {
            in_front_matter = true;
            for (i, line) in lines.iter().enumerate().skip(1) {
                if line.trim() == "---" {
                    in_front_matter = false;
                    line_idx = i + 1; // Start looking for headings after front matter
                    break;
                }
            }
        }

        // Look for the first heading
        for i in line_idx..lines.len() {
            let line = lines[i];

            // Skip if still in front matter
            if in_front_matter {
                continue;
            }

            // Check for ATX headings
            if ATX_PATTERN.is_match(line) {
                // Check for closed ATX (with trailing hashes)
                if line.trim().ends_with('#')
                    && !line.trim().chars().filter(|&c| c == '#').count() == line.trim().len()
                {
                    return Some(HeadingStyle::AtxClosed);
                } else {
                    return Some(HeadingStyle::Atx);
                }
            }

            // Check for Setext headings
            if i < lines.len() - 1 {
                let next_line = lines[i + 1];
                if !line.trim().is_empty() {
                    // Make sure this isn't a front matter delimiter
                    if line.trim() != "---" && next_line.trim() != "---" {
                        if next_line.trim().starts_with('=') {
                            return Some(HeadingStyle::Setext1);
                        } else if next_line.trim().starts_with('-') {
                            // Make sure this is actually a setext heading and not a list item
                            // or horizontal rule. A setext heading underline should consist of only -
                            let is_all_dashes = next_line.trim().chars().all(|c| c == '-');
                            if is_all_dashes && next_line.trim().len() >= 2 {
                                return Some(HeadingStyle::Setext2);
                            }
                        }
                    }
                }
            }
        }

        // Default to ATX style if no headings are found
        Some(HeadingStyle::Atx)
    }

    /// Check if we should use consistent mode (detect first style)
    fn is_consistent_mode(&self) -> bool {
        // Check for the Consistent variant explicitly
        self.style == HeadingStyle::Consistent
    }

    /// Gets the target heading style based on configuration and document content
    fn get_target_style(
        &self,
        content: &str,
        structure: Option<&DocumentStructure>,
    ) -> HeadingStyle {
        if !self.is_consistent_mode() {
            return self.style;
        }

        // If DocumentStructure is available, use the pre-computed first_heading_style
        if let Some(doc_structure) = structure {
            // Use the pre-computed style from the structure
            doc_structure
                .first_heading_style
                .unwrap_or(HeadingStyle::Atx)
        } else {
            // Fallback to manual detection if structure isn't available
            self.detect_first_heading_style(content)
                .unwrap_or(HeadingStyle::Atx)
        }
    }
}

impl Rule for MD003HeadingStyle {
    fn name(&self) -> &'static str {
        "MD003"
    }

    fn description(&self) -> &'static str {
        "Heading style"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check if there are any headings at all
        if !QUICK_HEADING_CHECK.is_match(content) {
            return Ok(Vec::new());
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Get all warnings with their fixes
        let warnings = self.check(ctx)?;

        // If no warnings, return original content
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Collect all fixes and sort by range start (descending) to apply from end to beginning
        let mut fixes: Vec<_> = warnings
            .iter()
            .filter_map(|w| {
                w.fix
                    .as_ref()
                    .map(|f| (f.range.start, f.range.end, &f.replacement))
            })
            .collect();
        fixes.sort_by(|a, b| b.0.cmp(&a.0));

        // Apply fixes from end to beginning to preserve byte offsets
        let mut result = ctx.content.to_string();
        for (start, end, replacement) in fixes {
            if start < result.len() && end <= result.len() && start <= end {
                result.replace_range(start..end, replacement);
            }
        }

        Ok(result)
    }

    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        // Early return for empty content or no headings
        if content.is_empty() || structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();

        // Get the target style using the pre-computed value from DocumentStructure
        let target_style = self.get_target_style(content, Some(structure));

        let lines: Vec<&str> = content.lines().collect();

        // Process only heading lines using structure.heading_lines
        for (i, &line_num) in structure.heading_lines.iter().enumerate() {
            // Skip headings in front matter
            if structure.is_in_front_matter(line_num) {
                continue;
            }

            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Get the heading level from the structure
            let level = *structure.heading_levels.get(i).unwrap_or(&1);

            // Determine the current style of the heading
            let current_line = lines.get(line_idx).unwrap_or(&"");
            let next_line_idx = line_idx + 1;

            let style = if next_line_idx < lines.len() {
                let next_line = lines[next_line_idx];
                // Check if it's a setext heading
                if next_line.trim_start().starts_with('=') {
                    HeadingStyle::Setext1
                } else if next_line.trim_start().starts_with('-')
                    && !current_line.trim_start().starts_with('#')
                {
                    HeadingStyle::Setext2
                } else if current_line.trim().ends_with('#') {
                    HeadingStyle::AtxClosed
                } else {
                    HeadingStyle::Atx
                }
            } else {
                // Must be ATX style (no next line available)
                if current_line.trim().ends_with('#') {
                    HeadingStyle::AtxClosed
                } else {
                    HeadingStyle::Atx
                }
            };

            // For Setext, levels 3+ must be ATX regardless of the target style
            let expected_style = if level > 2
                && (target_style == HeadingStyle::Setext1 || target_style == HeadingStyle::Setext2)
            {
                HeadingStyle::Atx
            } else {
                // For Setext, use the appropriate style based on level
                if (target_style == HeadingStyle::Setext1 || target_style == HeadingStyle::Setext2)
                    && level <= 2
                {
                    if level == 1 {
                        HeadingStyle::Setext1
                    } else {
                        HeadingStyle::Setext2
                    }
                } else {
                    target_style
                }
            };

            if style != expected_style {
                // Generate fix for this heading
                let fix = {
                    use crate::rules::heading_utils::HeadingUtils;

                    // Get the text content from the heading
                    let text_content = if next_line_idx < lines.len()
                        && (lines[next_line_idx].trim_start().starts_with('=')
                            || lines[next_line_idx].trim_start().starts_with('-'))
                    {
                        // Setext heading
                        current_line.to_string()
                    } else {
                        // ATX heading
                        HeadingUtils::get_heading_text(current_line).unwrap_or_default()
                    };

                    // Get indentation
                    let indentation = current_line
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>();

                    // Convert heading to target style
                    let converted_heading = HeadingUtils::convert_heading_style(
                        &format!("{}{}", indentation, text_content.trim()),
                        level as u32,
                        expected_style,
                    );

                    // Calculate the correct range for the heading
                    let line_index =
                        crate::utils::range_utils::LineIndex::new(ctx.content.to_string());
                    let range = if next_line_idx < lines.len()
                        && (lines[next_line_idx].trim_start().starts_with('=')
                            || lines[next_line_idx].trim_start().starts_with('-'))
                    {
                        // Setext heading spans two lines
                        let start_byte = line_index.line_col_to_byte_range(line_num, 1).start;
                        let end_byte = if line_num + 1 < lines.len() {
                            line_index.line_col_to_byte_range(line_num + 2, 1).start - 1
                        } else {
                            ctx.content.len()
                        };
                        start_byte..end_byte
                    } else {
                        // ATX heading is single line
                        let start_byte = line_index.line_col_to_byte_range(line_num, 1).start;
                        let end_byte = if line_num < lines.len() {
                            line_index.line_col_to_byte_range(line_num + 1, 1).start - 1
                        } else {
                            ctx.content.len()
                        };
                        start_byte..end_byte
                    };

                    Some(crate::rule::Fix {
                        range,
                        replacement: converted_heading,
                    })
                };

                // Calculate precise character range for the heading marker
                let (start_line, start_col, end_line, end_col) =
                    if style == HeadingStyle::Setext1 || style == HeadingStyle::Setext2 {
                        // For Setext headings, highlight the underline
                        if next_line_idx < lines.len() {
                            calculate_heading_range(line_num + 1, lines[next_line_idx])
                        } else {
                            calculate_heading_range(line_num, current_line)
                        }
                    } else {
                        // For ATX headings, highlight the hash markers
                        calculate_heading_range(line_num, current_line)
                    };

                result.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!(
                        "Heading style should be {:?}, found {:?}",
                        expected_style, style
                    ),
                    severity: Severity::Warning,
                    fix,
                });
            }
        }

        Ok(result)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || !QUICK_HEADING_CHECK.is_match(content)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "style".to_string(),
            toml::Value::String(self.style.to_string()),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD003", "style")
            .and_then(|s| HeadingStyle::from_str(&s).ok())
            .unwrap_or(HeadingStyle::Consistent);
        Box::new(MD003HeadingStyle::new(style))
    }
}

impl crate::utils::document_structure::DocumentStructureExtensions for MD003HeadingStyle {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        // This rule is only relevant if there are headings
        !doc_structure.heading_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_atx_heading_style() {
        let rule = MD003HeadingStyle::default();
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_setext_heading_style() {
        let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
        let content = "Heading 1\n=========\n\nHeading 2\n---------";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_front_matter() {
        let rule = MD003HeadingStyle::default();
        let content = "---\ntitle: Test\n---\n\n# Heading 1\n## Heading 2";

        // Test using document structure which should properly detect front matter
        let structure = DocumentStructure::new(content);
        assert!(
            structure.has_front_matter,
            "Document structure should detect front matter"
        );
        assert_eq!(
            structure.front_matter_range,
            Some((1, 3)),
            "Front matter should span lines 1-3"
        );

        // Make test more resilient - print details if warnings are found
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            result.is_empty(),
            "No warnings expected for content with front matter, found: {:?}",
            result
        );
        // Also check the direct check method
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "No warnings expected for content with front matter, found: {:?}",
            result
        );
    }

    #[test]
    fn test_consistent_heading_style() {
        // Default rule uses Atx which serves as our "consistent" mode
        let rule = MD003HeadingStyle::default();
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_with_document_structure() {
        // Test with consistent style (ATX)
        let rule = MD003HeadingStyle::new(HeadingStyle::Consistent);
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();

        // Make test more resilient
        assert!(
            result.is_empty(),
            "No warnings expected for consistent ATX style, found: {:?}",
            result
        );

        // Test with incorrect style
        let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
        let content = "# Heading 1 #\nHeading 2\n-----\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            !result.is_empty(),
            "Should have warnings for inconsistent heading styles"
        );

        // Test with setext style
        let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
        let content = "Heading 1\n=========\nHeading 2\n---------\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        // The level 3 heading can't be setext, so it's valid as ATX
        assert!(
            result.is_empty(),
            "No warnings expected for setext style with ATX for level 3, found: {:?}",
            result
        );
    }
}
