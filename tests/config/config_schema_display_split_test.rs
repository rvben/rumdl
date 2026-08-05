//! `Rule::default_config_section()` (what a user is shown) and `Rule::config_schema()`
//! (what the validator accepts) answer different questions, and for an `Option` field
//! defaulting to `None` they answer them oppositely: the listing omits the key, the
//! schema keeps it as a sentinel. These tests pin both halves of that split.
//!
//! Fusing them back together breaks in one of two directions, and each has a test
//! here: the validator rejecting a key the rule honors (a working setting reported as
//! "Unknown option"), or a sentinel reaching output (`rumdl config --defaults`
//! emitting a config its own binary cannot load).

use std::fs;
use std::process::Command;

use rumdl_lib::config::{RuleRegistry, SourcedConfig, validate_config_sourced};
use rumdl_lib::rule::Rule;
use tempfile::tempdir;

fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

fn all_rules() -> Vec<Box<dyn Rule>> {
    rumdl_lib::rules::all_rules(&rumdl_lib::config::Config::default())
}

/// A sentinel is a NUL-prefixed marker, which is what makes it unwritable as TOML and
/// therefore recognizable wherever it does not belong. Matched structurally rather
/// than by name so a sentinel added later is covered without editing this test.
fn sentinel_keys(value: &toml::Value) -> Vec<String> {
    let toml::Value::Table(table) = value else {
        return Vec::new();
    };
    table
        .iter()
        .filter(|(_, v)| matches!(v, toml::Value::String(s) if s.contains('\0')))
        .map(|(k, _)| k.clone())
        .collect()
}

#[test]
fn no_rule_shows_a_sentinel_in_its_user_facing_defaults() {
    let rules = all_rules();

    // Control: the probe must be able to see a sentinel, or "none found" means
    // nothing. The schema half is where they legitimately live.
    let schemas_with_sentinels: Vec<String> = rules
        .iter()
        .filter_map(|rule| rule.config_schema())
        .filter(|(_, value)| !sentinel_keys(value).is_empty())
        .map(|(name, _)| name)
        .collect();
    assert!(
        !schemas_with_sentinels.is_empty(),
        "control failed: no rule schema carries a sentinel, so this probe proves nothing"
    );

    let leaks: Vec<String> = rules
        .iter()
        .filter_map(|rule| rule.default_config_section())
        .flat_map(|(name, value)| {
            sentinel_keys(&value)
                .into_iter()
                .map(move |key| format!("{name}.{key}"))
        })
        .collect();
    assert!(
        leaks.is_empty(),
        "default_config_section() is user-facing output and must never carry a sentinel \
         (it is not writable as TOML): {leaks:?}"
    );
}

#[test]
fn every_key_a_rule_publishes_as_a_default_is_accepted_by_the_validator() {
    // The direction PR #794 reported, seen from the display side: a key shown in
    // `rumdl config` that the validator does not know is a key the user will copy
    // and then be warned about.
    let rules = all_rules();
    let registry = RuleRegistry::from_rules(&rules);

    let mut checked = 0;
    let mut unaccepted = Vec::new();
    for rule in &rules {
        let Some((name, toml::Value::Table(defaults))) = rule.default_config_section() else {
            continue;
        };
        for key in defaults.keys() {
            checked += 1;
            if registry.canonical_config_key(&name, key).is_none() {
                unaccepted.push(format!("{name}.{key}"));
            }
        }
    }

    assert!(checked > 100, "control failed: only {checked} default keys probed");
    assert!(
        unaccepted.is_empty(),
        "these keys are shown as defaults but rejected by the validator: {unaccepted:?}"
    );
}

/// Set every key of every rule's schema in one config, then run the production
/// load-and-validate path over it.
fn validation_warnings_for(config_body: &str) -> Vec<String> {
    let dir = tempdir().unwrap();
    let path = dir.path().join("rumdl.toml");
    fs::write(&path, config_body).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(path.to_str().unwrap()), None, true).unwrap();
    let rules = all_rules();
    let registry = RuleRegistry::from_rules(&rules);
    validate_config_sourced(&sourced, &registry)
        .into_iter()
        .map(|w| w.message)
        .collect()
}

/// Build a config setting every schema key, using the schema's own value so the type
/// is right by construction. A sentinel carries no type, so those keys get an
/// arbitrary value: the validator skips the type check for them, and it is the key
/// name this test is about.
fn config_with_every_schema_key() -> (String, usize) {
    let mut document = toml::map::Map::new();
    let mut keys = 0;
    for rule in all_rules() {
        let Some((name, toml::Value::Table(schema))) = rule.config_schema() else {
            continue;
        };
        let mut section = toml::map::Map::new();
        for (key, value) in &schema {
            let value = match value {
                toml::Value::String(s) if s.contains('\0') => toml::Value::String("rumdl-probe".to_string()),
                other => other.clone(),
            };
            section.insert(key.clone(), value);
            keys += 1;
        }
        if section.is_empty() {
            continue;
        }
        document.insert(name, toml::Value::Table(section));
    }
    (toml::to_string(&toml::Value::Table(document)).unwrap(), keys)
}

#[test]
fn every_key_a_rule_declares_in_its_schema_is_accepted_by_the_validator() {
    let (body, keys) = config_with_every_schema_key();
    assert!(keys > 100, "control failed: only {keys} schema keys probed");

    let unknown: Vec<String> = validation_warnings_for(&body)
        .into_iter()
        .filter(|m| m.contains("Unknown option"))
        .collect();
    assert!(
        unknown.is_empty(),
        "a key the rule deserializes must not be reported as unknown: {unknown:?}"
    );

    // Control: the same path must still catch a key no rule declares, so the clean
    // result above is not the harness failing to look.
    assert!(
        validation_warnings_for("[MD013]\nline-lengthh = 80\n")
            .iter()
            .any(|m| m.contains("Unknown option")),
        "control failed: a misspelled key went unreported"
    );
}

#[test]
fn the_published_defaults_load_back_without_warnings() {
    // The end-to-end shape of the same class: `rumdl config --defaults` is documented
    // as a starting point to copy, so what it prints must be a config rumdl accepts.
    // A sentinel here made five rules fall back to their defaults with an "Invalid
    // configuration" warning apiece.
    let defaults = Command::new(rumdl_bin())
        .args(["config", "--defaults", "--output", "toml"])
        .output()
        .unwrap();
    assert!(defaults.status.success(), "`rumdl config --defaults` failed");
    let defaults = String::from_utf8(defaults.stdout).unwrap();
    assert!(
        !defaults.contains('\0') && !defaults.contains("\\u0000"),
        "the published defaults contain a sentinel"
    );

    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), &defaults).unwrap();
    fs::write(dir.path().join("test.md"), "# Title\n\nBody text.\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    for complaint in ["Invalid configuration", "Unknown option", "Unknown global option"] {
        assert!(
            !stderr.contains(complaint),
            "rumdl rejects its own published defaults with a `{complaint}` warning:\n{stderr}"
        );
    }
}
