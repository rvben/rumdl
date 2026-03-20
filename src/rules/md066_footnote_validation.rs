use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Pattern to match footnote definitions: [^id]: content
/// Matches at start of line, with 0-3 leading spaces, caret in brackets
/// Also handles definitions inside blockquotes (after stripping > prefixes)
pub static FOOTNOTE_DEF_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ ]{0,3}\[\^([^\]]+)\]:").unwrap());

/// Pattern to match footnote references in text: [^id]
/// Callers must manually check that the match is NOT followed by `:` (which would make it a definition)
pub static FOOTNOTE_REF_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\^([^\]]+)\]").unwrap());

/// Strip blockquote prefixes from a line to check for footnote definitions
/// Handles nested blockquotes like `> > > ` and variations with/without spaces
pub fn strip_blockquote_prefix(line: &str) -> &str {
    let mut chars = line.chars().peekable();
    let mut last_content_start = 0;
    let mut pos = 0;

    while let Some(&c) = chars.peek() {
        match c {
            '>' => {
                chars.next();
                pos += 1;
                // Optionally consume one space after >
                if chars.peek() == Some(&' ') {
                    chars.next();
                    pos += 1;
                }
                last_content_start = pos;
            }
            ' ' => {
                // Allow leading spaces before >
                chars.next();
                pos += 1;
            }
            _ => break,
        }
    }

    &line[last_content_start..]
}

/// Find the (column, end_column) of a footnote definition marker `[^id]:` on a line.
/// Returns 1-indexed column positions pointing to `[^id]:`, not leading whitespace.
/// Handles blockquote prefixes and uses character counting for multi-byte support.
pub fn footnote_def_position(line: &str) -> (usize, usize) {
    let stripped = strip_blockquote_prefix(line);
    if let Some(caps) = FOOTNOTE_DEF_PATTERN.captures(stripped) {
        let prefix_chars = line.chars().count() - stripped.chars().count();
        let id_match = caps.get(1).unwrap();
        // `[^` is always 2 bytes before the ID capture group
        let bracket_byte_pos = id_match.start() - 2;
        let chars_before_bracket = stripped[..bracket_byte_pos].chars().count();
        let full_match_end = caps.get(0).unwrap().end();
        let marker_chars = stripped[bracket_byte_pos..full_match_end].chars().count();
        (
            prefix_chars + chars_before_bracket + 1,
            prefix_chars + chars_before_bracket + marker_chars + 1,
        )
    } else {
        (1, 1)
    }
}

/// Rule MD066: Footnote validation - ensure all footnote references have definitions and vice versa
///
/// This rule validates footnote usage in markdown documents:
/// - Detects orphaned footnote references (`[^1]`) without corresponding definitions
/// - Detects orphaned footnote definitions (`[^1]: text`) that are never referenced
///
/// Footnote syntax (common markdown extension, not part of CommonMark):
/// - Reference: `[^identifier]` in text
/// - Definition: `[^identifier]: definition text` (can span multiple lines with indentation)
///
/// ## Examples
///
/// **Valid:**
/// ```markdown
/// This has a footnote[^1] that is properly defined.
///
/// [^1]: This is the footnote content.
/// ```
///
/// **Invalid - orphaned reference:**
/// ```markdown
/// This references[^missing] a footnote that doesn't exist.
/// ```
///
/// **Invalid - orphaned definition:**
/// ```markdown
/// [^unused]: This footnote is defined but never referenced.
/// ```
#[derive(Debug, Clone, Default)]
pub struct MD066FootnoteValidation;

impl MD066FootnoteValidation {
    pub fn new() -> Self {
        Self
    }
}

