//! Integration tests for NUL-framed multi-document stdin.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn run_batch(dir: &Path, input: &[u8], args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute rumdl");

    child
        .stdin
        .as_mut()
        .expect("stdin must be piped")
        .write_all(input)
        .expect("failed to write batch stdin");

    child.wait_with_output().expect("failed to collect rumdl output")
}

#[test]
fn stdin_batch_lints_every_supplied_document() {
    let temp = tempfile::tempdir().unwrap();
    let input = b"docs/a.md\0# A\n\n### Skipped level\n\0docs/b.md\0# B\n\n### Skipped level\n\0";

    let output = run_batch(
        temp.path(),
        input,
        &["check", "--stdin-batch", "--no-cache", "--enable", "MD001", "--quiet"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");

    assert_eq!(output.status.code(), Some(1), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.contains("docs/a.md:3:1") && stdout.contains("docs/b.md:3:1"),
        "each supplied path must receive its own diagnostic.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stdout.matches("MD001").count(),
        2,
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stderr.is_empty(), "unexpected stderr:\n{stderr}");
}

#[test]
fn stdin_batch_cross_file_checks_use_supplied_content() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("docs")).unwrap();
    fs::write(temp.path().join("docs/b.md"), "# Disk version\n\n## Disk heading\n").unwrap();

    let input = b"docs/a.md\0# A\n\n[valid](b.md#batch-heading)\n\n[invalid](b.md#disk-heading)\n\0docs/b.md\0# Batch version\n\n## Batch heading\n\0";
    let output = run_batch(
        temp.path(),
        input,
        &["check", "--stdin-batch", "--no-cache", "--enable", "MD051", "--quiet"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");

    assert_eq!(output.status.code(), Some(1), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.contains("Link fragment 'disk-heading' not found in 'b.md'"),
        "the on-disk-only heading must be rejected.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("'batch-heading'"),
        "the supplied heading must resolve.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stderr.is_empty(), "unexpected stderr:\n{stderr}");
}

#[test]
fn stdin_batch_falls_back_to_disk_for_unsupplied_documents() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("docs")).unwrap();
    fs::write(temp.path().join("docs/b.md"), "# Disk target\n\n## Real heading\n").unwrap();

    let input = b"docs/a.md\0# A\n\n[valid](b.md#real-heading)\n\n[invalid](b.md#missing-heading)\n\0";
    let output = run_batch(
        temp.path(),
        input,
        &["check", "--stdin-batch", "--no-cache", "--enable", "MD051", "--quiet"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");

    assert_eq!(output.status.code(), Some(1), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.contains("Link fragment 'missing-heading' not found in 'b.md'"),
        "an unsupplied target must be indexed from disk.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("'real-heading'"),
        "the real on-disk heading must resolve.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stderr.is_empty(), "unexpected stderr:\n{stderr}");
}

#[test]
fn stdin_batch_treats_supplied_documents_as_existing_link_targets() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("docs")).unwrap();
    // An existing source canonicalizes to an absolute base path inside MD057;
    // the unsaved target still has to match its supplied relative identity.
    fs::write(temp.path().join("docs/a.md"), "# Saved A\n").unwrap();
    let input = b"docs/a.md\0# A\n\n[batch target](b.md)\n\0docs/b.md\0# B\n\0";

    let output = run_batch(
        temp.path(),
        input,
        &["check", "--stdin-batch", "--no-cache", "--enable", "MD057", "--quiet"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");

    assert_eq!(output.status.code(), Some(0), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(stdout.is_empty(), "unexpected stdout:\n{stdout}");
    assert!(stderr.is_empty(), "unexpected stderr:\n{stderr}");
}

#[test]
fn stdin_batch_closed_world_rejects_targets_outside_the_supplied_set() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("docs")).unwrap();
    fs::write(temp.path().join("docs/b.md"), "# On disk\n").unwrap();
    let input = b"docs/a.md\0# A\n\n[disk target](b.md)\n\0";

    let output = run_batch(
        temp.path(),
        input,
        &[
            "check",
            "--stdin-batch",
            "--stdin-batch-closed-world",
            "--no-cache",
            "--enable",
            "MD057",
            "--quiet",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");

    assert_eq!(output.status.code(), Some(1), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.contains("Relative link 'b.md' target not in the supplied document set"),
        "closed-world diagnostics must distinguish omitted targets from missing files.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("does not exist"), "stdout:\n{stdout}");
    assert!(stderr.is_empty(), "unexpected stderr:\n{stderr}");
}

#[test]
fn stdin_batch_reports_source_context_from_the_supplied_buffer() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("docs")).unwrap();
    fs::write(temp.path().join("docs/a.md"), "# Disk\n\n### Disk text\n").unwrap();
    let input = b"docs/a.md\0# Supplied\n\n### Supplied text\n\0";

    let output = run_batch(
        temp.path(),
        input,
        &[
            "check",
            "--stdin-batch",
            "--no-cache",
            "--enable",
            "MD001",
            "--output-format",
            "full",
            "--quiet",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");

    assert_eq!(output.status.code(), Some(1), "stdout:\n{stdout}");
    assert!(stdout.contains("### Supplied text"), "stdout:\n{stdout}");
    assert!(!stdout.contains("### Disk text"), "stdout:\n{stdout}");
}

#[test]
fn stdin_batch_rejects_malformed_framing() {
    let temp = tempfile::tempdir().unwrap();
    let output = run_batch(
        temp.path(),
        b"docs/a.md\0# Missing final delimiter\n",
        &["check", "--stdin-batch", "--quiet"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");

    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("batch input must end with a NUL byte"),
        "stderr:\n{stderr}"
    );
}

#[test]
fn stdin_batch_does_not_modify_the_workspace_index_cache() {
    let temp = tempfile::tempdir().unwrap();
    let cache_dir = temp.path().join("cache");
    fs::create_dir(&cache_dir).unwrap();
    let workspace_cache = cache_dir.join("workspace_index.bin");
    let sentinel = b"existing workspace snapshot";
    fs::write(&workspace_cache, sentinel).unwrap();

    let output = run_batch(
        temp.path(),
        b"docs/a.md\0# A\n\0",
        &[
            "check",
            "--stdin-batch",
            "--cache-dir",
            cache_dir.to_str().unwrap(),
            "--quiet",
        ],
    );

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(workspace_cache).unwrap(), sentinel);
}

#[test]
fn stdin_batch_rejects_file_arguments_and_single_stdin_metadata() {
    let temp = tempfile::tempdir().unwrap();
    for incompatible in [
        vec!["check", "--stdin-batch", "README.md"],
        vec!["check", "--stdin-batch", "--stdin-filename", "README.md"],
    ] {
        let output = run_batch(temp.path(), b"", &incompatible);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
        assert!(stderr.contains("cannot be used with"), "stderr:\n{stderr}");
    }
}

#[test]
fn stdin_batch_rejects_paths_that_normalize_to_the_same_document() {
    let temp = tempfile::tempdir().unwrap();
    let output = run_batch(
        temp.path(),
        b"docs/a.md\0# A\n\0docs/./a.md\0# Duplicate\n\0",
        &["check", "--stdin-batch", "--quiet"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");

    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(stderr.contains("duplicate path 'docs/./a.md'"), "stderr:\n{stderr}");
}

#[test]
fn stdin_batch_honors_per_directory_configuration() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("docs")).unwrap();
    fs::write(temp.path().join("docs/.rumdl.toml"), "enable = [\"MD001\"]\n").unwrap();
    let output = run_batch(
        temp.path(),
        b"docs/a.md\0# A\n\n### Skipped level\n\0",
        &["check", "--stdin-batch", "--no-cache", "--quiet"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");

    assert_eq!(output.status.code(), Some(1), "stdout:\n{stdout}");
    assert!(
        stdout.contains("docs/a.md:3:1") && stdout.contains("MD001"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn stdin_batch_emits_one_valid_structured_report() {
    let temp = tempfile::tempdir().unwrap();
    let output = run_batch(
        temp.path(),
        b"a.md\0# A\n\n### Bad\n\0b.md\0# B\n\n### Bad\n\0",
        &[
            "check",
            "--stdin-batch",
            "--no-cache",
            "--enable",
            "MD001",
            "--output-format",
            "json",
        ],
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid JSON ({error}):\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let report = report.as_array().expect("JSON report must be an array");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report.len(), 2, "report: {report:#?}");
    let serialized = serde_json::to_string(report).unwrap();
    assert!(serialized.contains("a.md") && serialized.contains("b.md"));
    assert!(output.stderr.is_empty());
}

#[test]
fn stdin_batch_propagates_inline_configuration_warnings() {
    let temp = tempfile::tempdir().unwrap();
    let output = run_batch(
        temp.path(),
        b"a.md\0<!-- rumdl-disable UNKNOWN -->\n# A\n\0",
        &[
            "check",
            "--stdin-batch",
            "--no-cache",
            "--deny-config-warnings",
            "--quiet",
        ],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");

    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("UNKNOWN") && stderr.contains("a.md"),
        "stderr:\n{stderr}"
    );
}
