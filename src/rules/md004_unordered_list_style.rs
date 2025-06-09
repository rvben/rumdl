/// Rule MD004: Use consistent style for unordered list markers
///
/// See [docs/md004.md](../../docs/md004.md) for full documentation, configuration, and examples.
///
/// Enforces that all unordered list items in a Markdown document use the same marker style ("*", "+", or "-") or are consistent with the first marker used, depending on configuration.
///
/// ## Purpose
///
/// Ensures visual and stylistic consistency for unordered lists, making documents easier to read and maintain.
///
/// ## Configuration Options
///
/// The rule supports configuring the required marker style:
/// ```yaml
/// MD004:
///   style: dash      # Options: "dash", "asterisk", "plus", or "consistent" (default)
/// ```
///
/// ## Examples
///
/// ### Correct (with style: dash)
/// ```markdown
/// - Item 1
/// - Item 2
///   - Nested item
/// - Item 3
/// ```
///
/// ### Incorrect (with style: dash)
/// ```markdown
/// * Item 1
/// - Item 2
/// + Item 3
/// ```
///
/// ## Behavior
///
/// - Checks each unordered list item for its marker character.
/// - In "consistent" mode, the first marker sets the style for the document.
/// - Skips code blocks and front matter.
/// - Reports a warning if a list item uses a different marker than the configured or detected style.
///
/// ## Fix Behavior
///
/// - Rewrites all unordered list markers to match the configured or detected style.
/// - Preserves indentation and content after the marker.
///
/// ## Rationale
///
/// Consistent list markers improve readability and reduce distraction, especially in large documents or when collaborating with others. This rule helps enforce a uniform style across all unordered lists.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::DocumentStructureExtensions;
use crate::LintContext;
use toml;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnorderedListStyle {
    Asterisk,   // "*"
    Plus,       // "+"
    Dash,       // "-"
    Consistent, // Use the first marker in a file consistently
}

impl Default for UnorderedListStyle {
    fn default() -> Self {
        Self::Consistent
    }
}


/// Rule MD004: Unordered list style
#[derive(Clone)]
pub struct MD004UnorderedListStyle {
    pub style: UnorderedListStyle,
    pub after_marker: usize,
}

impl MD004UnorderedListStyle {
    pub fn new(style: UnorderedListStyle) -> Self {
        Self {
            style,
            after_marker: 1,
        }
    }
}

