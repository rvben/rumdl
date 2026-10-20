//! Targeted benchmarks for the performance-audit hot paths.
//!
//! Each benchmark exercises one (or a small cluster of) audited hot path so
//! that a fix can be measured in isolation via criterion baselines:
//!
//!   cargo bench --bench perf_audit <filter> -- --save-baseline before
//!   # apply the fix
//!   cargo bench --bench perf_audit <filter> -- --baseline before
//!
//! Inputs are generated deterministically (no RNG) so runs are comparable.

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayon::prelude::*;
use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::{LinkTargetPolicy, LintContext};
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::heading_utils::{HeadingUtils, get_heading_level, is_heading, is_setext_heading_marker};
use rumdl_lib::rules::md013_line_length::md013_config::ReflowMode;
use rumdl_lib::rules::{
    AbsoluteLinksOption, MD011NoReversedLinks, MD013Config, MD013LineLength, MD018NoMissingSpaceAtx,
    MD021NoMultipleSpaceClosedAtx, MD027MultipleSpacesBlockquote, MD032BlanksAroundLists, MD033NoInlineHtml,
    MD051LinkFragments, MD052ReferenceLinkImages, MD057Config, MD057ExistingRelativeLinks, MD070NestedCodeFence,
    MD077ListContinuationIndent, MD087UnusedDisableComment,
};
use rumdl_lib::rules::{
    MD014CommandsShowOutput, MD024NoDuplicateHeading, MD037NoSpaceInEmphasis, MD039NoSpaceInLinks, MD044ProperNames,
    MD049EmphasisStyle, MD084InvisibleCharacters, MD089CjkSpacing, MD093NoFormattingInHeadings, MD095LinkConfusables,
};
use rumdl_lib::utils::emphasis_utils::{
    find_emphasis_markers, find_emphasis_spans, find_single_emphasis_spans, find_valid_emphasis_ranges,
};
use rumdl_lib::utils::regex_cache::{RegexCache, get_cached_regex};
use rumdl_lib::workspace_index::{CrossFileLinkIndex, FileIndex, LinkOrigin, WorkspaceIndex};
use std::hint::black_box;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Input generators
// ---------------------------------------------------------------------------

/// Mixed document: headings, paragraphs, lists and tables. Exercises
/// LintContext::new broadly (line offsets, list/table block detection,
/// element parsing).
fn gen_mixed(sections: usize) -> String {
    let mut s = String::with_capacity(sections * 200);
    for i in 0..sections {
        s.push_str(&format!("## Section {i}\n\n"));
        s.push_str("This is a paragraph of moderate length describing the section in a few words.\n\n");
        for j in 0..4 {
            s.push_str(&format!("- list item {i}.{j} with some trailing text here\n"));
        }
        s.push('\n');
        s.push_str("| col a | col b | col c |\n| --- | --- | --- |\n");
        for j in 0..3 {
            s.push_str(&format!("| r{i}.{j} a | r{i}.{j} b | r{i}.{j} c |\n"));
        }
        s.push('\n');
    }
    s
}

/// Long prose paragraphs with many sentences. Stresses the reflow sentence
/// splitter (is_sentence_boundary) and block-boundary detection.
fn gen_prose(paragraphs: usize, sentences: usize) -> String {
    let mut s = String::with_capacity(paragraphs * sentences * 60);
    for p in 0..paragraphs {
        for k in 0..sentences {
            s.push_str(&format!(
                "Sentence {k} of paragraph {p} carries enough words to be a realistic clause. "
            ));
        }
        s.push_str("\n\n");
    }
    s
}

/// Long paragraphs containing many inline links, for the word-wrap reflow path
/// (reflow_elements), which handles non-Text elements.
fn gen_prose_links(paragraphs: usize, items: usize) -> String {
    let mut s = String::with_capacity(paragraphs * items * 50);
    for p in 0..paragraphs {
        for k in 0..items {
            s.push_str(&format!(
                "see [link {k}](https://example.com/p{p}/{k}) and more words here "
            ));
        }
        s.push_str("\n\n");
    }
    s
}

/// Many `$$` math blocks, a quarter of them left unclosed within their line,
/// to stress the math-block line-map lookahead.
fn gen_math(blocks: usize) -> String {
    let mut s = String::with_capacity(blocks * 40);
    for i in 0..blocks {
        if i % 4 == 0 {
            // opener with no inline closer -> forward scan
            s.push_str("$$\n");
            s.push_str("a = b + c\n");
            s.push_str("$$\n\n");
        } else {
            s.push_str("$$ x = y $$\n\n");
        }
    }
    s
}

/// Many inline HTML tags interleaved with many tables. Drives the per-tag
/// `is_in_table_block` calls in MD033 against a large `table_blocks` vector.
fn gen_html_tables(n: usize) -> String {
    let mut s = String::with_capacity(n * 80);
    for i in 0..n {
        s.push_str(&format!("Text with <span>inline {i}</span> markup here.\n\n"));
        s.push_str("| a | b |\n| - | - |\n| 1 | 2 |\n\n");
    }
    s
}

/// Many blockquote lines interleaved with many standalone list blocks. Drives
/// the per-line `is_in_list_block` calls in MD027 against a large `list_blocks`
/// vector.
fn gen_quotes_lists(n: usize) -> String {
    let mut s = String::with_capacity(n * 60);
    for i in 0..n {
        s.push_str(&format!("> quoted line {i} with some text\n>\n"));
        s.push_str(&format!("- standalone list item {i}\n\n"));
    }
    s
}

/// Many ATX headings, some malformed (missing space / multiple spaces / closed
/// form), to drive the per-line regex checks in MD018/MD021.
fn gen_headings(n: usize) -> String {
    let mut s = String::with_capacity(n * 24);
    for i in 0..n {
        match i % 3 {
            0 => s.push_str(&format!("## Heading {i}\n\n")),
            1 => s.push_str(&format!("##Heading {i}\n\n")),
            _ => s.push_str(&format!("## Heading {i}  ##\n\n")),
        }
    }
    s
}

/// Many reversed-link candidates `(text)[url]` for MD011.
fn gen_reversed_links(n: usize) -> String {
    let mut s = String::with_capacity(n * 30);
    for i in 0..n {
        s.push_str(&format!(
            "Line {i} has a (link text {i})[https://example.com/{i}] in it.\n"
        ));
    }
    s
}

/// Many blockquoted list lines for MD032.
fn gen_blockquoted_lists(n: usize) -> String {
    let mut s = String::with_capacity(n * 20);
    for i in 0..n {
        s.push_str(&format!("> - quoted item {i}\n"));
        if i % 5 == 0 {
            s.push_str("> paragraph inside the quote\n");
        }
    }
    s
}

/// Many absolute-path reference definitions for MD057. With absolute_links =
/// Warn, each ref definition emits a diagnostic, exercising the per-warning
/// line lookup over a large document (the defs sit at the end, high line idx).
fn gen_refs(n: usize) -> String {
    let mut s = String::with_capacity(n * 40);
    for i in 0..n {
        s.push_str(&format!("See [topic {i}][ref{i}] for details.\n"));
    }
    s.push('\n');
    for i in 0..n {
        s.push_str(&format!("[ref{i}]: /docs/topic{i}.md\n"));
    }
    s
}

