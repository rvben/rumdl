//! Library entry points treat a leading UTF-8 byte order mark as an encoding
//! marker: analysis reads the text after it and reports relative to that text,
//! fixes keep the mark, and exactly one mark is skipped.

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
        // Fix ranges are relative to the text after the mark, so they address
        // the unmarked document unchanged.
        assert_eq!(
            plain.iter().map(|w| w.fix.clone()).collect::<Vec<_>>(),
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
    for doc in DOCS {
        let marked = format!("{BOM}{doc}");
        let (fixed, _) = DocumentRun::new(&marked, &rules, &config).fix(100).unwrap();
        let (plain_fixed, _) = DocumentRun::new(doc, &rules, &config).fix(100).unwrap();
        assert_eq!(fixed, format!("{BOM}{plain_fixed}"), "{doc:?}");
        let (again, _) = DocumentRun::new(&fixed, &rules, &config).fix(100).unwrap();
        assert_eq!(again, fixed, "idempotent: {doc:?}");

        let combined = DocumentRun::new(&marked, &rules, &config).analyze_and_fix(100);
        if let Some(Ok((prepared, _))) = combined.fixes {
            assert_eq!(prepared, fixed, "prepared fix agrees with fix(): {doc:?}");
        }
    }
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
