//! Flavor-level behavior for Markdown with Gherkin (`.feature.md`).
//!
//! MDG only exempts linting that would stop a document from parsing as Gherkin.
//! Everything else is steered toward the single form Gherkin accepts, so these
//! tests pin both directions: the corrections that must happen and the ones
//! that must be withheld.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::heading_utils::HeadingStyle;
use rumdl_lib::rules::{
    CodeBlockStyle, CodeFenceStyle, MD003HeadingStyle, MD046CodeBlockStyle, MD048CodeFenceStyle, MD060TableFormat,
};

fn examples_table(indent: usize) -> String {
    let spaces = " ".repeat(indent);
    format!(
        "# Feature: Eating\n\n#### Examples:\n\n{spaces}| start | eat | left |\n{spaces}|---|---|---|\n{spaces}| 12 | 5 | 7 |\n"
    )
}

fn data_table(indent: usize) -> String {
    let spaces = " ".repeat(indent);
    format!(
        "# Feature: Eating\n\n## Scenario: Eat\n\n* Given these items\n{spaces}| start | eat | left |\n{spaces}|---|---|---|\n{spaces}| 12 | 5 | 7 |\n"
    )
}

/// Two spaces is always a valid Gherkin table indent, so 3-5 are corrected to it
/// rather than preserved.
#[test]
fn md060_normalizes_mdg_table_indentation_to_two_spaces() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    let expected = "# Feature: Eating\n\n#### Examples:\n\n  | start | eat | left |\n  | ----- | --- | ---- |\n  | 12    | 5   | 7    |\n";

    for indent in [2, 3, 4, 5] {
        let input = examples_table(indent);
        let ctx = LintContext::new(&input, MarkdownFlavor::MDG, None);

        assert!(
            !rule.check(&ctx).unwrap().is_empty(),
            "the unaligned {indent}-space MDG table should be reported"
        );
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected, "MD060 must normalize a {indent}-space indent to 2");

        let fixed_ctx = LintContext::new(&fixed, MarkdownFlavor::MDG, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed, "MD060 fix must be idempotent");
    }
}

#[test]
fn md060_normalizes_data_table_indent_under_step() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    let expected = "# Feature: Eating\n\n## Scenario: Eat\n\n* Given these items\n  | start | eat | left |\n  | ----- | --- | ---- |\n  | 12    | 5   | 7    |\n";

    for indent in [2, 3, 4, 5] {
        let input = data_table(indent);
        let ctx = LintContext::new(&input, MarkdownFlavor::MDG, None);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected, "MD060 must normalize Data Table indent {indent} to 2");

        let fixed_ctx = LintContext::new(&fixed, MarkdownFlavor::MDG, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
    }
}

#[test]
fn md060_mdg_behavior_is_flavor_isolated() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    let input = examples_table(4);

    let mdg_fixed = rule.fix(&LintContext::new(&input, MarkdownFlavor::MDG, None)).unwrap();
    assert!(mdg_fixed.lines().any(|line| line.starts_with("  | start")));

    let standard_fixed = rule
        .fix(&LintContext::new(&input, MarkdownFlavor::Standard, None))
        .unwrap();
    assert!(standard_fixed.lines().any(|line| line.starts_with("| start")));
    assert!(!standard_fixed.lines().any(|line| line.starts_with("  | start")));
}

/// MDG parses `#{1,6} ` headings only, so every heading is steered to plain ATX
/// regardless of the configured style.
#[test]
fn md003_converges_on_plain_atx_under_mdg() {
    let input = "Feature: Checkout\n=================\n\n## Scenario: Purchase ##\n\nNotes\n-----\n";
    let expected = "# Feature: Checkout\n\n## Scenario: Purchase\n\n## Notes\n";

    for style in [
        HeadingStyle::Consistent,
        HeadingStyle::Atx,
        HeadingStyle::AtxClosed,
        HeadingStyle::Setext1,
        HeadingStyle::Setext2,
        HeadingStyle::SetextWithAtx,
        HeadingStyle::SetextWithAtxClosed,
    ] {
        let rule = MD003HeadingStyle::new(style);
        let ctx = LintContext::new(input, MarkdownFlavor::MDG, None);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected, "MDG must converge on plain ATX for {style:?}");

        let fixed_ctx = LintContext::new(&fixed, MarkdownFlavor::MDG, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
    }
}

/// Only a backtick fence can be a Doc String, so MDG converges on backtick
/// fences and never rewrites one into a tilde fence or an indented block.
#[test]
fn md046_and_md048_converge_on_backtick_fences_under_mdg() {
    let input = "# Feature: Payloads\n\n## Scenario: Mixed\n\n* Given a payload\n\n~~~text\nfrom a tilde fence\n~~~\n\nPlain sample:\n\n    from an indented block\n";

    let fixed = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent)
        .fix(&LintContext::new(input, MarkdownFlavor::MDG, None))
        .unwrap();
    let fixed = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent)
        .fix(&LintContext::new(&fixed, MarkdownFlavor::MDG, None))
        .unwrap();

    assert!(!fixed.contains("~~~"), "no tilde fence may survive: {fixed:?}");
    assert!(
        fixed.contains("from an indented block") && fixed.contains("```"),
        "the indented block must be fenced: {fixed:?}"
    );

    // Withheld directions: an explicit indented or tilde style would delete Doc
    // Strings, so neither is applied.
    let backticks =
        "# Feature: Payloads\n\n## Scenario: Doc String\n\n* Given a payload\n\n  ```json\n  {\"ok\": true}\n  ```\n";
    for (name, fixed) in [
        (
            "MD046 indented",
            MD046CodeBlockStyle::new(CodeBlockStyle::Indented)
                .fix(&LintContext::new(backticks, MarkdownFlavor::MDG, None))
                .unwrap(),
        ),
        (
            "MD048 tilde",
            MD048CodeFenceStyle::new(CodeFenceStyle::Tilde)
                .fix(&LintContext::new(backticks, MarkdownFlavor::MDG, None))
                .unwrap(),
        ),
    ] {
        assert_eq!(fixed, backticks, "{name} must be withheld under MDG");
    }
}