/// Many undefined reference-style links for MD052. Each undefined reference
/// runs the per-link skip guards (including is_in_math_context).
fn gen_ref_links(n: usize) -> String {
    let mut s = String::with_capacity(n * 60);
    for i in 0..n {
        s.push_str(&format!(
            "Paragraph {i} mentions [topic {i}][ref{i}] within ordinary prose text.\n\n"
        ));
    }
    s
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_lint_context_new(c: &mut Criterion) {
    let content = gen_mixed(400);
    c.bench_function("lint_context_new/mixed_standard", |b| {
        b.iter(|| LintContext::new(black_box(&content), MarkdownFlavor::Standard, None));
    });
    c.bench_function("lint_context_new/mixed_kramdown", |b| {
        b.iter(|| LintContext::new(black_box(&content), MarkdownFlavor::Kramdown, None));
    });

    let math = gen_math(600);
    c.bench_function("lint_context_new/math_blocks", |b| {
        b.iter(|| LintContext::new(black_box(&math), MarkdownFlavor::Standard, None));
    });

    let links = gen_prose_links(100, 20);
    c.bench_function("lint_context_new/link_dense", |b| {
        b.iter(|| LintContext::new(black_box(&links), MarkdownFlavor::Standard, None));
    });

    let code = "Paragraph with `inline code`.\n\n```md\n[code](url) **literal**\n```\n\n".repeat(1000);
    c.bench_function("lint_context_new/code_dense", |b| {
        b.iter(|| LintContext::new(black_box(&code), MarkdownFlavor::Standard, None));
    });

    let prose = gen_prose(400, 8);
    c.bench_function("lint_context_new/plain_prose", |b| {
        b.iter(|| LintContext::new(black_box(&prose), MarkdownFlavor::Standard, None));
    });

    let quoted =
        "> - item\n>\n>   | a | b |\n>   | --- | --- |\n>   | x | y |\n>\n>   Title\n>   =====\n\n".repeat(400);
    c.bench_function("lint_context_new/quoted_lists_and_tables", |b| {
        b.iter(|| LintContext::new(black_box(&quoted), MarkdownFlavor::Standard, None));
    });
}

/// Comment delimiters interleaved with code must not rescan every code range.
fn bench_comment_code_ranges(c: &mut Criterion) {
    let mut group = c.benchmark_group("comment_code_ranges");
    group.sample_size(10);
    for (name, unit, block) in [
        ("inline", "`literal <!-- -->` <!-- real --> text\n\n", false),
        ("fenced", "```\n<!-- literal -->\n```\n\n<!-- real -->\n\n", true),
        ("no-comment", "`literal` text\n\n", false),
    ] {
        for count in [128, 1024, 4096] {
            let content = unit.repeat(count);
            let code_end = if block {
                unit.find("\n\n").unwrap() + 1
            } else {
                unit[1..].find('`').unwrap() + 2
            };
            let ranges: Vec<_> = (0..count)
                .map(|i| (i * unit.len(), i * unit.len() + code_end))
                .collect();
            let (spans, blocks) = if block {
                (&[][..], ranges.as_slice())
            } else {
                (ranges.as_slice(), &[][..])
            };
            let expected = if name == "no-comment" { 0 } else { count };
            let scan = rumdl_lib::utils::skip_context::scan_html_comments(&content, spans, blocks, 0);
            assert_eq!(scan.ranges.len(), expected);
            assert!(scan.unterminated.is_none());
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert_eq!(ctx.html_comment_ranges().len(), expected);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(format!("scan/{name}"), count), |b| {
                b.iter(|| rumdl_lib::utils::skip_context::scan_html_comments(black_box(&content), spans, blocks, 0));
            });
            group.bench_function(BenchmarkId::new(format!("context/{name}"), count), |b| {
                b.iter(|| LintContext::new(black_box(&content), MarkdownFlavor::Standard, None));
            });
        }
    }
    group.finish();
}

/// Many links on one source line expose repeated prefix-to-column scans.
fn bench_context_long_line(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_long_line");
    group.sample_size(10);
    for (name, unit) in [
        ("ascii", "[link](https://example.com) <hello@example.com> "),
        (
            "unicode",
            "é日本🙂 [![图](image.png)](https://example.com) [未解決][missing] ",
        ),
    ] {
        for size in [64, 1024, 4096, 16384] {
            let content = unit.repeat(size);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_with_input(BenchmarkId::new(name, size), &content, |b, content| {
                b.iter(|| LintContext::new(black_box(content), MarkdownFlavor::Standard, None));
            });
        }
    }
    group.finish();
}

/// Native MDX extraction shares the indexed columns used by the other parsers.
fn bench_context_mdx_columns(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_mdx_columns");
    group.sample_size(10);
    for (name, unit) in [
        ("links", "[label](https://example.com) ![alt](image.png) "),
        ("unicode", "é日本🙂 [label](https://example.com) ![图](image.png) "),
        ("jsx", "é日本🙂 <span>x</span> <Card /> "),
    ] {
        for count in [128, 1024, 4096] {
            let content = format!("{}\n", unit.repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::MDX, None);
            if name == "jsx" {
                assert_eq!(ctx.html_tags().len(), count * 2);
            } else {
                assert_eq!(ctx.links().len(), count);
                assert_eq!(ctx.images().len(), count);
            }
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_with_input(BenchmarkId::new(name, count), &content, |b, content| {
                b.iter(|| LintContext::new(black_box(content), MarkdownFlavor::MDX, None));
            });
        }
    }
    group.finish();
}

/// Include first access to the lazy element caches: construction alone does
/// not materialize these source columns.
fn bench_context_inline_elements(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_inline_elements");
    group.sample_size(10);
    for (name, unit) in [
        ("code", "text `code` "),
        ("math", "é日本🙂 $x + y$ "),
        ("html", "é日本🙂 <span>x</span> "),
    ] {
        for size in [128, 2048, 8192, 32768] {
            let content = unit.repeat(size);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_with_input(BenchmarkId::new(name, size), &content, |b, content| {
                b.iter(|| {
                    let ctx = LintContext::new(black_box(content), MarkdownFlavor::Standard, None);
                    match name {
                        "code" => {
                            black_box(ctx.code_spans());
                        }
                        "math" => {
                            black_box(ctx.math_spans());
                        }
                        "html" => {
                            black_box(ctx.html_tags());
                        }
                        _ => unreachable!(),
                    }
                });
            });
        }
    }
    group.finish();
}

/// Parser-only costs for the same inputs, without rumdl's extraction and line
/// classification. Compare with context construction before replacing a parser.
fn bench_markdown_parse(c: &mut Criterion) {
    use rumdl_lib::utils::parser_options::rumdl_parser_options_for;
    for (name, content) in [
        ("mixed", gen_mixed(400)),
        ("link_dense", gen_prose_links(100, 20)),
        (
            "code_dense",
            "Paragraph with `inline code`.\n\n```md\n[code](url) **literal**\n```\n\n".repeat(1000),
        ),
    ] {
        c.bench_function(&format!("markdown_parse/{name}"), |b| {
            b.iter(|| {
                pulldown_cmark::Parser::new_ext(black_box(&content), rumdl_parser_options_for(&content))
                    .into_offset_iter()
                    .count()
            });
        });
    }
}

fn bench_lint_overhead(c: &mut Criterion) {
    // Empty rule set isolates per-lint overhead (LintContext::new +
    // ContentCharacteristics::analyze + setup) from rule execution.
    let content = gen_mixed(400);
    let rules: Vec<Box<dyn Rule>> = Vec::new();
    c.bench_function("lint_overhead/no_rules_mixed", |b| {
        b.iter(|| {
            rumdl_lib::lint(
                black_box(&content),
                black_box(&rules),
                false,
                MarkdownFlavor::Standard,
                None,
                None,
            )
        });
    });
}

fn bench_md013_reflow(c: &mut Criterion) {
    let content = gen_prose(60, 12);
    let cfg = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(cfg);
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    c.bench_function("md013_reflow/sentence_per_line_fix", |b| {
        b.iter(|| rule.fix(black_box(&ctx)));
    });

    // Word-wrap path (reflow_elements) with inline links (non-Text elements).
    let wrap_content = gen_prose_links(40, 12);
    let wrap_cfg = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: rumdl_lib::types::LineLength::new(40),
        ..Default::default()
    };
    let wrap_rule = MD013LineLength::from_config_struct(wrap_cfg);
    let wrap_ctx = LintContext::new(&wrap_content, MarkdownFlavor::Standard, None);
    c.bench_function("md013_reflow/wordwrap_elements_fix", |b| {
        b.iter(|| wrap_rule.fix(black_box(&wrap_ctx)));
    });
}

fn bench_block_scan(c: &mut Criterion) {
    // is_in_table_block: MD033 calls it per inline-HTML tag.
    let tables = gen_html_tables(500);
    let tctx = LintContext::new(&tables, MarkdownFlavor::Standard, None);
    let md033 = MD033NoInlineHtml::default();
    c.bench_function("block_scan/md033_html_tables_check", |b| {
        b.iter(|| md033.check(black_box(&tctx)))
    });

    // is_in_list_block: MD027 calls it while processing blockquote lines.
    let quotes = gen_quotes_lists(500);
    let qctx = LintContext::new(&quotes, MarkdownFlavor::Standard, None);
    let md027 = MD027MultipleSpacesBlockquote::new();
    c.bench_function("block_scan/md027_lists_check", |b| {
        b.iter(|| md027.check(black_box(&qctx)))
    });
}

