//! Whole-directory scans must never reuse partial or stale source coverage.
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn put(root: &Path, path: &str, content: impl AsRef<[u8]>) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn run(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.current_dir(root).arg("check-assets");
    if !args.contains(&"--root") {
        command.args(["--root", "."]);
    }
    command
        .args(["--source", "docs", "--assets", "docs/assets", "--output-format", "json"])
        .args(args)
        .output()
        .unwrap()
}

fn report(output: &Output, code: i32) -> serde_json::Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn links_definitions_and_html_preserve_referenced_assets() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for name in [
        "one space.png",
        "two.png",
        "three.png",
        "four.png",
        "five.png",
        "six.png",
        "seven.png",
        "a&b.png",
        "lazy.png",
        "real.png",
        "unused.png",
    ] {
        put(root, &format!("docs/assets/{name}"), [0xff, 0xfe]);
    }
    put(
        root,
        "docs/guide.MD",
        r#"---
cover: assets/seven.png
---
# Guide
[cover](assets/seven.png)

![one](<assets/one%20space.png?raw=true#x>)

[unused-definition]: assets/two.png
<img
 SRC='assets/three.png'
 srcset="assets/four.png 1x, assets/five.png 2x">
<video poster=assets/six.png></video>
<object data="assets/a&amp;b.png"></object>
<img data-src="assets/lazy.png" src="assets/real.png">
"#,
    );
    let value = report(&run(root, &[]), 1);
    assert_eq!(value["unreferenced"], serde_json::json!(["docs/assets/unused.png"]));
    assert_eq!(value["mentioned_but_unresolved"], serde_json::json!([]));
    assert_eq!(value["schema_version"], 1);
    assert!(
        value["source_extensions"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("mdx"))
    );
    assert_eq!(value["sources_scanned"], 1);
    assert_eq!(value["assets_considered"], 11);
}

#[test]
fn root_relative_links_and_cross_directory_sources_are_resolved() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/root.png", "image");
    put(dir.path(), "docs/assets/other.png", "image");
    put(dir.path(), "docs/page.md", "![x](/docs/assets/root.png#part)\n");
    put(
        dir.path(),
        "elsewhere/other.md",
        "[download](../docs/assets/other.png)\n",
    );
    let value = report(&run(dir.path(), &["--source", "elsewhere"]), 0);
    assert_eq!(value["unreferenced"], serde_json::json!([]));
    assert_eq!(value["sources_scanned"], 2);
}

#[test]
fn templates_wiki_imports_and_code_mentions_are_kept_without_claiming_resolution() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "template.png",
        "wiki.png",
        "import.png",
        "code.png",
        "inline.png",
        "truly-unused.png",
    ] {
        put(dir.path(), &format!("docs/assets/{name}"), "image");
    }
    put(
        dir.path(),
        "docs/page.mdx",
        "{{< figure src=\"template.png\" >}}\n![[wiki.png]]\nimport image from './assets/import.png'\n```md\n![sample](assets/code.png)\n```\n`![x](assets/inline.png)`\n",
    );
    let value = report(&run(dir.path(), &[]), 1);
    assert_eq!(
        value["unreferenced"],
        serde_json::json!(["docs/assets/truly-unused.png"])
    );
    assert_eq!(value["mentioned_but_unresolved"].as_array().unwrap().len(), 5);
}

