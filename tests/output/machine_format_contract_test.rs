//! End-to-end contract tests for the machine-readable output formats documented
//! in `docs/output-formats.md` and committed as stable surfaces in
//! `docs/stability.md`. These assert the documented field structure on real CLI
//! output so a field rename, a missing field, or a structural change fails CI.
//!
//! SARIF is checked for structural conformance (the required fields rumdl
//! publishes and the severity -> level mapping) rather than full SARIF 2.1.0
//! JSON-schema validation: the SARIF schema marks nearly every field optional,
//! so schema validation passes trivially, whereas asserting the published
//! contract actually catches regressions.

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::fs;
use std::path::Path;

/// Deterministic, fixable violations:
/// - closed-ATX heading -> MD003 (fixable)
/// - trailing whitespace -> MD009 (fixable)
/// - bare URL -> MD034 (fixable)
const FIXTURE: &str = "# Title\n\n## Closed Heading ##\n\ntrailing spaces here   \n\nSee http://example.com now.\n";

fn write_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, FIXTURE).unwrap();
    (dir, path)
}

fn run_format(path: &Path, format: &str) -> String {
    let output = cargo_bin_cmd!("rumdl")
        .args(["check", path.to_str().unwrap(), "--output-format", format, "--no-cache"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).expect("output is valid UTF-8")
}

#[test]
fn json_contract() {
    let (_dir, path) = write_fixture();
    let parsed: Value = serde_json::from_str(&run_format(&path, "json")).expect("valid JSON");

    let warnings = parsed.as_array().expect("json output is a flat array");
    assert!(!warnings.is_empty(), "fixture should produce violations");

    let mut saw_fix = false;
    for w in warnings {
        assert!(w["file"].is_string(), "file: {w}");
        assert!(w["line"].is_u64(), "line: {w}");
        assert!(w["column"].is_u64(), "column: {w}");
        assert!(w["rule"].is_string(), "rule: {w}");
        assert!(w["message"].is_string(), "message: {w}");
        assert!(
            matches!(w["severity"].as_str(), Some("error" | "warning" | "info")),
            "severity: {w}"
        );
        let fixable = w["fixable"].as_bool().expect("fixable is a boolean");

        let fix = w.get("fix").filter(|f| !f.is_null());
        if fixable {
            let fix = fix.expect("a fixable violation must carry a fix object");
            assert!(fix["range"]["start"].is_u64(), "fix.range.start: {w}");
            assert!(fix["range"]["end"].is_u64(), "fix.range.end: {w}");
            assert!(fix["replacement"].is_string(), "fix.replacement: {w}");
            saw_fix = true;
        }
    }
    assert!(
        saw_fix,
        "fixture should include at least one fixable violation with a fix"
    );
}

#[test]
fn json_lines_contract_omits_fix() {
    let (_dir, path) = write_fixture();
    let stdout = run_format(&path, "json-lines");

    let mut count = 0;
    for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
        let w: Value = serde_json::from_str(line).expect("each line is a valid JSON object");
        for field in ["file", "line", "column", "rule", "message", "severity", "fixable"] {
            assert!(w.get(field).is_some(), "json-lines missing {field:?}: {line}");
        }
        // json-lines intentionally omits the fix object (use json for fix detail).
        assert!(w.get("fix").is_none(), "json-lines must omit the fix object: {line}");
        count += 1;
    }
    assert!(count >= 1, "expected at least one json-lines record");
}

#[test]
fn sarif_contract() {
    let (_dir, path) = write_fixture();
    let v: Value = serde_json::from_str(&run_format(&path, "sarif")).expect("valid SARIF JSON");

    assert_eq!(v["version"], "2.1.0", "SARIF version");
    assert!(v["$schema"].is_string(), "$schema present");

    let runs = v["runs"].as_array().expect("runs array");
    assert_eq!(runs.len(), 1, "single run");

    let driver = &runs[0]["tool"]["driver"];
    assert_eq!(driver["name"], "rumdl", "driver name");
    assert!(driver["rules"].is_array(), "driver.rules array");

    let results = runs[0]["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "fixture should produce results");
    for r in results {
        assert!(r["ruleId"].is_string(), "ruleId: {r}");
        assert!(
            matches!(r["level"].as_str(), Some("error" | "warning" | "note")),
            "level (severity mapped): {r}"
        );
        assert!(r["message"]["text"].is_string(), "message.text: {r}");
        let loc = &r["locations"][0]["physicalLocation"];
        assert!(loc["artifactLocation"]["uri"].is_string(), "artifactLocation.uri: {r}");
        assert!(loc["region"]["startLine"].is_u64(), "region.startLine: {r}");
        assert!(loc["region"]["startColumn"].is_u64(), "region.startColumn: {r}");
    }
}

#[test]
fn junit_contract() {
    let (_dir, path) = write_fixture();
    let stdout = run_format(&path, "junit");

    assert!(stdout.starts_with("<?xml"), "XML declaration");
    assert!(stdout.contains("<testsuites name=\"rumdl\""), "testsuites element");
    assert!(stdout.contains("<testsuite name="), "testsuite element");
    assert!(stdout.contains("classname=\"rumdl\""), "testcase classname");

    let failures = stdout.matches("<failure type=\"MD").count();
    assert!(failures >= 1, "expected one <failure> per violation, got {failures}");
    assert!(stdout.contains(" at line "), "failure body includes line/column");
}
