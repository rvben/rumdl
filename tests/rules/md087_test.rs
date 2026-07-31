//! MD087 driven through the real lint pipeline.
//!
//! The rule's verdict comes from what the run around it suppressed, so every test
//! here goes through `rumdl_lib::lint`, the entry point the CLI and the LSP both
//! use. Calling `check` on the rule alone can only ever produce nothing.
//!
//! Each case carries a control that must change the outcome: a comment reported as
//! unused is paired with content that makes the same comment necessary.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::rule::{LintWarning, Rule};
use rumdl_lib::rules::md013_line_length::md013_config::{MD013Config, ReflowMode};
use rumdl_lib::rules::{MD009TrailingSpaces, MD012NoMultipleBlanks, MD013LineLength, MD087UnusedDisableComment};

fn lint(content: &str, rules: &[Box<dyn Rule>]) -> Vec<LintWarning> {
    rumdl_lib::lint(content, rules, false, MarkdownFlavor::Standard, None, None).unwrap()
}

fn md013_and_md087() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(MD013LineLength::default()),
        Box::new(MD087UnusedDisableComment::new()),
    ]
}

fn messages(warnings: &[LintWarning]) -> Vec<&str> {
    warnings.iter().map(|warning| warning.message.as_str()).collect()
}

/// A line long enough for MD013's 80-character default, comment included.
fn over_long() -> String {
    "word ".repeat(30)
}

#[test]
fn unused_disable_line_is_reported() {
    let content = "Short line. <!-- rumdl-disable-line MD013 -->\n";
    let warnings = lint(content, &md013_and_md087());

    assert_eq!(messages(&warnings), ["Unused disable-line comment: MD013"]);
    assert_eq!(warnings[0].line, 1);
    assert_eq!(warnings[0].column, 13);
    assert_eq!(warnings[0].end_column, 46);
    assert_eq!(warnings[0].rule_name.as_deref(), Some("MD087"));
}

#[test]
fn a_comment_that_suppresses_a_finding_is_left_alone() {
    let used = format!("{} <!-- rumdl-disable-line MD013 -->\n", over_long());
    assert!(
        lint(&used, &md013_and_md087()).is_empty(),
        "the comment silences MD013 on this line, so it is doing its job"
    );

    // Control: MD013 is what the comment silences.
    let without_comment = format!("{}\n", over_long());
    assert_eq!(
        lint(&without_comment, &md013_and_md087()).len(),
        1,
        "without the comment the line reports"
    );
}

#[test]
fn a_rule_the_run_does_not_carry_is_not_judged() {
    let content = "Short line. <!-- rumdl-disable-line MD013 -->\n";
    let md087_only: Vec<Box<dyn Rule>> = vec![Box::new(MD087UnusedDisableComment::new())];

    assert!(
        lint(content, &md087_only).is_empty(),
        "MD013 is absent from the run, so its silence proves nothing"
    );
    assert_eq!(
        lint(content, &md013_and_md087()).len(),
        1,
        "control: the same comment is judged once MD013 runs"
    );
}

#[test]
fn disable_next_line_is_judged_against_the_following_line() {
    let unused = "<!-- rumdl-disable-next-line MD013 -->\nshort\n";
    assert_eq!(
        messages(&lint(unused, &md013_and_md087())),
        ["Unused disable-next-line comment: MD013"]
    );

    let used = format!("<!-- rumdl-disable-next-line MD013 -->\n{}\n", over_long());
    assert!(lint(&used, &md013_and_md087()).is_empty());
}

#[test]
fn a_block_disable_is_judged_to_the_end_of_the_document() {
    let unused = "<!-- rumdl-disable MD013 -->\n\nshort\n\nalso short\n";
    assert_eq!(
        messages(&lint(unused, &md013_and_md087())),
        ["Unused disable comment: MD013"]
    );

    let used = format!("<!-- rumdl-disable MD013 -->\n\nshort\n\n{}\n", over_long());
    assert!(
        lint(&used, &md013_and_md087()).is_empty(),
        "a finding anywhere below the comment keeps it alive"
    );
}

#[test]
fn disable_file_covers_lines_above_the_comment() {
    let used = format!("{}\n\n<!-- rumdl-disable-file MD013 -->\n", over_long());
    assert!(
        lint(&used, &md013_and_md087()).is_empty(),
        "a file-scope comment reaches back over the whole document"
    );

    let unused = "short\n\n<!-- rumdl-disable-file MD013 -->\n";
    assert_eq!(
        messages(&lint(unused, &md013_and_md087())),
        ["Unused disable-file comment: MD013"]
    );
}