fn bench_atx_headings(c: &mut Criterion) {
    let content = gen_headings(800);
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    let md018 = MD018NoMissingSpaceAtx::default();
    let md021 = MD021NoMultipleSpaceClosedAtx;
    c.bench_function("atx_headings/md018_check", |b| b.iter(|| md018.check(black_box(&ctx))));
    c.bench_function("atx_headings/md021_check", |b| b.iter(|| md021.check(black_box(&ctx))));

    let links = gen_reversed_links(800);
    let lctx = LintContext::new(&links, MarkdownFlavor::Standard, None);
    let md011 = MD011NoReversedLinks;
    c.bench_function("atx_headings/md011_check", |b| b.iter(|| md011.check(black_box(&lctx))));
}

fn bench_md032(c: &mut Criterion) {
    let content = gen_blockquoted_lists(800);
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    let rule = MD032BlanksAroundLists::default();
    c.bench_function("md032/blockquoted_lists_check", |b| {
        b.iter(|| rule.check(black_box(&ctx)))
    });
}

fn bench_md052(c: &mut Criterion) {
    let content = gen_ref_links(800);
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    let rule = MD052ReferenceLinkImages::new();
    c.bench_function("md052/many_ref_links_check", |b| b.iter(|| rule.check(black_box(&ctx))));
}

fn bench_md057(c: &mut Criterion) {
    let content = gen_refs(600);
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::Warn,
        ..Default::default()
    };
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(".");
    c.bench_function("md057/many_refs_check", |b| b.iter(|| rule.check(black_box(&ctx))));
}

fn bench_workspace_link_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("workspace_link_extraction");
    group.sample_size(10);
    for (name, unit, suffix) in [
        ("relative", "é日本🙂 [label](target.md) ", ""),
        ("absolute", "é日本🙂 [label](/target.md) ", ""),
        ("angle", "é日本🙂 [label](<target.md>) ", ""),
        ("tail-angle", "é日本🙂 [label](target.md) ", " [last](<last.md>)"),
        ("tail-marker", "é日本🙂 [label](target.md) ", " <no destination>"),
    ] {
        for count in [128, 1024, 4096] {
            let content = format!("{}{suffix}\n", unit.repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            let links = rumdl_lib::workspace_index::extract_cross_file_links(&ctx);
            assert_eq!(
                links.relative.len() + links.root_relative.len(),
                count + usize::from(name == "tail-angle")
            );
            if name == "tail-angle" {
                assert!(links.relative.iter().all(|link| link.target_path == "last.md"));
            }
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| rumdl_lib::workspace_index::extract_cross_file_links(black_box(&ctx)))
            });
        }
    }
    group.finish();
}

fn bench_workspace_build(c: &mut Criterion) {
    let n = 400usize;
    c.bench_function("workspace_build/400_crosslinked", |b| {
        b.iter(|| {
            let mut idx = WorkspaceIndex::new();
            for i in 0..n {
                let mut fi = FileIndex::new();
                for k in 1..=5 {
                    fi.add_cross_file_link(CrossFileLinkIndex {
                        target_path: format!("doc{}.md", (i + k) % n),
                        fragment: String::new(),
                        line: k,
                        column: 1,
                        origin: LinkOrigin::Body,
                    });
                }
                idx.update_file(Path::new(&format!("doc{i}.md")), fi);
            }
            black_box(idx)
        });
    });
}

fn shell_document(with_output: bool) -> String {
    let mut content = String::new();
    for i in 0..100 {
        content.push_str(&format!("## Example {i}\n\n```bash\n$ cd /tmp\n"));
        for j in 0..8 {
            content.push_str(&format!("$ echo example-{i}-{j}\n"));
        }
        if with_output {
            content.push_str("example output\n");
        }
        content.push_str("```\n\n");
    }
    content
}

fn link_document(padded: bool, escaped: bool) -> String {
    let mut content = String::new();
    for i in 0..400 {
        let label = if escaped { r"label \[example\]" } else { "example label" };
        let pad = if padded { " " } else { "" };
        content.push_str(&format!(
            "[{pad}{label}{pad}](https://example.com/{i} \"Title\") ![{pad}{label}{pad}](image-{i}.png)\n\n"
        ));
    }
    content
}

fn bench_md057_positions(c: &mut Criterion) {
    let mut group = c.benchmark_group("md057_positions");
    group.sample_size(10);
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("sub");
    std::fs::create_dir(&base).unwrap();
    std::fs::write(base.join("target.md"), "target").unwrap();
    let source = base.join("page.md");
    std::fs::write(&source, "source").unwrap();
    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::Warn,
        compact_paths: true,
        self_referential_links: true,
        ..Default::default()
    };
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&base);
    for (name, unit, fixable) in [
        ("missing", "é日本🙂 [missing](missing.md) ", false),
        ("absolute", "é日本🙂 [absolute](/missing.md) ", false),
        ("compact", "é日本🙂 [compact](../sub/target.md#part) ", true),
        ("self", "é日本🙂 [self](page.md#part) ", true),
    ] {
        for count in [128, 1024, 4096] {
            let content = format!("{}\n", unit.repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, Some(source.clone()));
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), count);
            assert!(warnings.iter().all(|warning| warning.fix.is_some() == fixable));
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap())
            });
        }
    }
    group.finish();
}

fn bench_directive_code_ranges(c: &mut Criterion) {
    use rumdl_lib::inline_config::{InlineConfig, validate_inline_enables_against_active_rules};
    use rumdl_lib::rule::SuppressionReport;

    let mut group = c.benchmark_group("directive_code_ranges");
    group.sample_size(10);
    for stage in ["config", "observer", "validation"] {
        let directive = if stage == "validation" {
            "enable"
        } else {
            "disable-line"
        };
        for fenced in [false, true] {
            let unit = if fenced {
                format!("```md\n<!-- rumdl-{directive} MD013 -->\n```\n\n<!-- rumdl-{directive} MD013 -->\n\n")
            } else {
                format!("`<!-- rumdl-{directive} MD013 -->` <!-- rumdl-{directive} MD013 --> ")
            };
            for count in [128, 1024, 4096] {
                let content = unit.repeat(count);
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                let ranges = rumdl_lib::lint_context::code_ranges(&content, MarkdownFlavor::Standard);
                let mut report = SuppressionReport::default();
                report.judged_rules.insert("MD013".into());
                let rule = MD087UnusedDisableComment::new();
                let active = std::collections::HashSet::new();
                match stage {
                    "config" => {
                        let config =
                            InlineConfig::from_content_with_code_blocks(&content, &ranges.blocks, &ranges.spans);
                        assert!(config.is_rule_disabled("MD013", if fenced { 5 } else { 1 }));
                        if fenced {
                            assert!(!config.is_rule_disabled("MD013", 2));
                        }
                    }
                    "observer" => assert_eq!(rule.check_suppressions(&ctx, &report).unwrap().len(), count),
                    _ => assert_eq!(
                        validate_inline_enables_against_active_rules(
                            &content,
                            MarkdownFlavor::Standard,
                            &active,
                            &active
                        )
                        .len(),
                        count
                    ),
                }
                group.throughput(Throughput::Bytes(content.len() as u64));
                group.bench_function(BenchmarkId::new(format!("{stage}-fenced-{fenced}"), count), |b| {
                    b.iter(|| match stage {
                        "config" => {
                            black_box(InlineConfig::from_content_with_code_blocks(
                                black_box(&content),
                                &ranges.blocks,
                                &ranges.spans,
                            ));
                        }
                        "observer" => {
                            black_box(rule.check_suppressions(black_box(&ctx), &report).unwrap());
                        }
                        _ => {
                            black_box(validate_inline_enables_against_active_rules(
                                black_box(&content),
                                MarkdownFlavor::Standard,
                                &active,
                                &active,
                            ));
                        }
                    });
                });
            }
        }
    }
    group.finish();
}

