//! Hook metadata verified through the CLI and real subprocesses.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

#[path = "support/native_tool.rs"]
mod native_tool;

fn project(source: &str, metadata: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        format!(
            r#"[code-block-tools]
enabled = true
tool-version-definition = "{source}"
on-tool-version-mismatch = "fail"
binary-preferences = {{ ruff = "only-project" }}
[code-block-tools.tools.test]
command = ["ruff"]
stdin = true
stdout = true
[code-block-tools.languages.python]
lint = ["test"]
format = ["test"]
"#
        ),
    )
    .unwrap();
    fs::write(dir.path().join(source), metadata).unwrap();
    fs::write(dir.path().join("doc.md"), "```python\nbad\n```\n").unwrap();
    dir
}

fn install(root: &Path, name: &str, version: &str) {
    let fixture = native_tool::install(root, "quiet");
    let tool = fixture.with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    native_tool::copy_executable(&fixture, &tool);
    fs::write(tool.with_extension("mode"), "count-format").unwrap();
    fs::write(tool.with_extension("version"), version).unwrap();
}

fn run(root: &Path, command: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(root)
        .args([command, "--only-code-block-tools", "--no-cache", "doc.md"])
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
fn pre_commit_ruff_release_tag_verifies_the_selected_formatter() {
    let dir = project(
        ".pre-commit-config.yaml",
        "repos:\n- repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: v0.11.2\n  hooks:\n  - id: ruff-format\n",
    );
    install(dir.path(), "ruff", "ruff 0.11.2\n");
    let output = run(dir.path(), "fmt");
    let text = output_text(&output);
    assert!(output.status.success(), "{text}");
    assert!(dir.path().join("calls").exists(), "formatter did not run: {text}");
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "```python\ngood\n```\n"
    );

    install(dir.path(), "ruff", "ruff 0.12.1\n");
    fs::remove_file(dir.path().join("calls")).unwrap();
    let original = fs::read(dir.path().join("doc.md")).unwrap();
    let output = run(dir.path(), "fmt");
    let text = output_text(&output);
    assert!(!output.status.success() && text.contains("version mismatch"), "{text}");
    assert!(
        text.contains(".pre-commit-config.yaml") && text.contains("0.11.2"),
        "{text}"
    );
    assert!(!dir.path().join("calls").exists(), "mismatched formatter ran");
    assert_eq!(fs::read(dir.path().join("doc.md")).unwrap(), original);
}

#[test]
fn prek_toml_ruff_revision_is_verified() {
    let dir = project(
        "prek.toml",
        "[[repos]]\nrepo='https://github.com/astral-sh/ruff-pre-commit'\nrev='v0.11.2'\nhooks=[{id='ruff-check'}]\n",
    );
    install(dir.path(), "ruff", "ruff 0.11.2\n");
    let output = run(dir.path(), "check");
    let text = output_text(&output);
    assert!(output.status.success() && dir.path().join("calls").exists(), "{text}");
    assert!(
        !text.contains("Cannot ") && !text.contains("version mismatch"),
        "{text}"
    );
}

