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
    run_with_status(dir, args).1
}

fn run_with_status(dir: &Path, args: &[&str]) -> (bool, String) {
    let exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(exe)
        .current_dir(dir)
        .env("RUMDL_CACHE_DIR", dir.join(".rumdl_cache"))
        .args(args)
        .output()
        .expect("failed to execute rumdl");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), combined)
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
fn md057_cache_misses_when_a_missing_target_appears() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "").unwrap();
    fs::write(dir.join("a.md"), "# Source\n\n[link](b.md)\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(!first_success, "missing target must fail on the first run:\n{first}");
    assert!(first.contains("Relative link 'b.md' does not exist"), "got:\n{first}");

    fs::write(dir.join("b.md"), "# Target\n").unwrap();

    let (second_success, second) = run_with_status(dir, &["check", "a.md", "b.md", "--enable", "MD057"]);
    assert!(
        second_success,
        "creating the target must invalidate the cached MD057 warning:\n{second}"
    );
    assert!(!second.contains("MD057"), "got:\n{second}");
}

#[test]
fn md057_cache_misses_when_an_existing_target_disappears() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "").unwrap();
    fs::write(dir.join("a.md"), "# Source\n\n[link](b.md)\n").unwrap();
    fs::write(dir.join("b.md"), "# Target\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "b.md", "--enable", "MD057"]);
    assert!(first_success, "existing target must pass on the first run:\n{first}");

    fs::remove_file(dir.join("b.md")).unwrap();

    let (second_success, second) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(!second_success, "removing the target must invalidate the cached pass");
    assert!(second.contains("Relative link 'b.md' does not exist"), "got:\n{second}");
}

#[test]
fn md057_file_without_relative_links_keeps_hitting_the_cache() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "").unwrap();
    fs::write(dir.join("a.md"), "# Source\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(first_success, "first run must populate a clean cache entry:\n{first}");

    let (second_success, second) = run_with_status(dir, &["check", "a.md", "--enable", "MD057", "--verbose"]);
    assert!(second_success, "second run must remain clean:\n{second}");
    assert!(
        second.contains("Cache hit for"),
        "expected a measured cache hit:\n{second}"
    );
    assert!(!second.contains("Cache miss for"), "unexpected cache miss:\n{second}");
}

#[test]
fn md057_cache_misses_when_workspace_link_data_is_unavailable() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "").unwrap();
    fs::write(dir.join("a.md"), "# Source\n\n[link](b.md)\n").unwrap();
    fs::write(dir.join("b.md"), "# Target\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "b.md", "--enable", "MD057"]);
    assert!(first_success, "first run must populate both caches:\n{first}");
    delete_workspace_index(dir);

    let (second_success, second) = run_with_status(dir, &["check", "a.md", "b.md", "--enable", "MD057", "--verbose"]);
    assert!(
        second_success,
        "fallback lint must preserve the clean verdict:\n{second}"
    );
    assert!(
        second.contains("cross-file dependency state is unavailable"),
        "missing workspace data must force a cache miss:\n{second}"
    );
}

#[test]
fn cache_hit_does_not_require_workspace_data_when_md057_is_disabled() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "").unwrap();
    fs::write(dir.join("a.md"), "# Source\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "--enable", "MD013"]);
    assert!(first_success, "first run must populate a clean cache entry:\n{first}");
    assert!(!dir.join(".rumdl_cache/workspace_index.bin").exists());

    let (second_success, second) = run_with_status(dir, &["check", "a.md", "--enable", "MD013", "--verbose"]);
    assert!(second_success, "second run must remain clean:\n{second}");
    assert!(
        second.contains("Cache hit for"),
        "MD057-disabled run must hit:\n{second}"
    );
}

#[test]
fn md057_cache_misses_when_target_changes_from_file_to_directory() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "").unwrap();
    fs::write(dir.join("a.md"), "# Source\n\n[link](b.md)\n").unwrap();
    fs::write(dir.join("b.md"), "# Target\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "b.md", "--enable", "MD057"]);
    assert!(first_success, "file target must pass:\n{first}");

    fs::remove_file(dir.join("b.md")).unwrap();
    fs::create_dir(dir.join("b.md")).unwrap();

    let (second_success, second) = run_with_status(dir, &["check", "a.md", "--enable", "MD057", "--verbose"]);
    assert!(second_success, "directory target is also valid:\n{second}");
    assert!(
        second.contains("cross-file dependency state changed"),
        "target kind change must invalidate the cached identity:\n{second}"
    );
}

