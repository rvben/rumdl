use super::*;
use crate::config::MarkdownFlavor;
use crate::document_run::DocumentRun;
use crate::rule::{LintError, LintResult, Severity};
use proptest::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Keep the original check-then-fix implementation as an independent oracle.
#[derive(Clone)]
struct OriginalFixer(Box<dyn Rule>);

impl Rule for OriginalFixer {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn description(&self) -> &'static str {
        self.0.description()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self.0.as_any()
    }
    fn check(&self, ctx: &LintContext) -> LintResult {
        self.0.check(ctx)
    }
    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        self.0.fix(ctx)
    }
    fn should_skip(&self, ctx: &LintContext) -> bool {
        self.0.should_skip(ctx)
    }
    fn category(&self) -> crate::rule::RuleCategory {
        self.0.category()
    }
    fn skippable_by_category(&self) -> bool {
        self.0.skippable_by_category()
    }
    fn fix_capability(&self) -> FixCapability {
        self.0.fix_capability()
    }
    fn cross_file_scope(&self) -> crate::rule::CrossFileScope {
        self.0.cross_file_scope()
    }
    fn contribute_to_index(&self, ctx: &LintContext, index: &mut crate::workspace_index::FileIndex) {
        self.0.contribute_to_index(ctx, index);
    }
    fn observes_suppressions(&self) -> bool {
        self.0.observes_suppressions()
    }
    fn check_suppressions(&self, ctx: &LintContext, report: &crate::rule::SuppressionReport) -> LintResult {
        self.0.check_suppressions(ctx, report)
    }
}

#[derive(Clone)]
struct EditProbe {
    name: &'static str,
    checks: Arc<AtomicUsize>,
    from: &'static str,
    to: &'static str,
}

#[test]
fn consumes_prepared_native_outputs_only_on_the_checked_revision() {
    #[derive(Clone)]
    struct PreparedProbe {
        from: &'static str,
        to: &'static str,
        fix_calls: Arc<AtomicUsize>,
    }
    impl Rule for PreparedProbe {
        fn name(&self) -> &'static str {
            "PREPARED_PROBE"
        }
        fn description(&self) -> &'static str {
            "Prepared result on one configured instance and source"
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn check(&self, ctx: &LintContext) -> LintResult {
            Ok(ctx
                .content
                .contains(self.from)
                .then(|| LintWarning {
                    message: self.description().into(),
                    rule_name: Some(self.name().into()),
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 2,
                    severity: Severity::Warning,
                    fix: None,
                })
                .into_iter()
                .collect())
        }
        fn check_with_prepared_fix(&self, ctx: &LintContext) -> Result<(Vec<LintWarning>, Option<String>), LintError> {
            let warnings = self.check(ctx)?;
            let prepared = (!warnings.is_empty()).then(|| ctx.content.replace(self.from, self.to));
            Ok((warnings, prepared))
        }
        fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
            self.fix_calls.fetch_add(1, Ordering::Relaxed);
            Ok(ctx.content.replace(self.from, self.to))
        }
    }
    for limit in [1, 2, 100] {
        let fix_calls = Arc::new(AtomicUsize::new(0));
        // Same type and name, distinct settings. The second instance has no
        // finding until the first one rewrites the source.
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(PreparedProbe {
                from: "bad",
                to: "middle",
                fix_calls: fix_calls.clone(),
            }),
            Box::new(PreparedProbe {
                from: "middle",
                to: "good",
                fix_calls: fix_calls.clone(),
            }),
        ];
        let original: Vec<Box<dyn Rule>> = rules
            .iter()
            .map(|rule| Box::new(OriginalFixer(rule.clone())) as Box<dyn Rule>)
            .collect();
        let config = Config::default();
        let actual = DocumentRun::new("bad\n", &rules, &config).fix(limit).unwrap();
        assert_eq!(fix_calls.load(Ordering::Relaxed), 0, "must consume the prepared result");
        let expected = DocumentRun::new("bad\n", &original, &config).fix(limit).unwrap();
        assert_eq!(actual.0, expected.0);
        assert_fix_results(&actual.1, &expected.1);
    }
}