#[test]
fn official_hook_repositories_map_all_supported_executables_in_both_formats() {
    for (repo, id, name, version, output) in [
        (
            "psf/black",
            "black",
            "black",
            "25.1.0",
            "black, 25.1.0 (compiled: yes)\n",
        ),
        (
            "psf/black-pre-commit-mirror",
            "black-jupyter",
            "black",
            "25.1.0",
            "black, 25.1.0 (compiled: yes)\n",
        ),
        (
            "djlint/djLint",
            "djlint-reformat-django",
            "djlint",
            "v1.36.4",
            "djlint, version 1.36.4\n",
        ),
        (
            "sqlfluff/sqlfluff",
            "sqlfluff-fix",
            "sqlfluff",
            "3.3.1",
            "sqlfluff, version 3.3.1\n",
        ),
        (
            "charliermarsh/ruff-pre-commit",
            "ruff",
            "ruff",
            "v0.11.2",
            "ruff 0.11.2\n",
        ),
    ] {
        for source in [".pre-commit-config.yaml", "prek.toml"] {
            let metadata = if source == "prek.toml" {
                format!("[[repos]]\nrepo='https://github.com/{repo}'\nrev='{version}'\nhooks=[{{id='{id}'}}]\n")
            } else {
                format!("repos:\n- repo: https://github.com/{repo}\n  rev: {version}\n  hooks: [{{id: {id}}}]\n")
            };
            let dir = project(source, &metadata);
            let config = dir.path().join(".rumdl.toml");
            fs::write(&config, fs::read_to_string(&config).unwrap().replace("ruff", name)).unwrap();
            install(dir.path(), name, output);
            let output = run(dir.path(), "fmt");
            let text = output_text(&output);
            assert!(
                output.status.success() && dir.path().join("calls").exists(),
                "{source} {repo}: {text}"
            );
            assert!(
                !text.contains("Cannot ") && !text.contains("version mismatch"),
                "{text}"
            );
        }
    }
}

#[test]
fn ambiguous_revisions_and_hook_overrides_are_unresolved_and_redacted() {
    for (revision, hook, other) in [
        ("v0.11.2", "id: ruff-format, entry: null", ""),
        ("v0.11.2", "id: ruff-format, entry: ruff format", ""),
        ("v0.11.2", "id: ruff-format, language: system", ""),
        ("v0.11.2", "id: ruff-format, env: {SECRET: PRIVATE_SENTINEL}", ""),
        (
            "v0.11.2",
            "id: ruff-format, additional_dependencies: ['ruff==0.11.2']",
            "",
        ),
        ("v0.11.2", "id: ruff-format, additional_dependencies: ['plugin>=1']", ""),
        ("v0.11.2", "id: ruff-format, additional_dependencies: null", ""),
        ("main", "id: ruff-format", ""),
        ("0123456789abcdef0123456789abcdef01234567", "id: ruff-format", ""),
        ("0.11.2", "id: ruff-format", ""),
        ("v0.11.2-1", "id: ruff-format", ""),
        ("v0.11.2+PRIVATE_SENTINEL", "id: ruff-format", ""),
        ("v0.11.2.dev1", "id: ruff-format", ""),
        ("v1!0.11.2", "id: ruff-format", ""),
        (
            "v0.11.2",
            "id: ruff-format",
            "- repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: v0.12.1\n  hooks: [{id: ruff-check}]\n",
        ),
        (
            "v0.11.2",
            "id: ruff-format",
            "- repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: main\n  hooks: [{id: ruff-check}]\n",
        ),
        (
            "v0.11.2",
            "id: ruff-format",
            "- repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: v0.11.2\n  hooks: [{id: unknown-hook}]\n",
        ),
    ] {
        let metadata = format!(
            "repos:\n- repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: '{revision}'\n  hooks: [{{{hook}}}]\n{other}"
        );
        let dir = project(".pre-commit-config.yaml", &metadata);
        install(dir.path(), "ruff", "ruff 0.11.2\n");
        let output = run(dir.path(), "fmt");
        let text = output_text(&output);
        assert!(
            !output.status.success() && text.contains("Cannot resolve"),
            "{metadata}: {text}"
        );
        assert!(
            !dir.path().join("calls").exists(),
            "unresolved formatter ran: {metadata}"
        );
        assert!(!text.contains("PRIVATE_SENTINEL"), "{text}");
    }
}