impl Rule for MD066FootnoteValidation {
    fn name(&self) -> &'static str {
        "MD066"
    }

    fn description(&self) -> &'static str {
        "Footnote validation"
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

        // Early exit if no footnotes at all
        if ctx.footnote_refs.is_empty() && !ctx.content.contains("[^") {
            return Ok(warnings);
        }

        // Collect all footnote references (id is WITHOUT the ^ prefix)
        // Map from id -> list of (line, byte_offset) for each reference
        // Note: pulldown-cmark only finds references when definitions exist,
        // so we need to parse references directly to find orphaned ones
        let mut references: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

        // First, use pulldown-cmark's detected references (when definitions exist)
        for footnote_ref in &ctx.footnote_refs {
            // Skip if in code block, frontmatter, HTML comment, or HTML block
            if ctx.line_info(footnote_ref.line).is_some_and(|info| {
                info.in_code_block
                    || info.in_front_matter
                    || info.in_html_comment
                    || info.in_mdx_comment
                    || info.in_html_block
            }) {
                continue;
            }
            references
                .entry(footnote_ref.id.to_lowercase())
                .or_default()
                .push((footnote_ref.line, footnote_ref.byte_offset));
        }

        // Also parse references directly to find orphaned ones (without definitions)
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            // Skip if in code block, frontmatter, HTML comment, or HTML block
            if line_info.in_code_block
                || line_info.in_front_matter
                || line_info.in_html_comment
                || line_info.in_mdx_comment
                || line_info.in_html_block
            {
                continue;
            }

            let line = line_info.content(ctx.content);
            let line_num = line_idx + 1; // 1-indexed

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

                    if !in_code_span {
                        // Only add if not already found (avoid duplicates with pulldown-cmark)
                        references.entry(id).or_default().push((line_num, byte_offset));
                    }
                }
            }
        }

        // Deduplicate references (pulldown-cmark and regex might find the same ones)
        for occurrences in references.values_mut() {
            occurrences.sort();
            occurrences.dedup();
        }

        // Collect footnote definitions by parsing directly from content
        // Footnote definitions: [^id]: content (NOT in reference_defs which expects URLs)
        // Map from id (lowercase) -> list of (line, byte_offset) for duplicate detection
        let mut definitions: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            // Skip if in code block, frontmatter, HTML comment, or HTML block
            if line_info.in_code_block
                || line_info.in_front_matter
                || line_info.in_html_comment
                || line_info.in_mdx_comment
                || line_info.in_html_block
            {
                continue;
            }

            let line = line_info.content(ctx.content);
            // Strip blockquote prefixes to handle definitions inside blockquotes
            let line_stripped = strip_blockquote_prefix(line);

            if let Some(caps) = FOOTNOTE_DEF_PATTERN.captures(line_stripped)
                && let Some(id_match) = caps.get(1)
            {
                let id = id_match.as_str().to_lowercase();
                let line_num = line_idx + 1; // 1-indexed
                definitions
                    .entry(id)
                    .or_default()
                    .push((line_num, line_info.byte_offset));
            }
        }

        // Check for duplicate definitions
        for (def_id, occurrences) in &definitions {
            if occurrences.len() > 1 {
                // Report all duplicate definitions after the first one
                for (line, _byte_offset) in &occurrences[1..] {
                    let (col, end_col) = ctx
                        .lines
                        .get(*line - 1)
                        .map(|li| footnote_def_position(li.content(ctx.content)))
                        .unwrap_or((1, 1));
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: *line,
                        column: col,
                        end_line: *line,
                        end_column: end_col,
                        message: format!(
                            "Duplicate footnote definition '[^{def_id}]' (first defined on line {})",
                            occurrences[0].0
                        ),
                        severity: Severity::Error,
                        fix: None,
                    });
                }
            }
        }

        // Check for orphaned references (references without definitions)
        let defined_ids: HashSet<&String> = definitions.keys().collect();
        for (ref_id, occurrences) in &references {
            if !defined_ids.contains(ref_id) {
                // Report the first occurrence of each undefined reference
                let (line, byte_offset) = occurrences[0];
                // Compute character-based column from byte offset within the line.
                // Find the actual marker text in the source to get the real length,
                // since ref_id is lowercased and may differ from the original.
                let (col, end_col) = if let Some(line_info) = ctx.lines.get(line - 1) {
                    let line_content = line_info.content(ctx.content);
                    let byte_pos = byte_offset.saturating_sub(line_info.byte_offset);
                    let char_col = line_content.get(..byte_pos).map(|s| s.chars().count()).unwrap_or(0);
                    // Find the actual [^...] marker in the source at this position
                    let marker_chars = line_content
                        .get(byte_pos..)
                        .and_then(|rest| rest.find(']'))
                        .map(|end| line_content[byte_pos..byte_pos + end + 1].chars().count())
                        .unwrap_or_else(|| format!("[^{ref_id}]").chars().count());
                    (char_col + 1, char_col + marker_chars + 1)
                } else {
                    (1, 1)
                };
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line,
                    column: col,
                    end_line: line,
                    end_column: end_col,
                    message: format!("Footnote reference '[^{ref_id}]' has no corresponding definition"),
                    severity: Severity::Error,
                    fix: None,
                });
            }
        }

        // Check for orphaned definitions (definitions without references)
        let referenced_ids: HashSet<&String> = references.keys().collect();
        for (def_id, occurrences) in &definitions {
            if !referenced_ids.contains(def_id) {
                // Report the first definition location
                let (line, _byte_offset) = occurrences[0];
                let (col, end_col) = ctx
                    .lines
                    .get(line - 1)
                    .map(|li| footnote_def_position(li.content(ctx.content)))
                    .unwrap_or((1, 1));
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line,
                    column: col,
                    end_line: line,
                    end_column: end_col,
                    message: format!("Footnote definition '[^{def_id}]' is never referenced"),
                    severity: Severity::Error,
                    fix: None,
                });
            }
        }

        // Sort warnings by line number for consistent output
        warnings.sort_by_key(|w| w.line);

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // No automatic fix - user must decide what to do with orphaned footnotes
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD066FootnoteValidation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    fn check_md066(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        MD066FootnoteValidation::new().check(&ctx).unwrap()
    }

    // ==================== Valid cases ====================

    #[test]
    fn test_valid_single_footnote() {
        let content = "This has a footnote[^1].\n\n[^1]: The footnote content.";
        let warnings = check_md066(content);
        assert!(warnings.is_empty(), "Valid footnote should not warn: {warnings:?}");
    }

    #[test]
    fn test_valid_multiple_footnotes() {
        let content = r#"First footnote[^1] and second[^2].

[^1]: First definition.
[^2]: Second definition."#;
        let warnings = check_md066(content);
        assert!(warnings.is_empty(), "Valid footnotes should not warn: {warnings:?}");
    }

    #[test]
    fn test_valid_named_footnotes() {
        let content = r#"See the note[^note] and warning[^warning].

[^note]: This is a note.
[^warning]: This is a warning."#;
        let warnings = check_md066(content);
        assert!(warnings.is_empty(), "Named footnotes should not warn: {warnings:?}");
    }

    #[test]
    fn test_valid_footnote_used_multiple_times() {
        let content = r#"First[^1] and again[^1] and third[^1].

[^1]: Used multiple times."#;
        let warnings = check_md066(content);
        assert!(warnings.is_empty(), "Reused footnote should not warn: {warnings:?}");
    }

    #[test]
    fn test_valid_case_insensitive_matching() {
        let content = r#"Reference[^NOTE].

[^note]: Definition with different case."#;
        let warnings = check_md066(content);
        assert!(
            warnings.is_empty(),
            "Case-insensitive matching should work: {warnings:?}"
        );
    }

    #[test]
    fn test_no_footnotes_at_all() {
        let content = "Just regular markdown without any footnotes.";
        let warnings = check_md066(content);
        assert!(warnings.is_empty(), "No footnotes should not warn");
    }

    // ==================== Orphaned references ====================

    #[test]
    fn test_orphaned_reference_single() {
        let content = "This references[^missing] a non-existent footnote.";
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1, "Should detect orphaned reference");
        assert!(warnings[0].message.contains("missing"));
        assert!(warnings[0].message.contains("no corresponding definition"));
    }

    #[test]
    fn test_orphaned_reference_multiple() {
        let content = r#"First[^a], second[^b], third[^c].

[^b]: Only b is defined."#;
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 2, "Should detect 2 orphaned references: {warnings:?}");
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("[^a]")));
        assert!(messages.iter().any(|m| m.contains("[^c]")));
    }

    #[test]
    fn test_orphaned_reference_reports_first_occurrence() {
        let content = "First[^missing] and again[^missing] and third[^missing].";
        let warnings = check_md066(content);
        // Should only report once per unique ID
        assert_eq!(warnings.len(), 1, "Should report each orphaned ID once");
        assert!(warnings[0].message.contains("missing"));
    }

    // ==================== Orphaned definitions ====================

    #[test]
    fn test_orphaned_definition_single() {
        let content = "Regular text.\n\n[^unused]: This is never referenced.";
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1, "Should detect orphaned definition");
        assert!(warnings[0].message.contains("unused"));
        assert!(warnings[0].message.contains("never referenced"));
    }

    #[test]
    fn test_orphaned_definition_multiple() {
        let content = r#"Using one[^used].

[^used]: This is used.
[^orphan1]: Never used.
[^orphan2]: Also never used."#;
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 2, "Should detect 2 orphaned definitions: {warnings:?}");
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("orphan1")));
        assert!(messages.iter().any(|m| m.contains("orphan2")));
    }

    // ==================== Mixed cases ====================

    #[test]
    fn test_both_orphaned_reference_and_definition() {
        let content = r#"Reference[^missing].

[^unused]: Never referenced."#;
        let warnings = check_md066(content);
        assert_eq!(
            warnings.len(),
            2,
            "Should detect both orphaned ref and def: {warnings:?}"
        );
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("missing")),
            "Should find missing ref"
        );
        assert!(messages.iter().any(|m| m.contains("unused")), "Should find unused def");
    }

    // ==================== Code block handling ====================

    #[test]
    fn test_footnote_in_code_block_ignored() {
        let content = r#"```
[^1]: This is in a code block
```

Regular text without footnotes."#;
        let warnings = check_md066(content);
        assert!(warnings.is_empty(), "Footnotes in code blocks should be ignored");
    }

    #[test]
    fn test_footnote_reference_in_code_span_ignored() {
        // Note: This depends on whether pulldown-cmark parses footnotes inside code spans
        // If it does, we should skip them
        let content = r#"Use `[^1]` syntax for footnotes.

[^1]: This definition exists but the reference in backticks shouldn't count."#;
        // This is tricky - if pulldown-cmark doesn't parse [^1] in backticks as a footnote ref,
        // then the definition is orphaned
        let warnings = check_md066(content);
        // Expectation depends on parser behavior - test the actual behavior
        assert_eq!(
            warnings.len(),
            1,
            "Code span reference shouldn't count, definition is orphaned"
        );
        assert!(warnings[0].message.contains("never referenced"));
    }

    // ==================== Frontmatter handling ====================

    #[test]
    fn test_footnote_in_frontmatter_ignored() {
        let content = r#"---
note: "[^1]: yaml value"
---

Regular content."#;
        let warnings = check_md066(content);
        assert!(
            warnings.is_empty(),
            "Footnotes in frontmatter should be ignored: {warnings:?}"
        );
    }

    // ==================== Edge cases ====================

    #[test]
    fn test_empty_document() {
        let warnings = check_md066("");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_footnote_with_special_characters() {
        let content = r#"Reference[^my-note_1].

[^my-note_1]: Definition with special chars in ID."#;
        let warnings = check_md066(content);
        assert!(
            warnings.is_empty(),
            "Special characters in footnote ID should work: {warnings:?}"
        );
    }

    #[test]
    fn test_multiline_footnote_definition() {
        let content = r#"Reference[^long].

[^long]: This is a long footnote
    that spans multiple lines
    with proper indentation."#;
        let warnings = check_md066(content);
        assert!(
            warnings.is_empty(),
            "Multiline footnote definitions should work: {warnings:?}"
        );
    }

    #[test]
    fn test_footnote_at_end_of_sentence() {
        let content = r#"This ends with a footnote[^1].

[^1]: End of sentence footnote."#;
        let warnings = check_md066(content);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_footnote_mid_sentence() {
        let content = r#"Some text[^1] continues here.

[^1]: Mid-sentence footnote."#;
        let warnings = check_md066(content);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_adjacent_footnotes() {
        let content = r#"Text[^1][^2] with adjacent footnotes.

[^1]: First.
[^2]: Second."#;
        let warnings = check_md066(content);
        assert!(warnings.is_empty(), "Adjacent footnotes should work: {warnings:?}");
    }

    #[test]
    fn test_footnote_only_definitions_no_references() {
        let content = r#"[^1]: First orphan.
[^2]: Second orphan.
[^3]: Third orphan."#;
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 3, "All definitions should be flagged: {warnings:?}");
    }

    #[test]
    fn test_footnote_only_references_no_definitions() {
        let content = "Text[^1] and[^2] and[^3].";
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 3, "All references should be flagged: {warnings:?}");
    }

    // ==================== Blockquote handling ====================

    #[test]
    fn test_footnote_in_blockquote_valid() {
        let content = r#"> This has a footnote[^1].
>
> [^1]: Definition inside blockquote."#;
        let warnings = check_md066(content);
        assert!(
            warnings.is_empty(),
            "Footnotes inside blockquotes should be validated: {warnings:?}"
        );
    }

    #[test]
    fn test_footnote_in_nested_blockquote() {
        let content = r#"> > Nested blockquote with footnote[^nested].
> >
> > [^nested]: Definition in nested blockquote."#;
        let warnings = check_md066(content);
        assert!(
            warnings.is_empty(),
            "Footnotes in nested blockquotes should work: {warnings:?}"
        );
    }

    #[test]
    fn test_footnote_blockquote_orphaned_reference() {
        let content = r#"> This has an orphaned footnote[^missing].
>
> No definition here."#;
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1, "Should detect orphaned ref in blockquote");
        assert!(warnings[0].message.contains("missing"));
    }

    #[test]
    fn test_footnote_blockquote_orphaned_definition() {
        let content = r#"> Some text.
>
> [^unused]: Never referenced in blockquote."#;
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1, "Should detect orphaned def in blockquote");
        assert!(warnings[0].message.contains("unused"));
    }

    // ==================== Duplicate definitions ====================

    #[test]
    fn test_duplicate_definition_detected() {
        let content = r#"Reference[^1].

[^1]: First definition.
[^1]: Second definition (duplicate)."#;
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1, "Should detect duplicate definition: {warnings:?}");
        assert!(warnings[0].message.contains("Duplicate"));
        assert!(warnings[0].message.contains("[^1]"));
    }

    #[test]
    fn test_multiple_duplicate_definitions() {
        let content = r#"Reference[^dup].

[^dup]: First.
[^dup]: Second.
[^dup]: Third."#;
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 2, "Should detect 2 duplicate definitions: {warnings:?}");
        assert!(warnings.iter().all(|w| w.message.contains("Duplicate")));
    }

    #[test]
    fn test_duplicate_definition_case_insensitive() {
        let content = r#"Reference[^Note].

[^note]: Lowercase definition.
[^NOTE]: Uppercase definition (duplicate)."#;
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1, "Case-insensitive duplicate detection: {warnings:?}");
        assert!(warnings[0].message.contains("Duplicate"));
    }

    // ==================== HTML comment handling ====================

    #[test]
    fn test_footnote_reference_in_html_comment_ignored() {
        let content = r#"<!-- This has [^1] in a comment -->

Regular text without footnotes."#;
        let warnings = check_md066(content);
        assert!(
            warnings.is_empty(),
            "Footnote refs in HTML comments should be ignored: {warnings:?}"
        );
    }

    #[test]
    fn test_footnote_definition_in_html_comment_ignored() {
        let content = r#"<!--
[^1]: Definition in HTML comment
-->

Regular text."#;
        let warnings = check_md066(content);
        assert!(
            warnings.is_empty(),
            "Footnote defs in HTML comments should be ignored: {warnings:?}"
        );
    }

    #[test]
    fn test_footnote_outside_html_comment_still_validated() {
        let content = r#"<!-- Just a comment -->

Text with footnote[^1].

[^1]: Valid definition outside comment."#;
        let warnings = check_md066(content);
        assert!(warnings.is_empty(), "Valid footnote outside comment: {warnings:?}");
    }

    #[test]
    fn test_orphaned_ref_not_saved_by_def_in_comment() {
        let content = r#"Text with orphaned[^missing].

<!--
[^missing]: This definition is in a comment, shouldn't count
-->"#;
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1, "Def in comment shouldn't satisfy ref: {warnings:?}");
        assert!(warnings[0].message.contains("no corresponding definition"));
    }

    // ==================== HTML block handling ====================

    #[test]
    fn test_footnote_in_html_block_ignored() {
        // Regex character classes like [^abc] should be ignored in HTML blocks
        let content = r#"<table>
<tr>
<td><code>[^abc]</code></td>
<td>Negated character class</td>
</tr>
</table>

Regular markdown text."#;
        let warnings = check_md066(content);
        assert!(
            warnings.is_empty(),
            "Footnote-like patterns in HTML blocks should be ignored: {warnings:?}"
        );
    }

    #[test]
    fn test_footnote_in_html_table_ignored() {
        let content = r#"| Header |
|--------|
| Cell   |

<div>
<p>This has <code>[^0-9]</code> regex pattern</p>
</div>

Normal text."#;
        let warnings = check_md066(content);
        assert!(
            warnings.is_empty(),
            "Regex patterns in HTML div should be ignored: {warnings:?}"
        );
    }

    #[test]
    fn test_real_footnote_outside_html_block() {
        let content = r#"<div>
Some HTML content
</div>

Text with real footnote[^1].

[^1]: This is a real footnote definition."#;
        let warnings = check_md066(content);
        assert!(
            warnings.is_empty(),
            "Real footnote outside HTML block should work: {warnings:?}"
        );
    }

    // ==================== Combined edge cases ====================

    #[test]
    fn test_blockquote_with_duplicate_definitions() {
        let content = r#"> Text[^1].
>
> [^1]: First.
> [^1]: Duplicate in blockquote."#;
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1, "Should detect duplicate in blockquote: {warnings:?}");
        assert!(warnings[0].message.contains("Duplicate"));
    }

    #[test]
    fn test_all_enhancement_features_together() {
        let content = r#"<!-- Comment with [^comment] -->

Regular text[^valid] and[^missing].

> Blockquote text[^bq].
>
> [^bq]: Blockquote definition.

[^valid]: Valid definition.
[^valid]: Duplicate definition.
[^unused]: Never referenced."#;
        let warnings = check_md066(content);
        // Should find:
        // 1. [^missing] - orphaned reference
        // 2. [^valid] duplicate definition
        // 3. [^unused] - orphaned definition
        assert_eq!(warnings.len(), 3, "Should find all issues: {warnings:?}");

        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("missing")),
            "Should find orphaned ref"
        );
        assert!(
            messages.iter().any(|m| m.contains("Duplicate")),
            "Should find duplicate"
        );
        assert!(
            messages.iter().any(|m| m.contains("unused")),
            "Should find orphaned def"
        );
    }

    #[test]
    fn test_footnote_ref_at_end_of_file_no_newline() {
        let content = "[^1]: Definition here.\n\nText with[^1]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD066FootnoteValidation;
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Valid footnote pair without trailing newline should not warn: {result:?}"
        );
    }

    #[test]
    fn test_orphaned_footnote_ref_at_eof_no_newline() {
        let content = "Text with[^missing]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD066FootnoteValidation;
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "Orphaned ref at EOF without newline should warn: {result:?}"
        );
    }

    #[test]
    fn test_midline_footnote_ref_with_colon_detected_as_reference() {
        // [^note]: mid-line is a reference followed by colon, NOT a definition
        let content = "# Test\n\nI think [^note]: this is relevant.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD066FootnoteValidation;
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Mid-line [^note]: should be detected as undefined reference: {result:?}"
        );
        assert!(
            result[0].message.contains("no corresponding definition"),
            "Should warn about missing definition: {}",
            result[0].message
        );
    }

    #[test]
    fn test_midline_footnote_ref_with_colon_matched_to_definition() {
        // [^note]: mid-line is a reference; [^note]: at line start is the definition
        let content = "# Test\n\nI think [^note]: this is relevant.\n\n[^note]: The actual definition.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD066FootnoteValidation;
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Mid-line ref should match line-start definition: {result:?}"
        );
    }

    #[test]
    fn test_linestart_footnote_def_still_skipped_as_reference() {
        // [^note]: at line start IS a definition and should NOT be counted as reference
        let content = "# Test\n\n[^note]: The definition.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD066FootnoteValidation;
        let result = rule.check(&ctx).unwrap();
        // Should warn about orphaned definition (no reference)
        assert_eq!(result.len(), 1, "Orphaned def should be flagged: {result:?}");
        assert!(
            result[0].message.contains("never referenced"),
            "Should say 'never referenced': {}",
            result[0].message
        );
    }

    #[test]
    fn test_indented_footnote_def_still_skipped() {
        // [^note]: with 1-3 spaces indent is still a definition
        let content = "# Test\n\n   [^note]: Indented definition.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD066FootnoteValidation;
        let result = rule.check(&ctx).unwrap();
        // Should be treated as an orphaned definition (no reference)
        assert_eq!(result.len(), 1, "Indented def should still be detected: {result:?}");
        assert!(
            result[0].message.contains("never referenced"),
            "Should say 'never referenced': {}",
            result[0].message
        );
    }

    #[test]
    fn test_multiple_midline_refs_with_colons_on_same_line() {
        // Both [^a]: and [^b]: mid-line should be counted as references
        let content = "# Test\n\nText [^a]: and [^b]: more text.\n\n[^a]: Def A.\n[^b]: Def B.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD066FootnoteValidation;
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Both mid-line refs should match their definitions: {result:?}"
        );
    }

    #[test]
    fn test_blockquote_footnote_def_still_skipped() {
        // > [^note]: inside blockquote is a definition, not a reference
        let content = "# Test\n\n> [^note]: Definition in blockquote.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD066FootnoteValidation;
        let result = rule.check(&ctx).unwrap();
        // Orphaned definition (no reference uses it)
        assert_eq!(
            result.len(),
            1,
            "Blockquote def should be detected as orphaned: {result:?}"
        );
        assert!(
            result[0].message.contains("never referenced"),
            "Should say 'never referenced': {}",
            result[0].message
        );
    }

    #[test]
    fn test_list_item_footnote_ref_with_colon_is_reference() {
        // - [^note]: inside a list item is a reference, not a definition
        let content = "# Test\n\n- [^note]: list item text.\n\n[^note]: The actual definition.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD066FootnoteValidation;
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "List item [^note]: should be a ref matching the definition: {result:?}"
        );
    }

    // ==================== Warning position tests ====================

    #[test]
    fn test_orphaned_reference_column_position() {
        // "This references[^missing] a non-existent footnote."
        //  column 16:     ^
        let content = "This references[^missing] a non-existent footnote.";
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 1);
        assert_eq!(warnings[0].column, 16, "Column should point to '[^missing]'");
        // "[^missing]" is 10 chars, so end_column = 16 + 10 = 26
        assert_eq!(warnings[0].end_column, 26);
    }

    #[test]
    fn test_orphaned_definition_column_position() {
        // "[^unused]: Never referenced." starts at column 1
        let content = "Regular text.\n\n[^unused]: Never referenced.";
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
        assert_eq!(warnings[0].column, 1, "Definition at start of line");
        // "[^unused]:" is 10 chars
        assert_eq!(warnings[0].end_column, 11);
    }

    #[test]
    fn test_duplicate_definition_column_position() {
        let content = "Reference[^1].\n\n[^1]: First.\n[^1]: Second.";
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 4);
        assert_eq!(warnings[0].column, 1);
        // "[^1]:" is 5 chars
        assert_eq!(warnings[0].end_column, 6);
    }

    #[test]
    fn test_orphaned_definition_in_blockquote_column() {
        // "> [^unused]: Never referenced."
        //    ^ column 3 (after "> ")
        let content = "> Some text.\n>\n> [^unused]: Never referenced.";
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
        assert_eq!(warnings[0].column, 3, "Should point past blockquote prefix");
    }

    #[test]
    fn test_orphaned_reference_after_multibyte_chars() {
        // "日本語テキスト[^ref1] has no def."
        // "日本語テキスト" = 7 characters (each is 3 bytes in UTF-8)
        // Column should be 8 (character-based), not 22 (byte-based)
        let content = "日本語テキスト[^ref1] has no def.";
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].column, 8,
            "Column should be character-based, not byte-based"
        );
        // "[^ref1]" = 7 chars
        assert_eq!(warnings[0].end_column, 15);
    }

    #[test]
    fn test_orphaned_definition_with_indentation_column() {
        // "   [^note]:" — column should point to [^note]:, not the leading spaces
        let content = "# Heading\n\n   [^note]: Indented and orphaned.";
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1);
        // "[^note]:" starts at column 4 (after 3 spaces)
        assert_eq!(warnings[0].column, 4);
        // "[^note]:" is 8 chars, end_column = 4 + 8 = 12
        assert_eq!(warnings[0].end_column, 12);
    }

    #[test]
    fn test_orphaned_ref_end_column_uses_original_case() {
        // ref_id is stored lowercased, but end_column should reflect the actual source text
        let content = "Text with [^NOTE] here.";
        let warnings = check_md066(content);
        assert_eq!(warnings.len(), 1);
        // "Text with " = 10 chars, so [^NOTE] starts at column 11
        assert_eq!(warnings[0].column, 11);
        // "[^NOTE]" = 7 chars, end_column = 11 + 7 = 18
        assert_eq!(warnings[0].end_column, 18);
    }
}
