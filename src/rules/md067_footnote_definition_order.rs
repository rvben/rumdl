//! MD067: Footnote definitions should appear in order of first reference
//!
//! This rule enforces that footnote definitions appear in the same order
//! as their first references in the document. Out-of-order footnotes
//! can confuse readers.
//!
//! ## Example
//!
//! ### Incorrect
//! ```markdown
//! Text with [^2] and then [^1].
//!
//! [^1]: First definition
//! [^2]: Second definition
//! ```
//!
//! ### Correct
//! ```markdown
//! Text with [^2] and then [^1].
//!
//! [^2]: Referenced first
//! [^1]: Referenced second
//! ```

use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::md066_footnote_validation::{FOOTNOTE_DEF_PATTERN, footnote_def_position, strip_blockquote_prefix};
use crate::utils::fix_utils::{apply_warning_fixes, filter_warnings_by_inline_config};
use pulldown_cmark::{Event, Parser, Tag};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default, Clone)]
pub struct MD067FootnoteDefinitionOrder;

impl MD067FootnoteDefinitionOrder {
    pub fn new() -> Self {
        Self
    }
}

impl Rule for MD067FootnoteDefinitionOrder {
    fn name(&self) -> &'static str {
        "MD067"
    }

    fn description(&self) -> &'static str {
        "Footnote definitions should appear in order of first reference"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Other
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::ConditionallyFixable
    }

    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        Some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(67))
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.content.contains("[^")
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Track first reference position for each footnote ID
        let mut reference_order: Vec<String> = Vec::new();
        let mut seen_refs: HashMap<String, usize> = HashMap::new();

        // Track definition positions
        let mut definition_order: Vec<(String, usize, usize)> = Vec::new(); // (id, line, byte_offset)

        // Parser references exclude escapes, code, and missing definitions. Missing
        // definitions belong to MD066 and must not shift the expected order here.
        for reference in ctx.footnote_references() {
            // The shared parser can still expose references in front matter
            // or flavor-specific non-Markdown regions. Preserve those exclusions.
            let line = &ctx.lines[reference.line - 1];
            if line.in_code_block
                || line.in_front_matter
                || line.in_html_comment
                || line.in_mdx_comment
                || line.in_html_block
            {
                continue;
            }
            let id = reference.id.to_lowercase();
            if !seen_refs.contains_key(&id) {
                seen_refs.insert(id.clone(), reference_order.len());
                reference_order.push(id);
            }
        }

        // Second pass: collect definitions in document order
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            // Skip special contexts
            if line_info.in_code_block
                || line_info.in_front_matter
                || line_info.in_html_comment
                || line_info.in_mdx_comment
                || line_info.in_html_block
            {
                continue;
            }

            let line = line_info.content(ctx.content);
            // Strip blockquote prefixes
            let line_stripped = strip_blockquote_prefix(line);

            if let Some(caps) = FOOTNOTE_DEF_PATTERN.captures(line_stripped)
                && let Some(id_match) = caps.get(1)
            {
                let id = id_match.as_str().to_lowercase();
                let line_num = line_idx + 1;
                definition_order.push((id, line_num, line_info.byte_offset));
            }
        }

        // Compare definition order against reference order
        let mut expected_idx = 0;
        for (def_id, def_line, _byte_offset) in &definition_order {
            // Find this definition's expected position based on reference order
            if let Some(&ref_idx) = seen_refs.get(def_id) {
                if ref_idx != expected_idx {
                    // Find what was expected
                    if expected_idx < reference_order.len() {
                        let expected_id = &reference_order[expected_idx];
                        let (col, end_col) = ctx
                            .lines
                            .get(*def_line - 1)
                            .map_or((1, 1), |li| footnote_def_position(li.content(ctx.content)));
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: *def_line,
                            column: col,
                            end_line: *def_line,
                            end_column: end_col,
                            message: format!(
                                "Footnote definition '[^{def_id}]' is out of order; expected '[^{expected_id}]' next (based on reference order)"
                            ),
                            severity: Severity::Warning,
                            fix: None,
                        });
                    }
                }
                expected_idx = ref_idx + 1;
            }
            // Definitions without references are handled by MD066, skip them here
        }

        // Ordering is one operation over the definitions, even when the fix
        // is unsafe. Retain that full diagnostic span for suppression/MD087.
        warnings.truncate(1);
        if let Some(warning) = warnings.first_mut() {
            let mut last = definition_order
                .iter()
                .rev()
                .find(|(id, _, _)| seen_refs.contains_key(id))
                .unwrap()
                .1
                - 1;
            while let Some(line) = ctx.lines.get(last + 1) {
                if !line.in_footnote_definition
                    || FOOTNOTE_DEF_PATTERN.is_match(strip_blockquote_prefix(line.content(ctx.content)))
                {
                    break;
                }
                last += 1;
            }
            warning.end_line = last + 1;
            warning.end_column = ctx.lines[warning.end_line - 1].content(ctx.content).chars().count() + 1;
            warning.fix = reordering_fix(ctx, &definition_order, &seen_refs);
        }
        Ok(warnings)
    }

    fn fixes_from_diagnostics(&self) -> bool {
        true
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let warnings = filter_warnings_by_inline_config(self.check(ctx)?, ctx.inline_config(), self.name());
        apply_warning_fixes(ctx.content, &warnings).map_err(LintError::FixFailed)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD067FootnoteDefinitionOrder)
    }
}

