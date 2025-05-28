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

        // Early returns for performance
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any list markers before expensive processing
        if !content.contains('*') && !content.contains('-') && !content.contains('+') {
            return Ok(Vec::new());
        }

        let element_cache = ElementCache::new(content);
        let mut warnings = Vec::new();

        for item in element_cache.get_list_items() {
            // Only unordered list items
            // Skip list items inside code blocks (including YAML/front matter)
            if element_cache.is_in_code_block(item.line_number) {
                continue;
            }
            if matches!(
                item.marker_type,
                ListMarkerType::Asterisk | ListMarkerType::Plus | ListMarkerType::Minus
            ) {
                let expected_indent = item.nesting_level * self.indent;
                if item.indentation != expected_indent {
                    // Generate fix for this list item
                    let fix = {
                        let lines: Vec<&str> = content.lines().collect();
                        if let Some(line) = lines.get(item.line_number - 1) {
                            // Extract the marker and content
                            let re = regex::Regex::new(r"^(\s*)([*+-])(\s+)(.*)$").unwrap();
                            if let Some(caps) = re.captures(line) {
                                let content_part = caps.get(4).map_or("", |m| m.as_str());
                                let space_after_marker = caps.get(3).map_or(" ", |m| m.as_str());
                                let marker_char = caps.get(2).map_or("*", |m| m.as_str());
                                let correct_indent = " ".repeat(expected_indent);
                                let fixed_line = format!(
                                    "{}{}{}{}{}",
                                    item.blockquote_prefix,
                                    correct_indent,
                                    marker_char,
                                    space_after_marker,
                                    content_part
                                );

                                let line_index =
                                    crate::utils::range_utils::LineIndex::new(content.to_string());
                                let line_start =
                                    line_index.line_col_to_byte_range(item.line_number, 1).start;
                                let line_end = if item.line_number < lines.len() {
                                    line_index
                                        .line_col_to_byte_range(item.line_number + 1, 1)
                                        .start
                                        - 1
                                } else {
                                    content.len()
                                };
                                Some(crate::rule::Fix {
                                    range: line_start..line_end,
                                    replacement: fixed_line,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!(
                            "Incorrect indentation: expected {} spaces for nesting level {}, found {}",
                            expected_indent, item.nesting_level, item.indentation
                        ),
                        line: item.line_number,
                        column: item.blockquote_prefix.len() + item.indentation + 1, // correct column for marker
                        end_line: item.line_number,
                        end_column: item.blockquote_prefix.len() + item.indentation + 2,
                        severity: Severity::Warning,
                        fix,
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
        doc_structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;

        // Early return if no list items
        if doc_structure.list_lines.is_empty() {
            return Ok(Vec::new());
        }

        // Use ElementCache for detailed list analysis (still needed for nesting levels)
        let element_cache = ElementCache::new(content);
        let mut warnings = Vec::new();

        for item in element_cache.get_list_items() {
            // Only process unordered list items that are in our structure
            if !doc_structure.list_lines.contains(&item.line_number) {
                continue;
            }

            // Skip list items inside code blocks
            if doc_structure.is_in_code_block(item.line_number) {
                continue;
            }

            if matches!(
                item.marker_type,
                ListMarkerType::Asterisk | ListMarkerType::Plus | ListMarkerType::Minus
            ) {
                let expected_indent = item.nesting_level * self.indent;
                if item.indentation != expected_indent {
                    // Generate fix for this list item
                    let fix = {
                        let lines: Vec<&str> = content.lines().collect();
                        if let Some(line) = lines.get(item.line_number - 1) {
                            // Extract the marker and content
                            let re = regex::Regex::new(r"^(\s*)([*+-])(\s+)(.*)$").unwrap();
                            if let Some(caps) = re.captures(line) {
                                let content_part = caps.get(4).map_or("", |m| m.as_str());
                                let space_after_marker = caps.get(3).map_or(" ", |m| m.as_str());
                                let marker_char = caps.get(2).map_or("*", |m| m.as_str());
                                let correct_indent = " ".repeat(expected_indent);
                                let fixed_line = format!(
                                    "{}{}{}{}{}",
                                    item.blockquote_prefix,
                                    correct_indent,
                                    marker_char,
                                    space_after_marker,
                                    content_part
                                );

                                let line_index =
                                    crate::utils::range_utils::LineIndex::new(content.to_string());
                                let line_start =
                                    line_index.line_col_to_byte_range(item.line_number, 1).start;
                                let line_end = if item.line_number < lines.len() {
                                    line_index
                                        .line_col_to_byte_range(item.line_number + 1, 1)
                                        .start
                                        - 1
                                } else {
                                    content.len()
                                };
                                Some(crate::rule::Fix {
                                    range: line_start..line_end,
                                    replacement: fixed_line,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!(
                            "Incorrect indentation: expected {} spaces for nesting level {}, found {}",
                            expected_indent, item.nesting_level, item.indentation
                        ),
                        line: item.line_number,
                        column: item.blockquote_prefix.len() + item.indentation + 1, // correct column for marker
                        end_line: item.line_number,
                        end_column: item.blockquote_prefix.len() + item.indentation + 2,
                        severity: Severity::Warning,
                        fix,
                    });
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