fn bench_md087_suppressions(c: &mut Criterion) {
    use rumdl_lib::inline_config::{DisableLayer, validate_inline_config_rules};
    use rumdl_lib::rule::{SuppressedWarning, SuppressionReport};

    let mut group = c.benchmark_group("md087_suppressions");
    group.sample_size(10);
    let rule = MD087UnusedDisableComment::new();
    for (name, unit, layer, line_step) in [
        ("unused-line", "é日本🙂 <!-- rumdl-disable-line MD013 --> ", None, 1),
        (
            "used-lines",
            "text <!-- rumdl-disable-line MD013 -->\n",
            Some(DisableLayer::Line),
            1,
        ),
        (
            "used-blocks",
            "<!-- rumdl-disable MD013 -->\ntext\n",
            Some(DisableLayer::Block),
            2,
        ),
        (
            "config-line",
            "é日本🙂 <!-- rumdl-configure-file {\"MD013\":false} --> ",
            None,
            1,
        ),
        (
            "config-lines",
            "<!-- rumdl-configure-file {\"MD013\":false} -->\n",
            None,
            1,
        ),
    ] {
        for count in [128, 1024, 4096] {
            let content = unit.repeat(count);
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            let mut report = SuppressionReport::default();
            report.judged_rules.insert("MD013".into());
            if let Some(layer) = layer {
                report.suppressed = (1..=count)
                    .map(|line| SuppressedWarning {
                        rule_name: "MD013".into(),
                        line: line * line_step,
                        layer,
                    })
                    .collect();
            }
            assert_eq!(
                rule.check_suppressions(&ctx, &report).unwrap().len(),
                if layer.is_some() { 0 } else { count }
            );
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| rule.check_suppressions(black_box(&ctx), black_box(&report)).unwrap());
            });
            if name.starts_with("config-") {
                assert!(validate_inline_config_rules(&content, MarkdownFlavor::Standard).is_empty());
                group.bench_function(BenchmarkId::new(format!("validate-{name}"), count), |b| {
                    b.iter(|| validate_inline_config_rules(black_box(&content), MarkdownFlavor::Standard));
                });
            }
        }
    }
    // A broad disable can match the first finding even in a large report.
    // Constructing an index eagerly would turn this constant-time query linear.
    let content = "<!-- rumdl-disable-file MD013 -->\ntext\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    for count in [128, 4096, 65536] {
        let mut report = SuppressionReport::default();
        report.judged_rules.insert("MD013".into());
        report.suppressed = (1..=count)
            .map(|line| SuppressedWarning {
                rule_name: "MD013".into(),
                line,
                layer: DisableLayer::File,
            })
            .collect();
        assert!(rule.check_suppressions(&ctx, &report).unwrap().is_empty());
        group.throughput(Throughput::Elements(count as u64));
        group.bench_function(BenchmarkId::new("broad-disable", count), |b| {
            b.iter(|| rule.check_suppressions(black_box(&ctx), black_box(&report)).unwrap());
        });
    }
    group.finish();
}

fn bench_md030_indented_fallback(c: &mut Criterion) {
    use rumdl_lib::rules::MD030ListMarkerSpace;

    let rule = MD030ListMarkerSpace::default();
    let mut group = c.benchmark_group("md030_indented_fallback");
    group.sample_size(10);
    for (name, unit) in [("spaces", "    2.bad\n"), ("tabs", "\t2.bad\n")] {
        for count in [128, 1024, 4096] {
            let content = format!("- Root\n{}", unit.repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty());
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| black_box(rule.check(black_box(&ctx)).unwrap()));
            });
        }
    }
    group.finish();
}

fn bench_heading_anchor_lookup(c: &mut Criterion) {
    use rumdl_lib::utils::header_id_utils::{extract_heading_text, extract_html_anchor_ids};

    let mut group = c.benchmark_group("heading_anchor_lookup");
    group.sample_size(10);
    for (name, unit, has_anchors) in [
        ("html-code", "<span>word</span> `code` ", false),
        ("anchors", "<a id=\"live\"></a>`<a id=\"literal\"></a>` ", true),
    ] {
        for count in [128, 1024, 4096] {
            let content = format!("# {}\n", unit.repeat(count));
            assert_eq!(
                extract_html_anchor_ids(&content).len(),
                if has_anchors { count } else { 0 }
            );
            group.throughput(Throughput::Bytes(content.len() as u64));
            for stage in ["context", "text", "ids"] {
                group.bench_function(BenchmarkId::new(format!("{name}-{stage}"), count), |b| {
                    b.iter(|| match stage {
                        "context" => {
                            black_box(LintContext::new(black_box(&content), MarkdownFlavor::Standard, None));
                        }
                        "text" => {
                            black_box(extract_heading_text(black_box(&content)));
                        }
                        _ => {
                            black_box(extract_html_anchor_ids(black_box(&content)));
                        }
                    });
                });
            }
        }
    }
    group.finish();
}

fn bench_heading_anchor_attributes(c: &mut Criterion) {
    use rumdl_lib::rules::MD024NoDuplicateHeading;

    let mut group = c.benchmark_group("heading_anchor_attributes");
    group.sample_size(10);
    let rule = MD024NoDuplicateHeading::default();
    for name in ["id", "attributes"] {
        for count in [128, 512, 2048] {
            let unit = if name == "id" {
                "<a></a> {#id} "
            } else {
                "<a id=x></a> {: #id .class data='value'} "
            };
            let content = format!("# Word {}\n\n", unit.repeat(count)).repeat(2);
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            group.throughput(Throughput::Bytes(content.len() as u64));
            for stage in ["range", "MD024"] {
                group.bench_function(BenchmarkId::new(format!("{name}-{stage}"), count), |b| {
                    b.iter(|| {
                        if stage == "range" {
                            for heading in ctx.headings() {
                                black_box(heading.text_byte_range(black_box(&content)));
                            }
                        } else {
                            black_box(rule.check(black_box(&ctx)).unwrap());
                        }
                    });
                });
            }
        }
    }
    group.finish();
}

fn bench_heading_backtick_lookup(c: &mut Criterion) {
    use rumdl_lib::utils::header_id_utils::extract_heading_text;

    let mut group = c.benchmark_group("heading_backtick_lookup");
    group.sample_size(10);
    for name in ["unmatched", "paired", "recovered", "indexed-pairs"] {
        for count in [64, 256, 1024] {
            let unmatched = || {
                (1..=count)
                    .map(|len| format!("{}x ", "`".repeat(len)))
                    .collect::<String>()
            };
            let paired = || {
                (1..=count)
                    .map(|len| {
                        let ticks = "`".repeat(len);
                        format!("{ticks}x{ticks} ")
                    })
                    .collect::<String>()
            };
            let text = match name {
                "unmatched" => unmatched(),
                "paired" => paired(),
                "recovered" => format!("{}x {}", "`".repeat(count + 1), paired()),
                _ => {
                    let ticks = "`".repeat(count + 1);
                    format!("{}{}", unmatched(), format!("{ticks}x{ticks} ").repeat(count))
                }
            };
            let content = format!("# {text}<span>text</span>\n");
            group.throughput(Throughput::Bytes(content.len() as u64));
            for stage in ["text", "context"] {
                group.bench_function(BenchmarkId::new(format!("{name}-{stage}"), count), |b| {
                    b.iter(|| {
                        if stage == "text" {
                            black_box(extract_heading_text(black_box(&content)));
                        } else {
                            black_box(LintContext::new(black_box(&content), MarkdownFlavor::Standard, None));
                        }
                    });
                });
            }
        }
    }
    group.finish();
}

fn bench_heading_image_delimiters(c: &mut Criterion) {
    use rumdl_lib::utils::header_id_utils::extract_heading_text;

    let mut group = c.benchmark_group("heading_image_delimiters");
    group.sample_size(10);
    for name in [
        "description",
        "destination",
        "reference",
        "closed",
        "valid",
        "recovered",
    ] {
        for count in [128, 1024, 4096] {
            let text = match name {
                "description" => "![ ".repeat(count),
                "destination" => "![a]( ".repeat(count),
                "reference" => "![a][ ".repeat(count),
                "closed" => format!("{}{}", "![ ".repeat(count), "]".repeat(count)),
                "recovered" => format!("![ {}", "![alt](image) ".repeat(count)),
                _ => "![alt](image) ".repeat(count),
            };
            let content = format!("# {text}<span>text</span>\n");
            group.throughput(Throughput::Bytes(content.len() as u64));
            for stage in ["text", "context"] {
                group.bench_function(BenchmarkId::new(format!("{name}-{stage}"), count), |b| {
                    b.iter(|| {
                        if stage == "text" {
                            black_box(extract_heading_text(black_box(&content)));
                        } else {
                            black_box(LintContext::new(black_box(&content), MarkdownFlavor::Standard, None));
                        }
                    });
                });
            }
        }
    }
    group.finish();
}

