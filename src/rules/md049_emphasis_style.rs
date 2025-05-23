use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::emphasis_style::EmphasisStyle;
use crate::lint_context::LintContext;
use markdown::mdast::{Node, Emphasis};

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
        &'a self,
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
                    self.collect_emphasis(child, Some("Emphasis"), emphasis_nodes, ctx);
                }
            }
            Node::Link(_) | Node::Image(_) | Node::Code(_) => {
                // Do not recurse into these
            }
            _ => {
                if let Some(children) = node.children() {
                    for child in children {
                        self.collect_emphasis(child, parent_type, emphasis_nodes, ctx);
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
                self.collect_emphasis(ast, None, &mut emphasis_nodes, ctx);

                // If we have less than 2 emphasis nodes, no need to check consistency
                if emphasis_nodes.len() < 2 {
                    return Ok(warnings);
                }

                // Use the first emphasis marker found as the target style
                let target_marker = emphasis_nodes[0].2;

                // Check all subsequent emphasis nodes for consistency
                for (line, col, marker, _) in emphasis_nodes.iter().skip(1) {
                    if *marker != target_marker {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: *line,
                            column: *col,
                            message: format!("Emphasis should use {} instead of {}", target_marker, marker),
                            fix: None,
                            severity: Severity::Warning,
                        });
                    }
                }
            }
            EmphasisStyle::Asterisk | EmphasisStyle::Underscore => {
                let mut emphasis_nodes = vec![];
                self.collect_emphasis(ast, None, &mut emphasis_nodes, ctx);
                let (wrong_marker, correct_marker) = match self.style {
                    EmphasisStyle::Asterisk => ('_', '*'),
                    EmphasisStyle::Underscore => ('*', '_'),
                    EmphasisStyle::Consistent => unreachable!(),
                };
                for (line, col, marker, _) in &emphasis_nodes {
                    if *marker == wrong_marker {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: *line,
                            column: *col,
                            message: format!("Emphasis should use {} instead of {}", correct_marker, wrong_marker),
                            fix: None,
                            severity: Severity::Warning,
                        });
                    }
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let ast = &ctx.ast;
        let mut edits = vec![];
        match self.style {
            EmphasisStyle::Consistent => {
                // Collect all emphasis nodes from the entire document
                let mut emphasis_nodes = vec![];
                self.collect_emphasis(ast, None, &mut emphasis_nodes, ctx);

                // If we have less than 2 emphasis nodes, nothing to fix
                if emphasis_nodes.len() < 2 {
                    return Ok(ctx.content.to_string());
                }

                // Use the first emphasis marker found as the target style
                let target_marker = emphasis_nodes[0].2;

                // Fix all subsequent emphasis nodes that don't match the target style
                for (_, _, marker, em) in emphasis_nodes.iter().skip(1) {
                    if *marker != target_marker {
                        if let Some(pos) = &em.position {
                            let start = pos.start.offset;
                            let end = pos.end.offset;
                            edits.push((start, target_marker));
                            edits.push((end - 1, target_marker));
                        }
                    }
                }
            }
            EmphasisStyle::Asterisk | EmphasisStyle::Underscore => {
                let mut emphasis_nodes = vec![];
                self.collect_emphasis(ast, None, &mut emphasis_nodes, ctx);
                let (wrong_marker, correct_marker) = match self.style {
                    EmphasisStyle::Asterisk => ('_', '*'),
                    EmphasisStyle::Underscore => ('*', '_'),
                    EmphasisStyle::Consistent => unreachable!(),
                };
                for (_, _, marker, em) in &emphasis_nodes {
                    if *marker == wrong_marker {
                        if let Some(pos) = &em.position {
                            let start = pos.start.offset;
                            let end = pos.end.offset;
                            edits.push((start, correct_marker));
                            edits.push((end - 1, correct_marker));
                        }
                    }
                }
            }
        }
        let mut result = ctx.content.to_string();
        edits.sort_by(|a, b| b.0.cmp(&a.0));
        for (offset, marker) in edits {
            if offset < result.len() {
                result.replace_range(offset..offset + 1, &marker.to_string());
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
