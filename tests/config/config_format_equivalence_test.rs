//! The documented promise that every config location is "equivalent in capability"
//! (`docs/configuration/index.md`), asserted rather than described.
//!
//! `.rumdl.toml` and `pyproject.toml` are parsed by two separate functions
//! (`parse_rumdl_toml` and `parse_pyproject_toml`), each hand-enumerating the
//! sections it knows. Nothing in the type system makes those enumerations agree, so
//! a section taught to one parser and not the other is silently absent from the
//! other format. That is what happened to `[code-block-tools]` (issue #851).
//!
//! These tests are written to be generic over the config vocabulary: the corpus is
//! a set of configurations, and the property is that expressing one in either
//! format produces identical behavior. A section added to `Config` fails
//! `test_corpus_covers_every_config_section` until the corpus exercises it, so the
//! coverage list cannot silently fall behind the way the parsers did.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

/// A markdown document that gives several rules something to say, so a config
/// difference between the two formats shows up as an output difference.
const PROBE_MD: &str = "# Title\n\n\
    This single line is written well past eighty characters on purpose so that MD013 has something it can report about it.\n\n\
    ```python\n\
    x=1\n\
    ```\n\n\
    * item\n\
    * item\n";

/// Which of the two pyproject spellings of global options a conversion produces.
///
/// `pyproject.toml` accepts global options both directly under `[tool.rumdl]` and
/// under a `[tool.rumdl.global]` sub-table, and `.rumdl.toml` mirrors that with
/// bare top-level keys and a `[global]` section. Both spellings are documented, so
/// both have to be equivalent, and they are parsed by different code:
/// `[tool.rumdl.global]` goes through one `extract_global_config` call and the flat
/// table through another, with the rule-section loop reading the flat table as
/// well. A defect can therefore live in one spelling and not the other.
#[derive(Clone, Copy, PartialEq)]
enum GlobalStyle {
    /// `[global]` -> `[tool.rumdl]`, bare leading keys -> `[tool.rumdl]`.
    Flat,
    /// `[global]` -> `[tool.rumdl.global]`.
    Nested,
}

/// Rewrites a `.rumdl.toml` body into its `pyproject.toml` spelling.
///
/// Every section other than `[global]` is nested under `[tool.rumdl.]`; `[global]`
/// and any bare keys written before the first section header follow `style`.
///
/// A body may use bare leading keys or a `[global]` section, not both: the two
/// would convert to the same header and TOML forbids defining a table twice. The
/// panic keeps that limitation loud rather than producing a config file that fails
/// to parse for an unrelated reason.
fn to_pyproject(body: &str, style: GlobalStyle) -> String {
    let mut has_leading_bare_key = false;
    let mut seen_section = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            seen_section = true;
        } else if !seen_section && !trimmed.is_empty() && !trimmed.starts_with('#') {
            has_leading_bare_key = true;
        }
    }
    assert!(
        !(has_leading_bare_key && body.contains("[global]")),
        "corpus entry mixes bare leading keys with a [global] section; both map to \
         the same pyproject header and TOML forbids defining that table twice"
    );

    let global_header = match style {
        GlobalStyle::Flat => "[tool.rumdl]",
        GlobalStyle::Nested => "[tool.rumdl.global]",
    };

    let mut out = String::with_capacity(body.len() + 32);
    if has_leading_bare_key {
        out.push_str(global_header);
        out.push('\n');
    }
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = &trimmed[1..trimmed.len() - 1];
            if section == "global" {
                out.push_str(global_header);
            } else {
                out.push_str("[tool.rumdl.");
                out.push_str(section);
                out.push(']');
            }
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// Removes everything that legitimately differs between the two runs: the name of
/// the config file, ANSI styling, and the elapsed-time figure.
fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // A CSI sequence runs to its first alphabetic byte.
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    let out = out
        .replace("pyproject.toml", "<config>")
        .replace(".rumdl.toml", "<config>");

    // Strip the "(12ms)" timing that every check run prints.
    let mut result = String::with_capacity(out.len());
    let mut rest = out.as_str();
    while let Some(open) = rest.find('(') {
        let (before, after_open) = rest.split_at(open);
        let inner = &after_open[1..];
        match inner.find(')') {
            Some(close)
                if !inner[..close].is_empty()
                    && inner[..close].ends_with("ms")
                    && inner[..close]
                        .trim_end_matches("ms")
                        .chars()
                        .all(|c| c.is_ascii_digit()) =>
            {
                result.push_str(before);
                result.push_str("(<time>)");
                rest = &inner[close + 1..];
            }
            _ => {
                result.push_str(before);
                result.push('(');
                rest = inner;
            }
        }
    }
    result.push_str(rest);
    result
}

