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
use crate::utils::document_structure::DocumentStructure;
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

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
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

                // Reverted: Simple first marker detection
                if first_marker.is_none() {
                    first_marker = Some(marker.to_string());
                }

                // Reverted: Determine expected marker based only on style and the single first_marker
                let expected_marker = match self.style {
                    UnorderedListStyle::Consistent => {
                        // Use unwrap() safely as first_marker is guaranteed to be Some here if style is Consistent and it's not the first item
                        // If it IS the first item, first_marker was just set.
                        first_marker.as_ref().map_or(marker, |fm| fm.as_str())
                    }
                    UnorderedListStyle::Asterisk => "*",
                    UnorderedListStyle::Dash => "-",
                    UnorderedListStyle::Plus => "+",
                };

                // Reverted: Simple comparison, ignoring indentation level for Consistent style
                if marker != expected_marker {
                    warnings.push(LintWarning {
                        message: format!(
                            "Unordered list marker '{}' does not match expected style '{}'",
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

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
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
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "style".to_string(),
            toml::Value::String(
                match self.style {
                    UnorderedListStyle::Asterisk => "asterisk",
                    UnorderedListStyle::Plus => "plus",
                    UnorderedListStyle::Dash => "dash",
                    UnorderedListStyle::Consistent => "consistent",
                }
                .to_string(),
            ),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD004", "style")
            .map(|s| match s.as_str() {
                "asterisk" => UnorderedListStyle::Asterisk,
                "plus" => UnorderedListStyle::Plus,
                "dash" => UnorderedListStyle::Dash,
                _ => UnorderedListStyle::Consistent,
            })
            .unwrap_or(UnorderedListStyle::Consistent);
        Box::new(MD004UnorderedListStyle::new(style))
    }
}

impl crate::utils::document_structure::DocumentStructureExtensions for MD004UnorderedListStyle {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        let content = ctx.content;
        !content.is_empty() && !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_with_document_structure() {
        let content = "* Item 1\n- Item 2\n+ Item 3";
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let warnings = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Expected 2 warnings for inconsistent markers"
        );
    }
}
