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

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use rayon::prelude::*;
use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::heading_utils::{HeadingUtils, get_heading_level, is_heading, is_setext_heading_marker};
use rumdl_lib::rules::md013_line_length::md013_config::ReflowMode;
use rumdl_lib::rules::{
    AbsoluteLinksOption, MD011NoReversedLinks, MD013Config, MD013LineLength, MD018NoMissingSpaceAtx,
    MD021NoMultipleSpaceClosedAtx, MD027MultipleSpacesBlockquote, MD032BlanksAroundLists, MD033NoInlineHtml,
    MD052ReferenceLinkImages, MD057Config, MD057ExistingRelativeLinks,
};
use rumdl_lib::rules::{MD014CommandsShowOutput, MD024NoDuplicateHeading, MD039NoSpaceInLinks, MD044ProperNames};
use rumdl_lib::utils::regex_cache::{RegexCache, get_cached_regex};
use rumdl_lib::workspace_index::{CrossFileLinkIndex, FileIndex, LinkOrigin, WorkspaceIndex};
use std::hint::black_box;
use std::path::Path;

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

criterion_group!(
    benches,
    bench_lint_context_new,
    bench_lint_overhead,
    bench_md013_reflow,
    bench_block_scan,
    bench_atx_headings,
    bench_md032,
    bench_md052,
    bench_md057,
    bench_workspace_build,
    bench_rules,
    bench_lint,
    bench_heading_utilities,
    bench_md044_name_lookup,
    bench_dynamic_regex_cache,
    bench_md024_duplicate_headings,
);
criterion_main!(benches);
