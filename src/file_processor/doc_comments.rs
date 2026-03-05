//! Fix mode logic for Rust doc comment linting.
//!
//! Handles applying markdown fixes to doc comment blocks while preserving
//! the `///` or `//!` prefixes and original indentation.

use rumdl_lib::config as rumdl_config;
use rumdl_lib::doc_comment_lint::{DocCommentBlock, DocCommentKind, SKIPPED_RULES, extract_doc_comment_blocks};
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::md013_line_length::MD013LineLength;

/// Apply markdown fixes to all doc comment blocks in a Rust source file.
///
/// Processes blocks in reverse order to maintain byte offsets. For each block:
///
/// 1. Lint the extracted markdown
/// 2. Apply fixes via the fix coordinator
/// 3. Restore doc comment prefixes
/// 4. Replace the original block in the content
///
/// Returns the number of blocks that were formatted.
pub fn format_doc_comment_blocks(
    content: &mut String,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
) -> usize {
    let blocks = extract_doc_comment_blocks(content);

    if blocks.is_empty() {
        return 0;
    }

    let mut formatted_count = 0;

    // Process in reverse order to maintain byte offsets
    for block in blocks.into_iter().rev() {
        if block.markdown.trim().is_empty() {
            continue;
        }

        // Filter out skipped rules and apply doc-comment config overrides
        let block_rules: Vec<Box<dyn Rule>> = rules
            .iter()
            .filter(|rule| !SKIPPED_RULES.contains(&rule.name()))
            .map(|r| {
                // Disable code block checking for MD013 in doc comments.
                // Code blocks contain Rust code formatted by rustfmt.
                if r.name() == "MD013" {
                    if let Some(md013) = r.as_any().downcast_ref::<MD013LineLength>() {
                        return Box::new(md013.with_code_blocks_disabled()) as Box<dyn Rule>;
                    }
                }
                dyn_clone::clone_box(&**r)
            })
            .collect();

        // Lint the extracted markdown
        let ctx = LintContext::new(&block.markdown, config.markdown_flavor(), None);
        let mut warnings = Vec::new();
        for rule in &block_rules {
            if let Ok(rule_warnings) = rule.check(&ctx) {
                warnings.extend(rule_warnings);
            }
        }

        if warnings.is_empty() {
            continue;
        }

        // Apply fixes to the markdown
        let mut formatted = block.markdown.clone();
        let fixed = super::processing::apply_fixes_coordinated(
            &block_rules,
            &warnings,
            &mut formatted,
            true,
            true,
            config,
            None,
        );

        if fixed == 0 {
            continue;
        }

        // Determine if original block ended with a trailing newline
        let byte_end = block.byte_end.min(content.len());
        let original_ends_with_newline = content.as_bytes().get(byte_end.wrapping_sub(1)) == Some(&b'\n');

        // Restore doc comment prefixes
        let restored = restore_doc_comment_prefixes(&formatted, &block, original_ends_with_newline);
        content.replace_range(block.byte_start..byte_end, &restored);
        formatted_count += 1;
    }

    formatted_count
}

