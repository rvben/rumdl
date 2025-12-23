use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Pattern to match footnote definitions: [^id]: content
/// Matches at start of line, with 0-3 leading spaces, caret in brackets
/// Also handles definitions inside blockquotes (after stripping > prefixes)
pub static FOOTNOTE_DEF_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ ]{0,3}\[\^([^\]]+)\]:").unwrap());

/// Pattern to match footnote references in text: [^id]
/// Must NOT be followed by : (which would make it a definition)
/// Uses fancy_regex for negative lookahead support
pub static FOOTNOTE_REF_PATTERN: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"\[\^([^\]]+)\](?!:)").unwrap());

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
                info.in_code_block || info.in_front_matter || info.in_html_comment || info.in_html_block
            }) {
                continue;
            }
            references
                .entry(footnote_ref.id.to_lowercase())
                .or_default()
                .push((footnote_ref.line, footnote_ref.byte_offset));
        }

        // Also parse references directly to find orphaned ones (without definitions)
        let code_spans = ctx.code_spans();
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            // Skip if in code block, frontmatter, HTML comment, or HTML block
            if line_info.in_code_block
                || line_info.in_front_matter
                || line_info.in_html_comment
                || line_info.in_html_block
            {
                continue;
            }

            let line = line_info.content(ctx.content);
            let line_num = line_idx + 1; // 1-indexed

            for caps in FOOTNOTE_REF_PATTERN.captures_iter(line).flatten() {
                if let Some(id_match) = caps.get(1) {
                    let id = id_match.as_str().to_lowercase();

                    // Check if this match is inside a code span
                    let match_start = caps.get(0).unwrap().start();
                    let byte_offset = line_info.byte_offset + match_start;

                    let in_code_span = code_spans
                        .iter()
                        .any(|span| byte_offset >= span.byte_offset && byte_offset < span.byte_end);

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
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: *line,
                        column: 1,
                        end_line: *line,
                        end_column: 1,
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
                let (line, _byte_offset) = occurrences[0];
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line,
                    column: 1,
                    end_line: line,
                    end_column: 1,
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
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line,
                    column: 1,
                    end_line: line,
                    end_column: 1,
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
}