impl Rule for EditProbe {
    fn name(&self) -> &'static str {
        self.name
    }
    fn description(&self) -> &'static str {
        "Diagnostic edits with no separate fixer"
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn fixes_from_diagnostics(&self) -> bool {
        true
    }
    fn check(&self, ctx: &LintContext) -> LintResult {
        self.checks.fetch_add(1, Ordering::Relaxed);
        Ok(ctx
            .content
            .find(self.from)
            .map(|offset| {
                let (line, column) = ctx.offset_to_line_col(offset);
                LintWarning {
                    message: self.description().into(),
                    rule_name: Some(self.name().into()),
                    line,
                    column,
                    end_line: line,
                    end_column: column + self.from.chars().count(),
                    severity: Severity::Warning,
                    fix: Some(Fix::new(offset..offset + self.from.len(), self.to.into())),
                }
            })
            .into_iter()
            .collect())
    }
    fn fix(&self, _: &LintContext) -> Result<String, LintError> {
        panic!("the engine must consume the diagnostic edits")
    }
}

#[test]
fn consumes_initial_edits_and_preserves_configured_instance_identity() {
    let checks = Arc::new(AtomicUsize::new(0));
    // These instances have the same type and rule name, but different settings.
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(EditProbe {
            name: "EDIT_PROBE",
            checks: checks.clone(),
            from: "absent",
            to: "unused",
        }),
        Box::new(EditProbe {
            name: "EDIT_PROBE",
            checks: checks.clone(),
            from: "bad",
            to: "good",
        }),
    ];
    let config = Config::default();
    let result = DocumentRun::new("bad\n", &rules, &config).analyze_and_fix(100);
    let (text, fixed) = result.fixes.unwrap().unwrap();
    assert_eq!(text, "good\n");
    assert_eq!(checks.load(Ordering::Relaxed), 4);
    assert_eq!(fixed.context_creations, 1);
    assert_eq!(fixed.iterations, 2);
    assert!(fixed.converged);
}

#[test]
fn discards_edits_and_negative_checks_after_each_rewrite() {
    let checks = Arc::new(AtomicUsize::new(0));
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(EditProbe {
            name: "EDIT_FIRST",
            checks: checks.clone(),
            from: "bad",
            to: "middle",
        }),
        Box::new(EditProbe {
            name: "EDIT_SECOND",
            checks: checks.clone(),
            from: "middle",
            to: "good",
        }),
    ];
    let config = Config::default();
    let result = DocumentRun::new("bad\n", &rules, &config).analyze_and_fix(100);
    let (text, fixed) = result.fixes.unwrap().unwrap();
    assert_eq!(text, "good\n");
    assert_eq!(fixed.context_creations, 2);
    assert_eq!(fixed.rules_fixed, 2);
    assert_eq!(checks.load(Ordering::Relaxed), 5);
    assert!(result.remaining_warnings.unwrap().unwrap().is_empty());
}

