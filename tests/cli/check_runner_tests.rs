use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::tempdir;

fn rumdl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
}

#[test]
fn test_parallel_issue_count_is_deterministic() {
    let dir = tempdir().unwrap();

    for i in 0..20 {
        fs::write(
            dir.path().join(format!("file_{i:02}.md")),
            format!("# File {i}\n\nLine with trailing spaces   \n"),
        )
        .unwrap();
    }

    let run = || {
        rumdl()
            .args(["check", ".", "--enable", "MD009"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to run rumdl")
    };

    let first = run();
    let second = run();

    let stdout1 = String::from_utf8_lossy(&first.stdout);
    let stdout2 = String::from_utf8_lossy(&second.stdout);

    // rumdl processes files in parallel so per-file line order is non-deterministic,
    // but the total issue count must be stable across runs.
    assert!(
        stdout1.contains("Found 20 issues"),
        "First run: expected 20 issues, got:\n{stdout1}"
    );
    assert!(
        stdout2.contains("Found 20 issues"),
        "Second run: expected 20 issues, got:\n{stdout2}"
    );

    // Every file must appear in the output of both runs.
    for i in 0..20 {
        let name = format!("file_{i:02}.md");
        assert!(stdout1.contains(&name), "First run missing {name}:\n{stdout1}");
        assert!(stdout2.contains(&name), "Second run missing {name}:\n{stdout2}");
    }
}

#[test]
fn test_per_directory_config_selects_nearest_ancestor() {
    let dir = tempdir().unwrap();

    fs::write(dir.path().join(".rumdl.toml"), "[global]\ndisable = [\"MD009\"]\n").unwrap();

    let strict = dir.path().join("subdir").join("strict");
    fs::create_dir_all(&strict).unwrap();
    fs::write(strict.join(".rumdl.toml"), "[global]\nenable = [\"MD009\"]\n").unwrap();

    fs::write(dir.path().join("root_file.md"), "# Root\n\nTrailing spaces here   \n").unwrap();
    fs::write(strict.join("strict_file.md"), "# Strict\n\nTrailing spaces here   \n").unwrap();

    let output = rumdl()
        .args(["check", "."])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("strict_file.md"),
        "Expected strict_file.md to appear in violations:\n{combined}"
    );

    assert!(
        !combined.contains("root_file.md"),
        "root_file.md should not have violations (MD009 disabled by root config):\n{combined}"
    );
}

#[test]
fn test_stdin_input_is_linted() {
    let mut child = rumdl()
        .args(["check", "--stdin", "--enable", "MD009", "--stdin-filename", "stdin.md"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn rumdl");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"# Test\n\nTrailing spaces   \n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !output.status.success(),
        "Expected non-zero exit for stdin with violations:\n{combined}"
    );
    assert!(combined.contains("MD009"), "Expected MD009 in output:\n{combined}");
}
