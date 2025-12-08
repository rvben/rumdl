/// Rule MD029: Ordered list item prefix
///
/// See [docs/md029.md](../../docs/md029.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::regex_cache::ORDERED_LIST_MARKER_REGEX;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use toml;

mod md029_config;
pub use md029_config::{ListStyle, MD029Config};

#[derive(Debug, Clone, Default)]
pub struct MD029OrderedListPrefix {
    config: MD029Config,
}

impl MD029OrderedListPrefix {
    pub fn new(style: ListStyle) -> Self {
        Self {
            config: MD029Config { style },
        }
    }

    pub fn from_config_struct(config: MD029Config) -> Self {
        Self { config }
    }

    #[inline]
    fn parse_marker_number(marker: &str) -> Option<usize> {
        // Handle marker format like "1." or "1"
        let num_part = if let Some(stripped) = marker.strip_suffix('.') {
            stripped
        } else {
            marker
        };
        num_part.parse::<usize>().ok()
    }

    #[inline]
    fn get_expected_number(&self, index: usize, detected_style: Option<ListStyle>) -> usize {
        // Use detected_style when the configuration is auto-detect mode (OneOrOrdered or Consistent)
        // For explicit style configurations, always use the configured style
        let style = match self.config.style {
            ListStyle::OneOrOrdered | ListStyle::Consistent => detected_style.unwrap_or(ListStyle::OneOne),
            _ => self.config.style.clone(),
        };

        match style {
            ListStyle::One | ListStyle::OneOne => 1,
            ListStyle::Ordered => index + 1,
            ListStyle::Ordered0 => index,
            ListStyle::OneOrOrdered | ListStyle::Consistent => {
                // This shouldn't be reached since we handle these above
                1
            }
        }
    }

    /// Detect the style being used in a list by checking all items for prevalence
    fn detect_list_style(
        items: &[(
            usize,
            &crate::lint_context::LineInfo,
            &crate::lint_context::ListItemInfo,
        )],
    ) -> ListStyle {
        if items.len() < 2 {
            // With only one item, we can't determine the style, default to OneOne
            return ListStyle::OneOne;
        }

        let first_num = Self::parse_marker_number(&items[0].2.marker);
        let second_num = Self::parse_marker_number(&items[1].2.marker);

        // Fast path: Check for Ordered0 special case (starts with 0, 1)
        if matches!((first_num, second_num), (Some(0), Some(1))) {
            return ListStyle::Ordered0;
        }

        // Fast path: If first 2 items aren't both "1", it must be Ordered (O(1))
        // This handles ~95% of lists instantly: "1. 2. 3...", "2. 3. 4...", etc.
        if first_num != Some(1) || second_num != Some(1) {
            return ListStyle::Ordered;
        }

        // Slow path: Both first items are "1", check if ALL are "1" (O(n))
        // This is necessary for lists like "1. 1. 1..." vs "1. 1. 2. 3..."
        let all_ones = items
            .iter()
            .all(|(_, _, item)| Self::parse_marker_number(&item.marker) == Some(1));

        if all_ones {
            ListStyle::OneOne
        } else {
            ListStyle::Ordered
        }
    }

    /// Build a map from line number to list ID using pulldown-cmark's AST.
    /// This is the authoritative source of truth for list membership.
    fn build_commonmark_list_membership(content: &str) -> std::collections::HashMap<usize, usize> {
        let mut line_to_list: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();

        // Pre-compute line start offsets for byte-to-line conversion
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        let byte_to_line = |byte_offset: usize| -> usize {
            line_starts
                .iter()
                .rposition(|&start| start <= byte_offset)
                .map(|i| i + 1) // 1-indexed
                .unwrap_or(1)
        };

        let options = Options::empty();
        let parser = Parser::new_ext(content, options);

        let mut list_stack: Vec<(usize, bool)> = Vec::new(); // (list_id, is_ordered)
        let mut next_list_id = 0;

        for (event, range) in parser.into_offset_iter() {
            match event {
                Event::Start(Tag::List(start_num)) => {
                    let is_ordered = start_num.is_some();
                    list_stack.push((next_list_id, is_ordered));
                    next_list_id += 1;
                }
                Event::End(TagEnd::List(_)) => {
                    list_stack.pop();
                }
                Event::Start(Tag::Item) => {
                    // Record the line number of this item and its list ID
                    if let Some(&(list_id, is_ordered)) = list_stack.last()
                        && is_ordered
                    {
                        let line_num = byte_to_line(range.start);
                        line_to_list.insert(line_num, list_id);
                    }
                }
                _ => {}
            }
        }

        line_to_list
    }

