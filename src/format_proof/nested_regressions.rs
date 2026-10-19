//! Compare nested-list certificates and iterative formatting with native checks.

use super::*;
use crate::config::Config;
use crate::fix_coordinator::FixCoordinator;
use crate::lint_context::LintContext;
use crate::rule::Rule;

fn assert_matches_native(source: &str, flavor: MarkdownFlavor) {
    let facts = FormatProof::analyze(source, flavor);
    let md077 = crate::rules::MD077ListContinuationIndent::default();
    if NativeFormatPolicy::validated(&md077).unwrap().proven(&facts) {
        let context = LintContext::new(source, flavor, None);
        assert_eq!(md077.fix(&context).unwrap(), source, "{flavor:?}: {source:?}");
    }

    let mut config = Config::default();
    config.global.flavor = flavor;
    let rules: Vec<_> = crate::rules::all_rules(&config)
        .into_iter()
        .filter(|rule| ["MD004", "MD005", "MD007", "MD030", "MD032", "MD077"].contains(&rule.name()))
        .collect();
    let coordinator = FixCoordinator::for_formatter();
    let mut candidate = source.to_owned();
    let result = coordinator
        .apply_fixes_iterative(&rules, &[], &mut candidate, &config, 100, None)
        .unwrap();

    // Requesting stable analysis retains native checks on every source revision,
    // including revisions produced by another list rule earlier in the fix loop.
    let mut native = source.to_owned();
    let control = coordinator.apply_fixes_with_analysis(
        &rules,
        &mut native,
        &config,
        100,
        crate::DocumentPaths::same(None),
        None,
        Some(&mut |_, _| {}),
    );
    assert_eq!(candidate, native, "{flavor:?}: {source:?}");
    assert_eq!(result.converged, control.converged, "{flavor:?}: {source:?}");
    if control.converged {
        let fixed = candidate.clone();
        coordinator
            .apply_fixes_iterative(&rules, &[], &mut candidate, &config, 100, None)
            .unwrap();
        assert_eq!(candidate, fixed, "{flavor:?}: {source:?}");
    }
}

#[test]
fn nested_and_loose_lists_match_native_iterative_fixes() {
    for depth in 2..=3 {
        for loose in [false, true] {
            for indent in 0..8 {
                for tail in [
                    "prose\n",
                    "[r]: /target\n",
                    "- sibling\n  sibling prose\n",
                    "1. ordered\n   ordered prose\n",
                    "```text\ncode\n```\n",
                    "| a | b |\n| - | - |\n| c | d |\n",
                ] {
                    let mut source = String::new();
                    for level in 0..depth {
                        source.push_str(&format!("{}- item\n", " ".repeat(level * 2)));
                    }
                    if loose {
                        source.push('\n');
                    }
                    for line in tail.lines() {
                        source.push_str(&format!("{}{line}\n", " ".repeat(indent)));
                    }
                    if indent % 2 == 1 {
                        source.push_str("\n  ancestor prose\n    deeper prose\n");
                    }
                    for flavor in ALL_FLAVORS {
                        assert_matches_native(&source, flavor);
                    }
                }
            }
        }
    }
}
