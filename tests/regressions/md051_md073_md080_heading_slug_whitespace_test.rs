//! Regression tests for the whitespace an anchor element leaves in a heading.
//!
//! `## Alpha <a id="x"></a>` renders as "Alpha", but GitHub slugs the heading's
//! text content with the space the element left, so the page answers to
//! `#alpha-` (and to `#x`), not to `#alpha`. Removing the element used to trim
//! that space away as well, so MD051 accepted `#alpha` and flagged `#alpha-`,
//! MD073 rewrote `[Alpha](#alpha-)` to `[Alpha](#alpha)`, and MD080 reported
//! `## Alpha <a id="x"></a>` and `## Alpha` as a collision.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD051LinkFragments, MD073TocValidation, MD080HeadingAnchorCollision};
use rumdl_lib::workspace_index::WorkspaceIndex;
use std::path::PathBuf;

const HEADINGS: &str = "## Alpha <a id=\"x\"></a>\n\n## <a id=\"y\"></a> Beta\n\n## Foo <a id=\"z\"></a> Bar\n";

/// A document whose links sit one per line from line 3 on, above `HEADINGS`.
fn document(links: &[&str]) -> String {
    let mut doc = String::from("# Title\n\n");
    for link in links {
        doc.push_str(link);
        doc.push('\n');
    }
    doc.push('\n');
    doc.push_str(HEADINGS);
    doc
}

fn md051_flagged_lines(content: &str, flavor: MarkdownFlavor) -> Vec<usize> {
    let ctx = LintContext::new(content, flavor, None);
    // Built the way a lint run builds it, so an unpinned anchor style follows
    // the file's flavor.
    let mut lines: Vec<usize> = MD051LinkFragments::from_config(&Config::default())
        .check(&ctx)
        .unwrap()
        .iter()
        .map(|warning| warning.line)
        .collect();
    lines.sort_unstable();
    lines
}

#[test]
fn md051_github_slug_keeps_the_hyphen_the_anchor_element_leaves() {
    let doc = document(&[
        "[ok](#alpha-)",
        "[ok](#x)",
        "[ok](#-beta)",
        "[ok](#y)",
        "[ok](#foo--bar)",
        "[ok](#z)",
        "[stale](#alpha)",
        "[stale](#beta)",
        "[stale](#foo-bar)",
        "[control](#definitely-missing)",
    ]);
    assert_eq!(md051_flagged_lines(&doc, MarkdownFlavor::Standard), vec![9, 10, 11, 12]);
}

#[test]
fn md051_python_markdown_slug_trims_the_whitespace_the_anchor_element_leaves() {
    // MkDocs renders with Python-Markdown, which trims and collapses whitespace,
    // so there the trimmed fragments are the ones that reach the headings.
    let doc = document(&[
        "[ok](#alpha)",
        "[ok](#beta)",
        "[ok](#foo-bar)",
        "[ok](#x)",
        "[stale](#alpha-)",
        "[stale](#-beta)",
        "[stale](#foo--bar)",
    ]);
    assert_eq!(md051_flagged_lines(&doc, MarkdownFlavor::MkDocs), vec![7, 8, 9]);
}

#[test]
fn md051_indexes_the_untrimmed_slug_for_cross_file_links() {
    let rules = rumdl_lib::rules::all_rules(&Config::default());
    let source = "# Source\n\n[ok](./other.md#alpha-)\n[ok](./other.md#x)\n[stale](./other.md#alpha)\n";
    let target = "# Other\n\n## Alpha <a id=\"x\"></a>\n";
    let source_path = PathBuf::from("/test/main.md");
    let target_path = PathBuf::from("/test/other.md");

    let (_, source_index) = rumdl_lib::lint_and_index(source, &rules, false, MarkdownFlavor::default(), None, None);
    let (_, target_index) = rumdl_lib::lint_and_index(target, &rules, false, MarkdownFlavor::default(), None, None);
    let mut workspace_index = WorkspaceIndex::new();
    workspace_index.insert_file(source_path.clone(), source_index.clone());
    workspace_index.insert_file(target_path.clone(), target_index);

    let target_file_index = workspace_index.get_file(&target_path).unwrap();
    assert!(target_file_index.has_anchor("alpha-"));
    assert!(target_file_index.has_anchor("x"));
    assert!(!target_file_index.has_anchor("alpha"));

    let warnings = MD051LinkFragments::default()
        .cross_file_check(&source_path, &source_index, &workspace_index)
        .unwrap();
    let lines: Vec<usize> = warnings.iter().map(|warning| warning.line).collect();
    assert_eq!(lines, vec![5], "{warnings:?}");
}

#[test]
fn md073_writes_and_accepts_the_untrimmed_slug() {
    let rule = MD073TocValidation::new();

    let doc = format!("# Title\n\n<!-- toc -->\n<!-- tocstop -->\n\n{HEADINGS}");
    let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("- [Alpha](#x)\n- [Beta](#y)\n- [Foo Bar](#z)\n"),
        "generated TOC prefers the explicit anchors: {fixed}"
    );
    let ctx = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
    assert!(rule.check(&ctx).unwrap().is_empty(), "a second pass changes nothing");

    let generated_slugs = format!(
        "# Title\n\n<!-- toc -->\n- [Alpha](#alpha-)\n- [Beta](#-beta)\n- [Foo Bar](#foo--bar)\n<!-- tocstop -->\n\n{HEADINGS}"
    );
    let ctx = LintContext::new(&generated_slugs, MarkdownFlavor::Standard, None);
    assert!(
        rule.check(&ctx).unwrap().is_empty(),
        "the generated slugs reach the headings"
    );

    let trimmed_slugs = generated_slugs
        .replace("#alpha-", "#alpha")
        .replace("#-beta", "#beta")
        .replace("#foo--bar", "#foo-bar");
    let ctx = LintContext::new(&trimmed_slugs, MarkdownFlavor::Standard, None);
    assert_eq!(rule.check(&ctx).unwrap().len(), 1, "the trimmed slugs reach nothing");
}

#[test]
fn md080_keeps_the_hyphen_the_anchor_element_leaves_out_of_a_collision() {
    let rule = MD080HeadingAnchorCollision::from_config(&Config::default());

    // `#alpha-` and `#alpha` are different anchors on GitHub.
    let ctx = LintContext::new(
        "## Alpha <a id=\"x\"></a>\n\n## Alpha\n",
        MarkdownFlavor::Standard,
        None,
    );
    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");

    // The same untrimmed slug twice still collides.
    let ctx = LintContext::new(
        "## Alpha <a id=\"x\"></a>\n\n## Alpha <a id=\"y\"></a>\n",
        MarkdownFlavor::Standard,
        None,
    );
    assert_eq!(rule.check(&ctx).unwrap().len(), 1);

    // Python-Markdown trims, so under MkDocs both headings do slug to `alpha`.
    let ctx = LintContext::new("## Alpha <a id=\"x\"></a>\n\n## Alpha\n", MarkdownFlavor::MkDocs, None);
    assert_eq!(rule.check(&ctx).unwrap().len(), 1);
}