fn assert_edit_contract(source: &str, flavor: MarkdownFlavor, reflow: bool) {
    let mut config = Config::default();
    config.global.flavor = flavor;
    config.rules.insert(
        "MD013".into(),
        crate::config::RuleConfig {
            values: [
                ("reflow".into(), toml::Value::Boolean(reflow)),
                ("line-length".into(), toml::Value::Integer(40)),
            ]
            .into(),
            severity: None,
        },
    );
    let ctx = LintContext::new(source, flavor, None);
    for rule in crate::rules::all_rules(&config) {
        if !rule.fixes_from_diagnostics() {
            continue;
        }
        let warnings = rule.check(&ctx).unwrap();
        let actual = match FixPlan::capture(rule.as_ref(), &ctx, &warnings) {
            FixPlan::Unchanged => source.to_owned(),
            FixPlan::Edits(edits) => crate::utils::fix_utils::apply_edits(source, edits.iter())
                .unwrap()
                .into_owned(),
            FixPlan::Custom => unreachable!(),
        };
        assert_eq!(
            actual,
            rule.fix(&ctx).unwrap(),
            "{} / {flavor:?} / {source:?}",
            rule.name()
        );
    }

    // Exercise the complete initial-check, iterative-fix and final-report path.
    let rules = crate::rules::all_rules(&config);
    let original: Vec<Box<dyn Rule>> = rules
        .iter()
        .map(|rule| Box::new(OriginalFixer(rule.clone())) as Box<dyn Rule>)
        .collect();
    let (direct_actual, direct_actual_result) = DocumentRun::new(source, &rules, &config).fix(100).unwrap();
    let (direct_expected, direct_expected_result) = DocumentRun::new(source, &original, &config).fix(100).unwrap();
    assert_eq!(direct_actual, direct_expected, "direct engine output: {source:?}");
    assert_fix_results(&direct_actual_result, &direct_expected_result);

    {
        let (cli_actual, cli_result) = DocumentRun::new(source, &rules, &config)
            .fix_for_formatter(100)
            .unwrap();
        assert_eq!(cli_actual, direct_expected, "formatter effects output: {source:?}");
        let differences = fix_result_differences(&cli_result, &direct_expected_result);
        assert!(
            differences.iter().all(|field| *field == "context_creations"),
            "formatter effects result: {cli_result:?}; native: {direct_expected_result:?}"
        );
    }

    {
        let (_, first_result) = DocumentRun::new(source, &rules, &config).fix(1).unwrap();
        assert_eq!(
            !first_result.fixed_rule_names.is_empty(),
            !direct_expected_result.fixed_rule_names.is_empty(),
            "first-change formatter status: {source:?} / {flavor:?} / reflow={reflow}"
        );
        assert!(first_result.context_creations <= 1);

        {
            let (_, formatter_first) = DocumentRun::new(source, &rules, &config).fix_for_formatter(1).unwrap();
            assert_eq!(
                !formatter_first.fixed_rule_names.is_empty(),
                !direct_expected_result.fixed_rule_names.is_empty(),
                "combined first-change formatter status: {source:?} / {flavor:?} / reflow={reflow}"
            );
            assert!(formatter_first.context_creations <= 1);
        }
    }

    let actual = DocumentRun::new(source, &rules, &config).analyze_and_fix(100);
    let expected = DocumentRun::new(source, &original, &config).analyze_and_fix(100);
    assert_eq!(actual.warnings.unwrap(), expected.warnings.unwrap());
    assert_eq!(
        actual.remaining_warnings.map(Result::unwrap),
        expected.remaining_warnings.map(Result::unwrap)
    );
    let (actual_fix, expected_fix) = match (actual.fixes, expected.fixes) {
        (Some(actual), Some(expected)) => (actual.unwrap(), expected.unwrap()),
        (None, None) => return,
        _ => panic!("fix preparation differs: {source:?}"),
    };
    let (actual_text, actual_fix) = actual_fix;
    let (expected_text, expected_fix) = expected_fix;
    assert_eq!(actual_text, expected_text, "engine output: {source:?}");
    assert_fix_results(&actual_fix, &expected_fix);
}

fn assert_fix_results(actual_fix: &FixResult, expected_fix: &FixResult) {
    assert!(
        fix_result_differences(actual_fix, expected_fix).is_empty(),
        "actual: {actual_fix:?}; expected: {expected_fix:?}"
    );
}

fn fix_result_differences(actual: &FixResult, expected: &FixResult) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if actual.rules_fixed != expected.rules_fixed {
        fields.push("rules_fixed");
    }
    if actual.iterations != expected.iterations {
        fields.push("iterations");
    }
    if actual.context_creations != expected.context_creations {
        fields.push("context_creations");
    }
    if actual.fixed_rule_names != expected.fixed_rule_names {
        fields.push("fixed_rule_names");
    }
    if actual.converged != expected.converged {
        fields.push("converged");
    }
    if actual.conflict_cycle != expected.conflict_cycle {
        fields.push("conflict_cycle");
    }
    if actual.conflicting_rules.iter().collect::<HashSet<_>>()
        != expected.conflicting_rules.iter().collect::<HashSet<_>>()
    {
        fields.push("conflicting_rules");
    }
    fields
}

#[test]
fn edit_contract_matches_original_fixers_across_dialects_and_suppressions() {
    let source = "#Title!\n\n#  Next!  ##\ntext   \n\n\n-   item\n\t- child\n\n[ spaced ](target)\n\n```\ntext\n```\n\n<!-- rumdl-disable MD009 -->\nkeep   \n<!-- rumdl-enable MD009 -->\nlast";
    for flavor in crate::format_proof::ALL_FLAVORS {
        for reflow in [false, true] {
            for ending in ["\n", "\r\n", "\r", "mixed"] {
                let source = if ending == "mixed" {
                    source
                        .split_inclusive('\n')
                        .enumerate()
                        .fold(String::new(), |mut text, (i, line)| {
                            text.push_str(&line.replace('\n', if i % 2 == 0 { "\n" } else { "\r\n" }));
                            text
                        })
                } else {
                    source.replace('\n', ending)
                };
                assert_edit_contract(&source, flavor, reflow);
            }
        }
    }
}

