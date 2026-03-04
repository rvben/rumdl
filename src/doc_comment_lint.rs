//! Linting of markdown embedded in Rust doc comments (`///` and `//!`).
//!
//! This module provides extraction and check-only logic for line doc comments.
//! It is used by both the CLI and LSP to lint Rust doc comments.
//!
//! **Precondition:** Input content must be LF-normalized (no `\r\n`).
//! The CLI path handles this via `normalize_line_ending`, but callers using
//! these functions directly must normalize first.
//!
//! **Not supported:** Block doc comments (`/** ... */`) are not extracted.

use crate::config as rumdl_config;
use crate::lint_context::LintContext;
use crate::rule::{LintWarning, Rule};

/// The kind of doc comment: outer (`///`) or inner (`//!`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocCommentKind {
    /// Outer doc comment (`///`)
    Outer,
    /// Inner doc comment (`//!`)
    Inner,
}

/// Metadata for a single line in a doc comment block.
#[derive(Debug, Clone)]
pub struct DocCommentLineInfo {
    /// Leading whitespace before the doc comment prefix (e.g. `"    "` for indented code)
    pub leading_whitespace: String,
    /// The doc comment prefix as it appeared in source (e.g. `"/// "`, `"///"`, `"///\t"`)
    pub prefix: String,
}

/// A contiguous block of same-kind doc comments extracted from a Rust source file.
#[derive(Debug, Clone)]
pub struct DocCommentBlock {
    /// Whether this is an outer (`///`) or inner (`//!`) doc comment.
    pub kind: DocCommentKind,
    /// 0-indexed line number of the first line in the original file.
    pub start_line: usize,
    /// 0-indexed line number of the last line in the original file (inclusive).
    pub end_line: usize,
    /// Byte offset of the first character of the first line in the block.
    pub byte_start: usize,
    /// Byte offset past the last character (including `\n`) of the last line in the block.
    pub byte_end: usize,
    /// Extracted markdown content with prefixes stripped.
    pub markdown: String,
    /// Per-line metadata for prefix restoration during fix mode.
    pub line_metadata: Vec<DocCommentLineInfo>,
    /// Length of leading whitespace + prefix (in bytes) for column offset remapping.
    /// Each entry corresponds to a line in `line_metadata`.
    pub prefix_byte_lengths: Vec<usize>,
}

/// Classify a line as a doc comment, returning the kind, leading whitespace,
/// and the full prefix (including the conventional single space if present).
///
/// Returns `None` if the line is not a doc comment. A doc comment must start
/// with optional whitespace followed by `///` or `//!`. Lines starting with
/// `////` are regular comments (not doc comments).
///
/// Handles all valid rustdoc forms:
///
/// - `/// content` (space after prefix)
/// - `///content` (no space — valid rustdoc, content is `content`)
/// - `///` (bare prefix, empty content)
/// - `///\tcontent` (tab after prefix)
fn classify_doc_comment_line(line: &str) -> Option<(DocCommentKind, String, String)> {
    let trimmed = line.trim_start();
    let leading_ws = &line[..line.len() - trimmed.len()];

    // `////` is NOT a doc comment (regular comment)
    if trimmed.starts_with("////") {
        return None;
    }

    if let Some(after) = trimmed.strip_prefix("///") {
        // Determine the prefix: include the conventional space/tab if present
        let prefix = if after.starts_with(' ') || after.starts_with('\t') {
            format!("///{}", &after[..1])
        } else {
            "///".to_string()
        };
        Some((DocCommentKind::Outer, leading_ws.to_string(), prefix))
    } else if let Some(after) = trimmed.strip_prefix("//!") {
        let prefix = if after.starts_with(' ') || after.starts_with('\t') {
            format!("//!{}", &after[..1])
        } else {
            "//!".to_string()
        };
        Some((DocCommentKind::Inner, leading_ws.to_string(), prefix))
    } else {
        None
    }
}

/// Extract the markdown content from a doc comment line after stripping the prefix.
fn extract_markdown_from_line(trimmed: &str, kind: DocCommentKind) -> &str {
    let prefix = match kind {
        DocCommentKind::Outer => "///",
        DocCommentKind::Inner => "//!",
    };

    let after_prefix = &trimmed[prefix.len()..];
    // Strip exactly one leading space if present (conventional rustdoc formatting)
    if let Some(stripped) = after_prefix.strip_prefix(' ') {
        stripped
    } else {
        after_prefix
    }
}