    /// Group ordered items by their CommonMark list membership.
    /// Returns groups of (line_num, LineInfo, ListItemInfo) for each distinct list.
    fn group_items_by_commonmark_list<'a>(
        ctx: &'a crate::lint_context::LintContext,
        line_to_list: &std::collections::HashMap<usize, usize>,
    ) -> Vec<
        Vec<(
            usize,
            &'a crate::lint_context::LineInfo,
            &'a crate::lint_context::ListItemInfo,
        )>,
    > {
        // Collect all ordered items with their list IDs
        let mut items_with_list_id: Vec<(
            usize,
            usize,
            &crate::lint_context::LineInfo,
            &crate::lint_context::ListItemInfo,
        )> = Vec::new();

        for line_num in 1..=ctx.lines.len() {
            if let Some(line_info) = ctx.line_info(line_num)
                && let Some(list_item) = &line_info.list_item
                && list_item.is_ordered
            {
                // Get the list ID from pulldown-cmark's grouping
                if let Some(&list_id) = line_to_list.get(&line_num) {
                    items_with_list_id.push((list_id, line_num, line_info, list_item));
                }
            }
        }

        // Group by list_id
        let mut groups: std::collections::HashMap<
            usize,
            Vec<(
                usize,
                &crate::lint_context::LineInfo,
                &crate::lint_context::ListItemInfo,
            )>,
        > = std::collections::HashMap::new();

        for (list_id, line_num, line_info, list_item) in items_with_list_id {
            groups
                .entry(list_id)
                .or_default()
                .push((line_num, line_info, list_item));
        }

        // Convert to Vec, sort each group by line number, and sort groups by first line
        let mut result: Vec<_> = groups.into_values().collect();
        for group in &mut result {
            group.sort_by_key(|(line_num, _, _)| *line_num);
        }
        // Sort groups by their first item's line number for deterministic output
        result.sort_by_key(|group| group.first().map(|(ln, _, _)| *ln).unwrap_or(0));

        result
    }

    /// Check a CommonMark-grouped list for correct ordering
    fn check_commonmark_list_group(
        &self,
        _ctx: &crate::lint_context::LintContext,
        group: &[(
            usize,
            &crate::lint_context::LineInfo,
            &crate::lint_context::ListItemInfo,
        )],
        warnings: &mut Vec<LintWarning>,
        document_wide_style: Option<ListStyle>,
    ) {
        if group.is_empty() {
            return;
        }

        // Group items by indentation level (marker_column) to handle nested lists
        type LevelGroups<'a> = std::collections::HashMap<
            usize,
            Vec<(
                usize,
                &'a crate::lint_context::LineInfo,
                &'a crate::lint_context::ListItemInfo,
            )>,
        >;
        let mut level_groups: LevelGroups = std::collections::HashMap::new();

        for (line_num, line_info, list_item) in group {
            level_groups
                .entry(list_item.marker_column)
                .or_default()
                .push((*line_num, *line_info, *list_item));
        }

        // Process each indentation level in sorted order for deterministic output
        let mut sorted_levels: Vec<_> = level_groups.into_iter().collect();
        sorted_levels.sort_by_key(|(indent, _)| *indent);

        for (_indent, mut items) in sorted_levels {
            // Sort by line number
            items.sort_by_key(|(line_num, _, _)| *line_num);

            // Determine style for this group
            let detected_style = if let Some(doc_style) = document_wide_style.clone() {
                Some(doc_style)
            } else if self.config.style == ListStyle::OneOrOrdered {
                Some(Self::detect_list_style(&items))
            } else {
                None
            };

            // Check each item
            for (idx, (line_num, line_info, list_item)) in items.iter().enumerate() {
                if let Some(actual_num) = Self::parse_marker_number(&list_item.marker) {
                    let expected_num = self.get_expected_number(idx, detected_style.clone());

                    if actual_num != expected_num {
                        let marker_start = line_info.byte_offset + list_item.marker_column;
                        let number_len = if let Some(dot_pos) = list_item.marker.find('.') {
                            dot_pos
                        } else if let Some(paren_pos) = list_item.marker.find(')') {
                            paren_pos
                        } else {
                            list_item.marker.len()
                        };

                        let style_name = match detected_style.as_ref().unwrap_or(&ListStyle::Ordered) {
                            ListStyle::OneOne => "one",
                            ListStyle::Ordered => "ordered",
                            ListStyle::Ordered0 => "ordered0",
                            _ => "ordered",
                        };

                        let style_context = match self.config.style {
                            ListStyle::Consistent => format!("document style '{style_name}'"),
                            ListStyle::OneOrOrdered => format!("list style '{style_name}'"),
                            ListStyle::One | ListStyle::OneOne => "configured style 'one'".to_string(),
                            ListStyle::Ordered => "configured style 'ordered'".to_string(),
                            ListStyle::Ordered0 => "configured style 'ordered0'".to_string(),
                        };

                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            message: format!(
                                "Ordered list item number {actual_num} does not match {style_context} (expected {expected_num})"
                            ),
                            line: *line_num,
                            column: list_item.marker_column + 1,
                            end_line: *line_num,
                            end_column: list_item.marker_column + number_len + 1,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: marker_start..marker_start + number_len,
                                replacement: expected_num.to_string(),
                            }),
                        });
                    }
                }
            }
        }
    }
}

