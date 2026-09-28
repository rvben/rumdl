//! Where trailing spaces are a hard line break.
//!
//! Two or more trailing spaces end a line with `<br>` only inside a paragraph
//! that continues on the next line. Everywhere else they render as nothing, so
//! removing them is safe.

use crate::lint_context::LintContext;
use crate::lint_context::types::HeadingStyle;

/// Whether the line at `line_num` (0-indexed) is a setext heading underline (`===` or `---`).
///
/// rumdl marks `LineInfo::heading` only on the setext text line, not the underline; the
/// underline still parses as `is_paragraph_context` per its own flags. Detect it by looking
/// back to the previous line's heading style.
fn is_setext_underline(ctx: &LintContext, line_num: usize) -> bool {
    if line_num == 0 {
        return false;
    }
    ctx.line_info(line_num).is_some_and(|prev| {
        prev.heading
            .as_ref()
            .is_some_and(|h| matches!(h.style, HeadingStyle::Setext1 | HeadingStyle::Setext2))
    })
}

/// Whether a `<br>` produced by trailing spaces on the line at `line_num` (0-indexed)
/// would be meaningful — i.e. the line is paragraph-context AND the next line continues
/// the same paragraph.
///
/// Mirrors markdownlint's MD009 strict logic, which only allows the `br_spaces` exception
/// on lines covered by `[paragraph.startLine, paragraph.endLine - 1]`. The last line of a
/// paragraph (single-line paragraph, line before a blank, line before a heading, separated
/// list items, etc.) gets no useful break and is flagged.
pub(crate) fn br_produces_useful_break(ctx: &LintContext, line_num: usize) -> bool {
    let lines = ctx.raw_lines();
    let Some(current) = ctx.line_info(line_num + 1) else {
        return false;
    };
    if current.is_blank || !current.is_paragraph_context() || is_setext_underline(ctx, line_num) {
        return false;
    }
    let next_idx = line_num + 1;
    if next_idx >= lines.len() {
        return false;
    }
    let Some(next) = ctx.line_info(next_idx + 1) else {
        return false;
    };
    if next.is_blank || !next.is_paragraph_context() || next.list_item.is_some() || is_setext_underline(ctx, next_idx) {
        return false;
    }
    true
}
