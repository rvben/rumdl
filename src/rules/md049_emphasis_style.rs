use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::emphasis_style::EmphasisStyle;
use crate::utils::document_structure::DocumentStructure;
use crate::utils::emphasis_utils::{find_emphasis_markers, find_single_emphasis_spans, replace_inline_code};

mod md049_config;
use md049_config::MD049Config;

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
    config: MD049Config,
}

impl MD049EmphasisStyle {
    /// Create a new instance of MD049EmphasisStyle
    pub fn new(style: EmphasisStyle) -> Self {
        MD049EmphasisStyle {
            config: MD049Config { style },
        }
    }

    pub fn from_config_struct(config: MD049Config) -> Self {
        Self { config }
    }

    // Collect emphasis from a single line
    fn collect_emphasis_from_line(
        &self,
        line: &str,
        line_num: usize,
        emphasis_info: &mut Vec<(usize, usize, char, String)>, // (line, col, marker, content)
    ) {
        // Replace inline code to avoid false positives
        let line_no_code = replace_inline_code(line);

        // Find all emphasis markers
        let markers = find_emphasis_markers(&line_no_code);
        if markers.is_empty() {
            return;
        }

        // Find single emphasis spans (not strong emphasis)
        let spans = find_single_emphasis_spans(&line_no_code, markers);

        for span in spans {
            let marker_char = span.opening.as_char();
            let col = span.opening.start_pos + 1; // Convert to 1-based

            emphasis_info.push((line_num, col, marker_char, span.content.clone()));
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
        let content = ctx.content;

        // Early return if no emphasis markers
        if !content.contains('*') && !content.contains('_') {
            return Ok(warnings);
        }

        // Create document structure to skip code blocks
        let structure = DocumentStructure::new(content);

        // Collect all emphasis from the document
        let mut emphasis_info = vec![];

        // Track absolute position for fixes
        let mut abs_pos = 0;

        for (line_idx, line) in content.lines().enumerate() {
            let line_num = line_idx + 1;

            // Skip if in code block or front matter
            if structure.is_in_code_block(line_num) || structure.is_in_front_matter(line_num) {
                abs_pos += line.len() + 1; // +1 for newline
                continue;
            }

            // Skip if the line doesn't contain any emphasis markers
            if !line.contains('*') && !line.contains('_') {
                abs_pos += line.len() + 1;
                continue;
            }

            // Collect emphasis with absolute positions
            let line_start = abs_pos;
            self.collect_emphasis_from_line(line, line_num, &mut emphasis_info);

            // Update emphasis_info with absolute positions
            let last_emphasis_count = emphasis_info.len();
            for i in (0..last_emphasis_count).rev() {
                if emphasis_info[i].0 == line_num {
                    // Add line start position to column
                    let (line_num, col, marker, content) = emphasis_info[i].clone();
                    emphasis_info[i] = (line_num, line_start + col - 1, marker, content);
                } else {
                    break;
                }
            }

            abs_pos += line.len() + 1;
        }

        match self.config.style {
            EmphasisStyle::Consistent => {
                // If we have less than 2 emphasis nodes, no need to check consistency
                if emphasis_info.len() < 2 {
                    return Ok(warnings);
                }

                // Use the first emphasis marker found as the target style
                let target_marker = emphasis_info[0].2;

                // Check all subsequent emphasis nodes for consistency
                for (line_num, abs_col, marker, content) in emphasis_info.iter().skip(1) {
                    if *marker != target_marker {
                        // Calculate emphasis length (marker + content + marker)
                        let emphasis_len = 1 + content.len() + 1;

                        // Calculate line-relative column (1-based)
                        let line_start = content.lines().take(line_num - 1).map(|l| l.len() + 1).sum::<usize>();
                        let col = abs_col - line_start + 1;

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: *line_num,
                            column: col,
                            end_line: *line_num,
                            end_column: col + emphasis_len,
                            message: format!("Emphasis should use {target_marker} instead of {marker}"),
                            fix: Some(Fix {
                                range: *abs_col..*abs_col + emphasis_len,
                                replacement: format!("{target_marker}{content}{target_marker}"),
                            }),
                            severity: Severity::Warning,
                        });
                    }
                }
            }
            EmphasisStyle::Asterisk | EmphasisStyle::Underscore => {
                let (wrong_marker, correct_marker) = match self.config.style {
                    EmphasisStyle::Asterisk => ('_', '*'),
                    EmphasisStyle::Underscore => ('*', '_'),
                    EmphasisStyle::Consistent => {
                        // This case is handled separately above
                        // but fallback to asterisk style for safety
                        ('_', '*')
                    }
                };

                for (line_num, abs_col, marker, content) in &emphasis_info {
                    if *marker == wrong_marker {
                        // Calculate emphasis length (marker + content + marker)
                        let emphasis_len = 1 + content.len() + 1;

                        // Calculate line-relative column (1-based)
                        let line_start = ctx
                            .content
                            .lines()
                            .take(line_num - 1)
                            .map(|l| l.len() + 1)
                            .sum::<usize>();
                        let col = abs_col - line_start + 1;

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: *line_num,
                            column: col,
                            end_line: *line_num,
                            end_column: col + emphasis_len,
                            message: format!("Emphasis should use {correct_marker} instead of {wrong_marker}"),
                            fix: Some(Fix {
                                range: *abs_col..*abs_col + emphasis_len,
                                replacement: format!("{correct_marker}{content}{correct_marker}"),
                            }),
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
            .filter_map(|w| w.fix.as_ref().map(|f| (f.range.start, f.range.end, &f.replacement)))
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

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || (!ctx.content.contains('*') && !ctx.content.contains('_'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD049Config>(config);
        Box::new(Self::from_config_struct(rule_config))
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
