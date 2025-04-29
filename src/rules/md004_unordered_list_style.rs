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
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use toml;

lazy_static! {
    static ref UNORDERED_LIST_REGEX: FancyRegex =
        FancyRegex::new(r"^(?P<indent>[ \t]*)(?P<marker>[*+-])(?P<after>[ \t]+)(?P<content>.*)$")
            .unwrap();
    static ref CODE_BLOCK_START: Regex = Regex::new(r"^\s*(```|~~~)").unwrap();
    static ref CODE_BLOCK_END: Regex = Regex::new(r"^\s*(```|~~~)\s*$").unwrap();
    static ref FRONT_MATTER_DELIM: Regex = Regex::new(r"^---\s*$").unwrap();
}

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Default)]
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

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut in_code_block = false;
        let mut in_front_matter = false;
        let mut first_marker: Option<String> = None;
        let mut line_num = 0;
        for line in content.lines() {
            line_num += 1;
            // Front matter detection
            if line_num == 1 && FRONT_MATTER_DELIM.is_match(line) {
                in_front_matter = true;
                continue;
            }
            if in_front_matter {
                if FRONT_MATTER_DELIM.is_match(line) {
                    in_front_matter = false;
                }
                continue;
            }
            // Code block detection
            if !in_code_block && CODE_BLOCK_START.is_match(line) {
                in_code_block = true;
                continue;
            }
            if in_code_block && CODE_BLOCK_END.is_match(line) {
                in_code_block = false;
                continue;
            }
            if in_code_block {
                continue;
            }
            // List item detection
            if let Ok(Some(cap)) = UNORDERED_LIST_REGEX.captures(line) {
                let marker = cap.name("marker").unwrap().as_str();
                let indentation = cap.name("indent").map_or(0, |m| m.as_str().len());
                if first_marker.is_none() {
                    first_marker = Some(marker.to_string());
                }
                let expected_marker = match self.style {
                    UnorderedListStyle::Consistent => first_marker.as_ref().unwrap(),
                    UnorderedListStyle::Asterisk => "*",
                    UnorderedListStyle::Dash => "-",
                    UnorderedListStyle::Plus => "+",
                };
                if marker != expected_marker {
                    warnings.push(LintWarning {
                        message: format!(
                            "Unordered list marker '{}' does not match expected marker '{}' (consistent style)",
                            marker, expected_marker
                        ),
                        line: line_num,
                        column: indentation + 1,
                        severity: Severity::Warning,
                        fix: None,
                        rule_name: Some(self.name()),
                    });
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut in_code_block = false;
        let mut in_front_matter = false;
        let mut first_marker: Option<String> = None;
        let mut lines: Vec<String> = Vec::new();
        let mut line_num = 0;
        // First pass: determine the target marker
        for line in content.lines() {
            line_num += 1;
            if line_num == 1 && FRONT_MATTER_DELIM.is_match(line) {
                in_front_matter = true;
                continue;
            }
            if in_front_matter {
                if FRONT_MATTER_DELIM.is_match(line) {
                    in_front_matter = false;
                }
                continue;
            }
            if !in_code_block && CODE_BLOCK_START.is_match(line) {
                in_code_block = true;
                continue;
            }
            if in_code_block && CODE_BLOCK_END.is_match(line) {
                in_code_block = false;
                continue;
            }
            if in_code_block {
                continue;
            }
            if let Ok(Some(cap)) = UNORDERED_LIST_REGEX.captures(line) {
                let marker = cap.name("marker").unwrap().as_str();
                if first_marker.is_none() {
                    first_marker = Some(marker.to_string());
                }
            }
        }
        let target_marker = match self.style {
            UnorderedListStyle::Consistent => first_marker.as_deref().unwrap_or("*"),
            UnorderedListStyle::Asterisk => "*",
            UnorderedListStyle::Dash => "-",
            UnorderedListStyle::Plus => "+",
        };
        // Second pass: rewrite lines
        in_code_block = false;
        in_front_matter = false;
        line_num = 0;
        for line in content.lines() {
            line_num += 1;
            if line_num == 1 && FRONT_MATTER_DELIM.is_match(line) {
                in_front_matter = true;
                lines.push(line.to_string());
                continue;
            }
            if in_front_matter {
                lines.push(line.to_string());
                if FRONT_MATTER_DELIM.is_match(line) {
                    in_front_matter = false;
                }
                continue;
            }
            if !in_code_block && CODE_BLOCK_START.is_match(line) {
                in_code_block = true;
                lines.push(line.to_string());
                continue;
            }
            if in_code_block && CODE_BLOCK_END.is_match(line) {
                in_code_block = false;
                lines.push(line.to_string());
                continue;
            }
            if in_code_block {
                lines.push(line.to_string());
                continue;
            }
            if let Ok(Some(cap)) = UNORDERED_LIST_REGEX.captures(line) {
                let indent = cap.name("indent").map_or("", |m| m.as_str());
                let after = cap.name("after").map_or(" ", |m| m.as_str());
                let content = cap.name("content").map_or("", |m| m.as_str());
                let marker = cap.name("marker").unwrap().as_str();
                let new_marker = if marker != target_marker {
                    target_marker
                } else {
                    marker
                };
                let new_line = format!("{}{}{}{}", indent, new_marker, after, content);
                lines.push(new_line);
            } else {
                lines.push(line.to_string());
            }
        }
        // Always ensure a single trailing newline, regardless of input
        let mut result = lines.join("\n");
        if !result.ends_with('\n') {
            result.push('\n');
        }
        // Remove any extra trailing newlines (keep only one)
        while result.ends_with("\n\n") {
            result.pop();
        }
        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty()
            || (!content.contains('*') && !content.contains('-') && !content.contains('+'))
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
                UnorderedListStyle::Plus => "plus".to_string(),
                UnorderedListStyle::Dash => "dash".to_string(),
                UnorderedListStyle::Consistent => "consistent".to_string(),
            }),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }
}

impl DocumentStructureExtensions for MD004UnorderedListStyle {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // Rule is only relevant if there are list items
        !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        // Test with consistent style
        let rule = MD004UnorderedListStyle::default();

        // Test with consistent markers
        let content = "* Item 1\n* Item 2\n* Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());

        // Test with inconsistent markers
        let content = "* Item 1\n- Item 2\n+ Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should flag the - and + markers

        // Test specific style
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Dash);
        let content = "* Item 1\n- Item 2\n+ Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should flag the * and + markers
    }
}