#[test]
fn a_configure_file_entry_set_to_false_is_judged_like_a_disable() {
    let unused = "<!-- rumdl-configure-file { \"MD013\": false } -->\n\nshort\n";
    assert_eq!(
        messages(&lint(unused, &md013_and_md087())),
        ["Unused configure-file disable: MD013"]
    );

    let used = format!(
        "<!-- rumdl-configure-file {{ \"MD013\": false }} -->\n\n{}\n",
        over_long()
    );
    assert!(lint(&used, &md013_and_md087()).is_empty());
}

#[test]
fn a_comment_a_wider_one_already_covers_is_reported() {
    // The rule is off for the whole file, so the line comment silences nothing of
    // its own: removing it leaves the run reporting exactly what it does now.
    for wider in ["<!-- rumdl-disable-file MD013 -->", "<!-- rumdl-disable MD013 -->"] {
        let content = format!("{wider}\n\n{} <!-- rumdl-disable-line MD013 -->\n", over_long());
        assert_eq!(
            messages(&lint(&content, &md013_and_md087())),
            ["Unused disable-line comment: MD013"],
            "with {wider} above it"
        );
    }
}

#[test]
fn two_comments_covering_different_findings_are_both_left_alone() {
    // Control for the case above: neither comment is inside the other's reach, so
    // each is the only thing keeping its own line quiet.
    let content = format!(
        "{} <!-- rumdl-disable-line MD013 -->\n<!-- rumdl-disable MD013 -->\n{}\n",
        over_long(),
        over_long()
    );
    assert!(lint(&content, &md013_and_md087()).is_empty());
}

#[test]
fn a_block_disable_a_later_enable_closes_is_reported() {
    // The line comment does the work below the enable, so the block disable is the
    // one suppressing nothing.
    let content = format!(
        "<!-- rumdl-disable MD013 -->\n<!-- rumdl-enable MD013 -->\n{} <!-- rumdl-disable-line MD013 -->\n",
        over_long()
    );
    assert_eq!(
        messages(&lint(&content, &md013_and_md087())),
        ["Unused disable comment: MD013"]
    );
}

/// MD013 normalizing a paragraph reports it as one finding spanning every line of
/// it, which is what makes a range wider than a single line available here.
fn md013_reflow_and_md087() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(MD013LineLength::from_config_struct(MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        })),
        Box::new(MD087UnusedDisableComment::new()),
    ]
}

#[test]
fn only_the_first_covered_line_of_a_multi_line_finding_credits_a_comment() {
    let first = "This paragraph line is quite long and would normally be reflowed by the rule into shorter lines.";
    let second = "Second line of that same paragraph, also fairly long so that the whole block moves around a lot.";
    let comment = "<!-- rumdl-disable-line MD013 -->";
    let rules = md013_reflow_and_md087();

    let warnings = lint(&format!("{first}\n{second}\n"), &rules);
    assert_eq!(warnings.len(), 1, "got: {warnings:?}");
    assert_eq!(
        (warnings[0].line, warnings[0].end_line),
        (1, 2),
        "the finding has to span both lines for this test to mean anything"
    );

    // Either comment on its own is the only thing silencing that finding.
    for content in [
        format!("{first} {comment}\n{second}\n"),
        format!("{first}\n{second} {comment}\n"),
    ] {
        assert!(
            lint(&content, &rules).is_empty(),
            "one comment does the whole job: {content:?}"
        );
    }

    // Together, the first covered line silences the finding and the second comment
    // is left with nothing of its own to do.
    let warnings = lint(&format!("{first} {comment}\n{second} {comment}\n"), &rules);
    assert_eq!(messages(&warnings), ["Unused disable-line comment: MD013"]);
    assert_eq!(warnings[0].line, 2);
}

#[test]
fn a_configure_file_entry_carrying_options_is_left_alone() {
    // Options reconfigure a rule rather than switching it off, so there is no
    // suppression to look for.
    let content = "<!-- rumdl-configure-file { \"MD013\": { \"line_length\": 200 } } -->\n\nshort\n";
    assert!(lint(content, &md013_and_md087()).is_empty());
}