#[test]
fn setext_style_edits_survive_line_ending_normalization() {
    let source = "##Heading\r\n---\r\n```\r\n```\r\n##Heading";
    assert_edit_contract(source, MarkdownFlavor::Standard, false);
}

#[test]
fn current_main_container_and_table_fixes_keep_original_contract() {
    let sources = [
        "# Root\n\n> ### Jump\n\n# Another\n",
        "> # Root\n>\n> #### Jump\n",
        "# Root\n\n> title\n> ===\n",
        "- item\n\n    # First\n\n    # Second\n",
        "> paragraph\n> ***\n",
        "> [^a]: o\n>\n>         ***\n>         y\n\nref[^a]\n",
        "| a | b |\n| - | - |\n| 1 | 2\n",
        "Title\n---\u{2003}\nAfter\n",
    ];
    for flavor in crate::format_proof::ALL_FLAVORS {
        for source in sources {
            for reflow in [false, true] {
                for crlf in [false, true] {
                    let source = source.replace('\n', if crlf { "\r\n" } else { "\n" });
                    assert_edit_contract(&source, flavor, reflow);
                }
            }
        }
    }
}

#[test]
fn table_exposes_native_horizontal_rule_indentation() {
    let source = "| only |\n| --- |\n --- \nAfter\n";
    for flavor in crate::format_proof::ALL_FLAVORS {
        for reflow in [false, true] {
            for crlf in [false, true] {
                let source = source.replace('\n', if crlf { "\r\n" } else { "\n" });
                assert_edit_contract(&source, flavor, reflow);
            }
        }
    }
}

#[test]
fn rejected_parser_table_header_preserves_native_formatting_contract() {
    for flavor in crate::format_proof::ALL_FLAVORS {
        for reflow in [false, true] {
            for newline in ["\n", "\r\n"] {
                let source = "`unclosed | other\n--- | ---\n--- | ---\n".replace('\n', newline);
                assert_edit_contract(&source, flavor, reflow);
            }
        }
    }
}

#[test]
fn orphan_pipe_row_preserves_native_fix_sequence() {
    let source = "-   list\n| cell | other |\n\n    indented\n-   list\n    indented";
    for flavor in crate::format_proof::ALL_FLAVORS {
        for reflow in [false, true] {
            for crlf in [false, true] {
                let source = source.replace('\n', if crlf { "\r\n" } else { "\n" });
                assert_edit_contract(&source, flavor, reflow);
            }
        }
    }
}

#[test]
fn isolated_table_lists_keep_original_formatter_contract() {
    for list in [
        "- item\n- other",
        "1. item\n2. other",
        "- item\n  - child\n- other",
        "- item\\",
        "- item\n  continuation",
        "- item\n\n  continuation",
        "-   item\n+ other",
        "- item\n  # Heading",
    ] {
        for table in [
            "| cell | other |\n| --- | --- |",
            "| cell | other |\n| :--- | ---: |\n| `value` | α🦀 |",
            "cell | other\n--- | ---",
            "  | cell | other |\n  | --- | --- |",
            "| cell | other |\n| --- | --- |\n::: note",
            "| cell | other |\n| --- | --- |\n    indented",
        ] {
            for separator in ["\n", "\n\n", "\n \n"] {
                for source in [
                    format!("{list}{separator}{table}\n"),
                    format!("{table}{separator}{list}\n"),
                    format!("{list}{separator}{table}{separator}{list}\n"),
                ] {
                    for reflow in [false, true] {
                        assert_edit_contract(&source, MarkdownFlavor::Standard, reflow);
                    }
                }
            }
        }
    }
}

