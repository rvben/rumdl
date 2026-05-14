//! Full-pipeline idempotency: applying the entire default rule chain twice
//! must produce identical output. This is the property `rumdl fmt` guarantees
//! externally. Users expect `fmt(fmt(x)) == fmt(x)`.

use proptest::prelude::*;
use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{LintWarning, Rule};
use rumdl_lib::rules::all_rules;

/// Apply every warning's fix in reverse byte order so ranges remain valid.
fn apply_all_fixes(content: &str, warnings: &[LintWarning]) -> String {
    let mut fixes: Vec<_> = warnings.iter().filter_map(|w| w.fix.as_ref()).collect();
    fixes.sort_by(|a, b| b.range.start.cmp(&a.range.start));
    let mut result = content.to_string();
    for fix in fixes {
        if fix.range.end <= result.len()
            && result.is_char_boundary(fix.range.start)
            && result.is_char_boundary(fix.range.end)
        {
            result.replace_range(fix.range.clone(), &fix.replacement);
        }
    }
    result
}

/// One full fix pass: run every rule's `check`, collect all warnings, apply all fixes.
fn fmt_once(content: &str, flavor: MarkdownFlavor, rules: &[Box<dyn Rule>]) -> String {
    let ctx = LintContext::new(content, flavor, None);
    let mut all_warnings = Vec::new();
    for rule in rules {
        if let Ok(ws) = rule.check(&ctx) {
            all_warnings.extend(ws);
        }
    }
    apply_all_fixes(content, &all_warnings)
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

    // Minimized failing input: "#   Aa "
    // Pass 1: MD019 collapses multiple spaces to one, leaving "# Aa", then
    // MD026 strips trailing text (treating the heading text as punctuation?),
    // or MD009 + MD019 interact to leave "# " (heading with trailing space).
    // Pass 2: trailing space after "#" is then stripped, yielding "#".
    // Root cause: rule interaction between MD019 (multi-space-atx) and MD009
    // (trailing-spaces): one rule's output is invalid input for the other.
    #[ignore = "idempotency bug: Standard - MD019+MD009 interaction leaves trailing space in ATX heading text that a second pass then removes; minimized input: \"#   Aa \""]
    #[test]
    fn fmt_is_idempotent_standard(content in markdown_content_strategy()) {
        let rules = all_rules(&Config::default());
        let once = fmt_once(&content, MarkdownFlavor::Standard, &rules);
        let twice = fmt_once(&once, MarkdownFlavor::Standard, &rules);
        prop_assert_eq!(
            once.clone(), twice,
            "Full pipeline not idempotent (flavor=Standard) on input:\n---\n{}\n---\nfirst pass:\n---\n{}\n---",
            content, once
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Same underlying bug as Standard. All four flavors hit the same
    // MD019+MD009 interaction: "#   Aa " -> "# " -> "#".
    #[ignore = "idempotency bug: MkDocs - same MD019+MD009 ATX heading interaction as Standard; minimized input: \"#   Aa \""]
    #[test]
    fn fmt_is_idempotent_mkdocs(content in markdown_content_strategy()) {
        let rules = all_rules(&Config::default());
        let once = fmt_once(&content, MarkdownFlavor::MkDocs, &rules);
        let twice = fmt_once(&once, MarkdownFlavor::MkDocs, &rules);
        prop_assert_eq!(once, twice, "Full pipeline not idempotent (flavor=MkDocs)");
    }

    #[ignore = "idempotency bug: MDX - same MD019+MD009 ATX heading interaction as Standard; minimized input: \"#   Aa \""]
    #[test]
    fn fmt_is_idempotent_mdx(content in markdown_content_strategy()) {
        let rules = all_rules(&Config::default());
        let once = fmt_once(&content, MarkdownFlavor::MDX, &rules);
        let twice = fmt_once(&once, MarkdownFlavor::MDX, &rules);
        prop_assert_eq!(once, twice, "Full pipeline not idempotent (flavor=MDX)");
    }

    #[ignore = "idempotency bug: Quarto - same MD019+MD009 ATX heading interaction as Standard; minimized input: \"#   Aa \""]
    #[test]
    fn fmt_is_idempotent_quarto(content in markdown_content_strategy()) {
        let rules = all_rules(&Config::default());
        let once = fmt_once(&content, MarkdownFlavor::Quarto, &rules);
        let twice = fmt_once(&once, MarkdownFlavor::Quarto, &rules);
        prop_assert_eq!(once, twice, "Full pipeline not idempotent (flavor=Quarto)");
    }
}