#[test]
fn only_the_names_that_suppressed_nothing_are_reported() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD013LineLength::default()),
        Box::new(MD087UnusedDisableComment::new()),
    ];
    let content = "<!-- rumdl-disable-next-line MD009 MD013 -->\nshort line   \n";

    assert_eq!(
        messages(&lint(content, &rules)),
        ["Unused disable-next-line comment: MD013"],
        "MD009 fires on the covered line, MD013 does not"
    );

    // Control: MD009 is genuinely reporting on that line.
    let without_comment = "short line   \n";
    assert_eq!(lint(without_comment, &rules).len(), 1);
}

#[test]
fn a_comment_naming_no_rule_is_never_reported() {
    for content in [
        "Short line. <!-- rumdl-disable-line -->\n",
        "<!-- rumdl-disable -->\n\nshort\n",
        "<!-- rumdl-disable-file -->\n\nshort\n",
    ] {
        assert!(
            lint(content, &md013_and_md087()).is_empty(),
            "a rule-less comment covers rules this run may not carry: {content:?}"
        );
    }
}

#[test]
fn a_comment_inside_a_code_block_is_not_a_comment() {
    let content = "```markdown\nShort line. <!-- rumdl-disable-line MD013 -->\n```\n";
    assert!(lint(content, &md013_and_md087()).is_empty());
}

#[test]
fn an_unknown_rule_name_is_left_to_the_inline_config_warning() {
    let content = "Short line. <!-- rumdl-disable-line MD999 -->\n";
    assert!(lint(content, &md013_and_md087()).is_empty());
}

#[test]
fn a_prettier_ignore_comment_belongs_to_another_formatter() {
    let content = "<!-- prettier-ignore -->\nshort\n";
    assert!(lint(content, &md013_and_md087()).is_empty());
}

#[test]
fn the_markdownlint_prefix_is_judged_the_same_way() {
    let content = "Short line. <!-- markdownlint-disable-line MD013 -->\n";
    assert_eq!(
        messages(&lint(content, &md013_and_md087())),
        ["Unused disable-line comment: MD013"]
    );
}

#[test]
fn an_alias_is_reported_as_the_author_wrote_it() {
    let content = "Short line. <!-- rumdl-disable-line line-length -->\n";
    assert_eq!(
        messages(&lint(content, &md013_and_md087())),
        ["Unused disable-line comment: line-length"]
    );

    let used = format!("{} <!-- rumdl-disable-line line-length -->\n", over_long());
    assert!(
        lint(&used, &md013_and_md087()).is_empty(),
        "the alias disables the same rule it is judged against"
    );
}

#[test]
fn md087_can_be_disabled_inline_like_any_other_rule() {
    let content = "Short. <!-- rumdl-disable-line MD013 --> <!-- rumdl-disable-line MD087 -->\n";
    assert!(lint(content, &md013_and_md087()).is_empty());

    let without = "Short. <!-- rumdl-disable-line MD013 -->\n";
    assert_eq!(
        lint(without, &md013_and_md087()).len(),
        1,
        "control: the finding is there to be suppressed"
    );
}

#[test]
fn a_comment_is_judged_per_rule_it_names() {
    // Two comments, one live and one dead, for two rules that report on different
    // lines. Each verdict has to come from its own rule's findings.
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD012NoMultipleBlanks::default()),
        Box::new(MD013LineLength::default()),
        Box::new(MD087UnusedDisableComment::new()),
    ];
    let content = format!(
        "{} <!-- rumdl-disable-line MD013 -->\n\n\n<!-- rumdl-disable-next-line MD012 -->\nlast\n",
        over_long()
    );

    let warnings = lint(&content, &rules);
    let verdicts: Vec<&str> = warnings
        .iter()
        .filter(|warning| warning.rule_name.as_deref() == Some("MD087"))
        .map(|warning| warning.message.as_str())
        .collect();

    assert_eq!(
        verdicts,
        ["Unused disable-next-line comment: MD012"],
        "MD013's comment silences the long line; MD012 reports above its comment, not below it"
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.rule_name.as_deref() == Some("MD012")),
        "control: MD012 is reporting, which is what makes its comment's silence meaningful"
    );
}

#[test]
fn check_on_its_own_reports_nothing() {
    // The rule contributes through check_suppressions. A caller that only runs
    // check must see an empty result rather than a half-formed verdict.
    let rule = MD087UnusedDisableComment::new();
    let ctx = rumdl_lib::lint_context::LintContext::new(
        "Short line. <!-- rumdl-disable-line MD013 -->\n",
        MarkdownFlavor::Standard,
        None,
    );
    assert!(rule.check(&ctx).unwrap().is_empty());
}