#[test]
fn repository_identity_is_exact_and_unrecognized_urls_are_redacted() {
    for (repo, accepted) in [
        ("https://github.com/astral-sh/ruff-pre-commit//", false),
        ("https://github.com/astral-sh/ruff-pre-commit", true),
        ("https://github.com/ASTRAL-SH/RUFF-PRE-COMMIT.git", true),
        ("https://github.com/astral-sh/ruff-pre-commit/", true),
        ("git@github.com:astral-sh/ruff-pre-commit.git", true),
        ("ssh://git@github.com/astral-sh/ruff-pre-commit", true),
        ("https://PRIVATE_SENTINEL@github.com/astral-sh/ruff-pre-commit", false),
        ("https://github.com:443/astral-sh/ruff-pre-commit", false),
        ("https://github.com/astral-sh/ruff-pre-commit?PRIVATE_SENTINEL", false),
        ("https://github.com/astral-sh/ruff-pre-commit#PRIVATE_SENTINEL", false),
        ("https://github.com/astral-sh/%72uff-pre-commit", false),
        ("https://github.com/other/../astral-sh/ruff-pre-commit", false),
        ("https://github.com.evil/astral-sh/ruff-pre-commit", false),
        ("https://github.com@evil/astral-sh/ruff-pre-commit", false),
        ("https://evil/github.com/astral-sh/ruff-pre-commit", false),
        ("https://github.com./astral-sh/ruff-pre-commit", false),
        ("https://www.github.com/astral-sh/ruff-pre-commit", false),
        ("http://github.com/astral-sh/ruff-pre-commit", false),
        ("git://github.com/astral-sh/ruff-pre-commit", false),
        ("https://gіthub.com/astral-sh/ruff-pre-commit", false),
        ("file:///astral-sh/ruff-pre-commit", false),
        ("../astral-sh/ruff-pre-commit", false),
    ] {
        let dir = project(
            ".pre-commit-config.yaml",
            &format!("repos:\n- repo: {repo}\n  rev: v0.11.2\n  hooks: [{{id: ruff-format}}]\n"),
        );
        install(dir.path(), "ruff", "ruff 0.11.2\n");
        let output = run(dir.path(), "fmt");
        let text = output_text(&output);
        assert_eq!(output.status.success(), accepted, "{repo}: {text}");
        assert_eq!(dir.path().join("calls").exists(), accepted, "{repo}: {text}");
        assert!(!text.contains("PRIVATE_SENTINEL"), "{text}");
    }
}

#[test]
fn malformed_hook_metadata_and_non_string_revisions_are_unresolved() {
    let valid =
        "repos:\n- repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: v0.11.2\n  hooks: [{id: ruff-format}]\n";
    let mut cases = vec![
        (
            ".pre-commit-config.yaml",
            format!("{valid}- repo: https://example.invalid/PRIVATE_SENTINEL\n  hooks: [{{id: unrelated}}]\n"),
        ),
        (".pre-commit-config.yaml", "repos: PRIVATE_SENTINEL\n".into()),
        (".pre-commit-config.yaml", format!("{valid}---\nPRIVATE_SENTINEL\n")),
        (".pre-commit-config.yaml", valid.replace("v0.11.2", "1.10")),
        (".pre-commit-config.yaml", valid.replace("v0.11.2", "true")),
        (".pre-commit-config.yaml", valid.replace("v0.11.2", "null")),
        (
            ".pre-commit-config.yaml",
            valid.replace("v0.11.2", "!PRIVATE_SENTINEL v0.11.2"),
        ),
        (
            ".pre-commit-config.yaml",
            valid.replace("rev: v0.11.2", "rev: v0.12.1\n  rev: v0.11.2"),
        ),
        (
            "prek.toml",
            "[[repos]]\nrepo='https://github.com/astral-sh/ruff-pre-commit'\nrev=1.10\nhooks=[{id='ruff-format'}]\n"
                .into(),
        ),
        ("prek.toml", "repos = 'PRIVATE_SENTINEL'\n".into()),
    ];
    cases.push((
        ".pre-commit-config.yaml",
        format!("{valid}x-deep: {}0{}\n", "[".repeat(140), "]".repeat(140)),
    ));
    let mut aliases = "x-0: &a0 [x,x,x,x,x,x,x,x,x]\n".to_owned();
    for index in 1..10 {
        aliases.push_str(&format!(
            "x-{index}: &a{index} [{}]\n",
            vec![format!("*a{}", index - 1); 9].join(",")
        ));
    }
    cases.push((".pre-commit-config.yaml", format!("{valid}{aliases}")));
    for (source, metadata) in cases {
        let dir = project(source, &metadata);
        install(dir.path(), "ruff", "ruff 0.11.2\n");
        let output = run(dir.path(), "fmt");
        let text = output_text(&output);
        assert!(!output.status.success(), "{source}: {metadata}: {text}");
        assert!(
            text.contains("Cannot parse") || text.contains("Cannot resolve"),
            "{text}"
        );
        assert!(
            !dir.path().join("calls").exists(),
            "unresolved formatter ran: {metadata}"
        );
        assert!(!text.contains("PRIVATE_SENTINEL"), "{text}");
    }
}

