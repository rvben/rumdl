//! Regression test for issue #583 — rumdl fmt mangled the Material-for-MkDocs
//! grid-cards block on the landing page by normalizing the `-   ` marker to
//! `- `, which stranded the 4-space-indented continuation content as indented
//! code blocks that MD046 then rewrote as fenced `text` blocks.
//!
//! The `<div ... markdown>` opt-in from Python-Markdown's `md_in_html` is an
//! unambiguous author signal. rumdl must leave list markers and continuation
//! content inside such blocks alone regardless of the configured flavor —
//! otherwise a fmt run without an explicit flavor silently corrupts the page.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::lint;
use rumdl_lib::rules::{all_rules, filter_rules};

const GRID_CARDS: &str = "\
# rumdl

<div class=\"grid cards\" markdown>

-   :zap:{ .lg .middle } **Built for speed**

    ---

    Written in Rust for blazing fast performance.

    [:octicons-arrow-right-24: Benchmarks](#performance)

-   :mag:{ .lg .middle } **71 lint rules**

    ---

    Comprehensive coverage of common Markdown issues.

    [:octicons-arrow-right-24: View rules](rules.md)

</div>
";

fn fmt_with_flavor(content: &str, flavor: MarkdownFlavor) -> String {
    let mut config = Config::default();
    config.global.flavor = flavor;
    let rules = filter_rules(&all_rules(&config), &config.global);
    let warnings = lint(content, &rules, false, flavor, None, None).expect("lint must succeed");

    let coordinator = FixCoordinator::new();
    let mut out = content.to_string();
    coordinator
        .apply_fixes_iterative(&rules, &warnings, &mut out, &config, 10, None)
        .expect("fmt must succeed");
    out
}

#[test]
fn grid_cards_preserved_under_mkdocs_flavor() {
    let fixed = fmt_with_flavor(GRID_CARDS, MarkdownFlavor::MkDocs);
    assert_eq!(fixed, GRID_CARDS, "MkDocs-flavor fmt must leave grid cards untouched");
}

#[test]
fn grid_cards_preserved_under_standard_flavor() {
    // Issue #583 specifically: when the flavor is not MkDocs, MD030 used to
    // rewrite `-   ` -> `- `, MD046 then re-fenced the stranded continuation
    // paragraphs as ```text blocks, and the `    ---` separators were
    // outdented into top-level thematic breaks. The `<div ... markdown>`
    // attribute must be honored across flavors so this cannot happen.
    let fixed = fmt_with_flavor(GRID_CARDS, MarkdownFlavor::Standard);
    assert_eq!(
        fixed, GRID_CARDS,
        "standard-flavor fmt must leave `<div markdown>` content untouched"
    );
    assert!(
        !fixed.contains("```text"),
        "continuation paragraphs must not be re-fenced as `text` code blocks"
    );
}

#[test]
fn grid_cards_marker_spacing_preserved_under_standard_flavor() {
    // Focused assertion on the original regression: MD030 must not rewrite
    // `-   ` to `- ` for list items inside a `<div markdown>` block, even
    // when the file is formatted as standard Markdown.
    let fixed = fmt_with_flavor(GRID_CARDS, MarkdownFlavor::Standard);
    for line in fixed.lines() {
        if line.trim_start().starts_with("-   :") || line.trim_start().starts_with("- :") {
            assert!(
                line.contains("-   :"),
                "custom 3-space marker spacing lost on line: {line:?}"
            );
        }
    }
}
