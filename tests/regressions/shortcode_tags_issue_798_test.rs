//! Regression tests for issue #798: a template shortcode tag is opaque markup.
//!
//! `{{% name %}}` and `{{< name >}}` are tokens a site generator resolves against
//! a template. Their bytes never reach the reader as Markdown, so a rule that
//! reads them as prose reports a finding no edit can satisfy, and one that
//! rewrites them changes what the template receives. MD044 turning
//! `{{% nodejs %}}` into `{{% Node.js %}}` points the invocation at a shortcode
//! that does not exist.
//!
//! What is opaque is the TAG, not the content between a paired opening and
//! closing tag: a `{{% note %}}` body is ordinary Markdown that the generator
//! renders, so it must keep being linted. Every case below is therefore paired
//! with a control carrying the identical construct in prose, because a guard
//! that silences the control has not fixed the rule, it has broken it.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{
    MD033NoInlineHtml, MD037NoSpaceInEmphasis, MD038NoSpaceInCode, MD044ProperNames, MD045NoAltText,
    MD057ExistingRelativeLinks, MD085ParagraphContinuationIndent,
};
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

fn md044() -> MD044ProperNames {
    MD044ProperNames::new(vec!["Node.js".to_string()], false)
}

/// Assert a rule is silent on `content` and leaves every byte of it alone.
fn assert_opaque(rule: &dyn Rule, label: &str, content: &str) {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).expect("check must succeed");
    assert!(
        warnings.is_empty(),
        "{} must report nothing inside a shortcode tag ({label}): {warnings:?}",
        rule.name()
    );
    assert_eq!(
        rule.fix(&ctx).expect("fix must succeed"),
        content,
        "{} must not rewrite a shortcode tag ({label})",
        rule.name()
    );
}

/// Assert a rule still speaks for the same construct written as ordinary prose.
///
/// This is the half that fails if a guard is too wide, and it is asserted for
/// every rule rather than once, because each rule reaches the guard by its own
/// path.
fn assert_still_linted(rule: &dyn Rule, label: &str, content: &str) {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).expect("check must succeed");
    assert!(
        !warnings.is_empty(),
        "{} must still report {label} outside a shortcode tag",
        rule.name()
    );
}

/// The reported case, plus the delimiter and placement variations a Hugo site
/// actually contains.
#[test]
fn md044_leaves_proper_names_in_a_shortcode_tag_alone() {
    let rule = md044();

    for (label, content) in [
        // The reported case.
        ("percent delimiters", "{{% nodejs %}}\n"),
        ("angle delimiters", "{{< nodejs >}}\n"),
        // Hugo's whitespace-trimming delimiters.
        ("whitespace-trim delimiters", "{{%- nodejs -%}}\n"),
        // A name inside a named parameter, which is where it usually appears.
        ("named parameters", "{{< figure src=\"nodejs.png\" alt=\"nodejs\" >}}\n"),
        // A tag whose arguments are laid out over several lines.
        ("multi-line tag", "{{< figure\n   alt=\"nodejs\"\n>}}\n"),
        // A closing tag of a pair.
        ("closing tag of a pair", "{{% /nodejs %}}\n"),
    ] {
        assert_opaque(&rule, label, content);
    }

    assert_still_linted(&rule, "a proper name", "nodejs in plain prose\n");
}

/// The guard is a byte range over the tag, not a flag on the line.
///
/// A line holding a tag and prose must have only its prose half rewritten. A
/// line-scoped guard passes the tests above and fails this one.
#[test]
fn md044_rewrites_prose_on_the_same_line_as_a_shortcode_tag() {
    let content = "{{% x nodejs %}} then nodejs after\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = md044();

    let warnings = rule.check(&ctx).expect("check must succeed");
    assert_eq!(
        warnings.len(),
        1,
        "only the occurrence outside the tag is a proper name: {warnings:?}"
    );
    assert_eq!(
        rule.fix(&ctx).expect("fix must succeed"),
        "{{% x nodejs %}} then Node.js after\n",
        "the occurrence inside the tag must survive the rewrite of the one beside it"
    );
}

/// The body between a paired opening and closing tag is rendered as Markdown, so
/// it is not covered by the exemption.
#[test]
fn a_paired_shortcode_body_is_still_linted() {
    let content = "{{% note %}}\nnodejs in the body\n{{% /note %}}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = md044();

    let warnings = rule.check(&ctx).expect("check must succeed");
    assert_eq!(warnings.len(), 1, "the body between the tags is prose: {warnings:?}");
    assert_eq!(warnings[0].line, 2, "the finding belongs to the body line");
    assert_eq!(
        rule.fix(&ctx).expect("fix must succeed"),
        "{{% note %}}\nNode.js in the body\n{{% /note %}}\n",
        "both tags must survive while the body is rewritten"
    );
}