#[test]
fn yaml_merges_preserve_precedence_and_harmless_hook_options() {
    for metadata in [
        "x-default: &default {rev: v0.12.1}\nrepos:\n- <<: *default\n  repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: !!str v0.11.2\n  hooks: [{id: ruff-format, language: python, additional_dependencies: [], args: [--fix], x-note: metadata}]\n",
        "x-first: &first {rev: v0.11.2}\nx-last: &last {rev: v0.12.1}\nx-hook: &hook {id: ruff-format, language: python}\nrepos:\n- <<: [*first, *last]\n  repo: https://github.com/astral-sh/ruff-pre-commit\n  hooks: [{<<: *hook, name: My formatter}]\n",
        "x-base: &base {rev: v0.12.1}\nx-nested: &nested {<<: *base, rev: v0.11.2}\nrepos:\n- <<: *nested\n  repo: https://github.com/astral-sh/ruff-pre-commit\n  hooks: [{id: ruff-format}]\n- repo: local\n  hooks: [{id: ruff-format, entry: other, language: system}]\n",
    ] {
        let dir = project(".pre-commit-config.yaml", metadata);
        install(dir.path(), "ruff", "ruff 0.11.2\n");
        let output = run(dir.path(), "fmt");
        let text = output_text(&output);
        assert!(
            output.status.success() && dir.path().join("calls").exists(),
            "{metadata}: {text}"
        );
        assert!(
            !text.contains("Cannot ") && !text.contains("version mismatch"),
            "{text}"
        );
    }
}

#[test]
fn hook_sources_share_version_policies_and_probe_once_per_invocation() {
    for source in [".pre-commit-config.yaml", "prek.toml"] {
        for policy in ["warn", "ignore", "fail", "fail-fast"] {
            let metadata = if source == "prek.toml" {
                "[[repos]]\nrepo='https://github.com/astral-sh/ruff-pre-commit'\nrev='v0.12.1'\nhooks=[{id='ruff-check'}]\n"
            } else {
                "repos:\n- repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: v0.12.1\n  hooks: [{id: ruff-check}]\n"
            };
            let dir = project(source, metadata);
            let config = dir.path().join(".rumdl.toml");
            fs::write(
                &config,
                fs::read_to_string(&config)
                    .unwrap()
                    .replace("= \"fail\"", &format!("= \"{policy}\"")),
            )
            .unwrap();
            fs::write(dir.path().join("doc.md"), "```python\nbad\n```\n\n".repeat(32)).unwrap();
            install(dir.path(), "ruff", "ruff 0.11.2\n");
            let output = run(dir.path(), "check");
            let text = output_text(&output);
            let runs = matches!(policy, "warn" | "ignore");
            assert_eq!(output.status.success(), runs, "{source} {policy}: {text}");
            assert_eq!(dir.path().join("calls").exists(), runs, "{text}");
            if policy == "warn" {
                assert_eq!(text.matches("version mismatch").count(), 1, "{text}");
            }
            let probe = dir.path().join(if cfg!(windows) {
                ".venv/Scripts/ruff.version-calls"
            } else {
                ".venv/bin/ruff.version-calls"
            });
            assert_eq!(
                if probe.exists() {
                    fs::read_to_string(probe).unwrap().lines().count()
                } else {
                    0
                },
                usize::from(policy != "ignore")
            );
        }
    }
}

