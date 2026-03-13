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

use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::md066_footnote_validation::{
    FOOTNOTE_DEF_PATTERN, FOOTNOTE_REF_PATTERN, footnote_def_position, strip_blockquote_prefix,
};
use std::collections::HashMap;

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
        FixCapability::Unfixable
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

        // First pass: collect references in order of first occurrence
        for line_info in &ctx.lines {
            // Skip special contexts
            if line_info.in_code_block
                || line_info.in_front_matter
                || line_info.in_html_comment
                || line_info.in_html_block
            {
                continue;
            }

            let line = line_info.content(ctx.content);

            for caps in FOOTNOTE_REF_PATTERN.captures_iter(line) {
                if let Some(id_match) = caps.get(1) {
                    // Skip if this is a footnote definition (at line start with 0-3 spaces indent)
                    // Also handle blockquote prefixes (e.g., "> [^id]:")
                    let full_match = caps.get(0).unwrap();
                    if line.as_bytes().get(full_match.end()) == Some(&b':') {
                        let before_match = &line[..full_match.start()];
                        if before_match.chars().all(|c| c == ' ' || c == '>') {
                            continue;
                        }
                    }

                    let id = id_match.as_str().to_lowercase();

                    // Check if this match is inside a code span
                    let match_start = full_match.start();
                    let byte_offset = line_info.byte_offset + match_start;

                    let in_code_span = ctx.is_in_code_span_byte(byte_offset);

                    if !in_code_span && !seen_refs.contains_key(&id) {
                        seen_refs.insert(id.clone(), reference_order.len());
                        reference_order.push(id);
                    }
                }
            }
        }

        // Second pass: collect definitions in document order
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            // Skip special contexts
            if line_info.in_code_block
                || line_info.in_front_matter
                || line_info.in_html_comment
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
                            .map(|li| footnote_def_position(li.content(ctx.content)))
                            .unwrap_or((1, 1));
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

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Auto-fix would require reordering definitions which is complex
        // and could break multi-paragraph footnotes
        Ok(ctx.content.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LintContext;

    fn check(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        MD067FootnoteDefinitionOrder::new().check(&ctx).unwrap()
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
        // "[^2]:" is 5 chars
        assert_eq!(warnings[0].end_column, 6);
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
}
