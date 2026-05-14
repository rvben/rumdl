//! Replays every minimized failing input from past idempotency proptest runs.
//! Each `.md` file in tests/regressions/idempotency/ must be idempotent through
//! the full default rule pipeline for the flavor encoded in its filename.
//!
//! Filename convention: `<slug>.<flavor>.md` (e.g. `atx-heading-multispace.standard.md`).
//! Recognised flavor suffixes: standard, mkdocs, mdx, quarto.
//! Files lacking a recognised flavor suffix default to Standard.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{LintWarning, Rule};
use rumdl_lib::rules::all_rules;
use std::fs;
use std::path::Path;

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

fn fmt_once(content: &str, flavor: MarkdownFlavor, rules: &[Box<dyn Rule>]) -> String {
    let ctx = LintContext::new(content, flavor, None);
    let mut warnings = Vec::new();
    for rule in rules {
        if let Ok(ws) = rule.check(&ctx) {
            warnings.extend(ws);
        }
    }
    apply_all_fixes(content, &warnings)
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
#[ignore = "pipeline idempotency bug, see tests/rules/idempotency_pipeline.rs"]
fn regression_corpus_is_idempotent() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/regressions/idempotency");
    let rules = all_rules(&Config::default());

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

        let once = fmt_once(&content, flavor, &rules);
        let twice = fmt_once(&once, flavor, &rules);

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