/// Reorder complete parser-owned definitions in their existing source slots.
/// Separators and unrelated blocks stay in place; every replacement is atomic.
fn reordering_fix(
    ctx: &crate::lint_context::LintContext,
    definitions: &[(String, usize, usize)],
    reference_order: &HashMap<String, usize>,
) -> Option<Fix> {
    let mut unique = HashSet::new();
    if definitions.iter().any(|(id, _, _)| !unique.insert(id)) {
        return None;
    }

    let mut parents = Vec::new();
    let mut parsed = HashMap::new();
    let options = crate::utils::parser_options::rumdl_parser_options_for(ctx.content);
    for (event, range) in Parser::new_ext(ctx.content, options).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if let Tag::FootnoteDefinition(id) = &tag {
                    let first = ctx.lines.partition_point(|line| line.byte_offset <= range.start) - 1;
                    let mut last = ctx.lines.partition_point(|line| line.byte_offset < range.end) - 1;
                    // Blank separators belong to their slots, not the definition
                    // that happened to precede them (including quoted blanks).
                    while last > first
                        && strip_blockquote_prefix(ctx.lines[last].content(ctx.content))
                            .trim_matches([' ', '\t', '\r'])
                            .is_empty()
                    {
                        last -= 1;
                    }
                    let span = ctx.lines[first].byte_offset
                        ..ctx.lines[last].byte_offset + ctx.lines[last].content(ctx.content).len();
                    parsed.insert((id.to_lowercase(), first + 1), (span, parents.clone()));
                }
                parents.push(range.start);
            }
            Event::End(_) => {
                parents.pop();
            }
            _ => {}
        }
    }

    let mut slots = Vec::new();
    let mut container = None;
    for (id, line, _) in definitions {
        let Some(&rank) = reference_order.get(id) else { continue };
        let (range, parents) = parsed.get(&(id.clone(), *line))?;
        let last_line = ctx.lines.partition_point(|line| line.byte_offset < range.end);
        if let Some(following) = ctx.lines.get(last_line) {
            let text = strip_blockquote_prefix(following.content(ctx.content));
            if !text.trim_matches([' ', '\t', '\r']).is_empty() && !FOOTNOTE_DEF_PATTERN.is_match(text) {
                // A paragraph moved into a slot ending in code could absorb
                // the following prose as a lazy footnote continuation.
                return None;
            }
        }
        let first_line = ctx.lines[*line - 1].content(ctx.content);
        let prefix = &first_line[..first_line.find("[^")?];
        let scope = (parents, prefix);
        if container.is_some_and(|existing| existing != scope) {
            // Different containers or indentation can change whether a
            // definition belongs to an adjacent list after it moves.
            return None;
        }
        container = Some(scope);
        let next = ctx
            .footnote_references()
            .partition_point(|reference| reference.byte_offset < range.start);
        if ctx
            .footnote_references()
            .get(next)
            .is_some_and(|reference| reference.byte_offset < range.end)
        {
            // Moving references could change the order again on the next pass.
            return None;
        }
        slots.push((range.clone(), rank));
    }
    let first_start = slots.first()?.0.start;
    let first_line = ctx.lines.partition_point(|line| line.byte_offset < first_start);
    // A preceding next-line directive must stay attached to the same content,
    // including directives belonging to other tools or other rumdl rules.
    let span = ctx.lines[first_line.saturating_sub(1)].byte_offset..slots.last()?.0.end;
    if ctx
        .html_comment_ranges()
        .iter()
        .any(|comment| comment.start < span.end && comment.end > span.start)
        || ctx
            .mdx_comment_ranges()
            .iter()
            .any(|&(start, end)| start < span.end && end > span.start)
        || ctx.overlaps_mdx_code(span.start, span.end)
    {
        // Comments may carry directives whose scope must not change. Native
        // MDX syntax can also end a definition that pulldown considers open.
        return None;
    }
    let mut sorted: Vec<_> = slots.iter().collect();
    sorted.sort_unstable_by_key(|(_, rank)| *rank);
    let changed: Vec<_> = slots
        .iter()
        .zip(sorted)
        .filter(|((destination, _), (source, _))| destination != source)
        .collect();
    let start = changed.first()?.0.0.start;
    let end = changed.last()?.0.0.end;
    let mut replacement = String::with_capacity(end - start);
    let mut cursor = start;
    for ((destination, _), (source, _)) in changed {
        replacement.push_str(&ctx.content[cursor..destination.start]);
        replacement.push_str(&ctx.content[source.clone()]);
        cursor = destination.end;
    }
    // A reorder must survive or be skipped as a whole when another rule edits
    // one of these definitions. Separate edits could lose or duplicate content.
    Some(Fix::new(start..end, replacement))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LintContext;

    fn check(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        MD067FootnoteDefinitionOrder::new().check(&ctx).unwrap()
    }

    #[test]
    fn test_fix_reorders_complete_footnotes_atomically() {
        let input = "Hello [^1].\nWorld [^2].\n\n[^2]: Second footnote.\n\n    Another paragraph.\n\n    ```rust\n    code();\n    ```\n\n[^1]: First footnote.\n";
        let expected = "Hello [^1].\nWorld [^2].\n\n[^1]: First footnote.\n\n[^2]: Second footnote.\n\n    Another paragraph.\n\n    ```rust\n    code();\n    ```\n";
        let rule = MD067FootnoteDefinitionOrder::new();
        let ctx = LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
        assert_eq!(
            crate::utils::fix_utils::apply_warning_fixes(input, &warnings).unwrap(),
            expected
        );
        let fixed_ctx = LintContext::new(expected, crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
    }

    #[test]
    fn test_correct_order() {
        let content = r#"Text with [^1] and [^2].

[^1]: First definition
[^2]: Second definition
"#;
        let warnings = check(content);
        assert!(warnings.is_empty(), "Expected no warnings for correct order");
    }

    #[test]
    fn test_incorrect_order() {
        let content = r#"Text with [^1] and [^2].

[^2]: Second definition
[^1]: First definition
"#;
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("out of order"));
        assert!(warnings[0].message.contains("[^2]"));
    }

    #[test]
    fn test_named_footnotes_order() {
        let content = r#"Text with [^alpha] and [^beta].

[^beta]: Beta definition
[^alpha]: Alpha definition
"#;
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("[^beta]"));
    }

    #[test]
    fn test_multiple_refs_same_footnote() {
        let content = r#"Text with [^1] and [^2] and [^1] again.

[^1]: First footnote
[^2]: Second footnote
"#;
        let warnings = check(content);
        assert!(
            warnings.is_empty(),
            "Multiple refs to same footnote should use first occurrence"
        );
    }

    #[test]
    fn test_skip_code_blocks() {
        let content = r#"Text with [^1].

```
[^2]: In code block
```

[^1]: Real definition
"#;
        let warnings = check(content);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_skip_code_spans() {
        let content = r#"Text with `[^2]` in code and [^1].

[^1]: Only real reference
"#;
        let warnings = check(content);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_case_insensitive() {
        let content = r#"Text with [^Note] and [^OTHER].

[^note]: First (case-insensitive match)
[^other]: Second
"#;
        let warnings = check(content);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_definitions_without_references() {
        // Orphaned definitions are handled by MD066, not this rule
        let content = r#"Text with [^1].

[^1]: Referenced
[^2]: Orphaned
"#;
        let warnings = check(content);
        assert!(warnings.is_empty(), "Orphaned definitions handled by MD066");
    }

    #[test]
    fn test_three_footnotes_wrong_order() {
        let content = r#"Ref [^a], then [^b], then [^c].

[^c]: Third ref, first def
[^a]: First ref, second def
[^b]: Second ref, third def
"#;
        let warnings = check(content);
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_blockquote_definitions() {
        let content = r#"Text with [^1] and [^2].

> [^1]: First in blockquote
> [^2]: Second in blockquote
"#;
        let warnings = check(content);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_midline_footnote_ref_with_colon_counted_for_ordering() {
        // Mid-line [^a]: should count as a reference for ordering purposes
        let content = "# Test\n\nSecond ref [^b] here.\n\nFirst ref [^a]: and text.\n\n[^a]: First definition.\n[^b]: Second definition.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD067FootnoteDefinitionOrder;
        let result = rule.check(&ctx).unwrap();
        // Reference order is [^b] then [^a], but definitions are [^a] then [^b]
        assert!(!result.is_empty(), "Should detect ordering mismatch: {result:?}");
    }

    #[test]
    fn test_linestart_footnote_def_not_counted_as_reference_for_ordering() {
        // [^a]: at line start is a definition, not a reference
        let content = "# Test\n\n[^a] first ref.\n[^b] second ref.\n\n[^a]: First.\n[^b]: Second.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD067FootnoteDefinitionOrder;
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Correct order should pass: {result:?}");
    }

    // ==================== Warning position tests ====================

    #[test]
    fn test_out_of_order_column_position() {
        let content = "Text with [^1] and [^2].\n\n[^2]: Second definition\n[^1]: First definition\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
        assert_eq!(warnings[0].column, 1, "Definition at start of line");
        // The atomic reorder spans both definitions, so a disable on either
        // definition suppresses the complete operation.
        assert_eq!(warnings[0].end_line, 4);
        assert_eq!(warnings[0].end_column, 23);
    }

    #[test]
    fn test_out_of_order_blockquote_column_position() {
        let content = "Text with [^1] and [^2].\n\n> [^2]: Second in blockquote\n> [^1]: First in blockquote\n";
        let warnings = check(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
        // After "> " prefix (2 chars), definition starts at column 3
        assert_eq!(warnings[0].column, 3, "Should point past blockquote prefix");
    }

    #[test]
    fn test_fix_preserves_source_slots_and_reference_semantics() {
        for (input, expected) in [
            (
                "Refs[^a][^b][^c].\n\n[^c]: C\n[^a]: A\n[^b]: B\n",
                "Refs[^a][^b][^c].\n\n[^a]: A\n[^b]: B\n[^c]: C\n",
            ),
            (
                "Refs[^a][^b].\n\n[^b]: B\n\nKeep this paragraph.\n\n[^unused]: U\n\n[^a]: A\n",
                "Refs[^a][^b].\n\n[^a]: A\n\nKeep this paragraph.\n\n[^unused]: U\n\n[^b]: B\n",
            ),
            (
                "Refs[^é][^β].\r\n\r\n[^β]: 第二\r\n[^é]: café",
                "Refs[^é][^β].\r\n\r\n[^é]: café\r\n[^β]: 第二",
            ),
            (
                "Refs[^a][^b].\n\n> [^b]: B\n>\n>     Continued B.\n>\n> [^a]: A\n",
                "Refs[^a][^b].\n\n> [^a]: A\n>\n> [^b]: B\n>\n>     Continued B.\n",
            ),
            (
                "Refs[^a][^b].\n\n  [^b]: B\n  [^a]: A\n",
                "Refs[^a][^b].\n\n  [^a]: A\n  [^b]: B\n",
            ),
            (
                "Escaped \\[^b], missing[^missing], then real[^a][^b].\n\n[^b]: B\n[^a]: A\n",
                "Escaped \\[^b], missing[^missing], then real[^a][^b].\n\n[^a]: A\n[^b]: B\n",
            ),
        ] {
            let rule = MD067FootnoteDefinitionOrder::new();
            let ctx = LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, expected, "{input:?}");
            assert_eq!(
                crate::utils::fix_utils::apply_warning_fixes(input, &rule.check(&ctx).unwrap()).unwrap(),
                expected
            );
            let ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{fixed:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_fix_leaves_ambiguous_and_disabled_definitions_untouched() {
        for input in [
            "Refs[^a][^b].\n\n- item\n[^b]: B\n  [^a]: A\n",
            "Refs[^a][^b].\n\n[^b]: B\n\n    ```\n    code\n    ```\nParagraph after b.\n\n[^a]: A\n",
            "Refs[^a][^b].\n\n<!-- rumdl-disable-next-line MD013 -->\n[^b]: Long definition.\n[^a]: A\n",
            "Refs[^a][^b].\n\n[^b]: B\n[^a]: A\n[^a]: Duplicate A\n",
            "Refs[^a][^b].\n\n> [^b]: B\n\n> [^a]: A\n",
            "Refs[^a][^b].\n\n[^b]: B\n<!-- keep -->\n[^a]: A\n",
            "Refs[^a][^b].\n\n[^b]: B refers to [^a].\n[^a]: A\n",
            "<!-- rumdl-disable MD067 -->\nRefs[^a][^b].\n\n[^b]: B\n[^a]: A\n",
            "Refs[^a][^b].\n\n[^b]: B\n<!-- rumdl-disable-next-line MD067 -->\n[^a]: A\n",
        ] {
            let rule = MD067FootnoteDefinitionOrder::new();
            let ctx = LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
            assert!(
                !rule.check(&ctx).unwrap().is_empty(),
                "retain suppression evidence: {input:?}"
            );
            assert_eq!(rule.fix(&ctx).unwrap(), input);
        }
    }

    #[test]
    fn test_disable_on_later_definition_is_accounted_for() {
        let input = "Refs[^a][^b].\n\n[^b]: B\n<!-- rumdl-disable-next-line MD067 -->\n[^a]: A\n";
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MD067FootnoteDefinitionOrder::new()),
            Box::new(crate::rules::MD087UnusedDisableComment),
        ];
        let warnings = crate::lint(
            input,
            &rules,
            false,
            crate::config::MarkdownFlavor::Standard,
            None,
            None,
        )
        .unwrap();
        assert!(
            warnings.is_empty(),
            "the ordering operation spans both definitions: {warnings:?}"
        );
    }

    #[test]
    fn test_fix_preserves_nonbreaking_space_inside_a_definition() {
        let input = "Refs[^a][^b].\n\n[^b]: B\n\n    \u{a0}\n\n[^a]: A\n";
        let expected = "Refs[^a][^b].\n\n[^a]: A\n\n[^b]: B\n\n    \u{a0}\n";
        let rule = MD067FootnoteDefinitionOrder;
        let ctx = LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
    }

    #[test]
    fn test_real_reference_before_html_comment_controls_order() {
        let input = "Text [^b] <!--\nnote\n-->\nMore [^a] and [^b].\n\n[^b]: B\n[^a]: A\n";
        let ctx = LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD067FootnoteDefinitionOrder;
        assert!(rule.check(&ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), input);
    }

    #[test]
    fn test_fix_preserves_mdx_flow_syntax_next_to_definitions() {
        // ESM needs a blank separator: unlike expressions/JSX, it cannot
        // interrupt the preceding footnote paragraph.
        for syntax in ["{expr}", "<Widget />", "\nexport const value = 1;"] {
            let input = format!("Refs[^a][^b].\n\n[^b]: B\n{syntax}\n\n[^a]: A\n");
            let ctx = LintContext::new(&input, crate::config::MarkdownFlavor::MDX, None);
            let rule = MD067FootnoteDefinitionOrder;
            let warnings = rule.check(&ctx).unwrap();
            assert!(!warnings.is_empty(), "{syntax}");
            assert_eq!(rule.fix(&ctx).unwrap(), input, "{syntax}");
            assert!(warnings.iter().all(|warning| warning.fix.is_none()), "{syntax}");
        }
    }

    #[test]
    fn test_fix_preserves_mdx_directive_targets() {
        let input = "Refs[^a][^b].\n\n{/* rumdl-disable-next-line MD013 */}\n[^b]: Long definition.\n[^a]: A\n";
        let ctx = LintContext::new(input, crate::config::MarkdownFlavor::MDX, None);
        let rule = MD067FootnoteDefinitionOrder;
        assert!(!rule.check(&ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), input);
    }
}
