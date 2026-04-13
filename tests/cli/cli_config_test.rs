use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

/// rumdl config and rumdl config get must produce the same value for global.flavor
#[test]
fn test_flavor_display_is_consistent_between_config_and_config_get() {
    let temp_dir = tempdir().unwrap();

    let config_output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["config", "--no-config"])
        .output()
        .unwrap();
    let config_stdout = String::from_utf8_lossy(&config_output.stdout);

    let get_output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["config", "get", "global.flavor", "--no-config"])
        .output()
        .unwrap();
    let get_stdout = String::from_utf8_lossy(&get_output.stdout);

    assert!(
        config_stdout.contains("flavor = \"standard\""),
        "`rumdl config` output should contain `flavor = \"standard\"`, got:\n{config_stdout}"
    );
    assert!(
        get_stdout.contains("\"standard\""),
        "`rumdl config get global.flavor` should contain `\"standard\"`, got:\n{get_stdout}"
    );
}

/// Flavor value in rumdl config output must be lowercase regardless of which flavor is set
#[test]
fn test_flavor_display_lowercase_when_set_to_mkdocs() {
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join(".rumdl.toml"), "[global]\nflavor = \"mkdocs\"\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["config"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("flavor = \"mkdocs\""),
        "`rumdl config` with mkdocs flavor should show `flavor = \"mkdocs\"`, got:\n{stdout}"
    );
}

#[test]
fn test_config_get_bare_rule_name_returns_all_keys() {
    // rumdl config get MD076 must return all config keys for rule MD076
    let temp_dir = tempdir().unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["config", "get", "MD076", "--no-config"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "`rumdl config get MD076` should succeed, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("MD076.style"),
        "Output should contain MD076.style, got:\n{stdout}"
    );
    assert!(
        stdout.contains("MD076.allow-loose-continuation"),
        "Output should contain MD076.allow-loose-continuation, got:\n{stdout}"
    );
    assert!(
        stdout.contains("[from default]"),
        "Output should contain [from default] annotation, got:\n{stdout}"
    );
}

#[test]
fn test_config_get_bare_rule_name_shows_overridden_value() {
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join(".rumdl.toml"), "[MD076]\nstyle = \"sublist\"\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["config", "get", "MD076"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("MD076.style") && stdout.contains("\"sublist\""),
        "Should show overridden style value, got:\n{stdout}"
    );
    assert!(
        stdout.contains("[from project config]"),
        "Overridden value should show project config provenance, got:\n{stdout}"
    );
}

#[test]
fn test_config_get_unknown_bare_name_errors_gracefully() {
    let temp_dir = tempdir().unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["config", "get", "MD999", "--no-config"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "`rumdl config get MD999` should fail for unknown rule"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("MD999") || stderr.contains("Unknown"),
        "Error message should mention the unknown key, got:\n{stderr}"
    );
}
