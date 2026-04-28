//! End-to-end guards for `rumdl fmt`: the fix coordinator must dispatch to
//! `Rule::fix()` for rules that advertise `FixCapability != Unfixable`, even
//! when individual warnings carry no inline `Fix` struct (e.g. document-level
//! rewrites in MD036, MD046, MD076).
//!
//! Each test exercises the same code path the binary uses
//! (`apply_fixes_iterative`) so a regression in dispatch surfaces here, not
//! only in CLI output.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{
    CodeBlockStyle, ListItemSpacingStyle, MD036NoEmphasisAsHeading, MD046CodeBlockStyle, MD076ListItemSpacing,
    all_rules,
};

/// Run a rule end-to-end through the FixCoordinator (the same path `rumdl fmt`
/// takes via process_file_with_formatter -> apply_fixes_coordinated).
fn fmt_through_coordinator(content: &str, rules: Vec<Box<dyn Rule>>) -> String {
    let coordinator = FixCoordinator::new();
    let config = Config::default();
    let mut buf = content.to_string();
    coordinator
        .apply_fixes_iterative(&rules, &[], &mut buf, &config, 100, None)
        .expect("fix coordinator must not error");
    buf
}

// ── MD036 ────────────────────────────────────────────────────────────────

#[test]
fn md036_fmt_promotes_emphasis_to_heading_with_default_config() {
    // The rule advertises FullyFixable, so `rumdl fmt` with default config
    // must rewrite the bare emphasis line into a real heading.
    let before = "# Heading\n\n**Looks like a heading**\n\nSome content.\n";
    let expected = "# Heading\n\n## Looks like a heading\n\nSome content.\n";

    let rule = MD036NoEmphasisAsHeading::from_config(&Config::default());
    let after = fmt_through_coordinator(before, vec![rule]);

    assert_eq!(after, expected);
}

// ── MD076 ────────────────────────────────────────────────────────────────

#[test]
fn md076_fmt_normalizes_to_consistent_tight_list() {
    // 4 items → 3 transitions: [tight, tight, loose] → 2 tight vs 1 loose.
    // The "consistent" mode normalizes the loose outlier to tight.
    let before = "- a\n- b\n- c\n\n- d\n";
    let expected = "- a\n- b\n- c\n- d\n";

    let rule: Box<dyn Rule> = Box::new(MD076ListItemSpacing::new(ListItemSpacingStyle::Consistent));
    let after = fmt_through_coordinator(before, vec![rule]);

    assert_eq!(after, expected);
}

#[test]
fn md076_fmt_tie_prefers_tight() {
    // 3 items → 2 transitions: [tight, loose] → tied. Prefer tight on ties:
    //   - matches the minimal-whitespace convention used by Prettier and
    //     other formatters,
    //   - is the dominant style in real-world Markdown documents (most lists
    //     are tight; loose is opt-in for multi-paragraph items),
    //   - removes the unexpected blank line rather than inserting another one,
    //     which is the lower-impact edit on a tied document.
    let before = "- item one\n- item two\n\n- item three\n";
    let expected = "- item one\n- item two\n- item three\n";

    let rule: Box<dyn Rule> = Box::new(MD076ListItemSpacing::new(ListItemSpacingStyle::Consistent));
    let after = fmt_through_coordinator(before, vec![rule]);

    assert_eq!(after, expected);
}

#[test]
fn md076_fmt_removes_unexpected_blank_under_tight_style() {
    // Tight-style list with a stray blank line between items: fmt must remove
    // the blank.
    let before = "- alpha\n- beta\n\n- gamma\n- delta\n";
    let expected = "- alpha\n- beta\n- gamma\n- delta\n";

    let rule: Box<dyn Rule> = Box::new(MD076ListItemSpacing::new(ListItemSpacingStyle::Tight));
    let after = fmt_through_coordinator(before, vec![rule]);

    assert_eq!(after, expected);
}

#[test]
fn md076_fmt_inserts_missing_blank_under_loose_style() {
    // Loose-style list missing a blank line: fmt must insert one.
    let before = "- alpha\n\n- beta\n- gamma\n";
    let expected = "- alpha\n\n- beta\n\n- gamma\n";

    let rule: Box<dyn Rule> = Box::new(MD076ListItemSpacing::new(ListItemSpacingStyle::Loose));
    let after = fmt_through_coordinator(before, vec![rule]);

    assert_eq!(after, expected);
}

// ── MD046 ────────────────────────────────────────────────────────────────

#[test]
fn md046_fmt_converts_indented_to_fenced() {
    // A bare indented code block should be wrapped in fences when the target
    // style is fenced.
    let before = "Intro paragraph.\n\n    indented code\n    more code\n\nTrailing paragraph.\n";
    let expected = "Intro paragraph.\n\n```\nindented code\nmore code\n```\n\nTrailing paragraph.\n";

    let rule: Box<dyn Rule> = Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Fenced));
    let after = fmt_through_coordinator(before, vec![rule]);

    assert_eq!(after, expected);
}

