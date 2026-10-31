//! Bracket text with no reference definition is literal text in CommonMark, not
//! a link. Every rule must read it as prose: the same warnings and the same
//! fixes as when the first bracket is escaped (the literal-text control), and a
//! DEFINED reference must keep behaving as a link.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;

fn rule(name: &str) -> Box<dyn Rule> {
    rule_with(name, &Config::default())
}

fn rule_with(name: &str, config: &Config) -> Box<dyn Rule> {
    rumdl_lib::rules::all_rules(config)
        .into_iter()
        .find(|rule| rule.name() == name)
        .unwrap_or_else(|| panic!("no rule {name}"))
}

/// `(line, column)` of every warning the rule reports.
fn warnings(name: &str, flavor: MarkdownFlavor, content: &str) -> Vec<(usize, usize)> {
    let rule = rule(name);
    let ctx = LintContext::new(content, flavor, None);
    rule.check(&ctx).unwrap().iter().map(|w| (w.line, w.column)).collect()
}

fn fixed(name: &str, flavor: MarkdownFlavor, content: &str) -> String {
    let rule = rule(name);
    let ctx = LintContext::new(content, flavor, None);
    rule.fix(&ctx).unwrap()
}

const STD: MarkdownFlavor = MarkdownFlavor::Standard;

// ---- MD059 -----------------------------------------------------------------

#[test]
fn md059_ignores_every_undefined_reference_form() {
    let content = "# T\n\n- [link] x\n\n[here] alone in a paragraph.\n\n[more]\n\n[click here][]\n\n[here][missing]\n";
    assert_eq!(warnings("MD059", STD, content), vec![], "{content}");
}

#[test]
fn md059_issue_974_reproduction_in_a_list_quote_and_nesting() {
    for content in [
        "# T\n\n- [link] x\n\n[here] alone in a paragraph.\n\n[more]\n",
        "# T\n\n> [here][missing]\n",
        "# T\n\n- item\n  - [click here][]\n",
        "# T\n\n1. [more][nope]\n   > [here]\n",
    ] {
        assert_eq!(warnings("MD059", STD, content), vec![], "{content}");
    }
}

#[test]
fn md059_still_flags_defined_references() {
    for content in [
        "# T\n\n[here][r]\n\n[r]: /x\n",
        "# T\n\n[here]\n\n[here]: /x\n",
        "# T\n\n[click here][]\n\n[click here]: /x\n",
        "# T\n\n[here][R]\n\n[r]: /x\n",
        "# T\n\n[here][a  b]\n\n[a b]: /x\n",
        // The URL is empty, but the definition exists: still a link.
        "# T\n\n[here][r]\n\n[r]: <>\n",
        // Unicode case folding: STRASSE and straße are the same label.
        "# T\n\n[here][STRASSE]\n\n[straße]: /x\n",
    ] {
        assert_eq!(warnings("MD059", STD, content).len(), 1, "{content}");
    }
}

#[test]
fn md059_checks_defined_references_inside_markdown_containers_but_not_undefined_ones() {
    let defined = "# T\n\n<div markdown=\"1\">\n[here][r]\n</div>\n\n[r]: /x\n";
    let undefined = "# T\n\n<div markdown=\"1\">\n[here][missing]\n</div>\n\n[r]: /x\n";
    let inside = "# T\n\n<div markdown=\"1\">\n[here][r]\n[r]: /x\n</div>\n";
    for flavor in [MarkdownFlavor::MkDocs, STD] {
        assert_eq!(warnings("MD059", flavor, defined).len(), 1, "{flavor:?} {defined}");
        assert_eq!(warnings("MD059", flavor, undefined), vec![], "{flavor:?} {undefined}");
        assert_eq!(warnings("MD059", flavor, inside).len(), 1, "{flavor:?} {inside}");
    }
}

#[test]
fn md059_nested_real_link_inside_an_undefined_image_keeps_its_diagnostic() {
    let content = "# T\n\n![[here](missing.md)][absent]\n";
    assert_eq!(warnings("MD059", STD, content).len(), 1, "{content}");
}