impl Rule for MD029OrderedListPrefix {
    fn name(&self) -> &'static str {
        "MD029"
    }

    fn description(&self) -> &'static str {
        "Ordered list marker value"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early returns for performance
        if ctx.content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any ordered list markers before processing
        if !ctx.content.contains('.') || !ctx.content.lines().any(|line| ORDERED_LIST_MARKER_REGEX.is_match(line)) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        // Use pulldown-cmark's AST for authoritative list membership.
        // This fixes issues where heuristic-based grouping incorrectly splits lists.
        let line_to_list = Self::build_commonmark_list_membership(ctx.content);
        let list_groups = Self::group_items_by_commonmark_list(ctx, &line_to_list);

        if list_groups.is_empty() {
            return Ok(Vec::new());
        }

        // For Consistent style, detect document-wide prevalent style
        let document_wide_style = if self.config.style == ListStyle::Consistent {
            // Collect ALL ordered items from ALL groups
            let mut all_document_items = Vec::new();
            for group in &list_groups {
                for (line_num, line_info, list_item) in group {
                    all_document_items.push((*line_num, *line_info, *list_item));
                }
            }
            // Detect style across entire document
            if !all_document_items.is_empty() {
                Some(Self::detect_list_style(&all_document_items))
            } else {
                None
            }
        } else {
            None
        };

        // Process each CommonMark-defined list group
        for group in list_groups {
            self.check_commonmark_list_group(ctx, &group, &mut warnings, document_wide_style.clone());
        }

        // Sort warnings by line number for deterministic output
        warnings.sort_by_key(|w| (w.line, w.column));

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Use the same logic as check() - just apply the fixes from warnings
        let warnings = self.check(ctx)?;

        if warnings.is_empty() {
            // No changes needed
            return Ok(ctx.content.to_string());
        }

        // Collect fixes and sort by position
        let mut fixes: Vec<&Fix> = Vec::new();
        for warning in &warnings {
            if let Some(ref fix) = warning.fix {
                fixes.push(fix);
            }
        }
        fixes.sort_by_key(|f| f.range.start);

        let mut result = String::new();
        let mut last_pos = 0;
        let content_bytes = ctx.content.as_bytes();

        for fix in fixes {
            // Add content before the fix
            if last_pos < fix.range.start {
                let chunk = &content_bytes[last_pos..fix.range.start];
                result.push_str(
                    std::str::from_utf8(chunk).map_err(|_| LintError::InvalidInput("Invalid UTF-8".to_string()))?,
                );
            }
            // Add the replacement
            result.push_str(&fix.replacement);
            last_pos = fix.range.end;
        }

        // Add remaining content
        if last_pos < content_bytes.len() {
            let chunk = &content_bytes[last_pos..];
            result.push_str(
                std::str::from_utf8(chunk).map_err(|_| LintError::InvalidInput("Invalid UTF-8".to_string()))?,
            );
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_lists()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD029Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;
        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD029Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD029Config>(config);
        Box::new(MD029OrderedListPrefix::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        // Test with default style (ordered)
        let rule = MD029OrderedListPrefix::default();

        // Test with correctly ordered list
        let content = "1. First item\n2. Second item\n3. Third item";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with incorrectly ordered list
        let content = "1. First item\n3. Third item\n5. Fifth item";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should have warnings for items 3 and 5

        // Test with one-one style
        let rule = MD029OrderedListPrefix::new(ListStyle::OneOne);
        let content = "1. First item\n2. Second item\n3. Third item";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should have warnings for items 2 and 3

        // Test with ordered0 style
        let rule = MD029OrderedListPrefix::new(ListStyle::Ordered0);
        let content = "0. First item\n1. Second item\n2. Third item";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_redundant_computation_fix() {
        // This test confirms that the redundant computation bug is fixed
        // Previously: get_list_number() was called twice (once for is_some(), once for unwrap())
        // Now: get_list_number() is called once with if let pattern

        let rule = MD029OrderedListPrefix::default();

        // Test with mixed valid and edge case content
        let content = "1. First item\n3. Wrong number\n2. Another wrong number";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        // This should not panic and should produce warnings for incorrect numbering
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should have warnings for items 3 and 2

        // Verify the warnings have correct content
        assert!(result[0].message.contains("3") && result[0].message.contains("expected 2"));
        assert!(result[1].message.contains("2") && result[1].message.contains("expected 3"));
    }

    #[test]
    fn test_performance_improvement() {
        // This test verifies that the fix improves performance by avoiding redundant calls
        let rule = MD029OrderedListPrefix::default();

        // Create a larger list to test performance
        let mut content = String::new();
        for i in 1..=100 {
            content.push_str(&format!("{}. Item {}\n", i + 1, i)); // All wrong numbers
        }

        let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);

        // This should complete without issues and produce warnings for all items
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 100); // Should have warnings for all 100 items

        // Verify first and last warnings
        assert!(result[0].message.contains("2") && result[0].message.contains("expected 1"));
        assert!(result[99].message.contains("101") && result[99].message.contains("expected 100"));
    }

    #[test]
    fn test_one_or_ordered_with_all_ones() {
        // Test OneOrOrdered style with all 1s (should pass)
        let rule = MD029OrderedListPrefix::new(ListStyle::OneOrOrdered);

        let content = "1. First item\n1. Second item\n1. Third item";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "All ones should be valid in OneOrOrdered mode");
    }

    #[test]
    fn test_one_or_ordered_with_sequential() {
        // Test OneOrOrdered style with sequential numbering (should pass)
        let rule = MD029OrderedListPrefix::new(ListStyle::OneOrOrdered);

        let content = "1. First item\n2. Second item\n3. Third item";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Sequential numbering should be valid in OneOrOrdered mode"
        );
    }

    #[test]
    fn test_one_or_ordered_with_mixed_style() {
        // Test OneOrOrdered style with mixed numbering (should fail)
        let rule = MD029OrderedListPrefix::new(ListStyle::OneOrOrdered);

        let content = "1. First item\n2. Second item\n1. Third item";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Mixed style should produce one warning");
        assert!(result[0].message.contains("1") && result[0].message.contains("expected 3"));
    }

    #[test]
    fn test_one_or_ordered_separate_lists() {
        // Test OneOrOrdered with separate lists using different styles (should pass)
        let rule = MD029OrderedListPrefix::new(ListStyle::OneOrOrdered);

        let content = "# First list\n\n1. Item A\n1. Item B\n\n# Second list\n\n1. Item X\n2. Item Y\n3. Item Z";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Separate lists can use different styles in OneOrOrdered mode"
        );
    }
}
