use crate::lint_context::LintContext;
use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::emphasis_style::EmphasisStyle;
use crate::utils::range_utils::calculate_match_range;
use markdown::mdast::{Emphasis, Node};

/// Rule MD049: Emphasis style
///
/// See [docs/md049.md](../../docs/md049.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when the style for emphasis is inconsistent:
/// - Asterisks: `*text*`
/// - Underscores: `_text_`
///
/// This rule is focused on regular emphasis, not strong emphasis.
#[derive(Debug, Default, Clone)]
pub struct MD049EmphasisStyle {
    style: EmphasisStyle,
}

impl MD049EmphasisStyle {
    /// Create a new instance of MD049EmphasisStyle
    pub fn new(style: EmphasisStyle) -> Self {
        MD049EmphasisStyle { style }
    }

    // Recursively walk AST and collect all valid emphasis nodes with marker info
    fn collect_emphasis<'a>(
        node: &'a Node,
        parent_type: Option<&'static str>,
        emphasis_nodes: &mut Vec<(usize, usize, char, &'a Emphasis)>, // (line, col, marker, node)
        ctx: &LintContext,
    ) {
        match node {
            Node::Emphasis(em) => {
                if let Some(pos) = &em.position {
                    let start = pos.start.offset;
                    let (line, col) = ctx.offset_to_line_col(start);
                    let line_str = ctx.content.lines().nth(line - 1).unwrap_or("");
                    // Find marker at col-1 (1-based col)
                    let marker = line_str.chars().nth(col - 1).unwrap_or('*');
                    // Only consider if not inside ignored parent
                    if !matches!(parent_type, Some("Link" | "Image" | "Code")) {
                        emphasis_nodes.push((line, col, marker, em));
                    }
                }
                // Recurse into children
                for child in &em.children {
                    Self::collect_emphasis(child, Some("Emphasis"), emphasis_nodes, ctx);
                }
            }
            Node::Link(_) | Node::Image(_) | Node::Code(_) => {
                // Do not recurse into these
            }
            _ => {
                if let Some(children) = node.children() {
                    for child in children {
                        Self::collect_emphasis(child, parent_type, emphasis_nodes, ctx);
                    }
                }
            }
        }
    }
}

impl Rule for MD049EmphasisStyle {
    fn name(&self) -> &'static str {
        "MD049"
    }

    fn description(&self) -> &'static str {
        "Emphasis style should be consistent"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = vec![];
        let ast = &ctx.ast;
        match self.style {
            EmphasisStyle::Consistent => {
                // Collect all emphasis nodes from the entire document
                let mut emphasis_nodes = vec![];
                Self::collect_emphasis(ast, None, &mut emphasis_nodes, ctx);

                // If we have less than 2 emphasis nodes, no need to check consistency
                if emphasis_nodes.len() < 2 {
                    return Ok(warnings);
                }

                // Use the first emphasis marker found as the target style
                let target_marker = emphasis_nodes[0].2;

                // Check all subsequent emphasis nodes for consistency
                for (line, col, marker, em) in emphasis_nodes.iter().skip(1) {
                    if *marker != target_marker {
                        // Calculate precise character range for the entire emphasis
                        let line_str = ctx.content.lines().nth(line - 1).unwrap_or("");
                        let emphasis_start = col - 1; // Convert to 0-based
                        let emphasis_len = if let Some(pos) = &em.position {
                            pos.end.offset - pos.start.offset
                        } else {
                            1 // Fallback to single character
                        };
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(*line, line_str, emphasis_start, emphasis_len);

                        // Generate fix for this emphasis
                        let fix = if let Some(pos) = &em.position {
                            let start_offset = pos.start.offset;
                            let end_offset = pos.end.offset;

                            // Create fix for just the emphasis markers
                            if end_offset > start_offset
                                && start_offset < ctx.content.len()
                                && end_offset <= ctx.content.len()
                            {
                                let inner_content = &ctx.content[start_offset + 1..end_offset - 1];
                                Some(crate::rule::Fix {
                                    range: start_offset..end_offset,
                                    replacement: format!(
                                        "{}{}{}",
                                        target_marker, inner_content, target_marker
                                    ),
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!(
                                "Emphasis should use {
            } instead of {}",
                                target_marker, marker
                            ),
                            fix,
                            severity: Severity::Warning,
                        });
                    }
                }
            }
            EmphasisStyle::Asterisk | EmphasisStyle::Underscore => {
                let mut emphasis_nodes = vec![];
                Self::collect_emphasis(ast, None, &mut emphasis_nodes, ctx);
                let (wrong_marker, correct_marker) = match self.style {
                    EmphasisStyle::Asterisk => ('_', '*'),
                    EmphasisStyle::Underscore => ('*', '_'),
                    EmphasisStyle::Consistent => unreachable!(),
                };
                for (line, col, marker, em) in &emphasis_nodes {
                    if *marker == wrong_marker {
                        // Calculate precise character range for the entire emphasis
                        let line_str = ctx.content.lines().nth(line - 1).unwrap_or("");
                        let emphasis_start = col - 1; // Convert to 0-based
                        let emphasis_len = if let Some(pos) = &em.position {
                            pos.end.offset - pos.start.offset
                        } else {
                            1 // Fallback to single character
                        };
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(*line, line_str, emphasis_start, emphasis_len);

                        // Generate fix for this emphasis
                        let fix = if let Some(pos) = &em.position {
                            let start_offset = pos.start.offset;
                            let end_offset = pos.end.offset;

                            // Create fix for just the emphasis markers
                            if end_offset > start_offset
                                && start_offset < ctx.content.len()
                                && end_offset <= ctx.content.len()
                            {
                                let inner_content = &ctx.content[start_offset + 1..end_offset - 1];
                                Some(crate::rule::Fix {
                                    range: start_offset..end_offset,
                                    replacement: format!(
                                        "{}{}{}",
                                        correct_marker, inner_content, correct_marker
                                    ),
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!(
                                "Emphasis should use {
            } instead of {}",
                                correct_marker, wrong_marker
                            ),
                            fix,
                            severity: Severity::Warning,
                        });
                    }
                }
            }
        }
        Ok(warnings)
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
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
        let style = crate::config::get_rule_config_value::<String>(config, "MD049", "style")
            .unwrap_or_else(|| "consistent".to_string());
        let style = match style.as_str() {
            "asterisk" => EmphasisStyle::Asterisk,
            "underscore" => EmphasisStyle::Underscore,
            "consistent" => EmphasisStyle::Consistent,
            _ => EmphasisStyle::Consistent,
        };
        Box::new(MD049EmphasisStyle::new(style))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        let rule = MD049EmphasisStyle::default();
        assert_eq!(rule.name(), "MD049");
    }

    #[test]
    fn test_style_from_str() {
        assert_eq!(EmphasisStyle::from("asterisk"), EmphasisStyle::Asterisk);
        assert_eq!(EmphasisStyle::from("underscore"), EmphasisStyle::Underscore);
        assert_eq!(EmphasisStyle::from("other"), EmphasisStyle::Consistent);
    }
}