impl Rule for MD004UnorderedListStyle {
    fn name(&self) -> &'static str {
        "MD004"
    }

    fn description(&self) -> &'static str {
        "Use consistent style for unordered list markers"
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        // Early returns for performance
        if ctx.content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any list markers before processing
        if !ctx.content.contains(['*', '-', '+']) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let mut first_marker: Option<char> = None;

        // Use centralized list blocks for better performance and accuracy
        for list_block in &ctx.list_blocks {
            // Skip ordered lists
            if list_block.is_ordered {
                continue;
            }

            // Check each list item in this block
            for &item_line in &list_block.item_lines {
                if let Some(line_info) = ctx.line_info(item_line) {
                    if let Some(list_item) = &line_info.list_item {
                        // Skip ordered lists (safety check)
                        if list_item.is_ordered {
                            continue;
                        }

                        // Get the marker character
                        let marker = list_item.marker.chars().next().unwrap();
                        
                        // Calculate offset for the marker position
                        let offset = line_info.byte_offset + list_item.marker_column;

                        match self.style {
                            UnorderedListStyle::Consistent => {
                                // For consistent mode, we check consistency across the entire document
                                if let Some(first) = first_marker {
                                    // Check if current marker matches the first marker found
                                    if marker != first {
                                        let (line, col) = ctx.offset_to_line_col(offset);
                                        warnings.push(LintWarning {
                                            line,
                                            column: col,
                                            end_line: line,
                                            end_column: col + 1,
                                            message: format!(
                                                "List marker '{}' does not match expected style '{}'",
                                                marker, first
                                            ),
                                            severity: Severity::Warning,
                                            rule_name: Some(self.name()),
                                            fix: Some(Fix {
                                                range: offset..offset + 1,
                                                replacement: first.to_string(),
                                            }),
                                        });
                                    }
                                } else {
                                    // This is the first marker we've found - set the style
                                    first_marker = Some(marker);
                                }
                            }
                            _ => {
                                // Handle specific style requirements (asterisk, dash, plus)
                                let target_marker = match self.style {
                                    UnorderedListStyle::Asterisk => '*',
                                    UnorderedListStyle::Dash => '-',
                                    UnorderedListStyle::Plus => '+',
                                    _ => unreachable!(),
                                };
                                if marker != target_marker {
                                    let (line, col) = ctx.offset_to_line_col(offset);
                                    warnings.push(LintWarning {
                                        line,
                                        column: col,
                                        end_line: line,
                                        end_column: col + 1,
                                        message: format!(
                                            "List marker '{}' does not match expected style '{}'",
                                            marker, target_marker
                                        ),
                                        severity: Severity::Warning,
                                        rule_name: Some(self.name()),
                                        fix: Some(Fix {
                                            range: offset..offset + 1,
                                            replacement: target_marker.to_string(),
                                        }),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let mut lines: Vec<String> = ctx.content.lines().map(String::from).collect();
        let mut first_marker: Option<char> = None;

        // Use centralized list blocks
        for list_block in &ctx.list_blocks {
            // Skip ordered lists
            if list_block.is_ordered {
                continue;
            }

            // Process each list item in this block
            for &item_line in &list_block.item_lines {
                if let Some(line_info) = ctx.line_info(item_line) {
                    if let Some(list_item) = &line_info.list_item {
                        // Skip ordered lists (safety check)
                        if list_item.is_ordered {
                            continue;
                        }

                        let line_idx = item_line - 1;
                        if line_idx >= lines.len() {
                            continue;
                        }

                        let line = &lines[line_idx];
                        let marker = list_item.marker.chars().next().unwrap();
                        
                        // Determine the target marker
                        let target_marker = match self.style {
                            UnorderedListStyle::Consistent => {
                                if let Some(first) = first_marker {
                                    first
                                } else {
                                    first_marker = Some(marker);
                                    marker
                                }
                            }
                            UnorderedListStyle::Asterisk => '*',
                            UnorderedListStyle::Dash => '-',
                            UnorderedListStyle::Plus => '+',
                        };

                        // Replace the marker if needed
                        if marker != target_marker {
                            let marker_pos = list_item.marker_column;
                            if marker_pos < line.len() {
                                let mut new_line = String::new();
                                new_line.push_str(&line[..marker_pos]);
                                new_line.push(target_marker);
                                new_line.push_str(&line[marker_pos + 1..]);
                                lines[line_idx] = new_line;
                            }
                        }
                    }
                }
            }
        }

        let mut result = lines.join("\n");
        if ctx.content.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.content.contains(['*', '-', '+'])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "style".to_string(),
            toml::Value::String(match self.style {
                UnorderedListStyle::Asterisk => "asterisk".to_string(),
                UnorderedListStyle::Dash => "dash".to_string(),
                UnorderedListStyle::Plus => "plus".to_string(),
                UnorderedListStyle::Consistent => "consistent".to_string(),
            }),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD004", "style")
            .unwrap_or_else(|| "consistent".to_string());
        let style = match style.as_str() {
            "asterisk" => UnorderedListStyle::Asterisk,
            "dash" => UnorderedListStyle::Dash,
            "plus" => UnorderedListStyle::Plus,
            _ => UnorderedListStyle::Consistent,
        };
        Box::new(MD004UnorderedListStyle::new(style))
    }
}

impl DocumentStructureExtensions for MD004UnorderedListStyle {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        // Quick check for any list markers and unordered list blocks
        ctx.content.contains(['*', '-', '+']) && 
        ctx.list_blocks.iter().any(|block| !block.is_ordered)
    }
}
