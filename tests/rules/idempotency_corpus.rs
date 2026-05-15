//! Replays every minimized failing input from past idempotency proptest runs.
//! Each `.md` file in tests/regressions/idempotency/ must be idempotent through
//! the full default rule pipeline for the flavor encoded in its filename.
//!
//! Filename convention: `<slug>.<flavor>.md` (e.g. `atx-heading-multispace.standard.md`).
//! Recognised flavor suffixes: standard, mkdocs, mdx, quarto.
//! Files lacking a recognised flavor suffix default to Standard.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::rules::{all_rules, filter_rules};
use std::fs;
use std::path::Path;

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

fn flavor_from_filename(path: &Path) -> MarkdownFlavor {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let suffix = stem.rsplit('.').next().unwrap_or("");
    match suffix {
        "mkdocs" => MarkdownFlavor::MkDocs,
        "mdx" => MarkdownFlavor::MDX,
        "quarto" => MarkdownFlavor::Quarto,
        _ => MarkdownFlavor::Standard,
    }
}

#[test]
fn regression_corpus_is_idempotent() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/regressions/idempotency");

    let entries = fs::read_dir(&dir).expect("read corpus dir");
    let mut checked = 0;
    let mut failures = Vec::new();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let content = fs::read_to_string(&path).expect("read fixture");
        let flavor = flavor_from_filename(&path);

        let once = fmt(&content, flavor);
        let twice = fmt(&once, flavor);

        if once != twice {
            failures.push(format!(
                "  {} (flavor={:?})\n    once:  {:?}\n    twice: {:?}",
                path.display(),
                flavor,
                once,
                twice
            ));
        }
        checked += 1;
    }

    eprintln!("idempotency regression corpus: {checked} fixtures checked");
    if !failures.is_empty() {
        panic!("{} idempotency regression(s):\n{}", failures.len(), failures.join("\n"));
    }
}
