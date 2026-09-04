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

/// A section header with nothing under it does not distinguish "empty" from
/// "the printer had nothing to say", so `rumdl config` states the emptiness.
///
/// With no configuration file at all, every section is empty, which makes this
/// the case where a bare header would be most misleading.
#[test]
fn test_config_output_states_that_an_empty_section_is_empty() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let output = run_config(dir.path(), &[]);

    let lines: Vec<&str> = output.lines().map(|line| line.trim_end()).collect();
    let bare: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            line.starts_with('[')
                && lines
                    .get(index + 1)
                    .is_none_or(|next| next.is_empty() || next.starts_with('['))
        })
        .map(|(_, line)| *line)
        .collect();

    assert!(
        bare.is_empty(),
        "`rumdl config` prints these section headers with nothing under them: {bare:?}\n\
         Render an empty section's emptiness, the way an empty list prints as `enable = []`.\n\n\
         --- output ---\n{output}"
    );
}

/// The line length MD013 enforces in `dir`, read back from the rule's own
/// message.
///
/// Read from a lint run rather than from the configuration, because the point of
/// the comparison below is that the two are separate computations: a global
/// `line-length` reaches MD013 through the rule's construction, not by being
/// copied into the rule's section.
fn enforced_md013_limit(dir: &std::path::Path) -> usize {
    let probe = dir.join("probe.md");
    std::fs::write(&probe, format!("{}word\n", "word ".repeat(100))).expect("failed to write probe");
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "probe.md"])
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .env("RUMDL_CACHE_DIR", dir.join(".cache"))
        .output()
        .expect("failed to run rumdl check");
    let text = strip_ansi(&String::from_utf8_lossy(&output.stdout));
    text.split_once("exceeds ")
        .and_then(|(_, tail)| tail.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("a 500-character line must exceed any limit, but MD013 said nothing:\n{text}"))
}

/// `rumdl config` has to report the limit MD013 enforces, not the option's
/// static default.
///
/// `line-length` is spelled twice, once in `[global]` and once as an MD013
/// option, and MD013 measures against the global when it sets no option of its
/// own. Nothing structural ties the reporting path to the rule, so the two are
/// compared here directly: the number `rumdl config get` prints against the
/// number the rule puts in its own message.
///
/// Found by review after `[tool.rumdl]` keys stopped being copied into MD013's
/// section at parse time: the lint went on enforcing the global's value while
/// `rumdl config get MD013.line-length` answered 80.
#[test]
fn test_config_reports_the_line_length_md013_enforces() {
    // The rows differ in which of the two spellings wins, so a reporting path
    // that always answers one of them fails on the other.
    //
    // The expected value is stated as well as compared, so a matrix that stopped
    // discriminating - both sides answering 80 everywhere - fails rather than
    // agreeing with itself.
    let cases: &[(&str, &str, &str, usize)] = &[
        (
            "a global in .rumdl.toml",
            ".rumdl.toml",
            "[global]\nline-length = 101\n",
            101,
        ),
        (
            "a flat [tool.rumdl]",
            "pyproject.toml",
            "[tool.rumdl]\nline-length = 102\n",
            102,
        ),
        (
            "a nested [tool.rumdl.global]",
            "pyproject.toml",
            "[tool.rumdl.global]\nline-length = 103\n",
            103,
        ),
        ("MD013's own option", ".rumdl.toml", "[MD013]\nline-length = 104\n", 104),
        (
            "MD013's own option beside a global",
            ".rumdl.toml",
            "[global]\nline-length = 105\n\n[MD013]\nline-length = 106\n",
            106,
        ),
        // MD013's option holding the option's own default is indistinguishable
        // from an unset one, so the global still wins: the reported value has to
        // follow the rule rather than the file.
        (
            "MD013's option set to the option's default",
            ".rumdl.toml",
            "[global]\nline-length = 107\n\n[MD013]\nline-length = 80\n",
            107,
        ),
        ("neither", ".rumdl.toml", "[global]\ndisable = []\n", 80),
    ];

    for (label, config_name, body, expected) in cases {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        std::fs::write(dir.path().join(config_name), body).expect("failed to write config");

        let enforced = enforced_md013_limit(dir.path());
        assert_eq!(
            enforced, *expected,
            "with {label}, MD013 enforces {enforced} where this test expects {expected}. Either the \
             fixture no longer says what it means to say, or MD013's resolution of `line-length` \
             changed.\n--- config ({config_name}) ---\n{body}"
        );

        let reported = run_config(dir.path(), &["get", "MD013.line-length"]);
        let reported_value = reported
            .split_once('=')
            .and_then(|(_, tail)| tail.split_whitespace().next())
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or_else(|| panic!("could not read a number out of: {reported}"));

        assert_eq!(
            reported_value, enforced,
            "with {label}, `rumdl config get MD013.line-length` reports {reported_value} while MD013 \
             enforces {enforced}. Report the limit the rule uses, through \
             MD013Config::from_document_config.\n\
             --- config ({config_name}) ---\n{body}\n--- reported ---\n{reported}"
        );

        // The full listing is a second reporting path over the same question.
        let listing = run_config(dir.path(), &[]);
        let md013 = section_of(&listing, "MD013");
        let printed = md013
            .lines()
            .find(|line| {
                line.trim_start().starts_with("line-length =") || line.trim_start().starts_with("line_length =")
            })
            .and_then(|line| line.split_once('='))
            .and_then(|(_, tail)| tail.split_whitespace().next())
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or_else(|| panic!("`rumdl config` prints no line-length under [MD013]:\n{md013}"));

        assert_eq!(
            printed, enforced,
            "with {label}, `rumdl config` prints line-length = {printed} under [MD013] while MD013 \
             enforces {enforced}.\n--- config ({config_name}) ---\n{body}\n--- [MD013] ---\n{md013}"
        );
    }
}

/// The lines a named section owns, up to the next section header.
fn section_of(output: &str, name: &str) -> String {
    output
        .lines()
        .skip_while(|line| line.trim_end() != format!("[{name}]"))
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .collect::<Vec<_>>()
        .join("\n")
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