/// Extract all doc comment blocks from Rust source code.
///
/// Groups contiguous same-kind doc comment lines into blocks. A block boundary
/// occurs when:
///
/// - A line is not a doc comment
/// - The doc comment kind changes (from `///` to `//!` or vice versa)
///
/// Each block's `markdown` field contains the extracted markdown with prefixes
/// stripped. The `line_metadata` field preserves the original indentation and
/// prefix for each line, enabling faithful restoration during fix mode.
///
/// **Precondition:** `content` must be LF-normalized (no `\r\n`).
pub fn extract_doc_comment_blocks(content: &str) -> Vec<DocCommentBlock> {
    let mut blocks = Vec::new();
    let mut current_block: Option<DocCommentBlock> = None;
    let mut byte_offset = 0;

    let lines: Vec<&str> = content.split('\n').collect();
    let num_lines = lines.len();

    for (line_idx, line) in lines.iter().enumerate() {
        let line_byte_start = byte_offset;
        // Only add 1 for the newline if this is not the last segment
        let has_newline = line_idx < num_lines - 1 || content.ends_with('\n');
        let line_byte_end = byte_offset + line.len() + if has_newline { 1 } else { 0 };

        if let Some((kind, leading_ws, prefix)) = classify_doc_comment_line(line) {
            let trimmed = line.trim_start();
            let md_content = extract_markdown_from_line(trimmed, kind);

            // Compute column offset: leading whitespace bytes + prefix bytes
            let prefix_byte_len = leading_ws.len() + prefix.len();

            let line_info = DocCommentLineInfo {
                leading_whitespace: leading_ws,
                prefix,
            };

            match current_block.as_mut() {
                Some(block) if block.kind == kind => {
                    // Continue the current block
                    block.end_line = line_idx;
                    block.byte_end = line_byte_end;
                    block.markdown.push('\n');
                    block.markdown.push_str(md_content);
                    block.line_metadata.push(line_info);
                    block.prefix_byte_lengths.push(prefix_byte_len);
                }
                _ => {
                    // Flush any existing block
                    if let Some(block) = current_block.take() {
                        blocks.push(block);
                    }
                    // Start a new block
                    current_block = Some(DocCommentBlock {
                        kind,
                        start_line: line_idx,
                        end_line: line_idx,
                        byte_start: line_byte_start,
                        byte_end: line_byte_end,
                        markdown: md_content.to_string(),
                        line_metadata: vec![line_info],
                        prefix_byte_lengths: vec![prefix_byte_len],
                    });
                }
            }
        } else {
            // Not a doc comment line — flush current block
            if let Some(block) = current_block.take() {
                blocks.push(block);
            }
        }

        byte_offset = line_byte_end;
    }

    // Flush final block
    if let Some(block) = current_block.take() {
        blocks.push(block);
    }

    blocks
}

/// Rules that should be skipped when linting doc comment blocks.
///
/// - MD025: Multiple H1 headings are standard in rustdoc (`# Errors`, `# Examples`, `# Safety`).
/// - MD033: HTML tags like `<div class="warning">` are required syntax for rustdoc warning blocks.
/// - MD040: Rustdoc assumes unlabeled code blocks are Rust, so requiring language labels is noise.
/// - MD041: "First line should be a heading" doesn't apply — doc blocks aren't standalone documents.
/// - MD047: "File should end with a newline" doesn't apply for the same reason.
/// - MD051: Rustdoc anchors like `#method.bar` and `#structfield.name` aren't document headings.
/// - MD052: Intra-doc links like `[crate::io]` are rustdoc syntax, not markdown reference links.
/// - MD054: Shortcut reference style `[crate::module]` is the canonical intra-doc link syntax.
pub const SKIPPED_RULES: &[&str] = &["MD025", "MD033", "MD040", "MD041", "MD047", "MD051", "MD052", "MD054"];