/// `*` and `_` in a tag are literal characters the template receives, so closing
/// the spaces around them changes its arguments.
#[test]
fn md037_leaves_emphasis_markers_in_a_shortcode_tag_alone() {
    let rule = MD037NoSpaceInEmphasis;

    for (label, content) in [
        ("percent delimiters", "{{% a * y * b %}}\n"),
        ("angle delimiters", "{{< a * y * b >}}\n"),
        ("underscore markers", "{{% a _ y _ b %}}\n"),
    ] {
        assert_opaque(&rule, label, content);
    }

    assert_still_linted(&rule, "spaced emphasis", "a * y * b\n");
}

/// A backtick anywhere in a tag is part of the template's arguments, not a code
/// span whose padding rumdl may trim.
///
/// MD038 already exempted a backtick attached to the opening delimiter
/// (`{{raw `...`}}`), which is why the argument forms below were still rewritten.
#[test]
fn md038_leaves_code_spans_in_a_shortcode_tag_alone() {
    let rule = MD038NoSpaceInCode::new();

    for (label, content) in [
        ("percent delimiters", "{{% note `code ` %}}\n"),
        ("angle delimiters", "{{< note `code ` >}}\n"),
        ("leading space in the span", "{{% note ` code` %}}\n"),
    ] {
        assert_opaque(&rule, label, content);
    }

    assert_still_linted(&rule, "a padded code span", "a `code ` b\n");
}

/// How a multi-line tag lays its arguments out is the author's, not paragraph
/// indentation to strip.
#[test]
fn md085_leaves_a_multi_line_shortcode_tag_alone() {
    let rule = MD085ParagraphContinuationIndent;

    for (label, content) in [
        (
            "angle delimiters",
            "Some paragraph text.\n{{< figure\n   title=\"a\"\n>}}\n",
        ),
        (
            "percent delimiters",
            "Some paragraph text.\n{{% note\n   title=\"a\"\n%}}\n",
        ),
    ] {
        assert_opaque(&rule, label, content);
    }

    assert_still_linted(
        &rule,
        "an indented continuation line",
        "Some paragraph text.\n   continued here\n",
    );
}

/// `<b>` written in a tag is a string the template receives, not HTML the
/// document emits.
#[test]
fn md033_leaves_html_in_a_shortcode_tag_alone() {
    let rule = MD033NoInlineHtml::new();

    for (label, content) in [
        ("percent delimiters", "{{% note <b>x</b> %}}\n"),
        ("angle delimiters", "{{< note <b>x</b> >}}\n"),
    ] {
        assert_opaque(&rule, label, content);
    }

    assert_still_linted(&rule, "inline HTML", "a <b>x</b> b\n");
}

/// Image syntax in a tag is a parameter, so there is no alt-text slot to fill.
#[test]
fn md045_leaves_images_in_a_shortcode_tag_alone() {
    let rule = MD045NoAltText::new();

    for (label, content) in [
        ("percent delimiters", "{{% note ![](img.png) %}}\n"),
        ("angle delimiters", "{{< note ![](img.png) >}}\n"),
    ] {
        assert_opaque(&rule, label, content);
    }

    assert_still_linted(&rule, "an image without alt text", "a ![](img.png) b\n");
}

/// A path in a tag is resolved by the site generator's own rules, not relative to
/// the file holding it. Both of MD057's paths are covered: links are found by
/// scanning the line, images come from the parsed image list.
#[test]
fn md057_leaves_paths_in_a_shortcode_tag_alone() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();
    File::create(base_path.join("exists.md"))
        .unwrap()
        .write_all(b"# Test File")
        .unwrap();

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

    for (label, content) in [
        ("link in a percent tag", "{{% note [t](missing.md) %}}\n"),
        ("link in an angle tag", "{{< note [t](missing.md) >}}\n"),
        ("image in a percent tag", "{{% note ![alt](missing.png) %}}\n"),
        ("path in a named parameter", "{{< figure src=\"missing.png\" >}}\n"),
    ] {
        assert_opaque(&rule, label, content);
    }

    assert_still_linted(&rule, "a missing relative link", "a [t](missing.md) b\n");
    assert_still_linted(&rule, "a missing relative image", "a ![alt](missing.png) b\n");
}

/// Shortcode ranges are computed for every flavor, so the exemption does not
/// depend on the user having selected one.
#[test]
fn the_exemption_does_not_depend_on_the_flavor() {
    let rule = md044();
    let content = "{{% nodejs %}}\n";

    for flavor in [
        MarkdownFlavor::Standard,
        MarkdownFlavor::MkDocs,
        MarkdownFlavor::Obsidian,
    ] {
        let ctx = LintContext::new(content, flavor, None);
        assert!(
            rule.check(&ctx).expect("check must succeed").is_empty(),
            "MD044 reported inside a shortcode tag under {flavor:?}"
        );
    }
}
