/// Rule MD016: No multiple spaces after list marker
///
/// See [docs/md016.md](../../docs/md016.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::element_cache::ElementCache;
use crate::utils::element_cache::ListMarkerType;
use crate::utils::range_utils::LineIndex;
use toml;
use crate::rules::list_utils::{is_list_item, is_multi_line_item, ListType};

#[derive(Clone, Debug)]
pub struct MD016NoMultipleSpaceAfterListMarker {
    pub allow_multiple_spaces: bool,
    pub ul_single: usize,
    pub ul_multi: usize,
    pub ol_single: usize,
    pub ol_multi: usize,
}

impl Default for MD016NoMultipleSpaceAfterListMarker {
    fn default() -> Self {
        Self {
            allow_multiple_spaces: false,
            ul_single: 1,
            ul_multi: 1,
            ol_single: 1,
            ol_multi: 1,
        }
    }
}

impl MD016NoMultipleSpaceAfterListMarker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_allow_multiple_spaces(allow_multiple_spaces: bool) -> Self {
        Self {
            allow_multiple_spaces,
            ..Self::default()
        }
    }

    pub fn with_config(allow_multiple_spaces: bool, ul_single: usize, ul_multi: usize, ol_single: usize, ol_multi: usize) -> Self {
        Self {
            allow_multiple_spaces,
            ul_single,
            ul_multi,
            ol_single,
            ol_multi,
        }
    }

    pub fn get_expected_spaces(&self, list_type: ListType, is_multi: bool) -> usize {
        match (list_type, is_multi) {
            (ListType::Unordered, false) => self.ul_single,
            (ListType::Unordered, true) => self.ul_multi,
            (ListType::Ordered, false) => self.ol_single,
            (ListType::Ordered, true) => self.ol_multi,
        }
    }
}

