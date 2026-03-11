use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_grouped_output_format() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");

    let content = format!(
        r#"# Heading
Content with trailing space{}
*Emphasis without space*
"#,
        "   "
    ); // Add trailing spaces programmatically

    fs::write(&test_file, content).unwrap();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--output-format")
        .arg("grouped")
        .arg(test_file.to_str().unwrap());

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("test.md:"))
        .stdout(predicate::str::contains("MD022:")) // heading blank line
        .stdout(predicate::str::contains("MD009:")); // trailing space
}

#[test]
fn test_pylint_output_format() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");

    let content = format!(
        r#"# Heading
Content with trailing space{}
"#,
        "   "
    ); // Add trailing spaces programmatically

    fs::write(&test_file, content).unwrap();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--output-format")
        .arg("pylint")
        .arg(test_file.to_str().unwrap());

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains(":1:1: [CMD022]"))
        .stdout(predicate::str::contains(":2:28: [CMD009]"));
}

#[test]
fn test_azure_output_format() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");

    let content = format!(
        r#"# Heading
Content with trailing space{}
"#,
        "   "
    ); // Add trailing spaces programmatically

    fs::write(&test_file, content).unwrap();

    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.arg("check")
        .arg("--output-format")
        .arg("azure")
        .arg(test_file.to_str().unwrap());

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("##vso[task.logissue type=warning;sourcepath="))
        .stdout(predicate::str::contains("code=MD009]"))
        .stdout(predicate::str::contains("code=MD022]"));
}

// Config tests are currently disabled because config loading happens after output format determination
// TODO: Fix the order of config loading to support output format in config files

// #[test]
// fn test_output_format_from_config() {
//     let temp_dir = tempdir().unwrap();
//     let test_file = temp_dir.path().join("test.md");
//     let config_file = temp_dir.path().join(".rumdl.toml");
//
//     let config_content = r#"[global]
// output-format = "pylint"
// "#;
//
//     let md_content = r#"# Heading
// Content with trailing space
// "#;
//
//     fs::write(&test_file, md_content).unwrap();
//     fs::write(&config_file, config_content).unwrap();
//
//     let mut cmd = cargo_bin_cmd!("rumdl");
//     cmd.current_dir(&temp_dir)
//         .arg("check")
//         .arg("test.md");
//
//     cmd.assert()
//         .failure()
//         .stdout(predicate::str::contains(":1:1: [CMD022]"))
//         .stdout(predicate::str::contains(":2:29: [CMD009]"));
// }

// #[test]
// fn test_output_format_cli_overrides_config() {
//     let temp_dir = tempdir().unwrap();
//     let test_file = temp_dir.path().join("test.md");
//     let config_file = temp_dir.path().join(".rumdl.toml");
//
//     let config_content = r#"[global]
// output-format = "pylint"
// "#;
//
//     let md_content = r#"# Heading
// Content with trailing space
// "#;
//
//     fs::write(&test_file, md_content).unwrap();
//     fs::write(&config_file, config_content).unwrap();
//
//     let mut cmd = cargo_bin_cmd!("rumdl");
//     cmd.current_dir(&temp_dir)
//         .arg("check")
//         .arg("--output-format")
//         .arg("azure")
//         .arg("test.md");
//
//     // Should use azure format from CLI, not pylint from config
//     cmd.assert()
//         .failure()
//         .stdout(predicate::str::contains("##vso[task.logissue type=warning;"));
// }

#[test]
fn test_rumdl_output_format_env_var() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");

    let content = format!(
        r#"# Heading
Content with trailing space{}
"#,
        "   "
    );

    fs::write(&test_file, content).unwrap();

    // Test that RUMDL_OUTPUT_FORMAT env var is respected
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.env("RUMDL_OUTPUT_FORMAT", "github")
        .arg("check")
        .arg(test_file.to_str().unwrap());

    // GitHub format uses ::warning:: annotations
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("::warning file="));
}

#[test]
fn test_rumdl_output_format_env_var_cli_override() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");

    let content = format!(
        r#"# Heading
Content with trailing space{}
"#,
        "   "
    );

    fs::write(&test_file, content).unwrap();

    // CLI flag should override env var
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.env("RUMDL_OUTPUT_FORMAT", "github")
        .arg("check")
        .arg("--output-format")
        .arg("azure")
        .arg(test_file.to_str().unwrap());

    // Should use azure format from CLI, not github from env var
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("##vso[task.logissue type=warning;"));
}

#[test]
fn test_rumdl_output_format_env_var_overrides_config() {
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.md");
    let config_file = temp_dir.path().join(".rumdl.toml");

    let config_content = r#"[global]
output-format = "pylint"
"#;

    let md_content = format!(
        r#"# Heading
Content with trailing space{}
"#,
        "   "
    );

    fs::write(&test_file, md_content).unwrap();
    fs::write(&config_file, config_content).unwrap();

    // Env var should override config file
    let mut cmd = cargo_bin_cmd!("rumdl");
    cmd.current_dir(&temp_dir)
        .env("RUMDL_OUTPUT_FORMAT", "azure")
        .arg("check")
        .arg("test.md");

    // Should use azure format from env var, not pylint from config
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("##vso[task.logissue type=warning;"));
}
