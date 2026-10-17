//! Automatic version-source selection through the CLI and native subprocesses.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

#[path = "support/native_tool.rs"]
mod native_tool;

const LOCK: &str = "version=1\n[[package]]\nname='ruff'\nversion='0.11.2'\n";
const YAML: &str =
    "repos:\n- repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: v0.11.2\n  hooks: [{id: ruff-check}]\n";
const TOML: &str =
    "[[repos]]\nrepo='https://github.com/astral-sh/ruff-pre-commit'\nrev='v0.11.2'\nhooks=[{id='ruff-check'}]\n";
const PYPROJECT: &str = "[project]\ndependencies=['ruff>=0.11,<0.12']\n";
const SOURCES: [(&str, &str); 5] = [
    ("uv.lock", LOCK),
    ("prek.toml", TOML),
    (".pre-commit-config.yaml", YAML),
    (".pre-commit-config.yml", YAML),
    ("pyproject.toml", PYPROJECT),
];

fn project(source: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        format!(
            "[code-block-tools]\nenabled=true\ntool-version-definition='{source}'\non-tool-version-mismatch='fail'\nbinary-preferences={{ruff='only-project'}}\n[code-block-tools.tools.test]\ncommand=['ruff']\nstdin=true\nstdout=true\n[code-block-tools.languages.python]\nlint=['test']\nformat=['test']\n"
        ),
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), "```python\nbad\n```\n").unwrap();
    let fixture = native_tool::install(dir.path(), "quiet");
    let tool = fixture.with_file_name(format!("ruff{}", std::env::consts::EXE_SUFFIX));
    fs::copy(fixture, &tool).unwrap();
    fs::write(tool.with_extension("mode"), "count-format").unwrap();
    fs::write(tool.with_extension("version"), "ruff 0.11.2\n").unwrap();
    dir
}

fn run(root: &Path, command: &str, files: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(root)
        .args([command, "--only-code-block-tools", "--no-cache"])
        .args(files)
        .output()
        .unwrap()
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn auto_prefers_a_resolved_lock_over_other_sources_in_the_same_directory() {
    let dir = project("auto");
    fs::write(dir.path().join("uv.lock"), LOCK).unwrap();
    for source in [
        "prek.toml",
        ".pre-commit-config.yaml",
        ".pre-commit-config.yml",
        "pyproject.toml",
    ] {
        fs::write(dir.path().join(source), "invalid = [").unwrap();
    }
    let output = run(dir.path(), "fmt", &["doc.md"]);
    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "```python\ngood\n```\n"
    );
}

#[test]
fn yml_spelling_is_supported_explicitly_and_automatically() {
    for source in [".pre-commit-config.yml", "auto"] {
        let dir = project(source);
        fs::write(dir.path().join(".pre-commit-config.yml"), YAML).unwrap();
        let output = run(dir.path(), "fmt", &["doc.md"]);
        assert!(output.status.success(), "{source}: {}", output_text(&output));
        assert_eq!(
            fs::read_to_string(dir.path().join("doc.md")).unwrap(),
            "```python\ngood\n```\n"
        );
    }
}

