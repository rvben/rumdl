#![no_main]

//! Fuzz target: LintContext construction and lazy accessor evaluation must never panic.
//!
//! LintContext pre-processes lines, detects blocks, and lazily builds indexes.
//! Any input that crashes it is a bug.

use libfuzzer_sys::fuzz_target;
use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;

fuzz_target!(|data: &[u8]| {
    let Ok(content) = std::str::from_utf8(data) else {
        return;
    };

    if content.len() > 100_000 {
        return;
    }

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Exercise public fields and lazy accessors — any panic is a bug
    let _ = ctx.content;
    let _ = &ctx.lines;
    let _ = &ctx.code_blocks;
    let _ = &ctx.links;
    let _ = &ctx.images;
    let _ = ctx.raw_lines();
    let _ = ctx.code_spans();
    let _ = ctx.math_spans();
    let _ = ctx.html_tags();
    let _ = ctx.emphasis_spans();
    let _ = ctx.table_rows();
    let _ = ctx.bare_urls();
    let _ = ctx.lazy_continuation_lines();
    let _ = ctx.has_mixed_list_nesting();
    let _ = ctx.likely_has_headings();
    let _ = ctx.likely_has_tables();
    let _ = ctx.likely_has_code();
});