/// Check all doc comment blocks in a Rust source file and return lint warnings.
///
/// Warnings have their line numbers and column numbers remapped to point to the
/// correct location in the original Rust file. Fix suggestions are stripped
/// (fixes are only applied through the fix mode path in the binary crate).
///
/// Empty doc comment blocks (only whitespace content) are skipped.
pub fn check_doc_comment_blocks(
    content: &str,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
) -> Vec<LintWarning> {
    let blocks = extract_doc_comment_blocks(content);
    let mut all_warnings = Vec::new();

    for block in &blocks {
        // Skip empty blocks to avoid spurious warnings
        if block.markdown.trim().is_empty() {
            continue;
        }

        let ctx = LintContext::new(&block.markdown, config.markdown_flavor(), None);

        for rule in rules {
            if SKIPPED_RULES.contains(&rule.name()) {
                continue;
            }

            if let Ok(rule_warnings) = rule.check(&ctx) {
                for warning in rule_warnings {
                    // Remap line numbers:
                    // warning.line is 1-indexed within the block markdown
                    // block.start_line is 0-indexed in the file
                    // (1-indexed block) + (0-indexed file start) = 1-indexed file line
                    let file_line = warning.line + block.start_line;
                    let file_end_line = warning.end_line + block.start_line;

                    // Remap column: add the prefix byte length for the corresponding line
                    let block_line_idx = warning.line.saturating_sub(1);
                    let col_offset = block.prefix_byte_lengths.get(block_line_idx).copied().unwrap_or(0);
                    let file_column = warning.column + col_offset;

                    let block_end_line_idx = warning.end_line.saturating_sub(1);
                    let end_col_offset = block.prefix_byte_lengths.get(block_end_line_idx).copied().unwrap_or(0);
                    let file_end_column = warning.end_column + end_col_offset;

                    all_warnings.push(LintWarning {
                        line: file_line,
                        end_line: file_end_line,
                        column: file_column,
                        end_column: file_end_column,
                        fix: None,
                        ..warning
                    });
                }
            }
        }
    }

    all_warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_outer_doc_comment() {
        let (kind, ws, prefix) = classify_doc_comment_line("/// Hello").unwrap();
        assert_eq!(kind, DocCommentKind::Outer);
        assert_eq!(ws, "");
        assert_eq!(prefix, "/// ");
    }

    #[test]
    fn test_classify_inner_doc_comment() {
        let (kind, ws, prefix) = classify_doc_comment_line("//! Module doc").unwrap();
        assert_eq!(kind, DocCommentKind::Inner);
        assert_eq!(ws, "");
        assert_eq!(prefix, "//! ");
    }

    #[test]
    fn test_classify_empty_outer() {
        let (kind, ws, prefix) = classify_doc_comment_line("///").unwrap();
        assert_eq!(kind, DocCommentKind::Outer);
        assert_eq!(ws, "");
        assert_eq!(prefix, "///");
    }

    #[test]
    fn test_classify_empty_inner() {
        let (kind, ws, prefix) = classify_doc_comment_line("//!").unwrap();
        assert_eq!(kind, DocCommentKind::Inner);
        assert_eq!(ws, "");
        assert_eq!(prefix, "//!");
    }

    #[test]
    fn test_classify_indented() {
        let (kind, ws, prefix) = classify_doc_comment_line("    /// Indented").unwrap();
        assert_eq!(kind, DocCommentKind::Outer);
        assert_eq!(ws, "    ");
        assert_eq!(prefix, "/// ");
    }

    #[test]
    fn test_classify_no_space_after_prefix() {
        // `///content` is valid rustdoc — content is "content"
        let (kind, ws, prefix) = classify_doc_comment_line("///content").unwrap();
        assert_eq!(kind, DocCommentKind::Outer);
        assert_eq!(ws, "");
        assert_eq!(prefix, "///");
    }

    #[test]
    fn test_classify_tab_after_prefix() {
        let (kind, ws, prefix) = classify_doc_comment_line("///\tcontent").unwrap();
        assert_eq!(kind, DocCommentKind::Outer);
        assert_eq!(ws, "");
        assert_eq!(prefix, "///\t");
    }

    #[test]
    fn test_classify_inner_no_space() {
        let (kind, _, prefix) = classify_doc_comment_line("//!content").unwrap();
        assert_eq!(kind, DocCommentKind::Inner);
        assert_eq!(prefix, "//!");
    }

    #[test]
    fn test_classify_four_slashes_is_not_doc() {
        assert!(classify_doc_comment_line("//// Not a doc comment").is_none());
    }

    #[test]
    fn test_classify_regular_comment() {
        assert!(classify_doc_comment_line("// Regular comment").is_none());
    }

    #[test]
    fn test_classify_code_line() {
        assert!(classify_doc_comment_line("let x = 3;").is_none());
    }

    #[test]
    fn test_extract_no_space_content() {
        let content = "///no space here\n";
        let blocks = extract_doc_comment_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].markdown, "no space here");
    }

    #[test]
    fn test_extract_basic_outer_block() {
        let content = "/// First line\n/// Second line\nfn foo() {}\n";
        let blocks = extract_doc_comment_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, DocCommentKind::Outer);
        assert_eq!(blocks[0].start_line, 0);
        assert_eq!(blocks[0].end_line, 1);
        assert_eq!(blocks[0].markdown, "First line\nSecond line");
        assert_eq!(blocks[0].line_metadata.len(), 2);
    }

    #[test]
    fn test_extract_basic_inner_block() {
        let content = "//! Module doc\n//! More info\n\nuse std::io;\n";
        let blocks = extract_doc_comment_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, DocCommentKind::Inner);
        assert_eq!(blocks[0].markdown, "Module doc\nMore info");
    }

    #[test]
    fn test_extract_multiple_blocks() {
        let content = "/// Block 1\nfn foo() {}\n/// Block 2\nfn bar() {}\n";
        let blocks = extract_doc_comment_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].markdown, "Block 1");
        assert_eq!(blocks[0].start_line, 0);
        assert_eq!(blocks[1].markdown, "Block 2");
        assert_eq!(blocks[1].start_line, 2);
    }

    #[test]
    fn test_extract_mixed_kinds_separate_blocks() {
        let content = "//! Inner\n/// Outer\n";
        let blocks = extract_doc_comment_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].kind, DocCommentKind::Inner);
        assert_eq!(blocks[1].kind, DocCommentKind::Outer);
    }

    #[test]
    fn test_extract_empty_doc_line() {
        let content = "/// First\n///\n/// Third\n";
        let blocks = extract_doc_comment_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].markdown, "First\n\nThird");
    }

    #[test]
    fn test_extract_preserves_extra_space() {
        let content = "///  Two spaces\n";
        let blocks = extract_doc_comment_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].markdown, " Two spaces");
    }

    #[test]
    fn test_extract_indented_doc_comments() {
        let content = "    /// Indented\n    /// More\n";
        let blocks = extract_doc_comment_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].markdown, "Indented\nMore");
        assert_eq!(blocks[0].line_metadata[0].leading_whitespace, "    ");
    }

    #[test]
    fn test_no_doc_comments() {
        let content = "fn main() {\n    let x = 3;\n}\n";
        let blocks = extract_doc_comment_blocks(content);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_byte_offsets() {
        let content = "/// Hello\nfn foo() {}\n/// World\n";
        let blocks = extract_doc_comment_blocks(content);
        assert_eq!(blocks.len(), 2);
        // First block: "/// Hello\n" = 10 bytes
        assert_eq!(blocks[0].byte_start, 0);
        assert_eq!(blocks[0].byte_end, 10);
        // Second block starts after "fn foo() {}\n" (12 bytes), at offset 22
        assert_eq!(blocks[1].byte_start, 22);
        assert_eq!(blocks[1].byte_end, 32);
    }

    #[test]
    fn test_byte_offsets_no_trailing_newline() {
        let content = "/// Hello";
        let blocks = extract_doc_comment_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].byte_start, 0);
        // No trailing newline, so byte_end == content.len()
        assert_eq!(blocks[0].byte_end, content.len());
    }

    #[test]
    fn test_prefix_byte_lengths() {
        let content = "    /// Indented\n/// Top-level\n";
        let blocks = extract_doc_comment_blocks(content);
        assert_eq!(blocks.len(), 1);
        // "    " (4) + "/// " (4) = 8 bytes for first line
        assert_eq!(blocks[0].prefix_byte_lengths[0], 8);
        // "" (0) + "/// " (4) = 4 bytes for second line
        assert_eq!(blocks[0].prefix_byte_lengths[1], 4);
    }
}