#[test]
fn auto_uses_each_source_in_priority_order_without_parsing_lower_priority_files() {
    for (index, &(selected, metadata)) in SOURCES.iter().enumerate() {
        let dir = project("auto");
        fs::write(dir.path().join(selected), metadata).unwrap();
        for &(other, _) in &SOURCES[index + 1..] {
            fs::write(dir.path().join(other), "invalid = [").unwrap();
        }
        let output = run(dir.path(), "check", &["doc.md"]);
        assert!(
            output.status.success() && dir.path().join("calls").exists(),
            "{selected}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn auto_never_falls_back_from_an_invalid_or_unresolved_selected_source() {
    for (index, &(selected, _)) in SOURCES.iter().enumerate() {
        for invalid in [
            "invalid = [",
            if selected == "uv.lock" {
                "version=1\n"
            } else if selected == "pyproject.toml" {
                "[tool.other]\n"
            } else if selected == "prek.toml" {
                "repos=[]\n"
            } else {
                "repos: []\n"
            },
        ] {
            let dir = project("auto");
            for &(other, metadata) in &SOURCES[index + 1..] {
                fs::write(dir.path().join(other), metadata).unwrap();
            }
            fs::write(dir.path().join(selected), invalid).unwrap();
            let original = fs::read(dir.path().join("doc.md")).unwrap();
            let output = run(dir.path(), "fmt", &["doc.md"]);
            let text = output_text(&output);
            assert!(
                !output.status.success() && text.contains("Cannot "),
                "{selected}: {text}"
            );
            assert!(
                text.contains(&format!("auto selected {selected}; other sources were not tried")),
                "{selected}: {text}"
            );
            assert!(!dir.path().join("calls").exists(), "{selected}: {text}");
            assert_eq!(fs::read(dir.path().join("doc.md")).unwrap(), original);
        }
    }
}

#[test]
fn nearer_metadata_beats_all_outer_sources_and_invalid_nearer_metadata_stops_discovery() {
    for &(selected, metadata) in &SOURCES {
        let dir = project("auto");
        let nested = dir.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::copy(dir.path().join("doc.md"), nested.join("doc.md")).unwrap();
        fs::write(dir.path().join("uv.lock"), LOCK.replace("0.11.2", "0.12.1")).unwrap();
        fs::write(nested.join(selected), metadata).unwrap();
        let output = run(dir.path(), "check", &["nested/doc.md"]);
        assert!(output.status.success(), "{selected}: {}", output_text(&output));
        fs::remove_file(dir.path().join("calls")).unwrap();
        fs::write(dir.path().join("uv.lock"), LOCK).unwrap();
        fs::write(nested.join(selected), "invalid = [").unwrap();
        let output = run(dir.path(), "check", &["nested/doc.md"]);
        let text = output_text(&output);
        assert!(
            !output.status.success() && text.contains(&format!("auto selected {selected}")),
            "{selected}: {text}"
        );
        assert!(!dir.path().join("calls").exists(), "outer source was used: {text}");
    }
}

#[test]
fn auto_caches_selected_metadata_per_directory_while_sharing_the_executable_probe() {
    let dir = project("auto");
    for folder in ["a", "b"] {
        fs::create_dir(dir.path().join(folder)).unwrap();
        fs::copy(dir.path().join("doc.md"), dir.path().join(folder).join("doc.md")).unwrap();
    }
    fs::write(dir.path().join("a/uv.lock"), LOCK).unwrap();
    fs::write(dir.path().join("b/prek.toml"), TOML.replace("v0.11.2", "v0.12.1")).unwrap();
    let output = run(dir.path(), "check", &["a/doc.md", "b/doc.md"]);
    let text = output_text(&output);
    assert!(
        !output.status.success() && text.contains("version mismatch") && text.contains("auto selected prek.toml"),
        "{text}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("calls")).unwrap().lines().count(),
        1,
        "{text}"
    );
    let tool = dir.path().join(if cfg!(windows) {
        ".venv/Scripts/ruff.exe"
    } else {
        ".venv/bin/ruff"
    });
    assert_eq!(
        fs::read_to_string(tool.with_extension("version-calls"))
            .unwrap()
            .lines()
            .count(),
        1,
        "{text}"
    );
}

#[test]
fn auto_preserves_policies_single_probe_and_single_warning_across_many_blocks() {
    for policy in ["warn", "ignore", "fail", "fail-fast"] {
        let dir = project("auto");
        let config = dir.path().join(".rumdl.toml");
        fs::write(
            &config,
            fs::read_to_string(&config).unwrap().replace(
                "on-tool-version-mismatch='fail'",
                &format!("on-tool-version-mismatch='{policy}'"),
            ),
        )
        .unwrap();
        fs::write(dir.path().join("uv.lock"), LOCK.replace("0.11.2", "0.12.1")).unwrap();
        fs::write(dir.path().join("prek.toml"), TOML).unwrap();
        fs::write(dir.path().join("doc.md"), "```python\nbad\n```\n\n".repeat(32)).unwrap();
        let original = fs::read(dir.path().join("doc.md")).unwrap();
        let output = run(dir.path(), "check", &["doc.md"]);
        let text = output_text(&output);
        let allowed = matches!(policy, "warn" | "ignore");
        assert_eq!(output.status.success(), allowed, "{policy}: {text}");
        let calls = dir.path().join("calls");
        assert_eq!(calls.exists(), allowed, "{policy}: {text}");
        if allowed {
            assert_eq!(
                fs::read_to_string(calls).unwrap().lines().count(),
                32,
                "{policy}: {text}"
            );
        }
        let tool = dir.path().join(if cfg!(windows) {
            ".venv/Scripts/ruff.exe"
        } else {
            ".venv/bin/ruff"
        });
        let probes = tool.with_extension("version-calls");
        assert_eq!(probes.exists(), policy != "ignore", "{policy}: {text}");
        if policy != "ignore" {
            assert_eq!(
                fs::read_to_string(probes).unwrap().lines().count(),
                1,
                "{policy}: {text}"
            );
            assert_eq!(
                text.matches("auto selected uv.lock; other sources were not tried")
                    .count(),
                if policy == "fail" { 32 } else { 1 },
                "{policy}: {text}"
            );
        } else {
            assert!(!text.contains("version mismatch"), "{text}");
        }
        assert_eq!(fs::read(dir.path().join("doc.md")).unwrap(), original);
    }
}

#[test]
fn auto_does_not_search_outside_the_project_or_skip_an_unreadable_selected_candidate() {
    let dir = project("auto");
    fs::write(dir.path().join("uv.lock"), LOCK).unwrap();
    let nested = dir.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fs::create_dir(nested.join(".git")).unwrap();
    fs::copy(dir.path().join(".rumdl.toml"), nested.join(".rumdl.toml")).unwrap();
    let fixture = native_tool::install(&nested, "quiet");
    let tool = fixture.with_file_name(format!("ruff{}", std::env::consts::EXE_SUFFIX));
    fs::copy(fixture, &tool).unwrap();
    fs::write(tool.with_extension("mode"), "count").unwrap();
    fs::write(tool.with_extension("version"), "ruff 0.11.2\n").unwrap();
    fs::copy(dir.path().join("doc.md"), nested.join("doc.md")).unwrap();
    let output = run(dir.path(), "check", &["nested/doc.md"]);
    let text = output_text(&output);
    assert!(
        !output.status.success() && text.contains("no supported metadata file found within the project"),
        "{text}"
    );
    assert!(!dir.path().join("calls").exists(), "{text}");

    fs::remove_file(dir.path().join("uv.lock")).unwrap();
    fs::create_dir(dir.path().join("uv.lock")).unwrap();
    fs::write(dir.path().join("prek.toml"), TOML).unwrap();
    let output = run(dir.path(), "check", &["doc.md"]);
    let text = output_text(&output);
    assert!(
        !output.status.success() && text.contains("Cannot read uv.lock") && text.contains("auto selected uv.lock"),
        "{text}"
    );
    assert!(!dir.path().join("calls").exists(), "{text}");
}

#[test]
fn all_new_source_spellings_survive_config_output() {
    for source in ["auto", ".pre-commit-config.yml"] {
        let dir = project(source);
        for format in ["toml", "json", "text"] {
            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .current_dir(dir.path())
                .args(["config", "--output", format])
                .output()
                .unwrap();
            let text = output_text(&output);
            assert!(
                output.status.success() && text.contains("tool-version-definition") && text.contains(source),
                "{source}/{format}: {text}"
            );
        }
    }
}