// ---- MD045 -----------------------------------------------------------------

#[test]
fn md045_ignores_undefined_image_references_and_keeps_defined_ones() {
    for content in ["# T\n\n![][noalt]\n", "# T\n\n![]\n", "# T\n\n![][]\n"] {
        assert_eq!(warnings("MD045", STD, content), vec![], "{content}");
    }
    assert_eq!(
        warnings("MD045", STD, "# T\n\n![][i]\n\n[i]: /img.png\n").len(),
        1,
        "defined image reference without alt text"
    );
}

// ---- MD042 -----------------------------------------------------------------

#[test]
fn md042_undefined_reference_with_empty_text_is_literal_text() {
    // `[][missing]` is not a link, so there is no empty link to report. MD052
    // still reports the missing definition.
    assert_eq!(warnings("MD042", STD, "# T\n\n[][missing]\n"), vec![]);
    // Controls: a real link with an empty destination or empty text still warns.
    assert_eq!(warnings("MD042", STD, "# T\n\n[text]()\n").len(), 1);
    assert_eq!(warnings("MD042", STD, "# T\n\n[](#)\n").len(), 1);
}

// ---- MD051 -----------------------------------------------------------------

#[test]
fn md051_anchor_inside_an_undefined_image_reference_is_a_real_anchor() {
    let content = "# T\n\n![<a id=\"anc\"></a>][missing]\n\n[go](#anc)\n";
    assert_eq!(warnings("MD051", STD, content), vec![], "{content}");
    // Control: the anchor does not exist when nothing declares it.
    assert_eq!(warnings("MD051", STD, "# T\n\n[go](#anc)\n").len(), 1);
}

// ---- Emphasis rules --------------------------------------------------------

#[test]
fn md050_checks_strong_style_inside_undefined_references() {
    let content = "# T\n\n**x** and [__foo__][missing]\n";
    assert_eq!(warnings("MD050", STD, content).len(), 1);
    assert_eq!(fixed("MD050", STD, content), "# T\n\n**x** and [**foo**][missing]\n");
}

#[test]
fn md049_checks_emphasis_style_inside_undefined_references() {
    let content = "# T\n\n*x* and [_foo_][missing]\n";
    assert_eq!(warnings("MD049", STD, content).len(), 1);
    assert_eq!(fixed("MD049", STD, content), "# T\n\n*x* and [*foo*][missing]\n");
}

#[test]
fn md037_checks_emphasis_spacing_inside_undefined_image_references() {
    assert_eq!(warnings("MD037", STD, "# T\n\n![* foo *][missing]\n").len(), 1);
    // Control: a defined image reference keeps today's behavior (its alt text
    // is link content that MD037 leaves alone).
    assert_eq!(warnings("MD037", STD, "# T\n\n![* foo *][r]\n\n[r]: /i.png\n"), vec![]);
}

// ---- Text rewriting rules --------------------------------------------------

#[test]
fn md088_fixes_quotes_inside_undefined_references() {
    let content = "# T\n\n[\u{201c}foo\u{201d}][missing]\n";
    assert_eq!(warnings("MD088", STD, content).len(), 2);
    assert_eq!(fixed("MD088", STD, content), "# T\n\n[\"foo\"][missing]\n");
}

#[test]
fn md011_fixes_reversed_links_inside_undefined_references() {
    let content = "# T\n\n[(https://example.com)[label]][missing]\n";
    assert_eq!(warnings("MD011", STD, content).len(), 1);
    assert_eq!(
        fixed("MD011", STD, content),
        "# T\n\n[[label](https://example.com)][missing]\n"
    );
}

#[test]
fn md095_flags_confusable_urls_inside_undefined_references() {
    let content = "# T\n\n[https://ex\u{0430}mple.com][missing]\n";
    assert_eq!(warnings("MD095", STD, content).len(), 1, "{content}");
}

// ---- MD044 -----------------------------------------------------------------