#[test]
fn nearest_hook_file_is_authoritative_without_outer_or_other_source_fallback() {
    for source in [".pre-commit-config.yaml", "prek.toml"] {
        let valid = if source == "prek.toml" {
            "[[repos]]\nrepo='https://github.com/astral-sh/ruff-pre-commit'\nrev='v0.11.2'\nhooks=[{id='ruff-check'}]\n"
        } else {
            "repos:\n- repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: v0.11.2\n  hooks: [{id: ruff-check}]\n"
        };
        let dir = project(source, valid);
        install(dir.path(), "ruff", "ruff 0.11.2\n");
        let nested = dir.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("doc.md"), "```python\nbad\n```\n").unwrap();
        fs::write(
            nested.join(source),
            if source == "prek.toml" {
                "repos=[]\n"
            } else {
                "repos: []\n"
            },
        )
        .unwrap();
        fs::write(
            nested.join("uv.lock"),
            "version=1\n[[package]]\nname='ruff'\nversion='0.11.2'\n",
        )
        .unwrap();
        let output = run(&nested, "fmt");
        let text = output_text(&output);
        assert!(
            !output.status.success() && text.contains("Cannot resolve"),
            "{source}: {text}"
        );
        assert!(!nested.join("calls").exists(), "outer metadata was used");
    }
}

#[test]
fn nested_yaml_merges_resolve_sources_before_applying_precedence() {
    for metadata in [
        "x-base: &base {rev: v0.11.2}\nx-nested: &nested {<<: *base}\nx-hook-base: &hookbase {id: ruff-format}\nx-hook: &hook {<<: *hookbase}\nrepos:\n- <<: *nested\n  repo: https://github.com/astral-sh/ruff-pre-commit\n  hooks: [{<<: *hook}]\n",
        "x-base: &base {rev: v0.11.2}\nx-first: &first {<<: *base}\nx-last: &last {rev: v0.12.1}\nrepos:\n- <<: [*first, *last]\n  repo: https://github.com/astral-sh/ruff-pre-commit\n  hooks: [{id: ruff-format}]\n",
        "x-base: &base {rev: v0.12.2}\nx-middle: &middle {<<: *base, rev: v0.12.1}\nrepos:\n- <<: *middle\n  repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: v0.11.2\n  hooks: [{id: ruff-format}]\n",
    ] {
        let dir = project(".pre-commit-config.yaml", metadata);
        install(dir.path(), "ruff", "ruff 0.11.2\n");
        let output = run(dir.path(), "fmt");
        let text = output_text(&output);
        assert!(
            output.status.success() && dir.path().join("calls").exists(),
            "{metadata}: {text}"
        );
    }
}

#[test]
fn hook_environment_overrides_including_nested_merges_are_unresolved() {
    for (source, metadata) in [
        (
            ".pre-commit-config.yaml",
            "default_env: {PRIVATE_SENTINEL: x}\nrepos:\n- repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: v0.11.2\n  hooks: [{id: ruff-format}]\n",
        ),
        (
            ".pre-commit-config.yaml",
            "x-inner: &inner {default_env: {PRIVATE_SENTINEL: x}}\nx-outer: &outer {<<: *inner}\n<<: *outer\nrepos:\n- repo: https://github.com/astral-sh/ruff-pre-commit\n  rev: v0.11.2\n  hooks: [{id: ruff-format}]\n",
        ),
        (
            "prek.toml",
            "default_env={PRIVATE_SENTINEL='x'}\n[[repos]]\nrepo='https://github.com/astral-sh/ruff-pre-commit'\nrev='v0.11.2'\nhooks=[{id='ruff-format'}]\n",
        ),
    ] {
        let dir = project(source, metadata);
        install(dir.path(), "ruff", "ruff 0.11.2\n");
        let output = run(dir.path(), "fmt");
        let text = output_text(&output);
        assert!(
            !output.status.success() && text.contains("Cannot resolve"),
            "{metadata}: {text}"
        );
        assert!(
            !dir.path().join("calls").exists() && !text.contains("PRIVATE_SENTINEL"),
            "{text}"
        );
    }
}

