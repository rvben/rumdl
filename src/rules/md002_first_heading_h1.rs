use crate::rule::Rule;
use crate::rule::{Fix, LintError, LintResult, LintWarning, RuleCategory, Severity};
use crate::rules::heading_utils::HeadingStyle;
use crate::utils::range_utils::calculate_heading_range;
use crate::rule_config_serde::RuleConfig;
use toml;

mod md002_config;
use md002_config::MD002Config;

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
    config: MD002Config,
}

impl Default for MD002FirstHeadingH1 {
    fn default() -> Self {
        Self {
            config: MD002Config::default(),
        }
    }
}

impl MD002FirstHeadingH1 {
    pub fn new(level: u32) -> Self {
        Self {
            config: MD002Config { level },
        }
    }
    
    pub fn from_config_struct(config: MD002Config) -> Self {
        Self { config }
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

        // Find the first heading using pre-computed line info
        let first_heading = ctx.lines.iter().enumerate()
            .find_map(|(line_num, line_info)| {
                line_info.heading.as_ref().map(|h| (line_num, line_info, h))
            });

        if let Some((line_num, line_info, heading)) = first_heading {
            if heading.level != self.config.level as u8 {
                let message = format!(
                    "First heading should be level {}, found level {}",
                    self.config.level, heading.level
                );

                // Calculate the fix
                let fix = {
                    let replacement = crate::rules::heading_utils::HeadingUtils::convert_heading_style(
                        &heading.text, 
                        self.config.level, 
                        match heading.style {
                            crate::lint_context::HeadingStyle::ATX => {
                                if heading.has_closing_sequence {
                                    HeadingStyle::AtxClosed
                                } else {
                                    HeadingStyle::Atx
                                }
                            },
                            crate::lint_context::HeadingStyle::Setext1 => HeadingStyle::Setext1,
                            crate::lint_context::HeadingStyle::Setext2 => HeadingStyle::Setext2,
                        }
                    );

                    // Use line content range to replace the entire heading line
                    let line_index = crate::utils::range_utils::LineIndex::new(content.to_string());
                    Some(Fix {
                        range: line_index.line_content_range(line_num + 1), // Convert to 1-indexed
                        replacement,
                    })
                };

                // Calculate precise range: highlight the entire first heading
                let (start_line, start_col, end_line, end_col) =
                    calculate_heading_range(line_num + 1, &line_info.content);

                return Ok(vec![LintWarning {
                    message,
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix,
                    rule_name: Some(self.name()),
                }]);
            }
        }

        Ok(vec![])
    }


    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        
        // Find the first heading using pre-computed line info
        let first_heading = ctx.lines.iter().enumerate()
            .find_map(|(line_num, line_info)| {
                line_info.heading.as_ref().map(|h| (line_num, line_info, h))
            });

        if let Some((line_num, line_info, heading)) = first_heading {
            if heading.level == self.config.level as u8 {
                return Ok(content.to_string());
            }

            let lines: Vec<&str> = content.lines().collect();
            let mut fixed_lines = Vec::new();
            let mut i = 0;

            while i < lines.len() {
                if i == line_num {
                    // This is the first heading line that needs fixing
                    let indent = " ".repeat(line_info.indent);
                    let heading_text = heading.text.trim();
                    
                    match heading.style {
                        crate::lint_context::HeadingStyle::ATX => {
                            let hashes = "#".repeat(self.config.level as usize);
                            if heading.has_closing_sequence {
                                // Preserve closed ATX: # Heading #
                                fixed_lines.push(format!("{}{} {} {}", indent, hashes, heading_text, hashes));
                            } else {
                                // Standard ATX: # Heading
                                fixed_lines.push(format!("{}{} {}", indent, hashes, heading_text));
                            }
                            i += 1;
                        }
                        crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2 => {
                            // For Setext, we need to update the underline
                            fixed_lines.push(lines[i].to_string()); // Keep heading text as-is
                            i += 1;
                            if i < lines.len() {
                                // Replace the underline
                                let underline = if self.config.level == 1 { "=======" } else { "-------" };
                                fixed_lines.push(underline.to_string());
                                i += 1;
                            }
                        }
                    }
                    continue;
                }
                
                fixed_lines.push(lines[i].to_string());
                i += 1;
            }
            
            Ok(fixed_lines.join("\n"))
        } else {
            // No headings found
            Ok(content.to_string())
        }
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
        None
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD002Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;
        
        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD002Config::RULE_NAME.to_string(), toml::Value::Table(table)))
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD002Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}