fn run(args: &[&str], dir: &Path) -> (String, String, Option<i32>) {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(args)
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run rumdl");
    (
        normalize(&String::from_utf8_lossy(&output.stdout)),
        normalize(&String::from_utf8_lossy(&output.stderr)),
        output.status.code(),
    )
}

/// One configuration, plus any sibling files it references.
struct Case {
    name: &'static str,
    /// The configuration in `.rumdl.toml` spelling.
    body: &'static str,
    /// Extra files written beside the config, as (filename, contents).
    extra_files: &'static [(&'static str, &'static str)],
    /// Top-level `Config` sections this case exercises. Checked against `Config`'s
    /// own serialized shape by `test_corpus_covers_every_config_section`.
    covers: &'static [&'static str],
}

/// Every case is written in `.rumdl.toml` spelling and converted for the pyproject
/// run, so the two files always express the same configuration.
///
/// Cases deliberately use values that are diagnosable without any external program
/// on PATH: an unknown tool id in `[code-block-tools]` produces a validation
/// warning without the tool ever being executed, which makes the section's presence
/// observable on a machine that has no linters installed.
const CASES: &[Case] = &[
    Case {
        name: "global-bare-keys",
        body: "line-length = 40\ndisable = [\"MD012\"]\n",
        extra_files: &[],
        covers: &["global"],
    },
    Case {
        name: "global-section",
        body: "[global]\nline-length = 40\ndisable = [\"MD012\"]\nrespect-gitignore = false\n",
        extra_files: &[],
        covers: &["global"],
    },
    Case {
        name: "global-section-unknown-key",
        body: "[global]\ndisable = [\"MD012\"]\nnot-a-real-global-option = 1\n",
        extra_files: &[],
        covers: &["global"],
    },
    Case {
        name: "per-file-ignores",
        body: "[per-file-ignores]\n\"*.md\" = [\"MD013\", \"MD012\"]\n",
        extra_files: &[],
        covers: &["per-file-ignores"],
    },
    Case {
        name: "per-file-flavor",
        body: "[per-file-flavor]\n\"*.md\" = \"mkdocs\"\n",
        extra_files: &[],
        covers: &["per-file-flavor"],
    },
    Case {
        name: "code-block-tools",
        body: "[code-block-tools]\nenabled = true\ntimeout = 1234\n\n\
               [code-block-tools.languages.python]\nlint = [\"definitely-not-a-real-tool\"]\n",
        extra_files: &[],
        covers: &["code-block-tools"],
    },
    Case {
        name: "code-block-tools-only-section",
        body: "[code-block-tools]\nenabled = true\n\n\
               [code-block-tools.languages.python]\nlint = [\"definitely-not-a-real-tool\"]\n",
        extra_files: &[],
        covers: &["code-block-tools"],
    },
    Case {
        name: "rule-section",
        body: "[MD013]\nline-length = 40\n",
        extra_files: &[],
        covers: &["rules"],
    },
    Case {
        name: "rules-wrapper",
        body: "[rules.MD013]\nline-length = 40\n",
        extra_files: &[],
        covers: &["rules"],
    },
    Case {
        name: "extends",
        body: "extends = \"base.toml\"\n",
        extra_files: &[(
            "base.toml",
            "[global]\nline-length = 40\n\n[code-block-tools]\nenabled = true\n",
        )],
        covers: &["extends"],
    },
];