fn md044_config() -> Config {
    let mut config = Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config.values.insert(
        "names".to_string(),
        toml::Value::Array(vec![toml::Value::String("JavaScript".to_string())]),
    );
    config.rules.insert("MD044".to_string(), rule_config);
    config
}

fn md044_fixed(flavor: MarkdownFlavor, content: &str) -> String {
    let rule = rule_with("MD044", &md044_config());
    let ctx = LintContext::new(content, flavor, None);
    rule.fix(&ctx).unwrap()
}

#[test]
fn md044_corrects_both_brackets_of_an_undefined_reference() {
    assert_eq!(
        md044_fixed(STD, "# T\n\n[x][javascript] and [javascript]\n"),
        "# T\n\n[x][JavaScript] and [JavaScript]\n"
    );
}

#[test]
fn md044_keeps_the_label_of_a_defined_reference() {
    // A defined label is matched case-insensitively and rewriting it is
    // harmless, but the destination in the definition is never touched.
    let content = "# T\n\n[x][javascript]\n\n[javascript]: /javascript\n";
    let fixed = md044_fixed(STD, content);
    assert!(fixed.ends_with("[javascript]: /javascript\n"), "{fixed}");
}

#[test]
fn md044_keeps_mkdocs_autoref_identifiers() {
    // `[text][identifier]` is an autoref whose identifier is case-sensitive, so
    // the label bracket is a name, not prose. The text bracket is still prose.
    assert_eq!(
        md044_fixed(
            MarkdownFlavor::MkDocs,
            "# T\n\n[javascript][javascript] and [x][javascript]\n"
        ),
        "# T\n\n[JavaScript][javascript] and [x][javascript]\n"
    );
}

// ---- MD089 -----------------------------------------------------------------

#[test]
fn md089_spaces_cjk_inside_undefined_bracket_text() {
    assert_eq!(warnings("MD089", STD, "# T\n\n[\u{4e2d}\u{6587}text]\n").len(), 1);
    assert_eq!(
        fixed("MD089", STD, "# T\n\n[\u{4e2d}\u{6587}text]\n"),
        "# T\n\n[\u{4e2d}\u{6587} text]\n"
    );
    // Control: the same text with an escaped bracket is prose and is spaced.
    assert_eq!(
        fixed("MD089", STD, "# T\n\n\\[\u{4e2d}\u{6587}text]\n"),
        "# T\n\n\\[\u{4e2d}\u{6587} text]\n"
    );
}

#[test]
fn md089_never_renames_a_footnote_id() {
    for content in [
        "# T\n\n[^\u{4e2d}\u{6587}text]\n",
        "# T\n\ntext[^\u{4e2d}\u{6587}text]\n",
        "# T\n\n[^\u{4e2d}\u{6587}text]: body\n",
    ] {
        assert_eq!(fixed("MD089", STD, content), content, "{content}");
    }
}

#[test]
fn md089_never_edits_pandoc_implicit_heading_references() {
    for flavor in [MarkdownFlavor::Pandoc, MarkdownFlavor::Quarto] {
        for content in [
            "# \u{4e2d}\u{6587} text\n\nSee [\u{4e2d}\u{6587}text].\n",
            "# T\n\nSee [the \u{4e2d}\u{6587}text][\u{4e2d}\u{6587}text].\n",
        ] {
            assert_eq!(fixed("MD089", flavor, content), content, "{flavor:?} {content}");
        }
    }
}

#[test]
fn md089_never_edits_mkdocs_autorefs_but_spaces_prose_shortcuts() {
    let flavor = MarkdownFlavor::MkDocs;
    for content in [
        "# T\n\n[\u{4e2d}\u{6587}text][]\n",
        "# T\n\n[x][\u{4e2d}\u{6587}text]\n",
    ] {
        assert_eq!(fixed("MD089", flavor, content), content, "{content}");
    }
    // A shortcut is not an autoref, so it is prose.
    assert_eq!(
        fixed("MD089", flavor, "# T\n\n[\u{4e2d}\u{6587}text]\n"),
        "# T\n\n[\u{4e2d}\u{6587} text]\n"
    );
}