#[test]
fn separate_list_and_table_preserve_native_fix_sequence() {
    let source = "-   list\n   \n| cell | other |\n| cell | other |\n| --- | --- |\n    indented";
    for flavor in crate::format_proof::ALL_FLAVORS {
        for reflow in [false, true] {
            for crlf in [false, true] {
                let source = source.replace('\n', if crlf { "\r\n" } else { "\n" });
                assert_edit_contract(&source, flavor, reflow);
            }
        }
    }
}

#[test]
fn raw_comment_fences_keep_original_formatter_contract() {
    for source in [
        "<!-- comment\n~~~\n~~~\n```rust\n-->\n",
        "<!-- comment\n~~~\n~~~\n-->\n\n```rust\ncode\n```\n",
        "prefix <!-- comment\n~~~\n~~~\n-->\n\n```rust\ncode\n```\n",
        "`inline\n~~~\n~~~\n`\n\n```rust\ncode\n```\n",
        "<div>\n~~~\n~~~\n</div>\n\n```rust\ncode\n```\n",
    ] {
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MkDocs, MarkdownFlavor::MDX] {
            for reflow in [false, true] {
                assert_edit_contract(source, flavor, reflow);
                assert_edit_contract(&source.replace('\n', "\r\n"), flavor, reflow);
            }
        }
    }
}

#[test]
fn colon_container_after_table_preserves_native_output() {
    let source = "| cell | other |\n| --- | --- |\n::: note\n```";
    for flavor in crate::format_proof::ALL_FLAVORS {
        for reflow in [false, true] {
            for crlf in [false, true] {
                let source = source.replace('\n', if crlf { "\r\n" } else { "\n" });
                assert_edit_contract(&source, flavor, reflow);
            }
        }
    }
}

