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
use crate::utils::document_structure::DocumentStructureExtensions;
use lazy_static::lazy_static;
use regex::Regex;
use toml;

lazy_static! {
    static ref UNORDERED_LIST_REGEX: Regex = Regex::new(
        // Standard regex pattern is sufficient
        r"^(?P<indent>\s*)(?P<marker>[*+-])(?P<after>\s*)(?P<content>.*)$"
    ).unwrap();
    static ref CODE_BLOCK_START: Regex = Regex::new(r"^\s*(```|~~~)").unwrap();
    static ref CODE_BLOCK_END: Regex = Regex::new(r"^\s*(```|~~~)\s*$").unwrap();
    static ref FRONT_MATTER_DELIM: Regex = Regex::new(r"^---\s*$").unwrap();
}

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

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();
        let mut first_marker: Option<char> = None;
        let style = self.style;
        let ul_re = Regex::new(r"^(?P<indent>\s*)(?P<marker>[*+-])(?P<after>\s+)").unwrap();
        // Track code block and front matter state
        let mut in_code_block = false;
        let mut in_front_matter = false;
        let mut code_fence: Option<String> = None;
        // Find the first unordered marker for Consistent mode
        if style == UnorderedListStyle::Consistent {
            for line in content.lines() {
                // Front matter start/end
                if !in_front_matter && line.trim_start().starts_with("---") {
                    in_front_matter = true;
                    continue;
                } else if in_front_matter && line.trim_start().starts_with("---") {
                    in_front_matter = false;
                    continue;
                }
                // Code block start/end
                if !in_code_block && (line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")) {
                    in_code_block = true;
                    code_fence = Some(line.trim_start()[..3].to_string());
                    continue;
                } else if in_code_block && code_fence.is_some() && line.trim_start().starts_with(code_fence.as_ref().unwrap().as_str()) {
                    in_code_block = false;
                    code_fence = None;
                    continue;
                }
                if in_code_block || in_front_matter {
                    continue;
                }
                if let Some(cap) = ul_re.captures(line) {
                    first_marker = cap.name("marker").map(|m| m.as_str().chars().next().unwrap());
                    break;
                }
            }
        }
        // Reset state for main walk
        in_code_block = false;
        in_front_matter = false;
        code_fence = None;
        for (i, line) in content.lines().enumerate() {
            // Front matter start/end
            if !in_front_matter && line.trim_start().starts_with("---") {
                in_front_matter = true;
                continue;
            } else if in_front_matter && line.trim_start().starts_with("---") {
                in_front_matter = false;
                continue;
            }
            // Code block start/end
            if !in_code_block && (line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")) {
                in_code_block = true;
                code_fence = Some(line.trim_start()[..3].to_string());
                continue;
            } else if in_code_block && code_fence.is_some() && line.trim_start().starts_with(code_fence.as_ref().unwrap().as_str()) {
                in_code_block = false;
                code_fence = None;
                continue;
            }
            if in_code_block || in_front_matter {
                continue;
            }
            if let Some(cap) = ul_re.captures(line) {
                let marker = cap.name("marker").unwrap().as_str().chars().next().unwrap();
                let expected_marker = match style {
                    UnorderedListStyle::Consistent => first_marker.unwrap_or(marker),
                    UnorderedListStyle::Asterisk => '*',
                    UnorderedListStyle::Dash => '-',
                    UnorderedListStyle::Plus => '+',
                };
                if marker != expected_marker {
                    let marker_col = line.find(cap.name("marker").unwrap().as_str()).unwrap_or(0) + 1;
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: marker_col,
                        message: format!(
                            "marker '{}' does not match expected style '{}'",
                            marker, expected_marker
                        ),
                        severity: Severity::Warning,
                        rule_name: Some(self.name()),
                        fix: None,
                    });
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if content.is_empty() {
            return Ok(String::new());
        }
        let style = self.style;
        let ul_re = Regex::new(r"^(?P<indent>\s*)(?P<marker>[*+-])(?P<after>\s+)").unwrap();
        let mut first_marker: Option<char> = None;
        let mut in_code_block = false;
        let mut in_front_matter = false;
        let mut code_fence: Option<String> = None;
        // Find the first unordered marker for Consistent mode
        if style == UnorderedListStyle::Consistent {
            for line in content.lines() {
                // Front matter start/end
                if !in_front_matter && line.trim_start().starts_with("---") {
                    in_front_matter = true;
                    continue;
                } else if in_front_matter && line.trim_start().starts_with("---") {
                    in_front_matter = false;
                    continue;
                }
                // Code block start/end
                if !in_code_block && (line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")) {
                    in_code_block = true;
                    code_fence = Some(line.trim_start()[..3].to_string());
                    continue;
                } else if in_code_block && code_fence.is_some() && line.trim_start().starts_with(code_fence.as_ref().unwrap().as_str()) {
                    in_code_block = false;
                    code_fence = None;
                    continue;
                }
                if in_code_block || in_front_matter {
                    continue;
                }
                if let Some(cap) = ul_re.captures(line) {
                    first_marker = cap.name("marker").map(|m| m.as_str().chars().next().unwrap());
                    break;
                }
            }
        }
        // Reset state for main walk
        in_code_block = false;
        in_front_matter = false;
        code_fence = None;
        let mut lines: Vec<String> = Vec::new();
        for line in content.lines() {
            // Front matter start/end
            if !in_front_matter && line.trim_start().starts_with("---") {
                in_front_matter = true;
                lines.push(line.to_string());
                continue;
            } else if in_front_matter && line.trim_start().starts_with("---") {
                in_front_matter = false;
                lines.push(line.to_string());
                continue;
            }
            // Code block start/end
            if !in_code_block && (line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")) {
                in_code_block = true;
                code_fence = Some(line.trim_start()[..3].to_string());
                lines.push(line.to_string());
                continue;
            } else if in_code_block && code_fence.is_some() && line.trim_start().starts_with(code_fence.as_ref().unwrap().as_str()) {
                in_code_block = false;
                code_fence = None;
                lines.push(line.to_string());
                continue;
            }
            if in_code_block || in_front_matter {
                lines.push(line.to_string());
                continue;
            }
            if let Some(cap) = ul_re.captures(line) {
                let marker = cap.name("marker").unwrap().as_str().chars().next().unwrap();
                let target_marker = match style {
                    UnorderedListStyle::Consistent => first_marker.unwrap_or('-'),
                    UnorderedListStyle::Asterisk => '*',
                    UnorderedListStyle::Dash => '-',
                    UnorderedListStyle::Plus => '+',
                };
                if marker != target_marker {
                    let indent = cap.name("indent").unwrap().as_str();
                    let after = cap.name("after").unwrap().as_str();
                    let rest = &line[(indent.len() + cap.name("marker").unwrap().as_str().len() + after.len())..];
                    lines.push(format!("{}{}{}{}", indent, target_marker, after, rest));
                    continue;
                }
            }
            lines.push(line.to_string());
        }
        let orig_ends_with_newline = content.ends_with('\n');
        let mut result = lines.join("\n");
        if orig_ends_with_newline {
            // Ensure exactly one trailing newline
            result = result.trim_end_matches('\n').to_string() + "\n";
        } else {
            // No trailing newline
            result = result.trim_end_matches('\n').to_string();
        }
        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.content.contains(|c: char| c == '*' || c == '-' || c == '+')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        Some((
            self.name().to_string(),
            toml::Value::try_from(std::collections::BTreeMap::from([(
                "style".to_string(),
                toml::Value::String("consistent".to_string()),
            )]))
            .unwrap(),
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let default_rule_name = Self::new(UnorderedListStyle::default()).name();
        let style_str = config
            .rules
            .get(default_rule_name)
            .and_then(|rule_cfg| rule_cfg.values.get("style")) // Access .values field of RuleConfig
            .and_then(|val| val.as_str())
            .unwrap_or("consistent");

        let style = match style_str {
            "asterisk" => UnorderedListStyle::Asterisk,
            "plus" => UnorderedListStyle::Plus,
            "dash" => UnorderedListStyle::Dash,
            _ => UnorderedListStyle::Consistent,
        };
        Box::new(Self::new(style))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_with_document_structure() {
        let content = "* Item 1\n- Item 2\n+ Item 3";
        let rule = MD004UnorderedListStyle::new(UnorderedListStyle::Consistent);
        let structure = crate::utils::document_structure::DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let warnings = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Expected 2 warnings for inconsistent markers"
        );
    }
}