#[test]
fn md046_fmt_converts_list_internal_indented_to_fenced() {
    // The user's md046 repro: an indented code block inside a list item.
    // Per CommonMark, lines indented 4+ spaces past the list-item content
    // baseline (column 2 for `- `) are a code block within the list item.
    //
    // The fix must:
    //   - place the opening/closing fences at the list-item baseline
    //     (column 2) so the block stays attached to the bullet,
    //   - dedent the body so it sits at the same baseline (preserving any
    //     internal indentation past that point).
    let before = "- A list item\n\n      indented code block here\n      more code\n";
    let expected = "- A list item\n\n  ```\n  indented code block here\n  more code\n  ```\n";

    let rule: Box<dyn Rule> = Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Fenced));
    let after = fmt_through_coordinator(before, vec![rule]);

    assert_eq!(after, expected);
}

#[test]
fn md046_fmt_converts_fenced_to_indented() {
    // Symmetric direction: fenced -> indented when target style is indented.
    let before = "Intro.\n\n```\nfenced code\nmore code\n```\n\nTrailing.\n";
    let expected = "Intro.\n\n    fenced code\n    more code\n\nTrailing.\n";

    let rule: Box<dyn Rule> = Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Indented));
    let after = fmt_through_coordinator(before, vec![rule]);

    assert_eq!(after, expected);
}

// ── Regression guards on the warning surface ────────────────────────────

#[test]
fn md036_default_config_attaches_inline_fix_to_warnings() {
    // The advertised behavior is "Fix is always available." With default config,
    // warnings produced by check() must carry inline Fix objects so LSP code
    // actions can offer the rewrite.
    use rumdl_lib::lint_context::LintContext;

    let content = "# Title\n\n**Promote me**\n\nBody.\n";
    let rule = MD036NoEmphasisAsHeading::from_config(&Config::default());
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).expect("check must succeed");

    assert_eq!(warnings.len(), 1, "expected one MD036 warning");
    assert!(
        warnings[0].fix.is_some(),
        "MD036 warning must carry an inline Fix when fix capability is FullyFixable; got {:?}",
        warnings[0]
    );
}

// ── Consistency between check() and fix() under default Consistent style ──

#[test]
fn md046_consistent_no_warning_implies_no_fix_for_list_internal_indented_block() {
    // Under default `Consistent` style with a single indented code block
    // (the only block in the document), MD046 must:
    //   - Recognise the list-internal indented block via the same heuristic
    //     `check()` and `fix()` both consult, so `detect_style` picks
    //     `Indented`.
    //   - Emit zero warnings (the document is already self-consistent).
    //   - Leave the document unchanged when fmt runs the full rule set.
    //
    // The asymmetric failure mode this guard pins against: `check()`
    // detecting the block via pulldown-cmark's parsed view while
    // `detect_style` and the fix loop skip it via a list-context heuristic.
    // That asymmetry produces a warning fmt cannot act on.
    use rumdl_lib::lint_context::LintContext;

    let content = "- A list item\n\n      indented code block here\n      more code\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let rule: Box<dyn Rule> = Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Consistent));
    assert_eq!(
        rule.check(&ctx).unwrap().len(),
        0,
        "Consistent + lone list-internal indented block must be self-consistent (zero warnings)"
    );

    let after = fmt_through_coordinator(content, vec![rule]);
    assert_eq!(
        after, content,
        "Consistent + lone indented block must leave the document unchanged"
    );

    // Sanity: opting in to `style = \"fenced\"` still produces the rewrite.
    let fenced_rule: Box<dyn Rule> = Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Fenced));
    let after_fenced = fmt_through_coordinator(content, vec![fenced_rule]);
    assert!(
        after_fenced.contains("```"),
        "Explicit Fenced must convert the indented block. Got:\n{after_fenced}"
    );
}

#[test]
fn md046_full_rule_set_does_not_warn_without_fixing_under_default_config() {
    // End-to-end guard for the user's binary repro: with the full rule set
    // and default config (which selects `style = \"Consistent\"`),
    // /tmp/rumdl-repros/md046.md must converge — no warning AND no fix
    // attempted means a stable file with at most non-MD046 issues remaining.
    use rumdl_lib::lint_context::LintContext;

    let before = "- A list item\n\n      indented code block here\n      more code\n";
    let config = Config::default();
    let rules = all_rules(&config);

    let after = fmt_through_coordinator(before, rules);

    // The file is left unchanged because the document is self-consistent
    // under Consistent style. The invariant pinned here is "MD046 must not
    // warn without being able to fix" — not "Consistent should rewrite to
    // fenced".
    let ctx_after = LintContext::new(&after, MarkdownFlavor::Standard, None);
    let md046 = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let warnings = md046.check(&ctx_after).unwrap();
    assert_eq!(
        warnings.len(),
        0,
        "After fmt, MD046 must report zero warnings on the result. Output:\n{after}\n\
         Warnings: {warnings:?}"
    );
}