#[test]
fn setext_table_boundaries_preserve_native_fix_sequence() {
    let source = "é🦀 prose with   spaces and ` code `\n| cell | other |\n| --- | --- |\n---\né🦀 prose with   spaces and ` code `";
    for flavor in crate::format_proof::ALL_FLAVORS {
        for reflow in [false, true] {
            for crlf in [false, true] {
                let source = source.replace('\n', if crlf { "\r\n" } else { "\n" });
                assert_edit_contract(&source, flavor, reflow);
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn diagnostic_edit_contract_matches_original_fixers(
        lines in prop::collection::vec(prop_oneof![
            Just("#  heading!  ##"), Just("##Heading"), Just("-   list"),
            Just("1. ordered"), Just("    indented"), Just("```"), Just("~~~rust"),
            Just(""), Just("   "), Just("é🦀 prose with   spaces and ` code `"),
            Just("[ spaced ](url)"), Just("**strong** and _emphasis_"),
            Just("| cell | other |"), Just("| --- | --- |"),
            Just("<!-- rumdl-disable-next-line MD009 MD013 -->"),
            Just("text with a fairly long sentence that should wrap at the configured line width."),
            Just("---"), Just("title: Example"), Just("::: note"),
        ], 0..30),
        crlf in any::<bool>(), reflow in any::<bool>(),
    ) {
        let source = lines.join(if crlf { "\r\n" } else { "\n" });
        assert_edit_contract(&source, MarkdownFlavor::Standard, reflow);
    }
}

/// Exercise complete revisions, preserving every failure in one run. The
/// regular property above shrinks a discovered case into a permanent regression.
#[test]
#[ignore = "extended complete formatter cross-product"]
fn complete_formatter_crossproduct_matches_native_fixers() {
    let atoms = [
        "#  heading!  ##",
        "##Heading",
        "-   list",
        "1. ordered",
        "    indented",
        "```",
        "~~~rust",
        "",
        "   ",
        "é🦀 prose with   spaces and ` code `",
        "[ spaced ](url)",
        "**strong** and _emphasis_",
        "| cell | other |",
        "| --- | --- |",
        "<!-- rumdl-disable-next-line MD009 MD013 -->",
        "text with a fairly long sentence that should wrap at the configured line width.",
        "---",
        "title: Example",
        "::: note",
    ];
    let mut cases = Vec::new();
    for reflow in [false, true] {
        let mut config = Config::default();
        config.rules.insert(
            "MD013".into(),
            crate::config::RuleConfig {
                values: [
                    ("reflow".into(), toml::Value::Boolean(reflow)),
                    ("line-length".into(), toml::Value::Integer(40)),
                ]
                .into(),
                severity: None,
            },
        );
        let rules = crate::rules::all_rules(&config);
        let original: Vec<Box<dyn Rule>> = rules
            .iter()
            .map(|rule| Box::new(OriginalFixer(rule.clone())) as Box<dyn Rule>)
            .collect();
        cases.push((config, rules, original));
    }
    let mut state = 0x9b2a_ae4c_7138_d565u64;
    let mut failed = 0;
    let mut field_counts = std::collections::BTreeMap::<&str, usize>::new();
    for index in 0..32768 {
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let len = (next() % 30) as usize;
        let source = (0..len)
            .map(|_| atoms[next() as usize % atoms.len()])
            .collect::<Vec<_>>()
            .join(if index % 2 == 0 { "\n" } else { "\r\n" });
        let (config, rules, original) = &cases[index / 2 % cases.len()];
        let (actual_text, actual) = DocumentRun::new(&source, rules, config).fix(100).unwrap();
        let (expected_text, expected) = DocumentRun::new(&source, original, config).fix(100).unwrap();
        let mut fields = fix_result_differences(&actual, &expected);
        if actual_text != expected_text {
            fields.push("bytes");
        }
        if !fields.is_empty() {
            failed += 1;
            for field in &fields {
                *field_counts.entry(field).or_default() += 1;
            }
            println!(
                "CONTRACT_FAILURE {}",
                serde_json::json!({"index":index,"reflow":index / 2 % cases.len() == 1,"source":source,"fields":fields,"actual_text":actual_text,"expected_text":expected_text,"actual_result":format!("{actual:?}"),"expected_result":format!("{expected:?}")})
            );
        }
    }
    println!("32768 complete formatter cases; {failed} failures; {field_counts:?}");
    assert_eq!(failed, 0);
}

#[test]
fn prechecked_native_fixers_preserve_flavors_configuration_and_inline_disables() {
    let sources = [
        "words   with  spaces.  Next sentence.\n",
        "# Héading\r\n\r\nwords   with  spaces\r\n",
        "- Item\n\n continuation\n",
        "1. Item\n\n   continuation\n",
        "- Item\n\n ```rust\n code   here\n ```\n",
        "- [ ] task\n\n      continuation\n",
        "- Item\n\n  [link](target)\n",
        "!!! note\n    - item\n\n      continuation\n",
        "<!-- rumdl-disable MD064 -->\nwords   with  spaces\n",
        "<!-- rumdl-disable MD077 -->\n- Item\n\n continuation\n",
        "`code   span` and **strong   text**\n",
        "| one | two |\n| --- | --- |\n| a   b | c |\n",
    ];
    let mut changed = std::collections::HashSet::new();
    for flavor in crate::format_proof::ALL_FLAVORS {
        for mode in 0..4 {
            let mut config = Config::default();
            config.global.flavor = flavor;
            config.rules.insert(
                "MD064".into(),
                crate::config::RuleConfig {
                    values: [(
                        "allow-sentence-double-space".into(),
                        toml::Value::Boolean(mode % 2 == 1),
                    )]
                    .into(),
                    severity: None,
                },
            );
            let mut continuation = crate::config::RuleConfig {
                values: [(
                    "style".into(),
                    toml::Value::String(if mode % 2 == 0 { "any" } else { "aligned" }.into()),
                )]
                .into(),
                severity: None,
            };
            if mode >= 2 {
                continuation
                    .values
                    .insert("indent".into(), toml::Value::Integer(if mode == 3 { 6 } else { 4 }));
            }
            config.rules.insert("MD077".into(), continuation);
            for name in ["MD064", "MD077"] {
                let rule = crate::rules::create_rule_by_name(name, &config).unwrap();
                for source in sources {
                    let ctx = LintContext::new(source, flavor, None);
                    let warnings = rule.check(&ctx).unwrap();
                    let native = rule.fix(&ctx).unwrap();
                    let prepared = rule.fix_with_diagnostics(&ctx, &warnings).unwrap();
                    assert_eq!(prepared, native, "{name}, {flavor:?}, mode {mode}: {source:?}");
                    if native != source {
                        changed.insert(name);
                    }
                }
            }
        }
    }
    assert_eq!(changed.len(), 2, "Both native fixers must actually change fixtures");
}

#[test]
fn fresh_custom_fix_uses_checked_diagnostics_without_calling_the_standalone_fixer() {
    #[derive(Clone)]
    struct PreparedProbe {
        inner: Box<dyn Rule>,
        checks: Arc<AtomicUsize>,
        prepared: Arc<AtomicUsize>,
    }
    impl Rule for PreparedProbe {
        fn name(&self) -> &'static str {
            self.inner.name()
        }
        fn description(&self) -> &'static str {
            self.inner.description()
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn check(&self, ctx: &LintContext) -> LintResult {
            self.checks.fetch_add(1, Ordering::Relaxed);
            self.inner.check(ctx)
        }
        fn fix(&self, _: &LintContext) -> Result<String, LintError> {
            panic!("fresh native findings must be handed to the prepared fixer");
        }
        fn fix_with_diagnostics(&self, ctx: &LintContext, warnings: &[LintWarning]) -> Result<String, LintError> {
            assert!(!warnings.is_empty());
            self.prepared.fetch_add(1, Ordering::Relaxed);
            self.inner.fix_with_diagnostics(ctx, warnings)
        }
    }

    for (name, source) in [
        ("MD064", "words   with  spaces\n"),
        ("MD077", "- Item\n\n continuation\n"),
    ] {
        let config = Config::default();
        let inner = crate::rules::create_rule_by_name(name, &config).unwrap();
        let original: Vec<Box<dyn Rule>> = vec![Box::new(OriginalFixer(inner.clone()))];
        let (expected, expected_result) = DocumentRun::new(source, &original, &config).fix(100).unwrap();
        assert_ne!(expected, source);
        for coordinator in [FixCoordinator::new(), FixCoordinator::for_formatter()] {
            let checks = Arc::new(AtomicUsize::new(0));
            let prepared = Arc::new(AtomicUsize::new(0));
            let rules: Vec<Box<dyn Rule>> = vec![Box::new(PreparedProbe {
                inner: inner.clone(),
                checks: checks.clone(),
                prepared: prepared.clone(),
            })];
            let mut actual = source.to_owned();
            let result = coordinator
                .apply_fixes_iterative(&rules, &[], &mut actual, &config, 100, None)
                .unwrap();
            assert_eq!(actual, expected);
            assert_eq!(checks.load(Ordering::Relaxed), 2);
            assert_eq!(prepared.load(Ordering::Relaxed), 1);
            assert_fix_results(&result, &expected_result);
        }
    }
}

#[test]
fn native_regressions_preserve_native_formatter_contracts() {
    for flavor in crate::format_proof::ALL_FLAVORS {
        for explicit_break_style in [false, true] {
            let mut config = Config::default();
            config.global.flavor = flavor;
            if explicit_break_style {
                config.rules.insert(
                    "MD035".into(),
                    crate::config::RuleConfig {
                        values: [("style".into(), toml::Value::String("---".into()))].into(),
                        severity: None,
                    },
                );
            }
            let rules = crate::rules::all_rules(&config);
            let original: Vec<Box<dyn Rule>> = rules
                .iter()
                .map(|rule| Box::new(OriginalFixer(rule.clone())) as Box<dyn Rule>)
                .collect();
            for input in crate::format_proof::native_regressions::SOURCES {
                for ending in ["\n", "\r\n", "\r", "\n\r\n"] {
                    let source = input.replace('\n', ending);
                    let (expected, expected_result) = DocumentRun::new(&source, &original, &config).fix(100).unwrap();
                    let (direct, direct_result) = DocumentRun::new(&source, &rules, &config).fix(100).unwrap();
                    assert_eq!(
                        direct, expected,
                        "direct / {flavor} / style={explicit_break_style}: {source:?}"
                    );
                    assert_fix_results(&direct_result, &expected_result);
                    let (formatted, formatted_result) = DocumentRun::new(&source, &rules, &config)
                        .fix_for_formatter(100)
                        .unwrap();
                    assert_eq!(
                        formatted, expected,
                        "formatter / {flavor} / style={explicit_break_style}: {source:?}"
                    );
                    let differences = fix_result_differences(&formatted_result, &expected_result);
                    assert!(
                        differences.iter().all(|field| *field == "context_creations"),
                        "{flavor} / style={explicit_break_style}: {source:?}: {differences:?}"
                    );
                }
            }
        }
    }
}
