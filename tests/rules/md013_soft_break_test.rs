//! A soft line break is joined the same way in every container reflow handles.
//!
//! Each case writes a trailing space before the break, which is not a hard
//! break (that needs two), so the reflowed line must hold exactly one space
//! where the break was. Each case holds two sentences so that every reflowing
//! mode rewrites it, and the output must be a fixed point: `check` reports
//! nothing and a second `fix` changes nothing.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD013LineLength;
use rumdl_lib::rules::md013_line_length::md013_config::{MD013Config, ReflowMode};
use rumdl_lib::types::LineLength;

const MODES: [ReflowMode; 3] = [
    ReflowMode::Normalize,
    ReflowMode::SentencePerLine,
    ReflowMode::SemanticLineBreaks,
];

fn rule(mode: ReflowMode, line_length: usize) -> MD013LineLength {
    MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(line_length),
        reflow: true,
        reflow_mode: mode,
        ..Default::default()
    })
}

fn fix(rule: &MD013LineLength, content: &str, flavor: MarkdownFlavor) -> String {
    let ctx = LintContext::new(content, flavor, None);
    rule.fix(&ctx).unwrap()
}

/// Reflows `input` in every mode and asserts the result is a fixed point equal
/// to `one_line` in normalize mode and to `per_sentence` in the sentence modes.
fn assert_joined(input: &str, one_line: &str, per_sentence: &str, flavor: MarkdownFlavor) {
    for mode in MODES {
        let rule = rule(mode, 200);
        let fixed = fix(&rule, input, flavor);
        let expected = if mode == ReflowMode::Normalize {
            one_line
        } else {
            per_sentence
        };
        assert_eq!(fixed, expected, "{mode:?}: reflow of {input:?}");

        let ctx = LintContext::new(&fixed, flavor, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty(), "{mode:?}: check flags fixed output: {warnings:?}");
        assert_eq!(fix(&rule, &fixed, flavor), fixed, "{mode:?}: second fix changed output");
    }
}

#[test]
fn paragraph() {
    assert_joined(
        "Alpha beta gamma delta \nepsilon zeta. Eta theta.\n",
        "Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "Alpha beta gamma delta epsilon zeta.\nEta theta.\n",
        MarkdownFlavor::Standard,
    );
}

#[test]
fn list_item() {
    assert_joined(
        "- Alpha beta gamma delta \n  epsilon zeta. Eta theta.\n",
        "- Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "- Alpha beta gamma delta epsilon zeta.\n  Eta theta.\n",
        MarkdownFlavor::Standard,
    );
}

#[test]
fn list_item_second_paragraph() {
    assert_joined(
        "- First.\n\n  Alpha beta gamma delta \n  epsilon zeta. Eta theta.\n",
        "- First.\n\n  Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "- First.\n\n  Alpha beta gamma delta epsilon zeta.\n  Eta theta.\n",
        MarkdownFlavor::Standard,
    );
}

#[test]
fn blockquote() {
    assert_joined(
        "> Alpha beta gamma delta \n> epsilon zeta. Eta theta.\n",
        "> Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "> Alpha beta gamma delta epsilon zeta.\n> Eta theta.\n",
        MarkdownFlavor::Standard,
    );
}

#[test]
fn definition() {
    assert_joined(
        "Term\n: Alpha beta gamma delta \n  epsilon zeta. Eta theta.\n",
        "Term\n: Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "Term\n: Alpha beta gamma delta epsilon zeta.\n    Eta theta.\n",
        MarkdownFlavor::MkDocs,
    );
}

#[test]
fn mkdocs_admonition() {
    assert_joined(
        "!!! note\n    Alpha beta gamma delta \n    epsilon zeta. Eta theta.\n",
        "!!! note\n    Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "!!! note\n    Alpha beta gamma delta epsilon zeta.\n    Eta theta.\n",
        MarkdownFlavor::MkDocs,
    );
}

#[test]
fn mkdocs_content_tab() {
    assert_joined(
        "=== \"Tab\"\n\n    Alpha beta gamma delta \n    epsilon zeta. Eta theta.\n",
        "=== \"Tab\"\n\n    Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "=== \"Tab\"\n\n    Alpha beta gamma delta epsilon zeta.\n    Eta theta.\n",
        MarkdownFlavor::MkDocs,
    );
}

/// Footnotes are only reflowed when a line exceeds the limit, so this case
/// overflows it and checks that every rewrapped line is single-spaced.
#[test]
fn footnote() {
    let input = "Text[^1].\n\n[^1]: Alpha beta gamma delta \n    epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi.\n";
    for mode in MODES {
        let rule = rule(mode, 40);
        let fixed = fix(&rule, input, MarkdownFlavor::Standard);
        assert_ne!(fixed, input, "{mode:?}: footnote was not reflowed");
        assert!(!fixed.contains("delta \n"), "{mode:?}: trailing space kept: {fixed:?}");
        for line in fixed.lines() {
            assert!(!line.trim_start().contains("  "), "{mode:?}: double space in {line:?}");
        }
        assert_eq!(
            fix(&rule, &fixed, MarkdownFlavor::Standard),
            fixed,
            "{mode:?}: second fix changed output"
        );
    }
}

/// Semantic mode puts each CJK sentence on its own line and never merges a short
/// one back onto the previous sentence, which would insert a space the author
/// never wrote.
#[test]
fn semantic_mode_keeps_cjk_sentences_apart() {
    for line_length in [0, 200] {
        let rule = rule(ReflowMode::SemanticLineBreaks, line_length);
        for (input, expected) in [
            ("文字结束。第二句。\n", "文字结束。\n第二句。\n"),
            ("真的吗？是的！好。\n", "真的吗？\n是的！\n好。\n"),
            ("他说「结束。」第二句。\n", "他说「结束。」\n第二句。\n"),
        ] {
            let fixed = fix(&rule, input, MarkdownFlavor::Standard);
            assert_eq!(fixed, expected, "line-length {line_length}: reflow of {input:?}");
            assert!(
                rule.check(&LintContext::new(&fixed, MarkdownFlavor::Standard, None))
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                fix(&rule, &fixed, MarkdownFlavor::Standard),
                fixed,
                "line-length {line_length}: second fix of {input:?}"
            );
        }
    }
}