#[test]
fn md057_cache_tracks_configured_search_paths() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "[MD057]\nsearch-paths = [\"assets\"]\n").unwrap();
    fs::write(dir.join("a.md"), "# Source\n\n![image](photo.png)\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(!first_success, "missing search-path target must fail:\n{first}");

    fs::create_dir(dir.join("assets")).unwrap();
    fs::write(dir.join("assets/photo.png"), "image").unwrap();

    let (second_success, second) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(
        second_success,
        "search-path target must invalidate the warning:\n{second}"
    );
}

#[test]
fn md057_cache_tracks_obsidian_attachment_folder() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "").unwrap();
    fs::create_dir(dir.join(".obsidian")).unwrap();
    fs::write(
        dir.join(".obsidian/app.json"),
        r#"{"attachmentFolderPath":"Attachments"}"#,
    )
    .unwrap();
    fs::write(dir.join("a.md"), "# Source\n\n![image](photo.png)\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "--enable", "MD057", "--flavor", "obsidian"]);
    assert!(!first_success, "missing attachment must fail:\n{first}");

    fs::create_dir(dir.join("Attachments")).unwrap();
    fs::write(dir.join("Attachments/photo.png"), "image").unwrap();

    let (second_success, second) =
        run_with_status(dir, &["check", "a.md", "--enable", "MD057", "--flavor", "obsidian"]);
    assert!(
        second_success,
        "attachment target must invalidate the warning:\n{second}"
    );
}

#[test]
fn md057_cache_tracks_absolute_link_roots() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(
        dir.join(".rumdl.toml"),
        "[MD057]\nabsolute-links = \"relative_to_roots\"\nroots = [\"content\"]\n",
    )
    .unwrap();
    fs::write(dir.join("a.md"), "# Source\n\n[guide](/guide.md)\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(!first_success, "missing absolute target must fail:\n{first}");

    fs::create_dir(dir.join("content")).unwrap();
    fs::write(dir.join("content/guide.md"), "# Guide\n").unwrap();

    let (second_success, second) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(second_success, "root target must invalidate the warning:\n{second}");
}

#[test]
fn md057_cache_tracks_mkdocs_docs_dir() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(
        dir.join(".rumdl.toml"),
        "[MD057]\nabsolute-links = \"relative_to_docs\"\n",
    )
    .unwrap();
    fs::write(dir.join("mkdocs.yml"), "site_name: Test\ndocs_dir: docs\n").unwrap();
    fs::create_dir(dir.join("docs")).unwrap();
    fs::write(dir.join("a.md"), "# Source\n\n[guide](/guide.md)\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(!first_success, "missing docs-dir target must fail:\n{first}");

    fs::write(dir.join("docs/guide.md"), "# Guide\n").unwrap();

    let (second_success, second) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(second_success, "docs-dir target must invalidate the warning:\n{second}");
}

#[test]
fn md057_cache_tracks_reference_definition_targets() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "").unwrap();
    fs::write(
        dir.join("a.md"),
        "# Source\n\n[download][asset]\n\n[asset]: artifact.zip\n",
    )
    .unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(!first_success, "missing reference target must fail:\n{first}");

    fs::write(dir.join("artifact.zip"), "archive").unwrap();

    let (second_success, second) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(
        second_success,
        "reference target must invalidate the warning:\n{second}"
    );
}

#[test]
fn md057_cache_tracks_checked_frontmatter_targets() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "[MD057]\ncheck-frontmatter = true\n").unwrap();
    fs::write(dir.join("a.md"), "---\nmanual: docs/guide.pdf\n---\n\n# Source\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(!first_success, "missing frontmatter target must fail:\n{first}");

    fs::create_dir(dir.join("docs")).unwrap();
    fs::write(dir.join("docs/guide.pdf"), "guide").unwrap();

    let (second_success, second) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(
        second_success,
        "frontmatter target must invalidate the warning:\n{second}"
    );
}

#[test]
fn md057_fmt_does_not_replay_a_stale_warning() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    fs::write(dir.join(".rumdl.toml"), "").unwrap();
    fs::write(dir.join("a.md"), "# Source\n\n[link](b.md)\n").unwrap();

    let (first_success, first) = run_with_status(dir, &["check", "a.md", "--enable", "MD057"]);
    assert!(
        !first_success,
        "missing target must warm a failing cache entry:\n{first}"
    );

    fs::write(dir.join("b.md"), "# Target\n").unwrap();

    let (_, formatted) = run_with_status(dir, &["fmt", "a.md", "b.md", "--enable", "MD057"]);
    assert!(
        !formatted.contains("MD057"),
        "fmt must not replay the stale warning:\n{formatted}"
    );
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
