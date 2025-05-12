use std::process::Command;
use std::fs;

/// Helper to run a CLI tool and capture stdout as String
fn run_cli(cmd: &str, args: &[&str], cwd: &str) -> String {
    let output = Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("Failed to run CLI");
    if !output.status.success() {
        eprintln!("{} failed: {}", cmd, String::from_utf8_lossy(&output.stderr));
    }
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Normalize output: sort lines, trim whitespace, remove empty lines
fn normalize_output(s: &str) -> Vec<String> {
    let mut lines: Vec<String> = s
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    lines.sort();
    lines
}

fn filter_md030(lines: Vec<String>) -> Vec<String> {
    lines.into_iter().filter(|l| l.contains("MD030")).collect()
}

fn extract_file_line_rule(line: &str) -> Option<String> {
    // For rumdl: "tests/parity/md030_parity.md:84:1: [MD030] ..."
    // For markdownlint: "tests/parity/md030_parity.md:84:1 MD030/list-marker-space ..."
    let parts: Vec<_> = line.split_whitespace().collect();
    if parts.is_empty() { return None; }
    let file_line = parts[0];
    let rule = parts.iter().find(|s| s.contains("MD030")).cloned().unwrap_or("");
    if !file_line.is_empty() && !rule.is_empty() {
        Some(format!("{file_line} {rule}"))
    } else {
        None
    }
}

fn normalize_and_extract(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<_> = lines.into_iter()
        .filter_map(|l| extract_file_line_rule(&l))
        .collect();
    out.sort();
    out
}

#[test]
fn md030_parity_with_markdownlint() {
    // Path to the test file
    let test_file = "tests/parity/md030_parity.md";
    let cwd = ".";

    // Run rumdl (all rules enabled)
    let rumdl_output = run_cli(
        "target/debug/rumdl",
        &["check", test_file],
        cwd,
    );

    // Run markdownlint (all rules enabled)
    let markdownlint_output = run_cli(
        "markdownlint",
        &[test_file],
        cwd,
    );

    println!("--- FULL markdownlint output ---\n{}", markdownlint_output);

    let rumdl_lines = normalize_and_extract(filter_md030(normalize_output(&rumdl_output)));
    let markdownlint_lines = normalize_and_extract(filter_md030(normalize_output(&markdownlint_output)));

    if rumdl_lines != markdownlint_lines {
        println!("\n--- rumdl output ---\n{}\n--- markdownlint output ---\n{}",
            rumdl_lines.join("\n"),
            markdownlint_lines.join("\n")
        );
        panic!("MD030 parity test failed: outputs differ");
    }
}