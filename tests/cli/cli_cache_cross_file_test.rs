//! Regression tests for cross-file rule suppression (inline disable +
//! per-file-ignores), especially across the lint-cache fast path.
//!
//! When `rumdl` gets a lint-cache hit but the workspace-index cache is absent
//! or stale, it rebuilds the cross-file index via `build_file_index_only`. That
//! rebuild must honor the same suppression the normal lint path applies:
//! inline `<!-- rumdl-disable -->` blocks. Separately, `per-file-ignores` must
//! suppress cross-file rules for the ignored file on every path.
//!
//! MD051 (cross-file link-fragment validation) is the live cross-file rule used
//! to exercise these paths.

use std::fs;
use std::path::Path;
use std::process::Command;

/// Run the rumdl binary in `dir` with `args`, returning combined stdout+stderr.
///
/// Pins `RUMDL_CACHE_DIR` to `dir/.rumdl_cache` so the cache location is
/// deterministic regardless of any ambient `RUMDL_CACHE_DIR` in the test
/// environment (which would otherwise redirect or share the workspace index).
fn run(dir: &Path, args: &[&str]) -> String {
    let exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(exe)
        .current_dir(dir)
        .env("RUMDL_CACHE_DIR", dir.join(".rumdl_cache"))
        .args(args)
        .output()
        .expect("failed to execute rumdl");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

/// Delete only the workspace-index cache, leaving the lint cache intact, so the
/// next run takes the lint-cache-hit + index-rebuild path.
fn delete_workspace_index(dir: &Path) {
    let path = dir.join(".rumdl_cache").join("workspace_index.bin");
    assert!(
        path.exists(),
        "expected workspace index cache at {} after first run",
        path.display()
    );
    fs::remove_file(&path).expect("failed to remove workspace_index.bin");
}

/// b.md provides a heading anchor; a.md links to a *missing* fragment in b.md,
/// which is what MD051's cross-file check flags.
fn write_target(dir: &Path) {
    fs::write(dir.join("b.md"), "# Target\n\n## Real Heading\n").unwrap();
}

#[test]
fn cache_hit_respects_inline_disable_for_cross_file_rule() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "").unwrap();
    write_target(dir);
    fs::write(
        dir.join("a.md"),
        "# Source\n\n\
         <!-- rumdl-disable MD051 -->\n\
         [link](b.md#nonexistent)\n\
         <!-- rumdl-enable MD051 -->\n",
    )
    .unwrap();

    // First run: populate caches. MD051 is suppressed by the inline block.
    let first = run(dir, &["check", "."]);
    assert!(
        !first.contains("MD051"),
        "baseline: inline disable should suppress MD051, got:\n{first}"
    );

    // Drop only the workspace-index cache, forcing the index-rebuild path.
    delete_workspace_index(dir);

    // Second run: lint-cache hit + index rebuild must still honor the disable.
    let second = run(dir, &["check", "."]);
    assert!(
        !second.contains("MD051"),
        "MD051 must stay suppressed on a cache hit (inline disable), got:\n{second}"
    );
}

#[test]
fn per_file_ignores_suppresses_cross_file_rule_without_cache() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "[per-file-ignores]\n\"a.md\" = [\"MD051\"]\n").unwrap();
    write_target(dir);
    fs::write(dir.join("a.md"), "# Source\n\n[link](b.md#nonexistent)\n").unwrap();

    // --no-cache isolates this from the lint cache: per-file-ignores alone must
    // suppress the cross-file rule for a.md.
    let out = run(dir, &["check", ".", "--no-cache"]);
    assert!(
        !out.contains("MD051"),
        "per-file-ignores must suppress cross-file MD051 for a.md, got:\n{out}"
    );
}

#[test]
fn per_file_ignores_suppresses_cross_file_rule_on_cache_hit() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "[per-file-ignores]\n\"a.md\" = [\"MD051\"]\n").unwrap();
    write_target(dir);
    fs::write(dir.join("a.md"), "# Source\n\n[link](b.md#nonexistent)\n").unwrap();

    // First run populates caches.
    let _ = run(dir, &["check", "."]);
    delete_workspace_index(dir);

    // Cache-hit + index rebuild must still honor per-file-ignores.
    let out = run(dir, &["check", "."]);
    assert!(
        !out.contains("MD051"),
        "per-file-ignores must suppress cross-file MD051 on a cache hit, got:\n{out}"
    );
}