#[test]
fn md089_protects_the_label_of_an_unresolved_full_reference_but_not_its_text() {
    assert_eq!(
        fixed("MD089", STD, "# T\n\n[\u{4e2d}\u{6587}a][\u{4e2d}\u{6587}b]\n"),
        "# T\n\n[\u{4e2d}\u{6587} a][\u{4e2d}\u{6587}b]\n"
    );
}

#[test]
fn md089_defined_references_stay_opaque() {
    let content = "# T\n\n[\u{4e2d}\u{6587}text][r]\n\n[r]: /x\n";
    assert_eq!(fixed("MD089", STD, content), content);
}

// ---- MD013 -----------------------------------------------------------------

fn md013_warnings(content: &str) -> Vec<usize> {
    let mut config = Config::default();
    config.rules.insert(
        "MD013".to_string(),
        rumdl_lib::config::RuleConfig {
            severity: None,
            values: [("line-length".to_string(), toml::Value::Integer(20))]
                .into_iter()
                .collect(),
        },
    );
    let rule = rule_with("MD013", &config);
    let ctx = LintContext::new(content, STD, None);
    rule.check(&ctx).unwrap().iter().map(|w| w.line).collect()
}

#[test]
fn md013_line_made_only_of_an_undefined_reference_is_length_checked() {
    let long = "[a rather long undefined label][missing]";
    assert_eq!(md013_warnings(&format!("# T\n\n{long}\n")), vec![3]);
    assert!(md013_warnings(&format!("# T\n\n{long}\n\n[missing]: /x\n")).is_empty());
}

#[test]
fn md013_link_and_image_with_a_space_in_the_destination_stay_exempt() {
    assert!(md013_warnings("# T\n\n![alt](some directory/a very long image filename.png)\n").is_empty());
    assert!(md013_warnings("# T\n\n[text](some directory/a very long link target.md)\n").is_empty());
}

// ---- review follow-ups -------------------------------------------------------

#[test]
fn md059_and_md045_see_definitions_indented_inside_a_mkdocs_admonition() {
    let content = "# T\n\n!!! note\n    [here][r]\n\n    ![][im]\n\n    [r]: /x\n    [im]: /y.png\n";
    assert_eq!(warnings("MD059", MarkdownFlavor::MkDocs, content), vec![(4, 5)]);
    assert_eq!(warnings("MD045", MarkdownFlavor::MkDocs, content), vec![(6, 5)]);
    let undefined = "# T\n\n!!! note\n    [here][r]\n\n    ![][im]\n";
    assert!(warnings("MD059", MarkdownFlavor::MkDocs, undefined).is_empty());
    assert!(warnings("MD045", MarkdownFlavor::MkDocs, undefined).is_empty());
}

#[test]
fn md044_keeps_a_mkdocs_label_that_holds_an_escaped_bracket() {
    assert_eq!(
        md044_fixed(MarkdownFlavor::MkDocs, "# T\n\n[x][javascript\\[id]\n"),
        "# T\n\n[x][javascript\\[id]\n"
    );
}

#[test]
fn md089_protects_a_label_that_holds_an_escaped_bracket() {
    let content = "# T\n\n[x][\u{4e2d}\u{6587}text\\[id]\n";
    assert_eq!(fixed("MD089", STD, content), content);
}

#[test]
fn md089_spaces_text_after_an_escaped_footnote_opener() {
    assert_eq!(
        fixed("MD089", STD, "# T\n\n\\[^\u{4e2d}\u{6587}text]\n"),
        "# T\n\n\\[^\u{4e2d}\u{6587} text]\n"
    );
}

#[test]
fn md059_sees_mkdocs_container_definitions_with_trailing_space_and_a_continuation_line() {
    let content = "# T\n\n!!! note\n    [a][r1]\n\n    [b][r2]\n\n    [r1]: /x  \n    [r2]:\n        /y \"t\"\n";
    assert_eq!(
        warnings("MD059", MarkdownFlavor::MkDocs, content),
        Vec::<(usize, usize)>::new()
    );
    let content =
        "# T\n\n!!! note\n    [here][r1]\n\n    [click here][r2]\n\n    [r1]: /x  \n    [r2]:\n        /y \"t\"\n";
    assert_eq!(warnings("MD059", MarkdownFlavor::MkDocs, content), vec![(4, 5), (6, 5)]);
}