/// Restore doc comment prefixes to formatted markdown.
///
/// Maps each line of the formatted markdown back to its original prefix and
/// indentation. Uses the stored prefix from `line_metadata` to preserve the
/// original separator (space, tab, or nothing). Lines added by fixes use the
/// block's dominant indentation and standard `"/// "` or `"//! "` prefix.
///
/// Only appends a trailing newline if `trailing_newline` is true, matching the
/// original block's behavior to maintain idempotency.
fn restore_doc_comment_prefixes(markdown: &str, block: &DocCommentBlock, trailing_newline: bool) -> String {
    let md_lines: Vec<&str> = markdown.split('\n').collect();
    let mut result = String::new();

    // Determine the dominant indentation for new lines (added by fixes)
    let dominant_indent = block
        .line_metadata
        .first()
        .map(|m| m.leading_whitespace.as_str())
        .unwrap_or("");

    let bare_prefix = match block.kind {
        DocCommentKind::Outer => "///",
        DocCommentKind::Inner => "//!",
    };

    for (i, md_line) in md_lines.iter().enumerate() {
        if i > 0 {
            result.push('\n');
        }

        // Use original metadata if available, otherwise use block defaults
        let indent = block
            .line_metadata
            .get(i)
            .map(|m| m.leading_whitespace.as_str())
            .unwrap_or(dominant_indent);

        result.push_str(indent);

        if md_line.is_empty() {
            // Empty markdown line → bare prefix, no trailing space
            result.push_str(bare_prefix);
        } else if let Some(meta) = block.line_metadata.get(i) {
            // Use original prefix (preserves separator: space, tab, or nothing)
            result.push_str(&meta.prefix);
            result.push_str(md_line);
        } else {
            // New line from fix — use standard prefix with space
            result.push_str(bare_prefix);
            result.push(' ');
            result.push_str(md_line);
        }
    }

    if trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rumdl_lib::doc_comment_lint::{DocCommentBlock, DocCommentKind, DocCommentLineInfo};

    #[test]
    fn test_restore_prefixes_basic() {
        let block = DocCommentBlock {
            kind: DocCommentKind::Outer,
            start_line: 0,
            end_line: 1,
            byte_start: 0,
            byte_end: 30,
            markdown: "Hello\nWorld".to_string(),
            line_metadata: vec![
                DocCommentLineInfo {
                    leading_whitespace: "".to_string(),
                    prefix: "/// ".to_string(),
                },
                DocCommentLineInfo {
                    leading_whitespace: "".to_string(),
                    prefix: "/// ".to_string(),
                },
            ],
            prefix_byte_lengths: vec![4, 4],
        };

        let restored = restore_doc_comment_prefixes("Hello\nWorld", &block, true);
        assert_eq!(restored, "/// Hello\n/// World\n");
    }

    #[test]
    fn test_restore_prefixes_with_empty_line() {
        let block = DocCommentBlock {
            kind: DocCommentKind::Outer,
            start_line: 0,
            end_line: 2,
            byte_start: 0,
            byte_end: 30,
            markdown: "First\n\nThird".to_string(),
            line_metadata: vec![
                DocCommentLineInfo {
                    leading_whitespace: "".to_string(),
                    prefix: "/// ".to_string(),
                },
                DocCommentLineInfo {
                    leading_whitespace: "".to_string(),
                    prefix: "///".to_string(),
                },
                DocCommentLineInfo {
                    leading_whitespace: "".to_string(),
                    prefix: "/// ".to_string(),
                },
            ],
            prefix_byte_lengths: vec![4, 3, 4],
        };

        let restored = restore_doc_comment_prefixes("First\n\nThird", &block, true);
        assert_eq!(restored, "/// First\n///\n/// Third\n");
    }

    #[test]
    fn test_restore_prefixes_new_line_added() {
        let block = DocCommentBlock {
            kind: DocCommentKind::Outer,
            start_line: 0,
            end_line: 1,
            byte_start: 0,
            byte_end: 30,
            markdown: "# Heading\nText".to_string(),
            line_metadata: vec![
                DocCommentLineInfo {
                    leading_whitespace: "".to_string(),
                    prefix: "/// ".to_string(),
                },
                DocCommentLineInfo {
                    leading_whitespace: "".to_string(),
                    prefix: "/// ".to_string(),
                },
            ],
            prefix_byte_lengths: vec![4, 4],
        };

        // Simulate fix adding a blank line after heading (MD022)
        let restored = restore_doc_comment_prefixes("# Heading\n\nText", &block, true);
        assert_eq!(restored, "/// # Heading\n///\n/// Text\n");
    }

    #[test]
    fn test_restore_inner_doc_comment() {
        let block = DocCommentBlock {
            kind: DocCommentKind::Inner,
            start_line: 0,
            end_line: 0,
            byte_start: 0,
            byte_end: 15,
            markdown: "Module".to_string(),
            line_metadata: vec![DocCommentLineInfo {
                leading_whitespace: "".to_string(),
                prefix: "//! ".to_string(),
            }],
            prefix_byte_lengths: vec![4],
        };

        let restored = restore_doc_comment_prefixes("Module", &block, true);
        assert_eq!(restored, "//! Module\n");
    }

    #[test]
    fn test_restore_indented() {
        let block = DocCommentBlock {
            kind: DocCommentKind::Outer,
            start_line: 0,
            end_line: 0,
            byte_start: 0,
            byte_end: 20,
            markdown: "Indented".to_string(),
            line_metadata: vec![DocCommentLineInfo {
                leading_whitespace: "    ".to_string(),
                prefix: "/// ".to_string(),
            }],
            prefix_byte_lengths: vec![8],
        };

        let restored = restore_doc_comment_prefixes("Indented", &block, true);
        assert_eq!(restored, "    /// Indented\n");
    }

    #[test]
    fn test_restore_no_trailing_newline() {
        let block = DocCommentBlock {
            kind: DocCommentKind::Outer,
            start_line: 0,
            end_line: 0,
            byte_start: 0,
            byte_end: 9,
            markdown: "Hello".to_string(),
            line_metadata: vec![DocCommentLineInfo {
                leading_whitespace: "".to_string(),
                prefix: "/// ".to_string(),
            }],
            prefix_byte_lengths: vec![4],
        };

        // No trailing newline when the block doesn't end with one
        let restored = restore_doc_comment_prefixes("Hello", &block, false);
        assert_eq!(restored, "/// Hello");
    }

    #[test]
    fn test_restore_preserves_tab_prefix() {
        let block = DocCommentBlock {
            kind: DocCommentKind::Outer,
            start_line: 0,
            end_line: 0,
            byte_start: 0,
            byte_end: 15,
            markdown: "content".to_string(),
            line_metadata: vec![DocCommentLineInfo {
                leading_whitespace: "".to_string(),
                prefix: "///\t".to_string(),
            }],
            prefix_byte_lengths: vec![4],
        };

        let restored = restore_doc_comment_prefixes("content", &block, true);
        assert_eq!(restored, "///\tcontent\n");
    }

    #[test]
    fn test_restore_preserves_no_space_prefix() {
        let block = DocCommentBlock {
            kind: DocCommentKind::Outer,
            start_line: 0,
            end_line: 0,
            byte_start: 0,
            byte_end: 13,
            markdown: "content".to_string(),
            line_metadata: vec![DocCommentLineInfo {
                leading_whitespace: "".to_string(),
                prefix: "///".to_string(),
            }],
            prefix_byte_lengths: vec![3],
        };

        // With no-space prefix, content is placed directly after ///
        let restored = restore_doc_comment_prefixes("content", &block, true);
        assert_eq!(restored, "///content\n");
    }
}
