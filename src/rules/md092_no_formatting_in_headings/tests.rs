//! Behaviour matrix for MD092. Every reported row asserts what was found and
//! where; every kept row asserts silence, so a rule that simply stopped firing
//! fails the reported rows instead of passing everything.

use super::MD092NoFormattingInHeadings;
use super::md092_config::MD092Config;
use crate::config::MarkdownFlavor;
use crate::lint_context::LintContext;
use crate::rule::{LintWarning, Rule};

fn rule() -> MD092NoFormattingInHeadings {
    MD092NoFormattingInHeadings::default()
}

fn with_config(code: bool, strong: bool, emphasis: bool) -> MD092NoFormattingInHeadings {
    MD092NoFormattingInHeadings::from_config_struct(MD092Config { code, strong, emphasis })
}

fn check_with(rule: &MD092NoFormattingInHeadings, content: &str) -> Vec<LintWarning> {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    rule.check(&ctx).unwrap()
}

fn check(content: &str) -> Vec<LintWarning> {
    check_with(&rule(), content)
}

/// The rule reports exactly `expected` messages, and `fix` leaves the document
/// alone whether or not anything was reported.
fn assert_messages(content: &str, expected: &[&str]) {
    let warnings = check(content);
    let messages: Vec<&str> = warnings.iter().map(|warning| warning.message.as_str()).collect();
    assert_eq!(messages, expected, "messages for {content:?}");

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(
        rule().fix(&ctx).unwrap(),
        content,
        "MD092 reports only, so fix must never rewrite {content:?}"
    );
}

fn assert_silent(content: &str) {
    assert_messages(content, &[]);
}

#[test]
fn code_span_in_an_atx_heading() {
    assert_messages("## Method `map()`\n", &["Inline code in heading: `map()`"]);
}

#[test]
fn strong_in_an_atx_heading() {
    assert_messages("### **Practice**\n", &["Strong emphasis in heading: **Practice**"]);
}

#[test]
fn emphasis_in_an_atx_heading() {
    assert_messages("## The _.env_ file\n", &["Emphasis in heading: _.env_"]);
}

/// The accidental case: a path is not marked up at all in the author's mind,
/// but `__` around `tests` is strong emphasis, and the rendered heading loses
/// the underscores.
#[test]
fn a_path_that_became_strong_emphasis_by_itself() {
    assert_messages("## __tests__/gt.test.js\n", &["Strong emphasis in heading: __tests__"]);
}

#[test]
fn code_span_in_a_setext_heading() {
    assert_messages(
        "Heading with `code`\n-------------------\n",
        &["Inline code in heading: `code`"],
    );
}

#[test]
fn several_constructs_in_one_heading_are_reported_in_source_order() {
    assert_messages(
        "## `first` and **second** and _third_\n",
        &[
            "Inline code in heading: `first`",
            "Strong emphasis in heading: **second**",
            "Emphasis in heading: _third_",
        ],
    );
}

#[test]
fn the_position_points_at_the_span() {
    let warnings = check("## Method `map()`\n");
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 1);
    assert_eq!(warnings[0].column, 11);
    assert_eq!(warnings[0].end_column, 18);
}

#[test]
fn a_multibyte_heading_keeps_character_columns() {
    let warnings = check("## Метод `map()`\n");
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].column, 10);
}

/// A link in a heading is not a finding, but markup inside its text is: the
/// link text is what a table of contents carries.
#[test]
fn markup_inside_a_link_in_a_heading_is_reported() {
    assert_messages(
        "## [The `map()` docs](https://example.com)\n",
        &["Inline code in heading: `map()`"],
    );
}

#[test]
fn a_plain_link_in_a_heading_is_silent() {
    assert_silent("## [The docs](https://example.com)\n");
}

/// The heading set comes from `ctx.headings()`, which includes headings inside
/// blockquotes; `ctx.valid_headings()` would not. This pins the choice.
#[test]
fn a_heading_inside_a_blockquote_is_a_heading() {
    assert_messages(
        "> ## **Bold** and `code`\n",
        &["Strong emphasis in heading: **Bold**", "Inline code in heading: `code`"],
    );
}

#[test]
fn formatting_in_body_text_is_not_a_heading() {
    assert_silent("Text with `code`, **bold** and _italic_.\n");
}

#[test]
fn a_heading_inside_a_code_block_is_content() {
    assert_silent("```markdown\n## Method `map()`\n```\n");
}

/// rumdl keeps `##Text` as a heading whose space is missing rather than
/// dropping it, and MD018 reports the space, so the formatting inside it is
/// reported here as well.
#[test]
fn a_heading_missing_the_space_after_its_hashes_is_still_a_heading() {
    assert_messages("##Method `map()`\n", &["Inline code in heading: `map()`"]);
}

#[test]
fn a_plain_heading_is_silent() {
    assert_silent("## Method map()\n");
}

#[test]
fn each_construct_has_its_own_switch() {
    let content = "## `code` and **strong** and _emphasis_\n";

    let only_strong = check_with(&with_config(false, true, false), content);
    let messages: Vec<&str> = only_strong.iter().map(|w| w.message.as_str()).collect();
    assert_eq!(messages, ["Strong emphasis in heading: **strong**"]);

    let only_code = check_with(&with_config(true, false, false), content);
    let messages: Vec<&str> = only_code.iter().map(|w| w.message.as_str()).collect();
    assert_eq!(messages, ["Inline code in heading: `code`"]);

    assert!(check_with(&with_config(false, false, false), content).is_empty());
}

#[test]
fn a_long_span_is_shortened_in_the_message() {
    let warnings = check("## `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`\n");
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].message.ends_with("..."),
        "message was not shortened: {}",
        warnings[0].message
    );
}
