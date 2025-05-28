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
use lazy_static::lazy_static;
use regex::Regex;
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

lazy_static! {
    static ref UNORDERED_LIST_REGEX: Regex = Regex::new(
        // Match unordered list items: optional whitespace, optional blockquote prefix, marker, space, then content or end of line
        r"^(?P<indent>\s*)(?P<blockquote>(?:>\s*)*)(?P<marker>[*+-])(?P<after>\s+)(?P<content>.*?)$"
    ).unwrap();
    static ref CODE_BLOCK_START: Regex = Regex::new(r"^\s*(```|~~~)").unwrap();
    static ref CODE_BLOCK_END: Regex = Regex::new(r"^\s*(```|~~~)\s*$").unwrap();
    static ref FRONT_MATTER_DELIM: Regex = Regex::new(r"^---\s*$").unwrap();
    static ref ORDERED_LIST_REGEX: Regex = Regex::new(r"^\s*\d+[.)]").unwrap();
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
        let content = &ctx.content;
        let mut in_code_block = false;
        let mut in_front_matter = false;
        let mut first_marker: Option<char> = None;

        // Pre-compute line positions for efficient offset calculation
        let lines: Vec<&str> = content.lines().collect();
        let mut line_positions = Vec::with_capacity(lines.len());
        let mut pos = 0;
        for line in &lines {
            line_positions.push(pos);
            pos += line.len() + 1; // +1 for newline
        }

        for (i, line) in lines.iter().enumerate() {
            // Check for code block markers
            if CODE_BLOCK_START.is_match(line) {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }

            // Check for front matter
            if FRONT_MATTER_DELIM.is_match(line) {
                in_front_matter = !in_front_matter;
                continue;
            }
            if in_front_matter {
                continue;
            }

            // Skip ordered lists
            if ORDERED_LIST_REGEX.is_match(line) {
                continue;
            }

            // Check for unordered list items
            if let Some(caps) = UNORDERED_LIST_REGEX.captures(line) {
                let indent = caps
                    .name("indent")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let blockquote = caps
                    .name("blockquote")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let marker = caps
                    .name("marker")
                    .unwrap()
                    .as_str()
                    .chars()
                    .next()
                    .unwrap();
                // Use pre-computed line position
                let offset = line_positions[i] + indent.len() + blockquote.len();

                match self.style {
                    UnorderedListStyle::Consistent => {
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
                                        "marker '{}' does not match expected style '{}'",
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
                                    "marker '{}' does not match expected style '{}'",
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

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let content = &ctx.content;
        let mut result = content.to_string();
        let mut in_code_block = false;
        let mut in_front_matter = false;
        let mut first_marker: Option<char> = None;
        let mut edits = Vec::new();

        // Pre-compute line positions for efficient offset calculation
        let lines: Vec<&str> = content.lines().collect();
        let mut line_positions = Vec::with_capacity(lines.len());
        let mut pos = 0;
        for line in &lines {
            line_positions.push(pos);
            pos += line.len() + 1; // +1 for newline
        }

        for (i, line) in lines.iter().enumerate() {
            // Check for code block markers
            if CODE_BLOCK_START.is_match(line) {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }

            // Check for front matter
            if FRONT_MATTER_DELIM.is_match(line) {
                in_front_matter = !in_front_matter;
                continue;
            }
            if in_front_matter {
                continue;
            }

            // Skip ordered lists
            if ORDERED_LIST_REGEX.is_match(line) {
                continue;
            }

            // Check for unordered list items
            if let Some(caps) = UNORDERED_LIST_REGEX.captures(line) {
                let indent = caps
                    .name("indent")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let blockquote = caps
                    .name("blockquote")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let marker = caps
                    .name("marker")
                    .unwrap()
                    .as_str()
                    .chars()
                    .next()
                    .unwrap();
                // Use pre-computed line position
                let offset = line_positions[i] + indent.len() + blockquote.len();

                match self.style {
                    UnorderedListStyle::Consistent => {
                        if let Some(first) = first_marker {
                            if marker != first {
                                edits.push((offset, first));
                            }
                        } else {
                            first_marker = Some(marker);
                        }
                    }
                    UnorderedListStyle::Asterisk => {
                        if marker != '*' {
                            edits.push((offset, '*'));
                        }
                    }
                    UnorderedListStyle::Dash => {
                        if marker != '-' {
                            edits.push((offset, '-'));
                        }
                    }
                    UnorderedListStyle::Plus => {
                        if marker != '+' {
                            edits.push((offset, '+'));
                        }
                    }
                }
            }
        }

        // Apply edits in reverse order to maintain correct offsets
        edits.sort_by(|a, b| b.0.cmp(&a.0));
        for (offset, target_marker) in edits {
            if offset < result.len() {
                result.replace_range(offset..offset + 1, &target_marker.to_string());
            }
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
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        !doc_structure.list_items.is_empty()
    }
}