#[test]
fn md088_does_not_fix_a_mkdocs_autoref_identifier_but_fixes_the_text() {
    let content = "# T\n\n[x\u{201c}a\u{201d}][some\u{201c}id\u{201d}]\n";
    assert_eq!(
        fixed("MD088", MarkdownFlavor::MkDocs, content),
        "# T\n\n[x\"a\"][some\u{201c}id\u{201d}]\n"
    );
    assert_eq!(fixed("MD088", STD, content), "# T\n\n[x\"a\"][some\"id\"]\n");
}

#[test]
fn md089_footnote_like_text_inside_a_code_span_protects_nothing() {
    let fixed = fixed("MD089", STD, "# T\n\n`[^`\u{4e2d}\u{6587}text]\n");
    assert!(fixed.contains("\u{4e2d}\u{6587} text]"), "{fixed}");
}

#[test]
fn md088_does_not_fix_a_collapsed_mkdocs_autoref_identifier() {
    let content = "# T\n\n[some\u{201c}id\u{201d}][] and \u{201c}q\u{201d}\n";
    assert_eq!(
        fixed("MD088", MarkdownFlavor::MkDocs, content),
        "# T\n\n[some\u{201c}id\u{201d}][] and \"q\"\n"
    );
}

#[test]
fn md049_and_md050_leave_a_mkdocs_autoref_identifier_alone() {
    let strong = "# T\n\n**x** and [x][__dict__] and [__init__][]\n";
    assert_eq!(fixed("MD050", MarkdownFlavor::MkDocs, strong), strong);
    let emphasis = "# T\n\n*x* and [x][_dict_] and [_init_][]\n";
    assert_eq!(fixed("MD049", MarkdownFlavor::MkDocs, emphasis), emphasis);
    // Outside MkDocs the same brackets are plain text and are checked.
    assert_ne!(fixed("MD050", STD, strong), strong);
}

#[test]
fn md059_pandoc_heading_text_defines_an_implicit_reference() {
    let content = "# here\n\n[here]\n\n[nope]\n";
    assert_eq!(warnings("MD059", MarkdownFlavor::Pandoc, content), vec![(3, 1)]);
    assert!(warnings("MD059", STD, content).is_empty());
}

#[test]
fn md088_keeps_a_collapsed_autoref_identifier_inside_a_markdown_container() {
    let content = "# T\n\n<div markdown=\"1\">\n[some\u{201c}id\u{201d}][]\n</div>\n";
    assert_eq!(fixed("MD088", MarkdownFlavor::MkDocs, content), content);
}

#[test]
fn md089_unmatched_footnote_openers_on_a_long_line_stay_linear() {
    let line = format!("\u{4e2d}\u{6587}text {}", "[^".repeat(20_000));
    let start = std::time::Instant::now();
    let _ = warnings("MD089", STD, &format!("# T\n\n{line}\n"));
    assert!(start.elapsed().as_secs() < 5, "took {:?}", start.elapsed());
}

#[test]
fn md059_pandoc_heading_reference_matches_the_marked_up_heading_text() {
    // Verified against pandoc: `# *here*` is referenced by `[*here*]`, and the
    // plain `[here]` stays literal text.
    assert!(warnings("MD059", MarkdownFlavor::Pandoc, "# *here*\n\n[here]\n").is_empty());
    assert_eq!(
        warnings("MD059", MarkdownFlavor::Pandoc, "# click here\n\n[click here]\n"),
        vec![(3, 1)]
    );
}

#[test]
fn md089_footnote_opener_inside_an_html_attribute_protects_nothing() {
    let fixed = fixed("MD089", STD, "# T\n\n<span title=\"[^\">\u{4e2d}\u{6587}text]</span>\n");
    assert!(fixed.contains("\u{4e2d}\u{6587} text]"), "{fixed}");
}
