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

/// rumdl config --no-defaults must show non-default flavor in lowercase quoted form
#[test]
fn test_flavor_display_lowercase_in_no_defaults_mode() {
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join(".rumdl.toml"), "[global]\nflavor = \"mkdocs\"\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["config", "--no-defaults"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("flavor = \"mkdocs\""),
        "`rumdl config --no-defaults` should show `flavor = \"mkdocs\"`, got:\n{stdout}"
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
        stderr.contains("MD999"),
        "Error message should include the unknown rule name, got:\n{stderr}"
    );
}

/// rumdl config get must accept lowercase rule names (normalize_key handles case)
#[test]
fn test_config_get_bare_rule_name_is_case_insensitive() {
    let temp_dir = tempdir().unwrap();

    let lower = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["config", "get", "md076", "--no-config"])
        .output()
        .unwrap();

    let upper = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["config", "get", "MD076", "--no-config"])
        .output()
        .unwrap();

    assert!(
        lower.status.success(),
        "`rumdl config get md076` (lowercase) should succeed, stderr:\n{}",
        String::from_utf8_lossy(&lower.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&lower.stdout),
        String::from_utf8_lossy(&upper.stdout),
        "`rumdl config get md076` and `rumdl config get MD076` must produce identical output"
    );
}

/// Fields in bare-rule output must be sorted alphabetically
#[test]
fn test_config_get_bare_rule_name_output_is_sorted() {
    let temp_dir = tempdir().unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(temp_dir.path())
        .args(["config", "get", "MD076", "--no-config"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // MD076 has two fields: allow-loose-continuation and style.
    // Alphabetically, allow-loose-continuation < style, so it must appear first.
    let pos_allow = stdout
        .find("allow-loose-continuation")
        .expect("Output should contain allow-loose-continuation");
    let pos_style = stdout.find("MD076.style").expect("Output should contain MD076.style");

    assert!(
        pos_allow < pos_style,
        "Fields must be sorted alphabetically: allow-loose-continuation ({pos_allow}) should precede style ({pos_style})"
    );
}
