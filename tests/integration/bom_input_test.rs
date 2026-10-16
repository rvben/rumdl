//! Library entry points treat a leading UTF-8 byte order mark as an encoding
//! marker: analysis reads the text after it and reports relative to that text,
//! fixes address the supplied bytes, rewrites keep the mark, and exactly one mark is skipped.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::document_run::DocumentRun;
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::rule::{LintWarning, Rule};
use rumdl_lib::rules::{all_rules, filter_rules};

const BOM: &str = "\u{feff}";

const DOCS: &[&str] = &[
    "# Title\n\nText\n",
    "#Title\n\nText  \n",
    "Title\n=====\n\ntext\n",
    "<!-- rumdl-disable-next-line MD013 -->\nthis line is long enough that without the directive it would exceed the default limit of eighty characters\n",
    "<!-- rumdl-disable MD041 -->\nplain first line\n",
    "plain first line\r\n\r\n# Title\r\n",
];

fn rules() -> (Config, Vec<Box<dyn Rule>>) {
    let config = Config::default();
    let rules = filter_rules(&all_rules(&config), &config.global);
    (config, rules)
}

fn shape(w: &LintWarning) -> (Option<String>, usize, usize, usize, usize, String) {
    (
        w.rule_name.clone(),
        w.line,
        w.column,
        w.end_line,
        w.end_column,
        w.message.clone(),
    )
}

#[test]
fn lint_reports_the_unmarked_documents_warnings() {
    let (config, rules) = rules();
    let mut with_fixes = 0;
    for doc in DOCS {
        let plain = rumdl_lib::lint(doc, &rules, false, MarkdownFlavor::Standard, None, Some(&config)).unwrap();
        let marked = rumdl_lib::lint(
            &format!("{BOM}{doc}"),
            &rules,
            false,
            MarkdownFlavor::Standard,
            None,
            Some(&config),
        )
        .unwrap();
        assert_eq!(
            plain.iter().map(shape).collect::<Vec<_>>(),
            marked.iter().map(shape).collect::<Vec<_>>(),
            "{doc:?}"
        );
        with_fixes += plain.iter().filter(|w| w.fix.is_some()).count();
        // Public fix ranges address each supplied document, including its BOM.
        let mut expected = plain.iter().map(|w| w.fix.clone()).collect::<Vec<_>>();
        fn shift(fix: &mut rumdl_lib::rule::Fix) {
            fix.range.start += BOM.len();
            fix.range.end += BOM.len();
            for extra in &mut fix.additional_edits {
                shift(extra);
            }
        }
        for fix in expected.iter_mut().flatten() {
            shift(fix);
        }
        assert_eq!(
            expected,
            marked.iter().map(|w| w.fix.clone()).collect::<Vec<_>>(),
            "{doc:?}"
        );
    }
    assert!(with_fixes > 0);
}

#[test]
fn lint_and_index_hashes_the_supplied_text() {
    let (config, rules) = rules();
    let plain = "# Title\n\nText\n";
    let marked = format!("{BOM}{plain}");
    let (_, plain_index) =
        rumdl_lib::lint_and_index(plain, &rules, false, MarkdownFlavor::Standard, None, Some(&config));
    let (_, marked_index) =
        rumdl_lib::lint_and_index(&marked, &rules, false, MarkdownFlavor::Standard, None, Some(&config));
    assert_ne!(
        plain_index.content_hash, marked_index.content_hash,
        "the cache key tracks the supplied bytes"
    );
    assert_eq!(plain_index.headings, marked_index.headings);
    assert_eq!(marked_index.headings.first().map(|h| h.line), Some(1));
}

#[test]
fn index_only_path_sees_the_first_line_heading() {
    let (config, rules) = rules();
    let index = rumdl_lib::build_file_index_only_with_config(
        &format!("{BOM}# Title\n"),
        &rules,
        MarkdownFlavor::Standard,
        None,
        &config,
    );
    assert_eq!(
        index
            .headings
            .iter()
            .map(|h| (h.line, h.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "Title")]
    );
}

#[test]
fn document_run_fix_and_prepared_fix_keep_one_mark() {
    let (config, rules) = rules();
    let mut compared_prepared = 0;
    for doc in DOCS {
        let marked = format!("{BOM}{doc}");
        let (fixed, _) = DocumentRun::new(&marked, &rules, &config).fix(100).unwrap();
        let (plain_fixed, _) = DocumentRun::new(doc, &rules, &config).fix(100).unwrap();
        assert_eq!(fixed, format!("{BOM}{plain_fixed}"), "{doc:?}");
        let (again, _) = DocumentRun::new(&fixed, &rules, &config).fix(100).unwrap();
        assert_eq!(again, fixed, "idempotent: {doc:?}");

        let combined = DocumentRun::new(&marked, &rules, &config).analyze_and_fix(100);
        if let Some(Ok((prepared, _))) = combined.fixes {
            compared_prepared += 1;
            assert_eq!(prepared, fixed, "prepared fix agrees with fix(): {doc:?}");
        }
    }
    assert!(compared_prepared >= 2, "prepared rewrites must actually be checked");
}

