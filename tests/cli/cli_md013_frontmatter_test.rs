//! Issue #996: canonical and compatibility spellings across configuration layers.
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const KEYS: [&str; 3] = ["frontmatter", "front-matter", "front_matter"];
const YAML: &str =
    "---\ndescription: This frontmatter description is deliberately longer than forty characters.\n---\n";
const TOML: &str =
    "+++\ndescription = \"This frontmatter description is deliberately longer than forty characters.\"\n+++\n";

fn check(dir: &Path, config: &Path, cli_override: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command
        .current_dir(dir)
        .args([
            "check",
            "--no-cache",
            "--enable",
            "MD013",
            "--deny-config-warnings",
            "--output-format",
            "json",
            "--config",
        ])
        .arg(config);
    if let Some(setting) = cli_override {
        command.args(["--config", setting]);
    }
    command.arg("doc.md").output().unwrap()
}

fn assert_md013(output: Output, lines: &[u64]) {
    assert_eq!(output.status.code(), Some(i32::from(!lines.is_empty())), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let warnings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        warnings.iter().all(|warning| warning["rule"] == "MD013"),
        "{warnings:?}"
    );
    let actual: Vec<u64> = warnings
        .iter()
        .map(|warning| warning["line"].as_u64().unwrap())
        .collect();
    assert_eq!(actual, lines, "{warnings:?}");
}

#[test]
fn frontmatter_config_spellings_are_honored_without_warnings() {
    for file_key in KEYS {
        for enabled in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let config = dir.path().join("rumdl.toml");
            for input in [YAML, TOML] {
                fs::write(dir.path().join("doc.md"), input).unwrap();
                fs::write(&config, format!("[MD013]\nline-length = 40\n{file_key} = {enabled}\n")).unwrap();
                assert_md013(check(dir.path(), &config, None), if enabled { &[2] } else { &[] });
                // Seed the opposite value to prove each CLI spelling overrides
                // each file spelling rather than merely agreeing with it.
                fs::write(
                    &config,
                    format!("[MD013]\nline-length = 40\n{file_key} = {}\n", !enabled),
                )
                .unwrap();
                for cli_key in KEYS {
                    let setting = format!("MD013.{cli_key} = {enabled}");
                    assert_md013(
                        check(dir.path(), &config, Some(&setting)),
                        if enabled { &[2] } else { &[] },
                    );
                }
            }
        }
    }
}

#[test]
fn frontmatter_inline_spellings_override_file_configuration() {
    for file_key in KEYS {
        for inline_key in KEYS {
            for enabled in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let config = dir.path().join("rumdl.toml");
                fs::write(
                    &config,
                    format!("[MD013]\nline-length = 40\n{file_key} = {}\n", !enabled),
                )
                .unwrap();
                for input in [YAML, TOML] {
                    fs::write(
                        dir.path().join("doc.md"),
                        format!(
                            "{input}\n<!-- rumdl-configure-file {{\"MD013\": {{\"{inline_key}\": {enabled}}}}} -->\n"
                        ),
                    )
                    .unwrap();
                    assert_md013(check(dir.path(), &config, None), if enabled { &[2] } else { &[] });
                }
            }
        }
    }
}

#[test]
fn frontmatter_aliases_preserve_child_config_precedence() {
    for base_key in ["frontmatter", "front-matter", "front_matter", "Front-Matter"] {
        for child_key in ["frontmatter", "front-matter", "front_matter", "Front_Matter"] {
            for enabled in [false, true] {
                for pyproject in [false, true] {
                    let dir = tempfile::tempdir().unwrap();
                    fs::write(
                        dir.path().join("base.toml"),
                        format!("[MD013]\nline-length = 40\n{base_key} = {}\n", !enabled),
                    )
                    .unwrap();
                    let (name, prefix, section) = if pyproject {
                        ("pyproject.toml", "[tool.rumdl]\n", "tool.rumdl.MD013")
                    } else {
                        ("rumdl.toml", "", "MD013")
                    };
                    let config = dir.path().join(name);
                    fs::write(
                        &config,
                        format!("{prefix}extends = \"base.toml\"\n[{section}]\n{child_key} = {enabled}\n"),
                    )
                    .unwrap();
                    // The body also overflows the inherited 40-column limit,
                    // so fallback to the default 80 cannot pass unnoticed.
                    fs::write(
                        dir.path().join("doc.md"),
                        format!("{YAML}\nThis paragraph line is deliberately longer than forty characters.\n"),
                    )
                    .unwrap();
                    assert_md013(check(dir.path(), &config, None), if enabled { &[2, 5] } else { &[5] });
                }
            }
        }
    }
}

#[test]
fn frontmatter_config_get_resolves_aliases_and_preserves_provenance() {
    for file_key in KEYS {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("rumdl.toml");
        fs::write(&config, format!("[MD013]\n{file_key} = true\nenable-reflow = true\n")).unwrap();
        for query_key in [
            "frontmatter",
            "front-matter",
            "front_matter",
            "Front-Matter",
            "reflow",
            "enable-reflow",
            "enable_reflow",
        ] {
            let query = format!("MD013.{query_key}");
            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .current_dir(dir.path())
                .args(["config", "get", &query, "--config"])
                .arg(&config)
                .output()
                .unwrap();
            assert!(output.status.success(), "{query}: {output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
            let stdout = String::from_utf8(output.stdout).unwrap();
            assert!(stdout.contains("= true"), "{file_key} -> {query}: {stdout}");
            assert!(stdout.contains("[from project config]"), "{stdout}");
        }
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir.path())
            .args(["config", "get", "MD013", "--config"])
            .arg(&config)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains("MD013.frontmatter = true [from project config]"),
            "{stdout}"
        );
        assert!(stdout.contains("MD013.reflow = true [from project config]"), "{stdout}");
    }
}