#[test]
fn fresh_scans_observe_source_and_asset_additions_and_removals_deterministically() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/a.png", "image");
    put(dir.path(), "docs/page.md", "# Guide\n");
    let first = run(dir.path(), &[]);
    report(&first, 1);
    assert_eq!(first.stdout, run(dir.path(), &[]).stdout);
    put(dir.path(), "docs/second.md", "![x](assets/a.png)\n");
    report(&run(dir.path(), &[]), 0);
    put(dir.path(), "docs/assets/b.png", "image");
    let value = report(&run(dir.path(), &[]), 1);
    assert_eq!(value["unreferenced"], serde_json::json!(["docs/assets/b.png"]));
    fs::remove_file(dir.path().join("docs/second.md")).unwrap();
    assert_eq!(
        report(&run(dir.path(), &[]), 1)["unreferenced"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    fs::remove_file(dir.path().join("docs/assets/b.png")).unwrap();
    assert_eq!(
        report(&run(dir.path(), &[]), 1)["unreferenced"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn include_keep_and_overlapping_roots_are_explicit_and_deduplicated() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/keep.png", "image");
    put(dir.path(), "docs/assets/report.png", "image");
    put(dir.path(), "docs/assets/style.css", "style");
    put(dir.path(), "docs/page.md", "# Guide\n");
    let value = report(
        &run(
            dir.path(),
            &[
                "--source",
                "docs",
                "--assets",
                "docs/assets",
                "--include",
                "**/*.png",
                "--keep",
                "docs/assets/keep.png",
            ],
        ),
        1,
    );
    assert_eq!(value["sources_scanned"], 1);
    assert_eq!(value["assets_considered"], 1);
    assert_eq!(value["unreferenced"], serde_json::json!(["docs/assets/report.png"]));
}

#[test]
fn incomplete_or_empty_scans_are_tool_errors_and_emit_no_findings() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/a.png", [0xff]);
    let output = run(dir.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    put(dir.path(), "docs/page.md", [0xff]);
    let output = run(dir.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    put(dir.path(), "docs/page.md", "# Guide\n");
    for (args, reason) in [
        (vec!["--source", "docs/page.md"], "must be directories within"),
        (vec!["--source", "missing"], "Cannot scan"),
        (vec!["--include", "["], "Invalid asset glob"),
        (
            vec!["--config", "MD013.line-length=80"],
            "does not accept configuration flags",
        ),
        (vec!["--no-config"], "does not accept configuration flags"),
        (vec!["--isolated"], "does not accept configuration flags"),
    ] {
        let output = run(dir.path(), &args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(reason), "{args:?}");
    }
}

#[cfg(unix)]
#[test]
fn symlinked_source_directories_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/a.png", "image");
    put(dir.path(), "docs/page.md", "# Guide\n");
    fs::create_dir(dir.path().join("hidden")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("hidden"), dir.path().join("docs/link")).unwrap();
    let output = run(dir.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn escaped_and_encoded_names_resolve_without_the_literal_mention_fallback() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a_b.png", "a&b.png", "é.png", "what#.png"] {
        put(dir.path(), &format!("docs/assets/{name}"), "image");
    }
    put(
        dir.path(),
        "docs/page.md",
        "[a](assets/a\\_b.png)\n[b](assets/a&amp;b.png)\n[c](assets/%c3%a9.png)\n[d](assets/what%23.png?raw=true#x)\n",
    );
    let value = report(&run(dir.path(), &[]), 0);
    assert_eq!(value["mentioned_but_unresolved"], serde_json::json!([]));
}

#[test]
fn malformed_or_broken_destinations_do_not_abort_a_complete_scan() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/unreferenced.png", "image");
    put(
        dir.path(),
        "docs/page.md",
        format!("<img src=\"&#0;\">\n\n[x]({})\n", "a".repeat(300)),
    );
    assert_eq!(
        report(&run(dir.path(), &[]), 1)["unreferenced"],
        serde_json::json!(["docs/assets/unreferenced.png"])
    );
}

#[test]
fn mentions_alone_are_clean_and_html_inside_code_is_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/code.png", "image");
    put(
        dir.path(),
        "docs/page.md",
        "`<img src=\"assets/code.png\">`\n\n```html\n<img src=\"assets/code.png\">\n```\n",
    );
    let value = report(&run(dir.path(), &[]), 0);
    assert_eq!(
        value["mentioned_but_unresolved"],
        serde_json::json!(["docs/assets/code.png"])
    );
}

#[test]
fn frontmatter_is_not_parsed_as_body_and_first_body_definition_is_resolved() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/cover.png", "image");
    put(dir.path(), "docs/assets/definition.png", "image");
    put(
        dir.path(),
        "docs/page.md",
        "---\ncover: assets/cover.png\n---\n[first]: assets/definition.png\n",
    );
    put(dir.path(), "docs/frontmatter.md", "---\ncover: assets/cover.png\n---\n");
    let value = report(&run(dir.path(), &[]), 0);
    assert_eq!(
        value["mentioned_but_unresolved"],
        serde_json::json!(["docs/assets/cover.png"])
    );
}

#[test]
fn html_srcset_handles_data_urls_no_descriptors_and_trailing_commas() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.png", "b.png", "c.png"] {
        put(dir.path(), &format!("docs/assets/{name}"), "image");
    }
    put(
        dir.path(),
        "docs/page.md",
        "Text <img srcset=\"data:image/png;base64,AAAA 1x, assets/a.png,\nassets/b.png, assets/c.png 3x,\"> after.\n",
    );
    let value = report(&run(dir.path(), &[]), 0);
    assert_eq!(value["mentioned_but_unresolved"], serde_json::json!([]));
}

