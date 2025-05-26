/// Rule MD007: Unordered list indentation
///
/// See [docs/md007.md](../../docs/md007.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::element_cache::{ElementCache, ListMarkerType};
use lazy_static::lazy_static;
use regex::Regex;
use toml;

#[derive(Debug, Clone)]
pub struct MD007ULIndent {
    pub indent: usize,
}

impl Default for MD007ULIndent {
    fn default() -> Self {
        Self { indent: 2 }
    }
}

impl MD007ULIndent {
    pub fn new(indent: usize) -> Self {
        Self { indent }
    }

    #[allow(dead_code)]
    fn is_in_code_block(content: &str, line_idx: usize) -> bool {
        lazy_static! {
            static ref CODE_BLOCK_MARKER: Regex = Regex::new(r"^(```|~~~)").unwrap();
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;

        for (i, line) in lines.iter().enumerate() {
            if i > line_idx {
                break;
            }

            if CODE_BLOCK_MARKER.is_match(line.trim_start()) {
                in_code_block = !in_code_block;
            }

            if i == line_idx {
                return in_code_block;
            }
        }

        false
    }
}

impl Rule for MD007ULIndent {
    fn name(&self) -> &'static str {
        "MD007"
    }

    fn description(&self) -> &'static str {
        "Unordered list indentation"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let element_cache = ElementCache::new(content);
        let mut warnings = Vec::new();
        for item in element_cache.get_list_items() {
            // Only unordered list items
            // Skip list items inside code blocks (including YAML/front matter)
            if element_cache.is_in_code_block(item.line_number) {
                continue;
            }
            if matches!(item.marker_type, ListMarkerType::Asterisk | ListMarkerType::Plus | ListMarkerType::Minus) {
                let expected_indent = item.nesting_level * self.indent;
                if item.indentation != expected_indent {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!(
                            "Incorrect indentation: expected {} spaces for nesting level {}, found {}",
                            expected_indent, item.nesting_level, item.indentation
                        ),
                        line: item.line_number,
                        column: item.blockquote_prefix.len() + item.indentation + 1, // correct column for marker
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
            }
        }
        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> LintResult {
        self.check(ctx)
    }

            fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Get the warnings to know which lines need indentation fixing
        let warnings = self.check(ctx)?;
        let mut lines_to_fix: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for warning in &warnings {
            lines_to_fix.insert(warning.line);
        }

        let element_cache = ElementCache::new(content);
        let lines: Vec<&str> = content.lines().collect();
        let mut result_lines: Vec<String> = Vec::new();

        // Check if any list items have tabs that need normalization
        let mut has_tabs = false;
        for &line in &lines {
            if line.contains('\t') && element_cache.get_list_items().iter().any(|item| item.line_number == lines.iter().position(|&l| l == line).unwrap_or(0) + 1) {
                has_tabs = true;
                break;
            }
        }

        // If no warnings and no tabs to normalize, return original content
        if warnings.is_empty() && !has_tabs {
            return Ok(content.to_string());
        }

        for (line_idx, &line) in lines.iter().enumerate() {
            let line_number = line_idx + 1;

            // Check if this line is a list item
            if let Some(item) = element_cache.get_list_items().iter().find(|item| item.line_number == line_number) {
                if matches!(item.marker_type, ListMarkerType::Asterisk | ListMarkerType::Plus | ListMarkerType::Minus) {
                    // Determine if we need to fix this line
                    let needs_indentation_fix = lines_to_fix.contains(&line_number);
                    let needs_tab_normalization = line.contains('\t');

                    if needs_indentation_fix || needs_tab_normalization {
                        let expected_indent = item.nesting_level * self.indent;

                        // Reconstruct the line with correct indentation
                        let marker_char = match item.marker_type {
                            ListMarkerType::Asterisk => '*',
                            ListMarkerType::Plus => '+',
                            ListMarkerType::Minus => '-',
                            _ => unreachable!(),
                        };

                        // Extract the content after the marker and space
                        let re = regex::Regex::new(r"^(\s*)([*+-])(\s+)(.*)$").unwrap();
                        if let Some(caps) = re.captures(line) {
                            let content_part = caps.get(4).map_or("", |m| m.as_str());
                            let space_after_marker = caps.get(3).map_or(" ", |m| m.as_str());
                            let correct_indent = " ".repeat(expected_indent);
                            let fixed_line = format!("{}{}{}{}{}", item.blockquote_prefix, correct_indent, marker_char, space_after_marker, content_part);
                            result_lines.push(fixed_line);
                        } else {
                            // Fallback: just use the original line
                            result_lines.push(line.to_string());
                        }
                    } else {
                        result_lines.push(line.to_string());
                    }
                } else {
                    result_lines.push(line.to_string());
                }
            } else {
                result_lines.push(line.to_string());
            }
        }

        let result = result_lines.join("\n");
        if content.ends_with('\n') {
            Ok(result + "\n")
        } else {
            Ok(result)
        }
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty()
            || (!ctx.content.contains('*')
                && !ctx.content.contains('-')
                && !ctx.content.contains('+'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "indent".to_string(),
            toml::Value::Integer(self.indent as i64),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let indent =
            crate::config::get_rule_config_value::<usize>(config, "MD007", "indent").unwrap_or(2);
        Box::new(MD007ULIndent::new(indent))
    }
}

impl DocumentStructureExtensions for MD007ULIndent {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        // Use the document structure to check if there are any unordered list elements
        !doc_structure.list_lines.is_empty()
    }
}