fn bench_heading_anchor_spacing(c: &mut Criterion) {
    use rumdl_lib::utils::header_id_utils::extract_heading_text;

    let mut group = c.benchmark_group("heading_anchor_spacing");
    group.sample_size(10);
    for (name, unit) in [("single", "word <a id=x></a> "), ("adjacent", "é <a></a><a></a>  🙂 ")] {
        for count in [128, 2048, 16384] {
            let content = format!("# {}\n", unit.repeat(count));
            group.throughput(Throughput::Bytes(content.len() as u64));
            for stage in ["text", "context"] {
                group.bench_function(BenchmarkId::new(format!("{name}-{stage}"), count), |b| {
                    b.iter(|| {
                        if stage == "text" {
                            black_box(extract_heading_text(black_box(&content)));
                        } else {
                            black_box(LintContext::new(black_box(&content), MarkdownFlavor::Standard, None));
                        }
                    });
                });
            }
        }
    }
    group.finish();
}

fn bench_heading_trailing_anchors(c: &mut Criterion) {
    use rumdl_lib::rules::MD024NoDuplicateHeading;

    let mut group = c.benchmark_group("heading_trailing_anchors");
    group.sample_size(10);
    let rule = MD024NoDuplicateHeading::default();
    for (name, suffix) in [("plain", ""), ("custom-id", "{#custom}")] {
        for count in [128, 512, 2048] {
            let content = format!("# Word {}{suffix}\n\n", "<a id=x></a> ".repeat(count)).repeat(2);
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            group.throughput(Throughput::Bytes(content.len() as u64));
            for stage in ["range", "MD024"] {
                group.bench_function(BenchmarkId::new(format!("{name}-{stage}"), count), |b| {
                    b.iter(|| {
                        if stage == "range" {
                            for heading in ctx.headings() {
                                black_box(heading.text_byte_range(black_box(&content)));
                            }
                        } else {
                            black_box(rule.check(black_box(&ctx)).unwrap());
                        }
                    });
                });
            }
        }
    }
    group.finish();
}

fn bench_mdx_nested_context(c: &mut Criterion) {
    let mut group = c.benchmark_group("mdx_nested_context");
    group.sample_size(10);
    for name in ["nested", "unmatched", "paired"] {
        for count in [128, 1024, 4096] {
            let text = match name {
                "nested" => format!("{}text{}", "<b>".repeat(count), "</b>".repeat(count)),
                "unmatched" => format!("{}{}", "<b>".repeat(count), "</i>".repeat(count)),
                _ => "<b>text</b> ".repeat(count),
            };
            let content = format!("# {text}\n");
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| black_box(LintContext::new(black_box(&content), MarkdownFlavor::MDX, None)));
            });
        }
    }
    group.finish();
}

fn bench_md063_tag_pairing(c: &mut Criterion) {
    use rumdl_lib::rules::MD063HeadingCapitalization;

    let mut group = c.benchmark_group("md063_tag_pairing");
    group.sample_size(10);
    let rule = MD063HeadingCapitalization::default();
    for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MDX] {
        for name in ["unmatched", "distinct", "paired", "nested", "recovered"] {
            for count in [128, 1024, 4096] {
                let text = match name {
                    "paired" => "<b>text</b> ".repeat(count),
                    "recovered" => format!("<b></missing></b> {}", "<b>text</b> ".repeat(count)),
                    "nested" => format!("{}text{}", "<b>".repeat(count), "</b>".repeat(count)),
                    "distinct" => format!(
                        "{}{}",
                        "<b>".repeat(count),
                        (0..count).map(|i| format!("</missing{i}> ")).collect::<String>()
                    ),
                    _ => format!("{}{}", "<b>".repeat(count), "</i>".repeat(count)),
                };
                let content = format!("# {text}\n");
                let ctx = LintContext::new(&content, flavor, None);
                assert!(rule.check(&ctx).unwrap().is_empty());
                group.throughput(Throughput::Bytes(content.len() as u64));
                group.bench_function(BenchmarkId::new(format!("{flavor:?}-{name}"), count), |b| {
                    b.iter(|| black_box(rule.check(black_box(&ctx)).unwrap()));
                });
            }
        }
    }
    group.finish();
}

