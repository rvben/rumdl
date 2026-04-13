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
