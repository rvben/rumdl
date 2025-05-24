use crate::rule::Rule;
use crate::rule::{Fix, LintError, LintResult, LintWarning, RuleCategory, Severity};
use crate::rules::heading_utils::HeadingStyle;
use crate::utils::document_structure::DocumentStructure;
use lazy_static::lazy_static;
use regex::Regex;
use toml;

lazy_static! {
    static ref HEADING_PATTERN: Regex = Regex::new(r"^(\s*)(#{1,6})\s+(.+?)(?:\s+#*)?$").unwrap();
    static ref SETEXT_HEADING_1: Regex = Regex::new(r"^(\s*)=+\s*$").unwrap();
    static ref SETEXT_HEADING_2: Regex = Regex::new(r"^(\s*)-+\s*$").unwrap();
    static ref FRONT_MATTER: Regex = Regex::new(r"(?m)^---\s*$").unwrap();
}

/// Rule MD002: First heading should be a top-level heading
///
/// See [docs/md002.md](../../docs/md002.md) for full documentation, configuration, and examples.
///
/// This rule enforces that the first heading in a document is a top-level heading (typically h1),
/// which establishes the main topic or title of the document.
///
/// ## Purpose
///
/// - **Document Structure**: Ensures proper document hierarchy with a single top-level heading
/// - **Accessibility**: Improves screen reader navigation by providing a clear document title
/// - **SEO**: Helps search engines identify the primary topic of the document
/// - **Readability**: Provides users with a clear understanding of the document's main subject
///
/// ## Configuration Options
///
/// The rule supports customizing the required level for the first heading:
///
/// ```yaml
/// MD002:
///   level: 1  # The heading level required for the first heading (default: 1)
/// ```
///
/// Setting `level: 2` would require the first heading to be an h2 instead of h1.
///
/// ## Examples
///
/// ### Correct (with default configuration)
///
/// ```markdown
/// # Document Title
///
/// ## Section 1
///
/// Content here...
///
/// ## Section 2
///
/// More content...
/// ```
///
/// ### Incorrect (with default configuration)
///
/// ```markdown
/// ## Introduction
///
/// Content here...
///
/// # Main Title
///
/// More content...
/// ```
///
/// ## Behavior
///
/// This rule:
/// - Ignores front matter (YAML metadata at the beginning of the document)
/// - Works with both ATX (`#`) and Setext (underlined) heading styles
/// - Only examines the first heading it encounters
/// - Does not apply to documents with no headings
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Changes the level of the first heading to match the configured level
/// - Preserves the original heading style (ATX, closed ATX, or Setext)
/// - Maintains indentation and other formatting
///
/// ## Rationale
///
/// Having a single top-level heading establishes the document's primary topic and creates
/// a logical structure. This follows semantic HTML principles where each page should have
/// a single `<h1>` element that defines its main subject.
///
#[derive(Debug, Clone)]
pub struct MD002FirstHeadingH1 {
    level: u32,
}

impl Default for MD002FirstHeadingH1 {
    fn default() -> Self {
        Self { level: 1 }
    }
}

impl MD002FirstHeadingH1 {
    pub fn new(level: u32) -> Self {
        Self { level }
    }

    fn parse_heading(
        &self,
        content: &str,
        line_number: usize,
    ) -> Option<(String, String, u32, HeadingStyle)> {
        let lines: Vec<&str> = content.lines().collect();
        if line_number == 0 || line_number > lines.len() {
            return None;
        }

        let line = lines[line_number - 1];

        // Skip if line is within a code block
        if self.is_in_code_block(content, line_number) {
            return None;
        }

        // Check for ATX style headings
        if let Some(captures) = HEADING_PATTERN.captures(line) {
            let indent = captures.get(1).map_or("", |m| m.as_str());
            let level = captures.get(2).map_or(0, |m| m.as_str().len()) as u32;
            let text = captures.get(3).map_or("", |m| m.as_str());
            let style = if line.trim_end().ends_with('#') {
                HeadingStyle::AtxClosed
            } else {
                HeadingStyle::Atx
            };
            return Some((indent.to_string(), text.to_string(), level, style));
        }

        // Check for Setext style headings
        if line_number < lines.len() {
            let next_line = lines[line_number];
            if !next_line.trim().is_empty() {
                if let Some(captures) = SETEXT_HEADING_1.captures(next_line) {
                    let indent = captures.get(1).map_or("", |m| m.as_str());
                    return Some((
                        indent.to_string(),
                        line.trim().to_string(),
                        1,
                        HeadingStyle::Setext1,
                    ));
                } else if let Some(captures) = SETEXT_HEADING_2.captures(next_line) {
                    let indent = captures.get(1).map_or("", |m| m.as_str());
                    return Some((
                        indent.to_string(),
                        line.trim().to_string(),
                        2,
                        HeadingStyle::Setext2,
                    ));
                }
            }
        }

        None
    }