impl Rule for MD016NoMultipleSpaceAfterListMarker {
    fn name(&self) -> &'static str {
        "MD016"
    }

    fn description(&self) -> &'static str {
        "List markers should not be followed by multiple spaces"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        if self.allow_multiple_spaces {
            return Ok(Vec::new());
        }
        if !content.contains('*')
            && !content.contains('-')
            && !content.contains('+')
            && !content.contains("1.")
            && !content.contains("2.")
        {
            return Ok(Vec::new());
        }
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let element_cache = ElementCache::new(content);
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        for (i, &line) in lines.iter().enumerate() {
            let line_num = i + 1;
            if let Some(list_item) = element_cache.get_list_item(line_num) {
                let in_code_block = element_cache.is_in_code_block(line_num);
                if in_code_block {
                    continue;
                }
                if let Some((list_type, _matched, _spaces)) = is_list_item(line) {
                    let is_multi = is_multi_line_item(&lines, i);
                    let allowed = self.get_expected_spaces(list_type, is_multi);
                    // Count spaces after marker
                    let marker_end = list_item.indent_str.len() + list_item.marker.len();
                    let after_marker = &line[marker_end..];
                    let spaces = after_marker.chars().take_while(|c| c.is_whitespace()).count();
                    // Only flag multi-line items with more than allowed spaces
                    if is_multi {
                        if spaces > allowed {
                            let msg = if list_item.marker.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                                "Multiple spaces after ordered list marker"
                            } else {
                                "Multiple spaces after unordered list marker"
                            };
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                severity: Severity::Warning,
                                line: line_num,
                                column: list_item.indent_str.len() + list_item.marker.len() + 1,
                                message: msg.to_string(),
                                fix: Some(Fix {
                                    range: line_index.line_col_to_byte_range(line_num, 1),
                                    replacement: if list_item.content.is_empty() {
                                        format!("{}{}", list_item.indent_str, list_item.marker)
                                    } else {
                                        format!("{}{}{}{}", list_item.indent_str, list_item.marker, " ".repeat(allowed), list_item.content)
                                    },
                                }),
                            });
                        }
                    } else {
                        // For single-line items, only flag if config is > 1 and spaces > allowed
                        if allowed > 1 && spaces > allowed {
                            let msg = if list_item.marker.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                                "Multiple spaces after ordered list marker"
                            } else {
                                "Multiple spaces after unordered list marker"
                            };
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                severity: Severity::Warning,
                                line: line_num,
                                column: list_item.indent_str.len() + list_item.marker.len() + 1,
                                message: msg.to_string(),
                                fix: Some(Fix {
                                    range: line_index.line_col_to_byte_range(line_num, 1),
                                    replacement: if list_item.content.is_empty() {
                                        format!("{}{}", list_item.indent_str, list_item.marker)
                                    } else {
                                        format!("{}{}{}{}", list_item.indent_str, list_item.marker, " ".repeat(allowed), list_item.content)
                                    },
                                }),
                            });
                        }
                    }
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        // Skip processing if allowing multiple spaces
        if self.allow_multiple_spaces {
            return Ok(content.to_string());
        }
        // Always reset the element cache to avoid stale data
        crate::utils::element_cache::reset_element_cache();
        // Force cache rebuild after reset
        let element_cache = ElementCache::new(content);
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            if let Some(list_item) = element_cache.get_list_item(line_num) {
                let in_code_block = element_cache.is_in_code_block(line_num);
                if in_code_block {
                    result.push_str(line);
                } else {
                    let indentation = &list_item.indent_str;
                    let marker = &list_item.marker;
                    let content = &list_item.content;
                    let fixed_line = if content.is_empty() {
                        format!("{}{}", indentation, marker)
                    } else {
                        format!("{}{} {}", indentation, marker, content)
                    };
                    result.push_str(&fixed_line);
                }
            } else {
                result.push_str(line);
            }
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "allow_multiple_spaces".to_string(),
            toml::Value::Boolean(self.allow_multiple_spaces),
        );
        map.insert(
            "ul_single".to_string(),
            toml::Value::Integer(self.ul_single as i64),
        );
        map.insert(
            "ul_multi".to_string(),
            toml::Value::Integer(self.ul_multi as i64),
        );
        map.insert(
            "ol_single".to_string(),
            toml::Value::Integer(self.ol_single as i64),
        );
        map.insert(
            "ol_multi".to_string(),
            toml::Value::Integer(self.ol_multi as i64),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule> {
        let allow_multiple_spaces = crate::config::get_rule_config_value::<bool>(config, "MD016", "allow_multiple_spaces").unwrap_or(false);
        let ul_single = crate::config::get_rule_config_value::<usize>(config, "MD030", "ul-single").unwrap_or(1);
        let ul_multi  = crate::config::get_rule_config_value::<usize>(config, "MD030", "ul-multi").unwrap_or(1);
        let ol_single = crate::config::get_rule_config_value::<usize>(config, "MD030", "ol-single").unwrap_or(1);
        let ol_multi  = crate::config::get_rule_config_value::<usize>(config, "MD030", "ol-multi").unwrap_or(1);
        Box::new(MD016NoMultipleSpaceAfterListMarker::with_config(
            allow_multiple_spaces, ul_single, ul_multi, ol_single, ol_multi
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md016_check() {
        let rule = MD016NoMultipleSpaceAfterListMarker::default();

        // Valid test cases
        let content1 = "- Item with one space\n* Another item with one space\n+ A third item";
        let ctx1 = crate::lint_context::LintContext::new(content1);
        let warnings1 = rule.check(&ctx1).unwrap();
        assert_eq!(warnings1.len(), 0);

        // Invalid test cases
        let content2 =
            "-  Item with two spaces\n*   Another item with three spaces\n+    Four spaces";
        let ctx2 = crate::lint_context::LintContext::new(content2);
        let warnings2 = rule.check(&ctx2).unwrap();
        assert_eq!(warnings2.len(), 3);

        // Mixed case
        let content3 = "- Valid item\n-  Invalid item\n```
-  Ignored in code block\n```";
        let ctx3 = crate::lint_context::LintContext::new(content3);
        let warnings3 = rule.check(&ctx3).unwrap();
        // Now both the second and fourth lines are detected as list items, but the fourth is in a code block and should not be flagged
        assert_eq!(warnings3.len(), 1);

        // Test with allow_multiple_spaces = true
        let rule_allowing_spaces =
            MD016NoMultipleSpaceAfterListMarker::with_allow_multiple_spaces(true);
        let warnings4 = rule_allowing_spaces.check(&ctx2).unwrap();
        assert_eq!(warnings4.len(), 0);
    }

    #[test]
    fn test_md016_fix() {
        let rule = MD016NoMultipleSpaceAfterListMarker::default();

        // Fix test case
        let content =
            "-  Item with two spaces\n*   Another item with three spaces\n+    Four spaces";
        let expected = "- Item with two spaces\n* Another item with three spaces\n+ Four spaces";
        let ctx = crate::lint_context::LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);

        // Test with code blocks
        let content2 = "- Valid item\n-  Invalid item\n```
-  Ignored in code block\n```";
        let expected2 = "- Valid item\n- Invalid item\n```
-  Ignored in code block\n```";
        let ctx2 = crate::lint_context::LintContext::new(content2);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed2, expected2);
    }

    #[test]
    fn test_md016_multi_line_and_single_line_behavior() {
        // Config: ul_single=1, ul_multi=3, ol_single=1, ol_multi=2
        let rule = MD016NoMultipleSpaceAfterListMarker::with_config(false, 1, 3, 1, 2);

        // Single-line unordered, 1 space (allowed)
        let content = "- one\n- two";
        let ctx = crate::lint_context::LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "No warning for single-line unordered with 1 space");

        // Single-line unordered, 3 spaces (should NOT warn, only multi-line matters)
        let content = "-   one\n-   two";
        let ctx = crate::lint_context::LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "No warning for single-line unordered with 3 spaces");

        // Multi-line unordered, 4 spaces (should warn, allowed is 3)
        let content = "-   one\n    continued";
        let ctx = crate::lint_context::LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Warn for multi-line unordered with 3+ spaces");

        // Multi-line unordered, 3 spaces (allowed)
        let content = "-   one\n    continued";
        let ctx = crate::lint_context::LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Warn for multi-line unordered with 3+ spaces");

        // Single-line ordered, 2 spaces (should NOT warn, only multi-line matters)
        let content = "1.  one\n2.  two";
        let ctx = crate::lint_context::LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "No warning for single-line ordered with 2 spaces");

        // Multi-line ordered, 3 spaces (should warn, allowed is 2)
        let content = "1.  one\n   continued";
        let ctx = crate::lint_context::LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Warn for multi-line ordered with 3+ spaces");

        // Multi-line ordered, 2 spaces (allowed)
        let content = "1.  one\n   continued";
        let ctx = crate::lint_context::LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Warn for multi-line ordered with 2+ spaces");

        // Config: ul_single=2, ul_multi=3, ol_single=2, ol_multi=2
        let rule = MD016NoMultipleSpaceAfterListMarker::with_config(false, 2, 3, 2, 2);
        // Single-line unordered, 3 spaces (should warn, allowed is 2)
        let content = "-   one";
        let ctx = crate::lint_context::LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Warn for single-line unordered with more than allowed");
        // Single-line ordered, 3 spaces (should warn, allowed is 2)
        let content = "1.   one";
        let ctx = crate::lint_context::LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Warn for single-line ordered with more than allowed");
        // Single-line unordered, 2 spaces (allowed)
        let content = "-  one";
        let ctx = crate::lint_context::LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "No warning for single-line unordered with allowed spaces");
        // Multi-line unordered, 4 spaces (should warn, allowed is 3)
        let content = "-    one\n     continued";
        let ctx = crate::lint_context::LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Warn for multi-line unordered with more than allowed");
    }
}
