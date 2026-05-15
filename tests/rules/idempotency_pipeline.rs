//! Full-pipeline idempotency: applying the entire default rule chain twice
//! must produce identical output. This is the property `rumdl fmt` guarantees
//! externally. Users expect `fmt(fmt(x)) == fmt(x)`.

use proptest::prelude::*;
use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::rules::{all_rules, filter_rules};

/// One full `rumdl fmt` pass: drives the same iterative fix coordinator the
/// CLI uses. The coordinator re-checks every rule after each applied fix and
/// stops when content stabilises or hits the iteration cap.
fn fmt(content: &str, flavor: MarkdownFlavor) -> String {
    let mut config = Config::default();
    config.global.flavor = flavor;
    let rules = filter_rules(&all_rules(&config), &config.global);
    let coordinator = FixCoordinator::new();
    let mut result = content.to_string();
    // Match the production file processor's iteration cap so the test
    // models user-visible behavior. See src/file_processor/processing.rs.
    let fix_result = coordinator
        .apply_fixes_iterative(&rules, &[], &mut result, &config, 100, None)
        .expect("fix coordinator returned Err");
    assert!(
        fix_result.converged,
        "fix coordinator did not converge (conflicting rules: {:?}, cycle: {:?})",
        fix_result.conflicting_rules, fix_result.conflict_cycle
    );
    result
}

fn markdown_content_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            (1u8..=6, "[a-zA-Z0-9 ]{0,80}").prop_map(|(l, t)| format!("{} {}", "#".repeat(l as usize), t)),
            "[a-zA-Z0-9 ]{0,80}".prop_map(|t| format!("- {t}")),
            "[a-zA-Z0-9 ]{0,80}".prop_map(|t| format!("> {t}")),
            "[a-zA-Z0-9 ]{0,80}".prop_map(|t| t),
            Just(String::new()),
        ],
        0..30,
    )
    .prop_map(|lines| lines.join("\n"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn fmt_is_idempotent_standard(content in markdown_content_strategy()) {
        let once = fmt(&content, MarkdownFlavor::Standard);
        let twice = fmt(&once, MarkdownFlavor::Standard);
        prop_assert_eq!(
            once.clone(), twice,
            "Full pipeline not idempotent (flavor=Standard) on input:\n---\n{}\n---\nfirst pass:\n---\n{}\n---",
            content, once
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn fmt_is_idempotent_mkdocs(content in markdown_content_strategy()) {
        let once = fmt(&content, MarkdownFlavor::MkDocs);
        let twice = fmt(&once, MarkdownFlavor::MkDocs);
        prop_assert_eq!(once, twice, "Full pipeline not idempotent (flavor=MkDocs)");
    }

    #[test]
    fn fmt_is_idempotent_mdx(content in markdown_content_strategy()) {
        let once = fmt(&content, MarkdownFlavor::MDX);
        let twice = fmt(&once, MarkdownFlavor::MDX);
        prop_assert_eq!(once, twice, "Full pipeline not idempotent (flavor=MDX)");
    }

    #[test]
    fn fmt_is_idempotent_quarto(content in markdown_content_strategy()) {
        let once = fmt(&content, MarkdownFlavor::Quarto);
        let twice = fmt(&once, MarkdownFlavor::Quarto);
        prop_assert_eq!(once, twice, "Full pipeline not idempotent (flavor=Quarto)");
    }
}
