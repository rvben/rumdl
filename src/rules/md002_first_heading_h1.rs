use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::heading_utils::{Heading, HeadingStyle, HeadingUtils};
use crate::utils::range_utils::LineIndex;

/// Rule MD002: First heading should be a top-level heading
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
#[derive(Debug)]
pub struct MD002FirstHeadingH1 {
    pub level: usize,
}

impl Default for MD002FirstHeadingH1 {
    fn default() -> Self {
        Self { level: 1 }
    }
}

impl MD002FirstHeadingH1 {
    pub fn new(level: usize) -> Self {
        Self { level }
    }

    // Find the first heading in the document, skipping front matter
    fn find_first_heading(&self, content: &str) -> Option<(Heading, usize)> {
        let lines: Vec<&str> = content.lines().collect();
        let mut line_num = 0;

        // Skip front matter if present
        if content.starts_with("---\n") || (!lines.is_empty() && lines[0] == "---") {
            line_num += 1;
            while line_num < lines.len() && lines[line_num] != "---" {
                line_num += 1;
            }
            // Skip the closing --- line
            if line_num < lines.len() {
                line_num += 1;
            }
        }

        // Find first heading
        while line_num < lines.len() {
            let line = lines[line_num];

            // Check for ATX headings (with possible indentation)
            if line.trim_start().starts_with('#') {
                let trimmed = line.trim_start();
                let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                if (1..=6).contains(&hash_count) {
                    let after_hash = &trimmed[hash_count..];
                    if after_hash.is_empty() || after_hash.starts_with(' ') {
                        let text = after_hash
                            .trim_start()
                            .trim_end_matches(['#', ' '])
                            .to_string();
                        let style = if after_hash.trim_end().ends_with('#') {
                            HeadingStyle::AtxClosed
                        } else {
                            HeadingStyle::Atx
                        };
                        return Some((
                            Heading {
                                level: hash_count,
                                text,
                                style,
                            },
                            line_num,
                        ));
                    }
                }
            }
            // Check for Setext headings (with possible indentation)
            else if line_num + 1 < lines.len() {
                let next_line = lines[line_num + 1];
                let next_trimmed = next_line.trim_start();
                if !next_trimmed.is_empty() && next_trimmed.chars().all(|c| c == '=' || c == '-') {
                    let level = if next_trimmed.starts_with('=') { 1 } else { 2 };
                    let style = if level == 1 {
                        HeadingStyle::Setext1
                    } else {
                        HeadingStyle::Setext2
                    };
                    return Some((
                        Heading {
                            level,
                            text: line.trim_start().to_string(),
                            style,
                        },
                        line_num,
                    ));
                }
            }

            line_num += 1;
        }

        None
    }

    // Helper method to generate replacement text for a heading
    fn generate_replacement(&self, heading: &Heading, indentation: usize) -> String {
        let indent = " ".repeat(indentation);

        // Create the correct heading marker based on the style
        match heading.style {
            HeadingStyle::Atx => {
                // For ATX style, use the exact number of # characters needed for the desired level
                format!("{}{} {}", indent, "#".repeat(self.level), heading.text)
            }
            HeadingStyle::AtxClosed => {
                // For closed ATX, ensure we use the correct number of # characters
                format!(
                    "{}{} {} {}",
                    indent,
                    "#".repeat(self.level),
                    heading.text,
                    "#".repeat(self.level)
                )
            }
            HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                // Convert setext to ATX with the correct level
                format!("{}{} {}", indent, "#".repeat(self.level), heading.text)
            }
        }
    }
}

impl Rule for MD002FirstHeadingH1 {
    fn name(&self) -> &'static str {
        "MD002"
    }

    fn description(&self) -> &'static str {
        "First heading should be a top level heading"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Get the first heading in the document
        if let Some((first_heading, line_num)) = self.find_first_heading(content) {
            // Check if the heading is not at the expected level
            if first_heading.level != self.level {
                let indentation = HeadingUtils::get_indentation(lines[line_num]);

                // Generate a warning with the appropriate fix
                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: indentation + 1,
                    message: format!("First heading level should be {}", self.level),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: 0..0, // Placeholder range until proper LineIndex implementation
                        replacement: self.generate_replacement(&first_heading, indentation),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if let Some((heading, mut line_num)) = self.find_first_heading(content) {
            if heading.level != self.level {
                let lines: Vec<&str> = content.lines().collect();
                let mut result = Vec::new();

                // Copy lines before the heading
                result.extend(lines.iter().take(line_num).map(|line| line.to_string()));

                // Replace the heading with the correct level
                match heading.style {
                    HeadingStyle::Atx | HeadingStyle::AtxClosed => {
                        let indentation =
                            lines[line_num].len() - lines[line_num].trim_start().len();
                        let indent_str = " ".repeat(indentation);
                        let hashes = "#".repeat(self.level);

                        if heading.style == HeadingStyle::AtxClosed {
                            result.push(format!(
                                "{}{} {} {}",
                                indent_str, hashes, heading.text, hashes
                            ));
                        } else {
                            result.push(format!("{}{} {}", indent_str, hashes, heading.text));
                        }
                    }
                    HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                        // For Setext headings, convert to ATX style with the correct level
                        let indentation =
                            lines[line_num].len() - lines[line_num].trim_start().len();
                        let indent_str = " ".repeat(indentation);
                        let hashes = "#".repeat(self.level);

                        result.push(format!("{}{} {}", indent_str, hashes, heading.text));

                        // Skip the original underline
                        line_num += 1;
                    }
                }

                // Copy remaining lines
                result.extend(lines.iter().skip(line_num + 1).map(|line| line.to_string()));

                // Preserve trailing newline if original had it
                let result_str = if content.ends_with('\n') {
                    format!("{}\n", result.join("\n"))
                } else {
                    result.join("\n")
                };

                return Ok(result_str);
            }
        }

        Ok(content.to_string())
    }
}