#[test]
fn hook_revision_prefixes_prereleases_and_padding_follow_source_semantics() {
    for (repo, id, name, revision, actual, accepted) in [
        ("psf/black", "black", "black", "v25.1.0", "black, 25.1.0\n", false),
        (
            "sqlfluff/sqlfluff",
            "sqlfluff-lint",
            "sqlfluff",
            "v3.3.1",
            "sqlfluff, version 3.3.1\n",
            false,
        ),
        (
            "djlint/djLint",
            "djlint",
            "djlint",
            "1.36.4",
            "djlint, version 1.36.4\n",
            false,
        ),
        (
            "astral-sh/ruff-pre-commit",
            "ruff-check",
            "ruff",
            "V0.11.2",
            "ruff 0.11.2\n",
            false,
        ),
        ("psf/black", "black", "black", "25.1.0rc1", "black, 25.1.0rc1\n", true),
        (
            "astral-sh/ruff-pre-commit",
            "ruff-check",
            "ruff",
            "v0.11.2.post1",
            "ruff 0.11.2.post1\n",
            true,
        ),
        (
            "astral-sh/ruff-pre-commit",
            "ruff-check",
            "ruff",
            "v0.11.2.0",
            "ruff 0.11.2\n",
            true,
        ),
    ] {
        let metadata =
            format!("[[repos]]\nrepo='https://github.com/{repo}'\nrev='{revision}'\nhooks=[{{id='{id}'}}]\n");
        let dir = project("prek.toml", &metadata);
        let config = dir.path().join(".rumdl.toml");
        fs::write(&config, fs::read_to_string(&config).unwrap().replace("ruff", name)).unwrap();
        install(dir.path(), name, actual);
        let output = run(dir.path(), "fmt");
        let text = output_text(&output);
        assert_eq!(output.status.success(), accepted, "{revision}: {text}");
        assert_eq!(dir.path().join("calls").exists(), accepted, "{text}");
    }
}

#[test]
fn prek_toml_overrides_and_native_revision_types_are_unresolved() {
    for (rev, hook) in [
        ("'v0.11.2'", "id='ruff-check', entry='ruff check'"),
        ("'v0.11.2'", "id='ruff-check', env={PRIVATE_SENTINEL='x'}"),
        ("'v0.11.2'", "id='ruff-check', language='system'"),
        ("'v0.11.2'", "id='ruff-check', additional_dependencies=['plugin>=1']"),
        ("'v0.11.2'", "id='unknown-hook'"),
        ("true", "id='ruff-check'"),
        ("2026-10-17", "id='ruff-check'"),
        ("['v0.11.2']", "id='ruff-check'"),
    ] {
        let metadata =
            format!("[[repos]]\nrepo='https://github.com/astral-sh/ruff-pre-commit'\nrev={rev}\nhooks=[{{{hook}}}]\n");
        let dir = project("prek.toml", &metadata);
        install(dir.path(), "ruff", "ruff 0.11.2\n");
        let output = run(dir.path(), "fmt");
        let text = output_text(&output);
        assert!(!output.status.success(), "{metadata}: {text}");
        assert!(
            text.contains("Cannot resolve") || text.contains("Cannot parse"),
            "{text}"
        );
        assert!(
            !dir.path().join("calls").exists() && !text.contains("PRIVATE_SENTINEL"),
            "{text}"
        );
    }
}

#[test]
fn local_meta_builtin_and_unknown_repositories_cannot_supply_versions() {
    for repo in ["local", "meta", "builtin", "https://example.invalid/ruff-pre-commit"] {
        let dir = project(
            "prek.toml",
            &format!("[[repos]]\nrepo='{repo}'\nrev='v0.11.2'\nhooks=[{{id='ruff-check'}}]\n"),
        );
        install(dir.path(), "ruff", "ruff 0.11.2\n");
        let output = run(dir.path(), "fmt");
        let text = output_text(&output);
        assert!(
            !output.status.success() && text.contains("Cannot resolve"),
            "{repo}: {text}"
        );
        assert!(!dir.path().join("calls").exists(), "{repo}: {text}");
    }
}