#[test]
fn a_document_the_fix_leaves_alone_comes_back_byte_identical() {
    let (config, rules) = rules();
    let marked = format!("{BOM}# Title\n\nText\n");
    let (fixed, _) = DocumentRun::new(&marked, &rules, &config).fix(100).unwrap();
    assert_eq!(fixed, marked);
}

#[test]
fn fix_coordinator_restores_the_mark() {
    let (config, rules) = rules();
    let mut content = format!("{BOM}#Title\n");
    FixCoordinator::new()
        .apply_fixes_iterative(&rules, &[], &mut content, &config, 100, None)
        .unwrap();
    assert_eq!(content, format!("{BOM}# Title\n"));
}

#[test]
fn a_second_mark_is_text() {
    let (config, rules) = rules();
    let doc = format!("{BOM}{BOM}# Title\n");
    let warnings = rumdl_lib::lint(&doc, &rules, false, MarkdownFlavor::Standard, None, Some(&config)).unwrap();
    assert!(
        warnings.iter().any(|w| w.rule_name.as_deref() == Some("MD041")),
        "{warnings:?}"
    );
    let (fixed, _) = DocumentRun::new(&doc, &rules, &config).fix(100).unwrap();
    assert!(fixed.starts_with(&format!("{BOM}{BOM}")), "{fixed:?}");
}

/// The public lint/fix composition must never delete document text three bytes
/// before the intended edit when the supplied input begins with a BOM.
#[test]
fn public_lint_fixes_apply_to_the_supplied_document() {
    let (config, rules) = rules();
    let rules: Vec<_> = rules
        .into_iter()
        .filter(|r| r.name() == "MD009" || r.name() == "MD018")
        .collect();
    for (input, expected) in [
        ("#Title\n\ntext   \n", "# Title\n\ntext\n"),
        ("#Title\r\n\r\ntext   \r\n", "# Title\r\n\r\ntext\r\n"),
    ] {
        for prefix in ["", BOM, "\u{feff}\u{feff}"] {
            // A second BOM is document content and hides the first heading.
            let expected = if prefix == "\u{feff}\u{feff}" {
                input.replace("text   ", "text")
            } else {
                expected.to_string()
            };
            let supplied = format!("{prefix}{input}");
            let warnings =
                rumdl_lib::lint(&supplied, &rules, false, MarkdownFlavor::Standard, None, Some(&config)).unwrap();
            assert!(warnings.iter().any(|w| w.rule_name.as_deref() == Some("MD009")));
            assert_eq!(
                rumdl_lib::utils::fix_utils::apply_warning_fixes(&supplied, &warnings).unwrap(),
                format!("{prefix}{expected}")
            );
        }
    }
}

#[test]
fn public_fixes_keep_paired_edits_in_supplied_coordinates() {
    let config = Config::default();
    // Use a tiny custom rule to exercise paired ranges independently of rule configuration.
    #[derive(Clone)]
    struct Paired;
    impl Rule for Paired {
        fn name(&self) -> &'static str {
            "paired"
        }
        fn description(&self) -> &'static str {
            "paired edits"
        }
        fn check(&self, _: &rumdl_lib::lint_context::LintContext) -> rumdl_lib::rule::LintResult {
            Ok(vec![LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 2,
                rule_name: Some("paired".into()),
                message: "paired edits".into(),
                severity: rumdl_lib::rule::Severity::Warning,
                fix: Some(rumdl_lib::rule::Fix::with_additional_edits(
                    0..1,
                    "A".into(),
                    vec![rumdl_lib::rule::Fix::new(2..3, "C".into())],
                )),
            }])
        }
        fn fix(&self, ctx: &rumdl_lib::lint_context::LintContext) -> Result<String, rumdl_lib::rule::LintError> {
            Ok(ctx.content.into())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
    let rules: Vec<Box<dyn Rule>> = vec![Box::new(Paired)];
    let input = "\u{feff}abc\n";
    let warnings = rumdl_lib::lint(input, &rules, false, MarkdownFlavor::Standard, None, Some(&config)).unwrap();
    assert_eq!(warnings[0].fix.as_ref().unwrap().range, 3..4);
    assert_eq!(warnings[0].fix.as_ref().unwrap().additional_edits[0].range, 5..6);
    assert_eq!(
        rumdl_lib::utils::fix_utils::apply_warning_fixes(input, &warnings).unwrap(),
        "\u{feff}AbC\n"
    );
}

#[test]
fn fragment_fixing_preserves_a_real_leading_mark_as_content() {
    let (config, rules) = rules();
    let mut fragment = "\u{feff}# Inside\nbody   \n".to_string();
    let result = FixCoordinator::new().apply_fixes_to_fragment(&rules, &mut fragment, &config, 100);
    assert!(!result.fixed_rule_names.is_empty());
    assert_eq!(fragment, "\u{feff}# Inside\nbody\n");
    let again = FixCoordinator::new().apply_fixes_to_fragment(&rules, &mut fragment, &config, 100);
    assert!(again.fixed_rule_names.is_empty());
}