fn bench_md063_heading_regions(c: &mut Criterion) {
    use rumdl_lib::rules::MD063HeadingCapitalization;

    let mut group = c.benchmark_group("md063_heading_regions");
    group.sample_size(10);
    let rule = MD063HeadingCapitalization::default();
    for (name, unit, count_warnings) in [
        ("formatting", "é日本🙂 **strong** *emphasis* `code` ", 1),
        ("html-code", "<span>word</span> `code` ", 0),
        ("links", "[word](target) `code` ", 1),
    ] {
        for count in [128, 1024, 4096] {
            let content = format!("# {}\n", unit.repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), count_warnings);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

fn bench_list_code_context(c: &mut Criterion) {
    let mut group = c.benchmark_group("list_code_context");
    group.sample_size(10);
    for blank in [false, true] {
        let unit = if blank {
            "   ```\n   code\n   ```\n\n"
        } else {
            "   ```\n   code\n   ```\n"
        };
        for count in [128, 1024, 4096] {
            let content = format!("- Item\n{}", unit.repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert_eq!(ctx.code_block_details.len(), count);
            assert_eq!(ctx.list_blocks.len(), 1);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(format!("blank-{blank}"), count), |b| {
                b.iter(|| LintContext::new(black_box(&content), MarkdownFlavor::Standard, None));
            });
        }
    }
    group.finish();
}

fn bench_md031_list_membership(c: &mut Criterion) {
    use rumdl_lib::rules::MD031BlanksAroundFences;

    let mut group = c.benchmark_group("md031_list_membership");
    group.sample_size(10);
    let rule = MD031BlanksAroundFences::new(false);
    for (name, flavor, unit) in [
        ("fences", MarkdownFlavor::Standard, "   ```\n   code\n   ```\n"),
        ("admonitions", MarkdownFlavor::MkDocs, "   !!! note\n       contents\n"),
    ] {
        for count in [128, 1024, 4096] {
            let content = format!("- Item\n{}", unit.repeat(count));
            let ctx = LintContext::new(&content, flavor, None);
            assert!(rule.check(&ctx).unwrap().is_empty());
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

fn bench_spacing_fence_lookup(c: &mut Criterion) {
    use rumdl_lib::rules::{MD031BlanksAroundFences, MD076ListItemSpacing};

    let mut group = c.benchmark_group("spacing_fence_lookup");
    group.sample_size(10);
    let rules: [&dyn Rule; 2] = [&MD031BlanksAroundFences::default(), &MD076ListItemSpacing::default()];
    for rule in rules {
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MkDocs] {
            for count in [128, 1024, 4096] {
                let content = "1. Item\n\n   ```toml\n   key = 1\n   ```\n\n".repeat(count);
                let ctx = LintContext::new(&content, flavor, None);
                assert!(rule.check(&ctx).unwrap().is_empty());
                group.throughput(Throughput::Bytes(content.len() as u64));
                group.bench_function(BenchmarkId::new(format!("{}-{flavor:?}", rule.name()), count), |b| {
                    b.iter(|| rule.check(black_box(&ctx)).unwrap());
                });
            }
        }
    }
    group.finish();
}

fn bench_md077_fence_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("md077_fence_lookup");
    group.sample_size(10);
    let rule = MD077ListContinuationIndent::default();
    for (name, flavor, unit) in [
        ("standard", MarkdownFlavor::Standard, "- Item\n\n ```\n code\n ```\n\n"),
        (
            "mkdocs",
            MarkdownFlavor::MkDocs,
            "1. Item\n\n   ```toml\n   key = 1\n   ```\n\n",
        ),
    ] {
        for count in [128, 1024, 4096] {
            let content = unit.repeat(count);
            let ctx = LintContext::new(&content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), count);
            assert!(warnings.iter().all(|warning| warning.fix.is_some()));
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

fn bench_md070_fence_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("md070_fence_lookup");
    group.sample_size(10);
    let rule = MD070NestedCodeFence::new();
    for (name, unit) in [
        ("closed", "```rust\nlet x = 1;\n```\n\n"),
        ("container-ended", "- Item:\n\n  ```yaml\n  a: b\n\nEnd.\n\n"),
    ] {
        for count in [128, 1024, 4096, 8192] {
            let content = unit.repeat(count);
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert_eq!(ctx.code_block_details.len(), count);
            assert!(rule.check(&ctx).unwrap().is_empty());
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

fn bench_md093_positions(c: &mut Criterion) {
    let mut group = c.benchmark_group("md093_positions");
    group.sample_size(10);
    let rule = MD093NoFormattingInHeadings::default();
    for (name, prefix, suffix) in [
        ("atx", "# ", "\n"),
        ("setext", "", "\n===\n"),
        ("quoted", "> # ", "\r\n"),
    ] {
        for count in [128, 1024, 4096] {
            let content = format!(
                "{prefix}{}{suffix}",
                "é日本🙂 **strong** *emphasis* `code` ".repeat(count)
            );
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), count * 3);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

fn bench_md084_fixes(c: &mut Criterion) {
    let mut group = c.benchmark_group("md084_fixes");
    group.sample_size(10);
    for strict in [false, true] {
        let mut config = rumdl_lib::config::Config::default();
        config
            .rules
            .entry("MD084".to_string())
            .or_default()
            .values
            .insert("strict".to_string(), strict.into());
        let rule = MD084InvisibleCharacters::from_config(&config);
        for (name, unit, expected) in [
            ("invisible", "é日本🙂 x\u{200b} y ", 1),
            ("tone-mark", "é日本🙂 x\u{0340} y ", 1),
            ("clusters", "é日本🙂 x\u{200b}\u{200b}y ", if strict { 2 } else { 1 }),
            ("nonfixable", "é日本🙂 x\u{fffc}y ", 1),
            ("sparse", "é日本🙂 text ", 0),
        ] {
            for count in [128, 1024, 4096] {
                let suffix = if name == "sparse" { " \u{200b}" } else { "" };
                let content = format!("{}{suffix}\n", unit.repeat(count));
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                assert!(!rule.should_skip(&ctx));
                let warnings = rule.check(&ctx).unwrap();
                assert_eq!(warnings.len(), expected * count + usize::from(name == "sparse"));
                assert!(
                    warnings
                        .iter()
                        .all(|warning| warning.fix.is_some() == (name != "nonfixable"))
                );
                group.throughput(Throughput::Bytes(content.len() as u64));
                group.bench_function(BenchmarkId::new(format!("{name}/strict={strict}"), count), |b| {
                    b.iter(|| rule.check(black_box(&ctx)).unwrap());
                });
            }
        }
    }
    group.finish();
}

fn bench_heading_link_ranges(c: &mut Criterion) {
    let mut group = c.benchmark_group("heading_link_ranges");
    group.sample_size(10);
    for (name, unit, flavor) in [
        (
            "atx",
            "## Repeated heading\n\nparagraph [jump](#repeated-heading).\n\n",
            MarkdownFlavor::Standard,
        ),
        (
            "list",
            "- ## Repeated heading\n\n  paragraph [jump](#repeated-heading).\n\n",
            MarkdownFlavor::Standard,
        ),
        (
            "quoted-list",
            "> - ## Repeated heading\n>\n>   paragraph [jump](#repeated-heading).\n\n",
            MarkdownFlavor::Standard,
        ),
        (
            "mdx",
            "## Repeated heading\n\nparagraph [jump](#repeated-heading).\n\n",
            MarkdownFlavor::MDX,
        ),
    ] {
        for count in [128, 2048, 8192] {
            let content = unit.repeat(count);
            let ctx = LintContext::new(&content, flavor, None);
            assert_eq!(ctx.headings().count(), count);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| LintContext::new(black_box(&content), flavor, None));
            });
        }
    }
    group.finish();
}

fn bench_md089_indexes(c: &mut Criterion) {
    let mut group = c.benchmark_group("md089_indexes");
    group.sample_size(10);
    for (name, unit, expected) in [
        (
            "links",
            "é日本🙂 [label](https://example.com) ![图](https://example.com/image.png) ",
            0,
        ),
        ("badges", "中文[![图](img.png)](target)中文 ", 0),
        ("mixed", "中文[label](target)中文 ![图](img.png) ", 2),
        ("warnings", "é日本🙂 中文english ", 1),
    ] {
        for count in [128, 1024, 4096] {
            let content = format!("{}\n", unit.repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            let rule = MD089CjkSpacing::default();
            assert!(!rule.should_skip(&ctx));
            assert_eq!(rule.check(&ctx).unwrap().len(), expected * count);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

fn bench_md095_positions(c: &mut Criterion) {
    let mut group = c.benchmark_group("md095_positions");
    group.sample_size(10);
    for (name, unit, expected) in [
        ("safe-html", "é日本🙂 &amp; <span>text</span> ", 0),
        ("links", "é日本🙂 [x](https://еxample.com) ", 1),
        (
            "html-entities",
            "é日本🙂 <a href=\"https://&#x435;xample.com\">x</a> ",
            1,
        ),
        ("bare-urls", "é日本🙂 https://еxample.com ", 1),
        ("html-body", "<div> é日本🙂 [x](https://еxample.com) </div> ", 0),
    ] {
        for count in [128, 1024, 4096] {
            let content = format!("{}\n", unit.repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            let rule = MD095LinkConfusables;
            assert!(!rule.should_skip(&ctx));
            assert_eq!(rule.check(&ctx).unwrap().len(), expected * count);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

fn bench_md049_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("md049_filter");
    group.sample_size(10);
    for (name, unit, flavor, warnings_per_unit) in [
        ("mixed", "é日本🙂 *good* _other_ ", MarkdownFlavor::Standard, 1),
        ("literal", "*foo * bar* ", MarkdownFlavor::Standard, 0),
        (
            "markup",
            "é日本🙂 *good* _other_ ^^_insert_^^ ++ctrl+_key_++ :icon_name: ",
            MarkdownFlavor::MkDocs,
            1,
        ),
        (
            "obsidian",
            "é日本🙂 *good* _other_ ==_mark_== ",
            MarkdownFlavor::Obsidian,
            1,
        ),
    ] {
        for count in [128, 1024, 4096] {
            let content = format!("{}\n", unit.repeat(count));
            let ctx = LintContext::new(&content, flavor, None);
            let rule = MD049EmphasisStyle::default();
            assert_eq!(rule.check(&ctx).unwrap().len(), count * warnings_per_unit);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

fn bench_md037_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("md037_filter");
    group.sample_size(10);
    for (name, unit, warnings_per_unit) in [
        ("spaced", "é日本🙂 * bad * _ other _ ", 2),
        ("html", "é日本🙂 * bad * <span>text</span> ", 1),
        ("code", "é日本🙂 `literal` * bad * ", 1),
        ("math", "é日本🙂 $x$ * bad * ", 1),
        ("images", "é日本🙂 ![image](image.png) * bad * ", 1),
        ("literal", "*foo * bar* ", 0),
    ] {
        for count in [128, 1024, 4096] {
            let content = format!("{}\n", unit.repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert_eq!(
                MD037NoSpaceInEmphasis.check(&ctx).unwrap().len(),
                count * warnings_per_unit
            );
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter(|| MD037NoSpaceInEmphasis.check(black_box(&ctx)).unwrap());
            });
        }
    }
    group.finish();
}

fn bench_emphasis_pairing(c: &mut Criterion) {
    let mut group = c.benchmark_group("emphasis_pairing");
    group.sample_size(10);
    for (name, unit) in [
        ("unmatched_single", "*word "),
        ("unmatched_strong", "__word "),
        ("mixed_strength", "*word **word "),
        ("literal_marker", "*foo * bar* "),
    ] {
        for count in [128, 1024, 4096] {
            let content = unit.repeat(count);
            let markers = find_emphasis_markers(&content);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_with_input(BenchmarkId::new(name, count), &content, |b, content| {
                b.iter(|| {
                    black_box(find_emphasis_spans(black_box(content), black_box(&markers)));
                    black_box(find_single_emphasis_spans(black_box(content), black_box(&markers)));
                    black_box(find_valid_emphasis_ranges(black_box(content), black_box(&markers)));
                });
            });
        }
    }
    group.finish();
}

fn bench_rules(c: &mut Criterion) {
    let commands = shell_document(false);
    let output = shell_document(true);
    let links = link_document(true, false);
    let escaped = link_document(true, true);
    let clean = link_document(false, false);
    let md014 = MD014CommandsShowOutput::new();
    let md039 = MD039NoSpaceInLinks;

    for (name, rule, content, expected) in [
        ("md014_commands", &md014 as &dyn Rule, &commands, 800),
        ("md014_with_output", &md014, &output, 0),
        ("md039_padded", &md039 as &dyn Rule, &links, 800),
        ("md039_escaped", &md039, &escaped, 800),
        ("md039_clean", &md039, &clean, 0),
    ] {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        // Parse lazy context data before timing rule execution.
        assert_eq!(rule.check(&ctx).unwrap().len(), expected);
        c.bench_function(&format!("rules/{name}/check"), |b| {
            b.iter(|| rule.check(black_box(&ctx)).unwrap())
        });
        if name == "md014_commands" || name == "md039_padded" {
            let fixed = rule.fix(&ctx).unwrap();
            let fixed_ctx = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            c.bench_function(&format!("rules/{name}/fix"), |b| {
                b.iter(|| rule.fix(black_box(&ctx)).unwrap())
            });
        }
    }
}

fn bench_lint(c: &mut Criterion) {
    let content = format!("{}{}", shell_document(false), link_document(true, false));
    let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD014CommandsShowOutput::new()), Box::new(MD039NoSpaceInLinks)];
    let lint = || {
        rumdl_lib::lint(
            black_box(&content),
            black_box(&rules),
            false,
            MarkdownFlavor::Standard,
            None,
            None,
        )
        .unwrap()
    };
    assert_eq!(lint().len(), 1600);
    c.bench_function("lint/affected_rules", |b| b.iter(&lint));

    // Keep the pool alive across samples; include document parsing and lint setup.
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    c.bench_function("lint/affected_rules_16_documents_4_threads", |b| {
        b.iter(|| {
            pool.install(|| {
                (0..16).into_par_iter().for_each(|_| {
                    black_box(lint());
                });
            });
        });
    });

    let config = rumdl_lib::config::Config::default();
    let all_rules = rumdl_lib::rules::all_rules(&config);
    let default_rules = rumdl_lib::rules::filter_rules(&all_rules, &config.global);
    c.bench_function("lint/default_rules", |b| {
        b.iter(|| {
            rumdl_lib::lint(
                black_box(&content),
                black_box(&default_rules),
                false,
                MarkdownFlavor::Standard,
                None,
                Some(&config),
            )
            .unwrap()
        });
    });
}

fn bench_heading_utilities(c: &mut Criterion) {
    let lines: Vec<&str> = ["## Heading", "plain text", "Title", "======", "---", "### Closed ###"].repeat(200);
    c.bench_function("public_utilities/heading_detection_1200_lines", |b| {
        b.iter(|| {
            for (i, line) in lines.iter().enumerate() {
                black_box(is_heading(black_box(line)));
                black_box(is_setext_heading_marker(black_box(line)));
                black_box(get_heading_level(black_box(&lines), i));
            }
        });
    });
    c.bench_function("public_utilities/heading_fragment", |b| {
        b.iter(|| HeadingUtils::heading_to_fragment(black_box("An <em>example</em> heading: café & Rust")))
    });
}

fn bench_md044_name_lookup(c: &mut Criterion) {
    for count in [6, 32] {
        let mut names: Vec<String> = ["JavaScript", "TypeScript", "Node.js", "Café", "VS Code", "Rust"]
            .into_iter()
            .map(str::to_string)
            .collect();
        names.extend((6..count).map(|i| format!("Product{i}")));
        let content: String = (0..640)
            .map(|i| format!("Mention {} in this paragraph.\n\n", names[i % count].to_lowercase()))
            .collect();
        let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let rule = MD044ProperNames::new(names.clone(), false);
        assert_eq!(rule.check(&ctx).unwrap().len(), 640);

        c.bench_function(&format!("name_lookup/{count}_names/warm_check"), |b| {
            b.iter(|| rule.check(black_box(&ctx)).unwrap());
        });
        // Fresh instances prevent content memoization from hiding the scan cost.
        c.bench_function(&format!("name_lookup/{count}_names/cold_check"), |b| {
            b.iter_batched(
                || MD044ProperNames::new(names.clone(), false),
                |rule| rule.check(black_box(&ctx)).unwrap(),
                BatchSize::SmallInput,
            );
        });
        c.bench_function(&format!("name_lookup/{count}_names/construct"), |b| {
            b.iter(|| MD044ProperNames::new(black_box(names.clone()), false));
        });

        if count == 32 {
            let plain = "A paragraph without any configured product labels.\n\n".repeat(640);
            let plain_ctx = LintContext::new(&plain, MarkdownFlavor::Standard, None);
            assert!(rule.check(&plain_ctx).unwrap().is_empty());
            c.bench_function("name_lookup/no_matches", |b| {
                b.iter(|| rule.check(black_box(&plain_ctx)).unwrap());
            });
            c.bench_function("name_lookup/lint_with_construction", |b| {
                b.iter(|| {
                    let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD044ProperNames::new(names.clone(), false))];
                    rumdl_lib::lint(black_box(&content), &rules, false, MarkdownFlavor::Standard, None, None).unwrap()
                });
            });
        }
    }
}

fn bench_md044_long_lines(c: &mut Criterion) {
    let mut group = c.benchmark_group("MD044_long_lines");
    group.sample_size(10);
    for (name, unit) in [
        ("ascii", "javascript text "),
        ("unicode", "日本🙂 javascript text "),
        ("angles", "<custom:data> javascript "),
        ("comments", "<!-- javascript text --> "),
    ] {
        for count in [1024, 4096, 8192] {
            let content = format!("{}\n", unit.repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            let rule = MD044ProperNames::new(vec!["JavaScript".into()], false);
            assert_eq!(rule.check(&ctx).unwrap().len(), count);
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(format!("{name}-cached"), count), |b| {
                b.iter(|| black_box(rule.check(black_box(&ctx)).unwrap()));
            });
            group.bench_function(BenchmarkId::new(format!("{name}-fresh"), count), |b| {
                b.iter_batched(
                    || MD044ProperNames::new(vec!["JavaScript".into()], false),
                    |rule| black_box(rule.check(black_box(&ctx)).unwrap()),
                    BatchSize::SmallInput,
                );
            });
        }
    }
    group.finish();
}

fn bench_md044_link_destinations(c: &mut Criterion) {
    let mut group = c.benchmark_group("MD044_link_destinations");
    group.sample_size(10);
    for (name, unit) in [
        ("regular", "[label](target) javascript "),
        ("wiki", "[[target]](path) javascript "),
        ("nested", "[[target]](a(b)c) javascript "),
        ("unclosed", "[[target]](path javascript "),
        ("overlap", "[[target]](a"),
    ] {
        for count in [1024, 4096, 8192] {
            let content = if name == "overlap" {
                format!("{}javascript{} javascript\n", unit.repeat(count), ")".repeat(count))
            } else {
                format!("{}\n", unit.repeat(count))
            };
            let ctx = LintContext::new(&content, MarkdownFlavor::Obsidian, None);
            assert!(ctx.links().len() >= count);
            let expected = if name == "overlap" { 1 } else { count };
            assert_eq!(
                MD044ProperNames::new(vec!["JavaScript".into()], false)
                    .check(&ctx)
                    .unwrap()
                    .len(),
                expected
            );
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter_batched(
                    || MD044ProperNames::new(vec!["JavaScript".into()], false),
                    |rule| black_box(rule.check(black_box(&ctx)).unwrap()),
                    BatchSize::SmallInput,
                );
            });
        }
    }
    group.finish();
}

fn bench_md044_fallback_ranges(c: &mut Criterion) {
    let mut group = c.benchmark_group("MD044_fallback_ranges");
    group.sample_size(10);
    for (name, prefix, unit, suffix) in [
        ("comment", "<!--\n", "javascript text ", "\n-->"),
        ("html", "<div>\n", "javascript text ", "\n</div>"),
        ("frontmatter", "---\ntitle: ", "javascript text ", "\n---\n"),
        ("comment_links", "<!--\n", "[label](target) javascript ", "\n-->"),
        ("comment_code", "<!--\n", "`literal` javascript ", "\n-->"),
        ("html_code", "<div>\n", "`literal` javascript ", "\n</div>"),
        ("malformed_tail", "<!--\n[label](javascript) ", "[", "\n-->"),
        ("malformed_labels", "<!--\n", "[javascript ", "\n-->"),
        ("malformed_destinations", "<!--\n", "[label](javascript ", "\n-->"),
        ("malformed_references", "<!--\n", "[label][javascript ", "\n-->"),
    ] {
        for count in [512, 2048, 4096] {
            let content = format!("{prefix}{}{suffix}\n", unit.repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            let expected = if name == "malformed_tail" { 0 } else { count };
            assert_eq!(
                MD044ProperNames::new(vec!["JavaScript".into()], false)
                    .check(&ctx)
                    .unwrap()
                    .len(),
                expected
            );
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_function(BenchmarkId::new(name, count), |b| {
                b.iter_batched(
                    || MD044ProperNames::new(vec!["JavaScript".into()], false),
                    |rule| black_box(rule.check(black_box(&ctx)).unwrap()),
                    BatchSize::SmallInput,
                );
            });
        }
    }
    group.finish();
}

fn bench_dynamic_regex_cache(c: &mut Criterion) {
    let pattern = format!(
        "(?i)({})",
        (0..100).map(|i| format!("Product{i}")).collect::<Vec<_>>().join("|")
    );
    let mut cache = RegexCache::new();
    cache.get_regex(&pattern).unwrap();
    c.bench_function("dynamic_cache/1000_hits", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                black_box(cache.get_regex(black_box(&pattern)).unwrap());
            }
        });
    });

    get_cached_regex(&pattern).unwrap();
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    c.bench_function("dynamic_cache/1000_hits_4_threads", |b| {
        b.iter(|| {
            pool.install(|| {
                (0..4).into_par_iter().for_each(|_| {
                    for _ in 0..250 {
                        black_box(get_cached_regex(black_box(&pattern)).unwrap());
                    }
                });
            });
        });
    });
}

fn bench_md024_duplicate_headings(c: &mut Criterion) {
    for (mode, allow_nesting, siblings_only) in
        [("all", false, false), ("level", true, false), ("siblings", false, true)]
    {
        for duplicates in [false, true] {
            let mut content = String::new();
            for parent in 0..40 {
                content.push_str(&format!("# Parent {parent}\n\n"));
                for child in 0..16 {
                    let name = if duplicates { child % 4 } else { parent * 16 + child };
                    content.push_str(&format!(
                        "## Heading {name}: details about configuration and supported options\n\n"
                    ));
                }
            }
            let rule = MD024NoDuplicateHeading::new(allow_nesting, siblings_only);
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            let expected = if !duplicates {
                0
            } else if siblings_only {
                480
            } else {
                636
            };
            assert_eq!(rule.check(&ctx).unwrap().len(), expected);
            let workload = if duplicates { "duplicates" } else { "unique" };
            c.bench_function(&format!("duplicate_headings/{mode}/{workload}"), |b| {
                b.iter(|| rule.check(black_box(&ctx)).unwrap());
            });
            if siblings_only {
                c.bench_function(&format!("duplicate_headings/lint/{workload}"), |b| {
                    let rules: Vec<Box<dyn Rule>> = vec![Box::new(rule.clone())];
                    b.iter(|| {
                        rumdl_lib::lint(black_box(&content), &rules, false, MarkdownFlavor::Standard, None, None)
                            .unwrap()
                    });
                });
            }
        }
    }
}

/// The same document is checked and then indexed. Keep each context fresh so
/// lazy analysis is charged once, as it is in a real document run.
fn bench_link_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("link_analysis");
    for sections in [50, 500] {
        let mut content = String::new();
        for i in 0..sections {
            content.push_str(&format!(
                "## Section {i}\n\n[local](#section-{i}) [external](https://example.com/{i}) \
                 [page](/docs/page-{i}.md#part)\n\n<span id=\"target-{i}\">Text</span>\n\n"
            ));
        }
        let fragments = MD051LinkFragments::new();
        let paths = MD057ExistingRelativeLinks::from_config_struct(MD057Config {
            absolute_links: AbsoluteLinksOption::Warn,
            ..Default::default()
        });
        let run = |ctx: &LintContext<'_>| {
            let warnings = (fragments.check(ctx).unwrap(), paths.check(ctx).unwrap());
            let mut index = FileIndex::new();
            fragments.contribute_to_index(ctx, &mut index);
            paths.contribute_to_index(ctx, &mut index);
            black_box((warnings, index));
        };
        group.bench_function(format!("check_and_index/{sections}"), |b| {
            b.iter_batched(
                || LintContext::new(&content, MarkdownFlavor::Standard, Some("/docs/source.md".into())),
                |ctx| run(black_box(&ctx)),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(format!("parse_check_and_index/{sections}"), |b| {
            b.iter(|| {
                let ctx = LintContext::new(
                    black_box(&content),
                    MarkdownFlavor::Standard,
                    Some("/docs/source.md".into()),
                );
                run(&ctx);
            });
        });
    }
    group.finish();
}

fn bench_input_preparation(c: &mut Criterion) {
    let lf = gen_mixed(1000);
    let crlf = lf.replace('\n', "\r\n");
    let mixed = format!("{lf}{crlf}");
    let mut group = c.benchmark_group("input_preparation");
    for (label, content) in [("lf", &lf), ("crlf", &crlf), ("mixed", &mixed)] {
        group.bench_function(format!("line_endings/{label}"), |b| {
            b.iter(|| rumdl_lib::utils::NormalizedLineEndingMap::new(black_box(content)))
        });
        group.bench_function(format!("conflict_scan/{label}"), |b| {
            b.iter(|| rumdl_lib::merge_conflict::detect(black_box(content)))
        });
    }
    let late_conflict = format!("{lf}<<<<<<< HEAD\n");
    group.bench_function("conflict_scan/late", |b| {
        b.iter(|| rumdl_lib::merge_conflict::detect(black_box(&late_conflict)))
    });
    group.finish();
}

fn bench_link_target_policy(c: &mut Criterion) {
    let mut group = c.benchmark_group("link_target_policy");
    for (label, document_count, target_count, depth, directories, absolute) in [
        ("small_batch_no_targets", 3, 0, 2, 100, false),
        ("large_batch_no_targets", 1191, 0, 2, 100, false),
        ("large_batch_with_targets", 1191, 8142, 2, 100, false),
        ("shared_directories", 3, 8142, 2, 100, false),
        ("deep_directories", 3, 8142, 6, 100, false),
        ("distinct_directories", 3, 8142, 2, 8142, false),
        ("absolute_targets", 3, 512, 2, 100, true),
    ] {
        let documents: Vec<PathBuf> = (0..document_count)
            .map(|i| PathBuf::from(format!("docs/page{i}.md")))
            .collect();
        let prefix: String = (0..depth).map(|i| format!("level{i}/")).collect();
        let cwd = std::env::current_dir().unwrap();
        let targets: Vec<PathBuf> = (0..target_count)
            .map(|i| {
                let path = PathBuf::from(format!("{prefix}group{}/file{i}.pdf", i % directories));
                if absolute { cwd.join(path) } else { path }
            })
            .collect();
        group.bench_function(label, |b| {
            b.iter(|| {
                LinkTargetPolicy::with_declared_targets(
                    black_box(&documents),
                    black_box(&targets).iter().cloned(),
                    std::iter::empty(),
                    false,
                )
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_link_target_policy,
    bench_link_analysis,
    bench_input_preparation,
    bench_lint_context_new,
    bench_comment_code_ranges,
    bench_context_long_line,
    bench_context_mdx_columns,
    bench_context_inline_elements,
    bench_markdown_parse,
    bench_lint_overhead,
    bench_md013_reflow,
    bench_block_scan,
    bench_atx_headings,
    bench_md032,
    bench_md052,
    bench_md057,
    bench_md057_positions,
    bench_workspace_build,
    bench_workspace_link_extraction,
    bench_rules,
    bench_emphasis_pairing,
    bench_md037_filter,
    bench_md049_filter,
    bench_md095_positions,
    bench_md089_indexes,
    bench_heading_link_ranges,
    bench_md084_fixes,
    bench_md093_positions,
    bench_md070_fence_lookup,
    bench_md077_fence_lookup,
    bench_spacing_fence_lookup,
    bench_md031_list_membership,
    bench_list_code_context,
    bench_md063_heading_regions,
    bench_md063_tag_pairing,
    bench_mdx_nested_context,
    bench_heading_anchor_lookup,
    bench_heading_trailing_anchors,
    bench_heading_anchor_spacing,
    bench_heading_image_delimiters,
    bench_heading_backtick_lookup,
    bench_heading_anchor_attributes,
    bench_md030_indented_fallback,
    bench_md087_suppressions,
    bench_directive_code_ranges,
    bench_lint,
    bench_heading_utilities,
    bench_md044_name_lookup,
    bench_md044_long_lines,
    bench_md044_link_destinations,
    bench_md044_fallback_ranges,
    bench_dynamic_regex_cache,
    bench_md024_duplicate_headings,
);
criterion_main!(benches);