#[test]
fn explicit_site_root_and_ignores_do_not_reduce_source_coverage() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/a.png", "image");
    put(dir.path(), "docs/page.md", "[x](/assets/a.png)\n");
    put(dir.path(), ".rumdl.toml", "this is invalid TOML");
    put(dir.path(), "docs/.gitignore", "page.md\n");
    let value = report(&run(dir.path(), &["--root", "docs"]), 0);
    assert_eq!(value["mentioned_but_unresolved"], serde_json::json!([]));
    let output = run(dir.path(), &["--root", "docs/assets"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("within"));
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_asset_names_fail_with_tool_error_instead_of_json_panic() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/page.md", "# Guide\n");
    fs::create_dir(dir.path().join("docs/assets")).unwrap();
    fs::write(
        dir.path()
            .join("docs/assets")
            .join(std::ffi::OsStr::from_bytes(b"\xff.png")),
        "image",
    )
    .unwrap();
    let output = run(dir.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not valid UTF-8"));
}

#[test]
fn repeated_mentions_preserve_thousands_of_assets_with_shared_names() {
    let dir = tempfile::tempdir().unwrap();
    put(
        dir.path(),
        "docs/page.md",
        "{{< figure src=\"shared.png\" >}}\n".repeat(2_000),
    );
    for i in 0..12_000 {
        put(dir.path(), &format!("docs/assets/{i}/shared.png"), "image");
    }
    let value = report(&run(dir.path(), &[]), 0);
    assert_eq!(value["mentioned_but_unresolved"].as_array().unwrap().len(), 12_000);
}

#[test]
fn text_output_labels_findings_mentions_and_counts() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/a.png", "image");
    put(dir.path(), "docs/assets/b.png", "image");
    put(dir.path(), "docs/page.md", "the b.png example\n");
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args([
            "check-assets",
            "--root",
            ".",
            "--source",
            "docs",
            "--assets",
            "docs/assets",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("docs/assets/a.png: unreferenced in scanned Markdown sources"));
    assert!(text.contains("docs/assets/b.png: mentioned but unresolved (kept)"));
    assert!(text.contains("2 assets; 1 unreferenced, 1 mentioned but unresolved"));
}

#[cfg(unix)]
#[test]
fn asset_only_symlinks_fail_closed_without_scanning_their_targets() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/page.md", "# Guide\n");
    fs::create_dir(dir.path().join("docs/assets")).unwrap();
    fs::create_dir(dir.path().join("assets")).unwrap();
    std::os::unix::fs::symlink("missing", dir.path().join("assets/link")).unwrap();
    let output = run(dir.path(), &["--assets", "assets"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("symlink"));
}

#[test]
fn empty_asset_selection_is_a_clean_complete_scan() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/page.md", "# Guide\n");
    fs::create_dir(dir.path().join("docs/assets")).unwrap();
    let value = report(&run(dir.path(), &[]), 0);
    assert_eq!(value["assets_considered"], 0);
    assert_eq!(value["sources_scanned"], 1);
    put(dir.path(), "docs/assets/a.png", "image");
    for args in [vec!["--include", "**/*.pdf"], vec!["--keep", "**/*"]] {
        let value = report(&run(dir.path(), &args), 0);
        assert_eq!(value["assets_considered"], 0);
    }
}

#[test]
fn html_entities_resolve_and_malformed_numeric_entities_do_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a&b.png", "é.png", "a_b.png", "unused.png"] {
        put(dir.path(), &format!("docs/assets/{name}"), "image");
    }
    put(
        dir.path(),
        "docs/page.md",
        r#"<img src="assets/a&amp;b.png">
<img src="assets/&eacute;.png">
<img src="assets/a&#x5F;b.png">
<img src="&#999999999999999999999999999999;">
<img src="&#xFFFFFFFFFFFFFFFFFFFFFFFFFFFF;">
<img src="&#x;">
<img src="&#-1;">
<img src="&unknown;">
"#,
    );
    let value = report(&run(dir.path(), &[]), 1);
    assert_eq!(value["unreferenced"], serde_json::json!(["docs/assets/unused.png"]));
    assert_eq!(value["mentioned_but_unresolved"], serde_json::json!([]));
}

#[test]
fn a_failure_after_readable_sources_emits_no_partial_findings() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/page.md", "# Guide\n");
    put(dir.path(), "docs/z-invalid.md", [0xff]);
    put(dir.path(), "docs/assets/unused.png", "image");
    let output = run(dir.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn markdown_inside_asset_trees_requires_explicit_source_coverage() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("docs/assets")).unwrap();
    put(dir.path(), "docs/page.md", "# Guide\n");
    put(dir.path(), "media/gallery.md", "![image](image.png)\n");
    put(dir.path(), "media/image.png", "image");
    let output = run(dir.path(), &["--assets", "media"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("add its directory with --source"));
    let value = report(&run(dir.path(), &["--assets", "media", "--source", "media"]), 0);
    assert_eq!(value["sources_scanned"], 2);
    assert_eq!(value["mentioned_but_unresolved"], serde_json::json!([]));
}

#[test]
fn differently_cased_paths_follow_filesystem_identity() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/photo.png", "image");
    put(dir.path(), "docs/page.md", "![x](assets/PHOTO.PNG)\n");
    let case_insensitive = dir.path().join("docs/assets/PHOTO.PNG").exists();
    let value = report(&run(dir.path(), &[]), if case_insensitive { 0 } else { 1 });
    assert_eq!(
        value["unreferenced"].as_array().unwrap().len(),
        usize::from(!case_insensitive)
    );
    assert_eq!(value["mentioned_but_unresolved"], serde_json::json!([]));
}

#[test]
fn encoded_relative_and_root_paths_resolve_from_canonical_scan_roots() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "docs/assets/photo.png", "image");
    put(dir.path(), "docs/assets/other.png", "image");
    put(
        dir.path(),
        "docs/nested/page.md",
        "![x](../assets/%70hoto.png)\n![y](/docs/assets/%6fther.png)\n",
    );
    let value = report(&run(dir.path(), &[]), 0);
    assert_eq!(value["mentioned_but_unresolved"], serde_json::json!([]));
}