    fn is_in_code_block(&self, content: &str, line_number: usize) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        let mut fence_char = None;

        for (i, line) in lines.iter().enumerate() {
            if i >= line_number {
                break;
            }

            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    fence_char = Some(&trimmed[..3]);
                    in_code_block = true;
                } else if Some(&trimmed[..3]) == fence_char {
                    in_code_block = false;
                }
            }
        }

        in_code_block
    }
}

impl Rule for MD002FirstHeadingH1 {
    fn name(&self) -> &'static str {
        "MD002"
    }

    fn description(&self) -> &'static str {
        "First heading should be top level"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        // Early return for empty content
        if content.is_empty() {
            return Ok(vec![]);
        }

        let structure = DocumentStructure::new(content);
        if structure.heading_lines.is_empty() {
            return Ok(vec![]);
        }
        self.check_with_structure(ctx, &structure)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        let mut result = Vec::new();
        if structure.heading_lines.is_empty() {
            return Ok(result);
        }
        let first_heading_line = structure.heading_lines[0];
        let first_heading_level = structure.heading_levels[0];
        if first_heading_level as u32 != self.level {
            let message = format!(
                "First heading should be level {}, found level {}",
                self.level, first_heading_level
            );
            let fix = self.parse_heading(content, first_heading_line).map(
                |(_indent, text, _level, style)| {
                    let replacement =
                        crate::rules::heading_utils::HeadingUtils::convert_heading_style(
                            &text, self.level, style,
                        );
                    Fix {
                        range: first_heading_line..first_heading_line,
                        replacement,
                    }
                },
            );
            result.push(LintWarning {
                message,
                line: first_heading_line,
                column: 1,
                severity: Severity::Warning,
                fix,
                rule_name: Some(self.name()),
            });
        }
        Ok(result)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let structure = DocumentStructure::new(content);
        if structure.heading_lines.is_empty() {
            return Ok(content.to_string());
        }
        let first_heading_line = structure.heading_lines[0];
        let first_heading_level = structure.heading_levels[0];
        if first_heading_level == self.level as usize {
            return Ok(content.to_string());
        }
        let lines: Vec<&str> = content.lines().collect();
        let mut fixed_lines = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            if i + 1 < lines.len() {
                // Detect setext h2 heading (Heading + -------)
                let is_setext_h2 = {
                    let line = lines[i];
                    let next = lines[i + 1];
                    !line.trim().is_empty()
                        && next.trim().chars().all(|c| c == '-')
                        && next.trim().len() >= 3
                };
                if is_setext_h2 && i == first_heading_line - 1 {
                    // Replace with setext h1 and skip underline, preserve heading text and blank lines
                    let heading_text = lines[i].trim_end();
                    fixed_lines.push(heading_text.to_string());
                    fixed_lines.push("=======".to_string());
                    i += 2;
                    // Preserve any blank lines after the heading underline
                    while i < lines.len() && lines[i].trim().is_empty() {
                        fixed_lines.push(lines[i].to_string());
                        i += 1;
                    }
                    continue;
                }
            }
            if i == first_heading_line - 1 {
                // ATX or closed ATX heading: preserve indentation and closing hashes if present
                let (indent, text, _level, style) = self
                    .parse_heading(content, i + 1)
                    .unwrap_or_else(|| ("".to_string(), "".to_string(), 1, HeadingStyle::Atx));
                let heading_text = text.trim();
                match style {
                    HeadingStyle::AtxClosed => {
                        // Preserve closed ATX: # Heading #
                        fixed_lines.push(format!("{}# {} #", indent, heading_text));
                    }
                    HeadingStyle::Atx => {
                        // Standard ATX: # Heading
                        fixed_lines.push(format!("{}# {}", indent, heading_text));
                    }
                    _ => {
                        // Fallback: ATX
                        fixed_lines.push(format!("{}# {}", indent, heading_text));
                    }
                }
                i += 1;
                continue;
            }
            fixed_lines.push(lines[i].to_string());
            i += 1;
        }
        Ok(fixed_lines.join("\n"))
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty()
            || (!content.contains('#') && !content.contains('=') && !content.contains('-'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert("level".to_string(), toml::Value::Integer(self.level as i64));
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let level =
            crate::config::get_rule_config_value::<u32>(config, "MD002", "level").unwrap_or(1);
        Box::new(MD002FirstHeadingH1::new(level))
    }
}

impl crate::utils::document_structure::DocumentStructureExtensions for MD002FirstHeadingH1 {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        let content = ctx.content;
        !content.is_empty() && !doc_structure.heading_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_with_document_structure() {
        let rule = MD002FirstHeadingH1::default();

        // Test with correct heading level
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(result.is_empty());

        // Test with incorrect heading level
        let content = "## Heading 2\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
    }
}
