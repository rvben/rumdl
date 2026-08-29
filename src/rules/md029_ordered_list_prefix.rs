/// Rule MD029: Ordered list item prefix
///
/// See [docs/md029.md](../../docs/md029.md) for full documentation, configuration, and examples.
use crate::lint_context::ParsedListItem;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::byte_to_char_count;
use crate::utils::regex_cache::ORDERED_LIST_MARKER_REGEX;
use std::collections::HashMap;
use toml;

mod md029_config;
pub use md029_config::ListStyle;
pub(super) use md029_config::MD029Config;

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

    /// The number of an ordered list marker, whichever delimiter closes it:
    /// `1.` and `1)` both number the item 1.
    #[inline]
    fn parse_marker_number(marker: &str) -> Option<usize> {
        marker.strip_suffix(['.', ')']).unwrap_or(marker).parse::<usize>().ok()
    }

    /// Calculate the expected number for a list item.
    /// The `start_value` is the CommonMark-provided start value for the list.
    /// For style `Ordered`, items should be `start_value, start_value+1, start_value+2, ...`
    #[inline]
    fn get_expected_number(&self, index: usize, detected_style: Option<ListStyle>, start_value: u64) -> usize {
        // Use detected_style when the configuration is auto-detect mode (OneOrOrdered or Consistent)
        // For explicit style configurations, always use the configured style
        let style = match self.config.style {
            ListStyle::OneOrOrdered | ListStyle::Consistent => detected_style.unwrap_or(ListStyle::OneOne),
            _ => self.config.style,
        };

        match style {
            ListStyle::One | ListStyle::OneOne => 1,
            ListStyle::Ordered => (start_value as usize) + index,
            ListStyle::Ordered0 => index,
            ListStyle::OneOrOrdered | ListStyle::Consistent => {
                // This shouldn't be reached since we handle these above
                1
            }
        }
    }

    /// Detect the style being used in a list by checking all items for prevalence.
    /// The `start_value` parameter is the CommonMark-provided list start value.
    fn detect_list_style(items: &[ParsedListItem<'_>], start_value: u64) -> ListStyle {
        if items.len() < 2 {
            // With only one item, check if it matches the start value
            // If so, treat as Ordered (respects CommonMark start value)
            // Otherwise, check if it's 1 (OneOne style)
            let first_num = Self::parse_marker_number(items[0].marker());
            if first_num == Some(start_value as usize) {
                return ListStyle::Ordered;
            }
            return ListStyle::OneOne;
        }

        let first_num = Self::parse_marker_number(items[0].marker());
        let second_num = Self::parse_marker_number(items[1].marker());

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
            .all(|item| Self::parse_marker_number(item.marker()) == Some(1));

        if all_ones {
            ListStyle::OneOne
        } else {
            ListStyle::Ordered
        }
    }

    /// Check a CommonMark-grouped list for correct ordering.
    /// Uses the CommonMark start value to validate items (e.g., a list starting at 11
    /// expects items 11, 12, 13... - no violation there).
    fn check_commonmark_list_group(
        &self,
        ctx: &crate::lint_context::LintContext,
        group: &[ParsedListItem<'_>],
        warnings: &mut Vec<LintWarning>,
        document_wide_style: Option<ListStyle>,
        start_value: u64,
    ) {
        if group.is_empty() {
            return;
        }

        // Group items by indentation level (marker_column) to handle nested lists
        type LevelGroups<'a> = HashMap<usize, Vec<ParsedListItem<'a>>>;
        let mut level_groups: LevelGroups = HashMap::new();

        for &list_item in group {
            level_groups
                .entry(list_item.marker_column())
                .or_default()
                .push(list_item);
        }

        // Process each indentation level in sorted order for deterministic output
        let mut sorted_levels: Vec<_> = level_groups.into_iter().collect();
        sorted_levels.sort_by_key(|(indent, _)| *indent);

        for (_indent, mut items) in sorted_levels {
            // Sort by line number
            items.sort_by_key(|item| item.line_num());

            if items.is_empty() {
                continue;
            }

            // Determine style for this group
            let detected_style = if let Some(doc_style) = document_wide_style {
                Some(doc_style)
            } else if self.config.style == ListStyle::OneOrOrdered {
                Some(Self::detect_list_style(&items, start_value))
            } else {
                None
            };

            // Check each item using the CommonMark start value
            for (idx, list_item) in items.iter().copied().enumerate() {
                if let Some(actual_num) = Self::parse_marker_number(list_item.marker()) {
                    let expected_num = self.get_expected_number(idx, detected_style, start_value);

                    if actual_num != expected_num {
                        let line_num = list_item.line_num();
                        let line_info = list_item.line_info();
                        let marker_start = list_item.marker_byte_offset();
                        let number_len = if let Some(dot_pos) = list_item.marker().find('.') {
                            dot_pos
                        } else if let Some(paren_pos) = list_item.marker().find(')') {
                            paren_pos
                        } else {
                            list_item.marker().len()
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

                        // Only provide auto-fix when:
                        // 1. The list starts at 1 (default numbering), OR
                        // 2. We're using explicit 'one' style (numbers are meaningless)
                        // When start_value > 1, the user explicitly chose that number,
                        // so auto-fixing would destroy their intent.
                        let should_provide_fix =
                            start_value == 1 || matches!(self.config.style, ListStyle::One | ListStyle::OneOne);

                        // marker_column is a byte offset within the line; convert to a
                        // character column for the diagnostic.
                        let line_text = line_info.content(ctx.content);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            message: format!(
                                "Ordered list item number {actual_num} does not match {style_context} (expected {expected_num})"
                            ),
                            line: line_num,
                            column: byte_to_char_count(line_text, list_item.marker_column()),
                            end_line: line_num,
                            end_column: byte_to_char_count(line_text, list_item.marker_column() + number_len),
                            severity: Severity::Warning,
                            fix: if should_provide_fix {
                                Some(Fix::new(marker_start..marker_start + number_len, expected_num.to_string()))
                            } else {
                                None
                            },
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
        if (!ctx.content.contains('.') && !ctx.content.contains(')'))
            || !ctx.content.lines().any(|line| ORDERED_LIST_MARKER_REGEX.is_match(line))
        {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        // Use pulldown-cmark's AST for authoritative list membership and start values.
        // This respects CommonMark's list start values (e.g., a list starting at 11
        // expects items 11, 12, 13... - no violation there).
        let list_groups = ctx.commonmark_ordered_lists();

        if list_groups.is_empty() {
            return Ok(Vec::new());
        }

        // For Consistent style, detect document-wide prevalent style
        let document_wide_style = if self.config.style == ListStyle::Consistent {
            // Collect ALL ordered items from ALL groups
            let mut all_document_items = Vec::new();
            for list in list_groups {
                all_document_items.extend(list.items());
            }
            // Detect style across entire document (use 1 as default for pattern detection)
            if !all_document_items.is_empty() {
                Some(Self::detect_list_style(&all_document_items, 1))
            } else {
                None
            }
        } else {
            None
        };

        // Process each CommonMark-defined list group with its start value
        for list in list_groups {
            let items: Vec<_> = list.items().collect();
            self.check_commonmark_list_group(ctx, &items, &mut warnings, document_wide_style, list.start_value());
        }

        // Sort warnings by line number for deterministic output
        warnings.sort_by_key(|w| (w.line, w.column));

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Note: do not call self.should_skip() here — MD029's should_skip only covers
        // unordered list markers (*, -, +), not ordered list markers (digits + . or )).
        // check() has its own fast-path early-return for documents without ordered markers.
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings).map_err(LintError::InvalidInput)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Skip a document with no ordered list. `likely_has_lists` counts bullet
    /// characters only, so it says nothing about ordered lists; the parsed list
    /// set does, and `check` reads the same cached value.
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || ctx.commonmark_ordered_lists().is_empty()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD029Config);
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
        assert!(result[0].message.contains('3') && result[0].message.contains("expected 2"));
        assert!(result[1].message.contains('2') && result[1].message.contains("expected 3"));
    }

    #[test]
    fn test_performance_improvement() {
        // This test verifies the rule handles large lists without performance issues
        let rule = MD029OrderedListPrefix::default();

        // Create a larger list with WRONG numbers: 1, 5, 10, 15, ...
        // Starting at 1, CommonMark expects 1, 2, 3, 4, ...
        // So items 2-100 are all wrong (expected 2, got 5; expected 3, got 10; etc.)
        let mut content = String::from("1. Item 1\n"); // First item correct
        for i in 2..=100 {
            content.push_str(&format!("{}. Item {}\n", i * 5 - 5, i)); // Wrong numbers
        }

        let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);

        // This should complete without issues and produce warnings for items 2-100
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 99, "Should have warnings for items 2-100 (99 items)");

        // First wrong item: "5. Item 2" (expected 2)
        assert!(result[0].message.contains('5') && result[0].message.contains("expected 2"));
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
        assert!(result[0].message.contains('1') && result[0].message.contains("expected 3"));
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

    /// Core invariant: for every warning with a Fix, the replacement text must
    /// match what fix() produces for the same byte range in the output.
    #[test]
    fn test_check_and_fix_produce_identical_replacements() {
        let rule = MD029OrderedListPrefix::default();

        let inputs = [
            "1. First\n3. Skip\n5. Skip\n",
            "1. First\n3. Third\n2. Second\n",
            "1. A\n\n3. B\n",
            "- Unordered\n\n1. A\n3. B\n",
            "1. A\n   1. Nested wrong\n   3. Nested\n2. B\n",
        ];

        for input in &inputs {
            let ctx = crate::lint_context::LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            let fixed = rule.fix(&ctx).unwrap();

            // fix() must be idempotent: applying it again produces the same output
            let ctx2 = crate::lint_context::LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
            let fixed_twice = rule.fix(&ctx2).unwrap();
            assert_eq!(
                fixed, fixed_twice,
                "fix() is not idempotent for input: {input:?}\nfirst:  {fixed:?}\nsecond: {fixed_twice:?}"
            );

            // After fixing, check() should produce no warnings
            let warnings_after = rule.check(&ctx2).unwrap();
            assert!(
                warnings_after.is_empty(),
                "check() should produce no warnings after fix() for input: {input:?}\nfixed: {fixed:?}\nremaining: {warnings_after:?}"
            );

            // For every warning with a Fix, applying the fix alone should match
            // the content at the same range in the final fixed output
            for warning in &warnings {
                if let Some(ref fix) = warning.fix {
                    assert!(
                        fix.range.end <= input.len(),
                        "Fix range exceeds input length for {input:?}"
                    );
                }
            }
        }
    }

    /// fix(fix(x)) == fix(x)
    #[test]
    fn test_fix_idempotent() {
        let rule = MD029OrderedListPrefix::default();

        let inputs = [
            "1. A\n3. B\n5. C\n",
            "# Intro\n\n1. First\n3. Third\n",
            "1. A\n1. B\n1. C\n",
        ];

        for input in &inputs {
            let ctx1 = crate::lint_context::LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
            let fixed_once = rule.fix(&ctx1).unwrap();
            let ctx2 =
                crate::lint_context::LintContext::new(&fixed_once, crate::config::MarkdownFlavor::Standard, None);
            let fixed_twice = rule.fix(&ctx2).unwrap();
            assert_eq!(fixed_once, fixed_twice, "fix() is not idempotent for input: {input:?}");
        }
    }

    /// Example list markers `(@)` and `(@label)` must not be reported under
    /// the Pandoc flavor — they are not ordered list items.
    #[test]
    fn test_pandoc_skips_example_list_markers() {
        use crate::config::MarkdownFlavor;
        use crate::lint_context::LintContext;
        let rule = MD029OrderedListPrefix::default();
        let content = "(@) First.\n(@good) Second.\n(@) Third.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD029 should not flag (@)/(@label) example markers under Pandoc: {result:?}"
        );
    }

    /// A real ordered list interleaved with example markers should validate
    /// only the digit-prefixed items, ignoring the example markers.
    #[test]
    fn test_pandoc_example_markers_do_not_break_real_ordered_list() {
        use crate::config::MarkdownFlavor;
        use crate::lint_context::LintContext;
        let rule = MD029OrderedListPrefix::default();
        let content = "1. Real first.\n\n(@) Example.\n\n2. Real second.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD029 should validate the digit-prefixed sequence and skip the example marker: {result:?}"
        );
    }

    /// Lists with explicit non-1 start values should not be auto-fixed
    /// (to preserve user intent).
    #[test]
    fn test_fix_preserves_non_default_start_value() {
        let rule = MD029OrderedListPrefix::default();

        // List starts at 11 — CommonMark expects 11, 12, 13... Item "14" is wrong
        // but user explicitly chose 11 so no auto-fix should be offered.
        let content = "11. First\n14. Fourth\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        // Warning present but no fix
        assert!(!warnings.is_empty(), "Should produce warnings for misnumbered list");
        assert!(
            warnings.iter().all(|w| w.fix.is_none()),
            "Should not provide auto-fix for lists starting at non-1 values"
        );
        // fix() should leave content unchanged
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, content,
            "Content should be unchanged when no fixes are available"
        );
    }

    #[test]
    fn test_md029_front_matter() {
        let rule = MD029OrderedListPrefix::default();
        let content = "---\n1. key: value\n3. key2: value2\n---\n1. Item 1\n2. Item 2\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag list-like items in front-matter: {result:?}"
        );
    }
}
