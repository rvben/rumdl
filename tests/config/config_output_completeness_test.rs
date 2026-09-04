//! `rumdl config` must show every part of the effective configuration.
//!
//! The human-readable printers in `src/formatter.rs` are hand-written: each
//! section and each global key is an explicit block. Nothing ties them to the
//! `Config` struct, so a field added to the configuration is invisible in
//! `rumdl config` until someone remembers to add a printing block for it. The
//! machine-readable forms (`--output toml` / `--output json`) serialize `Config`
//! directly and so never drift.
//!
//! These tests derive what the output must contain from `Config::default()`
//! itself, so adding a configuration section or a global key fails here until
//! the printers render it.
//!
//! Discovered while investigating #851: `[code-block-tools]` drove findings
//! while `rumdl config` claimed no such section existed.

use std::collections::BTreeSet;
use std::process::Command;

/// A configuration that sets every section to a non-default value, so the
/// `--no-defaults` printer has something to say about each of them.
///
/// When a new section is added to `Config`, add a non-default value for it here
/// as well; the coverage assertion below names it.
const FULL_CONFIG: &str = r#"
[global]
disable = ["MD013"]
line-length = 123
force-exclude = true
cache = false
editorconfig = true
fixable = ["MD009"]
unfixable = ["MD010"]
enable = ["MD001"]
exclude = ["vendor"]
include = ["docs"]
respect-gitignore = false
flavor = "mkdocs"
extend-enable = ["MD046"]
extend-disable = ["MD012"]

[per-file-ignores]
"CHANGELOG.md" = ["MD024"]

[per-file-flavor]
"docs/**/*.md" = "mkdocs"

[code-block-tools]
enabled = true
timeout = 4321

[code-block-tools.languages.python]
lint = ["ruff:check"]

[MD007]
indent = 3
"#;

fn run_config(dir: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("config")
        .args(args)
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .env("RUMDL_CACHE_DIR", dir.join(".cache"))
        .output()
        .expect("failed to run rumdl config");
    assert!(
        output.status.success(),
        "rumdl config {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    strip_ansi(&String::from_utf8_lossy(&output.stdout))
}

/// Provenance labels are dimmed even under `NO_COLOR`, so the escapes have to go
/// before the text can be matched.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn write_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join(".rumdl.toml"), FULL_CONFIG).expect("failed to write config");
    dir
}

/// Serialize `Config::default()` and read back the sections it declares.
///
/// `extends` is `Option<String>` and skipped when unset, `rules` is flattened,
/// so what remains are exactly the named configuration sections.
fn declared_sections() -> BTreeSet<String> {
    let value = toml::Value::try_from(rumdl_lib::config::Config::default()).expect("Config must serialize to TOML");
    let table = value.as_table().expect("Config serializes to a table");
    table
        .iter()
        .filter(|(_, v)| v.is_table())
        .map(|(k, _)| k.to_string())
        .collect()
}

fn declared_global_keys() -> BTreeSet<String> {
    let value = toml::Value::try_from(rumdl_lib::config::Config::default()).expect("Config must serialize to TOML");
    value
        .get("global")
        .and_then(|g| g.as_table())
        .expect("Config declares a [global] section")
        .keys()
        .map(|k| k.to_string())
        .collect()
}

/// The lines the `[global]` section owns, up to the next section header.
///
/// Global keys have to be looked for here rather than anywhere in the output:
/// several rules carry an option of the same name (MD013 has `line-length`), so
/// a whole-output search reports a global key as present that is not.
fn global_section(output: &str) -> String {
    output
        .lines()
        .skip_while(|line| line.trim_end() != "[global]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The printers spell keys with underscores while configs are written with
/// hyphens; both spellings are accepted on input, so either satisfies this.
fn mentions_key(section: &str, key: &str) -> bool {
    section.contains(&format!("{key} =")) || section.contains(&format!("{} =", key.replace('-', "_")))
}

#[test]
fn test_config_output_shows_every_declared_section() {
    let dir = write_fixture();

    for (label, args) in [
        ("rumdl config", &[][..]),
        ("rumdl config --no-defaults", &["--no-defaults"]),
    ] {
        let output = run_config(dir.path(), args);
        let missing: Vec<String> = declared_sections()
            .into_iter()
            .filter(|section| !output.contains(&format!("[{section}]")))
            .collect();
        assert!(
            missing.is_empty(),
            "`{label}` does not render these configuration sections: {missing:?}\n\
             Every section of `Config` has to be printed, or the effective configuration is\n\
             reported incompletely. Add a block to the matching printer in src/formatter.rs\n\
             (and a non-default value to FULL_CONFIG in this test).\n\n\
             --- output ---\n{output}"
        );
    }
}

#[test]
fn test_config_output_shows_every_global_key() {
    let dir = write_fixture();

    for (label, args) in [
        ("rumdl config", &[][..]),
        ("rumdl config --no-defaults", &["--no-defaults"]),
    ] {
        let output = run_config(dir.path(), args);
        let section = global_section(&output);
        let missing: Vec<String> = declared_global_keys()
            .into_iter()
            .filter(|key| !mentions_key(&section, key))
            .collect();
        assert!(
            missing.is_empty(),
            "`{label}` does not render these [global] keys: {missing:?}\n\
             Add them to the matching printer in src/formatter.rs.\n\n\
             --- output ---\n{output}"
        );
    }
}

/// The assertions above are only as good as the fixture: a section or key the
/// fixture leaves at its default cannot appear in `--no-defaults` output, so it
/// would fail for the wrong reason. This states the coverage separately, so the
/// failure names the fixture rather than the printer.
#[test]
fn test_fixture_sets_every_declared_section_and_global_key() {
    let fixture: toml::Value = toml::from_str(FULL_CONFIG).expect("FULL_CONFIG must be valid TOML");
    let fixture_table = fixture.as_table().expect("FULL_CONFIG is a table");

    let uncovered: Vec<String> = declared_sections()
        .into_iter()
        .filter(|section| !fixture_table.contains_key(section))
        .collect();
    assert!(
        uncovered.is_empty(),
        "FULL_CONFIG in this test leaves these sections at their defaults: {uncovered:?}\n\
         Set each to a non-default value, or the printer tests pass vacuously."
    );

    let fixture_global = fixture_table
        .get("global")
        .and_then(|g| g.as_table())
        .expect("FULL_CONFIG has a [global] section");
    let uncovered: Vec<String> = declared_global_keys()
        .into_iter()
        .filter(|key| !fixture_global.contains_key(key))
        .collect();
    assert!(
        uncovered.is_empty(),
        "FULL_CONFIG in this test leaves these [global] keys at their defaults: {uncovered:?}\n\
         Set each to a non-default value, or `--no-defaults` has nothing to print for them."
    );
}
