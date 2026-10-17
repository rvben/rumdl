//! Table sorting through configuration, check, formatter and quick-fix edits.

use std::fs;
use std::process::{Command, Output};

fn run(dir: &std::path::Path, args: &[&str]) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(["--config", "rumdl.toml"])
        .args(args)
        .arg("--no-cache")
        .output()
        .expect("run rumdl");
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    assert!(
        !stderr.contains("unknown") && !stderr.contains("invalid configuration"),
        "{output:?}"
    );
    output
}

#[test]
fn md096_configured_sort_converges_with_table_formatting() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("rumdl.toml"), "[global]\nenable = ['MD060', 'table-sort']\n[MD060]\nstyle = 'aligned'\n[MD096]\ncolumn = 'Name'\norder = 'ASC'\n").unwrap();
    let path = dir.path().join("test.md");
    fs::write(
        &path,
        "|Name|Value|\n|---|---|\n|[Zulu](https://a.test)|2|\n|Alpha|1|\n",
    )
    .unwrap();
    let check = run(dir.path(), &["check", "--output-format", "json", "test.md"]);
    assert_eq!(check.status.code(), Some(1), "{check:?}");
    let warnings: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert!(
        warnings
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["rule"] == "MD096" && w["fixable"] == true)
    );
    let fix = run(dir.path(), &["check", "--fix", "test.md"]);
    assert_eq!(fix.status.code(), Some(0), "{fix:?}");
    let once = fs::read_to_string(&path).unwrap();
    assert!(once.find("| Alpha").unwrap() < once.find("| [Zulu]").unwrap(), "{once}");
    let check = run(dir.path(), &["check", "test.md"]);
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    let format = run(dir.path(), &["fmt", "test.md"]);
    assert_eq!(format.status.code(), Some(0), "{format:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), once);
}

#[test]
fn md096_enabled_section_sorts_descending_and_without_column_preserves_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.md");
    let source = "| Name |\n| --- |\n| Alpha |\n| Zulu |\n";
    fs::write(&path, source).unwrap();
    fs::write(
        dir.path().join("rumdl.toml"),
        "[global]\nenable = ['MD096']\n[MD096]\nenabled = true\norder = 'DESC'\n",
    )
    .unwrap();
    let check = run(dir.path(), &["check", "test.md"]);
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    fs::write(
        dir.path().join("rumdl.toml"),
        "[global]\nenable = ['MD096']\n[MD096]\nenabled = true\ncolumn = 'Name'\norder = 'DESC'\n",
    )
    .unwrap();
    let fmt = run(dir.path(), &["fmt", "test.md"]);
    assert_eq!(fmt.status.code(), Some(0), "{fmt:?}");
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "| Name |\n| --- |\n| Zulu |\n| Alpha |\n"
    );
}

#[test]
fn md096_declared_column_is_still_opt_in() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("rumdl.toml"), "[MD096]\ncolumn = 'Name'\n").unwrap();
    let source = "# Tools\n\n| Name |\n| --- |\n| Zulu |\n| Alpha |\n";
    let path = dir.path().join("test.md");
    fs::write(&path, source).unwrap();
    let output = run(dir.path(), &["check", "--output-format", "json", "test.md"]);
    let warnings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(warnings.as_array().unwrap().iter().all(|w| w["rule"] != "MD096"));
    let fmt = run(dir.path(), &["fmt", "test.md"]);
    assert_eq!(fmt.status.code(), Some(0), "{fmt:?}");
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
fn md096_converges_after_orphaned_rows_are_rejoined() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("rumdl.toml"),
        "[global]\nenable = ['MD060', 'MD075', 'MD096']\n[MD060]\nstyle = 'aligned'\n[MD096]\ncolumn = 'Name'\n",
    )
    .unwrap();
    let path = dir.path().join("test.md");
    fs::write(
        &path,
        "| Name | Value |\n| --- | --- |\n| Zulu | 1 |\n\n| Alpha | 2 |\n| Beta | 3 |\n",
    )
    .unwrap();
    let fix = run(dir.path(), &["check", "--fix", "test.md"]);
    assert_eq!(fix.status.code(), Some(0), "{fix:?}");
    let once = fs::read_to_string(&path).unwrap();
    assert!(once.find("Alpha").unwrap() < once.find("Beta").unwrap());
    assert!(once.find("Beta").unwrap() < once.find("Zulu").unwrap());
    let check = run(dir.path(), &["check", "test.md"]);
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    let fmt = run(dir.path(), &["fmt", "test.md"]);
    assert_eq!(fmt.status.code(), Some(0), "{fmt:?}");
    assert_eq!(fs::read_to_string(path).unwrap(), once);
}

#[test]
fn md096_enabled_flags_and_section_alias_override_global_selection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.md");
    let source = "| Name |\n| --- |\n| Zulu |\n| Alpha |\n";
    fs::write(&path, source).unwrap();
    // The section alone enables the opt-in rule, using its documented alias.
    fs::write(
        dir.path().join("rumdl.toml"),
        "[table-sort]\nenabled = true\ncolumn = 'Name'\n",
    )
    .unwrap();
    let check = run(dir.path(), &["check", "--output-format", "json", "test.md"]);
    assert_eq!(check.status.code(), Some(1), "{check:?}");
    let warnings: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert!(warnings.as_array().unwrap().iter().any(|w| w["rule"] == "MD096"));
    // A rule-specific disable takes precedence over explicit global enabling.
    fs::write(
        dir.path().join("rumdl.toml"),
        "[global]\nenable = ['MD096']\n[MD096]\nenabled = false\ncolumn = 'Name'\n",
    )
    .unwrap();
    let fix = run(dir.path(), &["check", "--fix", "test.md"]);
    assert_eq!(fix.status.code(), Some(0), "{fix:?}");
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}