/// Writes one case in one format and returns what rumdl does with it.
fn observe(case: &Case, config_name: &str, config_body: &str) -> (String, String, Option<i32>, String, String) {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(config_name), config_body).unwrap();
    fs::write(dir.path().join("probe.md"), PROBE_MD).unwrap();
    for (name, contents) in case.extra_files {
        fs::write(dir.path().join(name), contents).unwrap();
    }
    let (check_out, check_err, code) = run(&["check", "--no-cache", "probe.md"], dir.path());
    let (config_out, config_err, _) = run(&["config"], dir.path());
    (check_out, check_err, code, config_out, config_err)
}

#[test]
fn test_rumdl_toml_and_pyproject_are_equivalent() {
    let mut failures = Vec::new();

    for case in CASES {
        let baseline = observe(case, ".rumdl.toml", case.body);

        // The nested spelling is only a faithful translation of an explicit
        // `[global]` section. Bare top-level keys in `.rumdl.toml` are the
        // shorthand for the flat `[tool.rumdl]` table, and some of them (`extends`)
        // are read only from there in both formats, so rewriting them into
        // `[tool.rumdl.global]` would assert an equivalence neither format claims.
        let mut styles = vec![("flat", GlobalStyle::Flat)];
        if case.body.contains("[global]") {
            styles.push(("nested", GlobalStyle::Nested));
        }

        for (style_name, style) in styles {
            let pyproject_body = to_pyproject(case.body, style);
            let observed = observe(case, "pyproject.toml", &pyproject_body);

            for (surface, a, b) in [
                ("check stdout", &baseline.0, &observed.0),
                ("check stderr", &baseline.1, &observed.1),
                ("config stdout", &baseline.3, &observed.3),
                ("config stderr", &baseline.4, &observed.4),
            ] {
                if a != b {
                    failures.push(format!(
                        "case '{}' ({style_name} global spelling) differs on {surface}\n  \
                         .rumdl.toml:\n{}\n  pyproject.toml:\n{}\n  (pyproject form was:\n{})",
                        case.name,
                        indent(a),
                        indent(b),
                        indent(&pyproject_body),
                    ));
                }
            }
            if baseline.2 != observed.2 {
                failures.push(format!(
                    "case '{}' ({style_name} global spelling) differs on exit code: \
                     .rumdl.toml {:?} vs pyproject.toml {:?}",
                    case.name, baseline.2, observed.2
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} configuration(s) behave differently in pyproject.toml than in .rumdl.toml:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| format!("    | {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The tripwire that keeps the corpus honest.
///
/// The section list is derived from `Config`'s own serialized shape rather than
/// written out here, so adding a field to `Config` fails this test until a case
/// exercises it. An enumeration that fails loudly when it falls behind is a
/// different thing from one that drifts silently, which is the defect this whole
/// file exists to prevent.
#[test]
fn test_corpus_covers_every_config_section() {
    let default = rumdl_lib::config::Config::default();
    let serialized = toml::Value::try_from(&default).expect("Config should serialize to TOML");
    let table = serialized.as_table().expect("Config serializes as a table");

    let mut expected: Vec<String> = table.keys().cloned().collect();
    // `extends` is `skip_serializing_if = "Option::is_none"` and rule sections are
    // `#[serde(flatten)]`, so neither shows up in a default serialization. Both are
    // real config surfaces and the corpus must still cover them.
    expected.push("extends".to_string());
    expected.push("rules".to_string());
    expected.sort();
    expected.dedup();

    let mut covered: Vec<String> = CASES
        .iter()
        .flat_map(|c| c.covers.iter().map(|s| s.to_string()))
        .collect();
    covered.sort();
    covered.dedup();

    let missing: Vec<&String> = expected.iter().filter(|k| !covered.contains(k)).collect();
    assert!(
        missing.is_empty(),
        "config section(s) {missing:?} are not exercised by the equivalence corpus. \
         Add a Case covering each, so the section is checked in both config formats. \
         (covered: {covered:?})"
    );

    let stale: Vec<&String> = covered.iter().filter(|k| !expected.contains(k)).collect();
    assert!(
        stale.is_empty(),
        "corpus claims to cover {stale:?}, which are not sections of Config (expected: {expected:?})"
    );
}
