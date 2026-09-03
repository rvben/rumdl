use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to execute rumdl")
}

fn run_with_stdin(dir: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute rumdl");
    child.stdin.as_mut().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .replace("\r\n", "\n")
}

fn embedded_markdown_project(enabled: bool, language_enabled: Option<bool>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let language_enabled = language_enabled.map_or(String::new(), |value| format!("enabled = {value}\n"));
    fs::write(
        dir.path().join(".rumdl.toml"),
        format!(
            "[code-block-tools]\nenabled = {enabled}\n\n[code-block-tools.languages.markdown]\n{language_enabled}lint = [\"rumdl\"]\n"
        ),
    )
    .unwrap();
    dir
}

#[test]
fn only_mode_enables_configured_tools_and_suppresses_outer_rules() {
    let dir = embedded_markdown_project(false, None);
    fs::write(
        dir.path().join("doc.md"),
        "# Title\n\noutside \n\n```markdown\ninside  \n```\n",
    )
    .unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--enable",
            "MD009",
            "--no-cache",
            "--quiet",
            "--color",
            "never",
            "doc.md",
        ],
    );
    let text = output_text(&output);

    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("doc.md:6:"), "embedded warning missing:\n{text}");
    assert!(!text.contains("doc.md:3:"), "outer warning must be suppressed:\n{text}");
    assert_eq!(text.matches("MD009").count(), 1, "{text}");
}

#[test]
fn no_mode_disables_tools_but_keeps_outer_rules() {
    let dir = embedded_markdown_project(true, None);
    fs::write(
        dir.path().join("doc.md"),
        "# Title\n\noutside \n\n```markdown\ninside  \n```\n",
    )
    .unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--no-code-block-tools",
            "--enable",
            "MD009",
            "--no-cache",
            "--quiet",
            "--color",
            "never",
            "doc.md",
        ],
    );
    let text = output_text(&output);

    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("doc.md:3:"), "outer warning missing:\n{text}");
    assert!(
        !text.contains("doc.md:6:"),
        "embedded warning must be suppressed:\n{text}"
    );
    assert_eq!(text.matches("MD009").count(), 1, "{text}");
}

#[test]
fn no_mode_still_validates_preserved_tool_configuration() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        concat!(
            "[code-block-tools]\n",
            "enabled = true\n\n",
            "[code-block-tools.languages.python]\n",
            "lint = [\"definitely-not-a-tool\"]\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), "# Title\n").unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--no-code-block-tools",
            "--deny-config-warnings",
            "--no-cache",
            "--quiet",
            "doc.md",
        ],
    );
    let text = output_text(&output);

    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(text.contains("Unknown tool"), "{text}");
}

#[test]
fn only_mode_keeps_editorconfig_warnings_for_embedded_rumdl_rules() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".editorconfig"), "[*.md]\nindent_style = tab\n").unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        concat!(
            "[global]\n",
            "editorconfig = true\n\n",
            "[code-block-tools]\n",
            "enabled = false\n\n",
            "[code-block-tools.languages.markdown]\n",
            "lint = [\"rumdl\"]\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), "# Title\n\n```markdown\ntext\n```\n").unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--enable",
            "MD010",
            "--deny-config-warnings",
            "--no-cache",
            "--quiet",
            "doc.md",
        ],
    );
    let text = output_text(&output);

    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(text.contains("indent_style = tab"), "{text}");
}

#[test]
fn only_external_tools_drop_irrelevant_editorconfig_rule_warnings() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".editorconfig"), "[*.md]\nindent_style = tab\n").unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        concat!(
            "[global]\n",
            "editorconfig = true\n\n",
            "[code-block-tools]\n",
            "enabled = false\n\n",
            "[code-block-tools.tools.fakefmt]\n",
            "command = [\"true\"]\n\n",
            "[code-block-tools.languages.python]\n",
            "format = [\"fakefmt\"]\n",
        ),
    )
    .unwrap();
    // No python block, so the tool is configured but never invoked.
    fs::write(dir.path().join("doc.md"), "# Title\n").unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--enable",
            "MD010",
            "--deny-config-warnings",
            "--no-cache",
            "--quiet",
            "doc.md",
        ],
    );
    let text = output_text(&output);

    assert!(output.status.success(), "{text}");
    assert!(!text.contains("indent_style"), "{text}");
}

#[test]
fn only_mode_respects_a_disabled_language() {
    let dir = embedded_markdown_project(false, Some(false));
    fs::write(
        dir.path().join("doc.md"),
        "# Title\n\noutside  \n\n```markdown\ninside  \n```\n",
    )
    .unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--enable",
            "MD009",
            "--no-cache",
            "--quiet",
            "doc.md",
        ],
    );

    assert!(output.status.success(), "{}", output_text(&output));
}

#[test]
fn only_mode_respects_per_file_rule_ignores_inside_markdown_fences() {
    let dir = embedded_markdown_project(true, None);
    fs::write(
        dir.path().join(".rumdl.toml"),
        concat!(
            "[code-block-tools]\n",
            "enabled = true\n\n",
            "[code-block-tools.languages.markdown]\n",
            "lint = [\"rumdl\"]\n\n",
            "[per-file-ignores]\n",
            "\"doc.md\" = [\"MD009\"]\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), "# Title\n\n```markdown\ninside  \n```\n").unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--enable",
            "MD009",
            "--no-cache",
            "--quiet",
            "doc.md",
        ],
    );

    assert!(output.status.success(), "{}", output_text(&output));
}

#[test]
fn only_mode_overlays_a_subdirectory_configuration() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(
        dir.path().join("sub/.rumdl.toml"),
        concat!(
            "[code-block-tools]\n",
            "enabled = false\n\n",
            "[code-block-tools.languages.markdown]\n",
            "lint = [\"rumdl\"]\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join("sub/doc.md"), "# Title\n\n```markdown\ninside  \n```\n").unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--enable",
            "MD009",
            "--no-cache",
            "--quiet",
            "sub/doc.md",
        ],
    );

    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
}

#[test]
fn fmt_only_formats_embedded_markdown_without_touching_the_outer_document() {
    let dir = embedded_markdown_project(false, None);
    fs::write(dir.path().join("doc.md"), "#Outside\n\n```markdown\n#Inside\n```\n").unwrap();

    let output = run(
        dir.path(),
        &[
            "fmt",
            "--only-code-block-tools",
            "--enable",
            "MD018",
            "--no-cache",
            "--quiet",
            "doc.md",
        ],
    );

    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "#Outside\n\n```markdown\n# Inside\n```\n"
    );
}

#[test]
fn fmt_no_tools_formats_the_outer_document_without_touching_embedded_markdown() {
    let dir = embedded_markdown_project(true, None);
    fs::write(dir.path().join("doc.md"), "#Outside\n\n```markdown\n#Inside\n```\n").unwrap();

    let output = run(
        dir.path(),
        &[
            "fmt",
            "--no-code-block-tools",
            "--enable",
            "MD018",
            "--no-cache",
            "--quiet",
            "doc.md",
        ],
    );

    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "# Outside\n\n```markdown\n#Inside\n```\n"
    );
}

#[test]
fn check_fix_only_formats_embedded_markdown_without_touching_the_outer_document() {
    let dir = embedded_markdown_project(false, None);
    fs::write(dir.path().join("doc.md"), "#Outside\n\n```markdown\n#Inside\n```\n").unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--fix",
            "--only-code-block-tools",
            "--enable",
            "MD018",
            "--no-cache",
            "--quiet",
            "doc.md",
        ],
    );

    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "#Outside\n\n```markdown\n# Inside\n```\n"
    );
}

#[test]
fn diff_only_previews_embedded_changes_without_writing() {
    let dir = embedded_markdown_project(false, None);
    let original = "#Outside\n\n```markdown\n#Inside\n```\n";
    fs::write(dir.path().join("doc.md"), original).unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--diff",
            "--only-code-block-tools",
            "--enable",
            "MD018",
            "--no-cache",
            "--color",
            "never",
            "doc.md",
        ],
    );
    let text = output_text(&output);

    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("+# Inside"), "embedded diff missing:\n{text}");
    assert!(!text.contains("+# Outside"), "outer Markdown was formatted:\n{text}");
    assert_eq!(fs::read_to_string(dir.path().join("doc.md")).unwrap(), original);
}

#[test]
fn mode_is_part_of_the_lint_cache_identity() {
    let dir = embedded_markdown_project(true, None);
    fs::write(dir.path().join("doc.md"), "# Title\n\noutside \n").unwrap();

    let normal = run(dir.path(), &["check", "--enable", "MD009", "--quiet", "doc.md"]);
    assert_eq!(normal.status.code(), Some(1), "{}", output_text(&normal));

    let only = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--enable",
            "MD009",
            "--quiet",
            "doc.md",
        ],
    );
    assert!(only.status.success(), "{}", output_text(&only));
}

#[test]
fn auxiliary_phase_is_part_of_the_lint_cache_identity() {
    let dir = embedded_markdown_project(true, None);
    fs::write(dir.path().join("doc.md"), "# Title\n\n```markdown\ninside  \n```\n").unwrap();

    let format_check = run(
        dir.path(),
        &[
            "fmt",
            "--check",
            "--only-code-block-tools",
            "--enable",
            "MD009",
            "--quiet",
            "doc.md",
        ],
    );
    assert_eq!(format_check.status.code(), Some(1), "{}", output_text(&format_check));

    let lint = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--enable",
            "MD009",
            "--quiet",
            "--color",
            "never",
            "doc.md",
        ],
    );
    let text = output_text(&lint);
    assert_eq!(lint.status.code(), Some(1), "{text}");
    assert!(
        text.contains("MD009"),
        "lint result was hidden by a format cache entry:\n{text}"
    );
}

#[test]
fn rust_doc_comments_follow_the_outer_document_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.rs");
    let original = "/// #Bad\npub fn f() {}\n";
    fs::write(&path, original).unwrap();

    let no_tools = run(
        dir.path(),
        &[
            "fmt",
            "--no-code-block-tools",
            "--no-config",
            "--no-cache",
            "--quiet",
            "lib.rs",
        ],
    );
    assert!(no_tools.status.success(), "{}", output_text(&no_tools));
    assert_eq!(fs::read_to_string(&path).unwrap(), "/// # Bad\npub fn f() {}\n");

    fs::write(&path, original).unwrap();
    let only_tools = run(
        dir.path(),
        &[
            "fmt",
            "--only-code-block-tools",
            "--no-config",
            "--no-cache",
            "--quiet",
            "lib.rs",
        ],
    );
    assert!(only_tools.status.success(), "{}", output_text(&only_tools));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn mutually_exclusive_modes_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("doc.md"), "# Title\n").unwrap();
    let output = run(
        dir.path(),
        &["check", "--no-code-block-tools", "--only-code-block-tools", "doc.md"],
    );
    assert_eq!(output.status.code(), Some(2), "{}", output_text(&output));
}

#[test]
fn modes_are_rejected_for_every_stdin_entry_point() {
    let dir = tempfile::tempdir().unwrap();
    for flag in ["--no-code-block-tools", "--only-code-block-tools"] {
        for args in [
            vec!["check", "--stdin", flag],
            vec!["check", flag, "-"],
            vec!["check", "--stdin-batch", flag],
            vec!["fmt", "--stdin", flag],
            vec!["fmt", flag, "-"],
        ] {
            let output = run_with_stdin(dir.path(), &args, b"# Title\n");
            assert_eq!(output.status.code(), Some(2), "args={args:?}\n{}", output_text(&output));
        }
    }
}

/// Both dirs get the same project, so the two runs differ only in the flag.
#[cfg(unix)]
fn phase_recording_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        concat!(
            "[code-block-tools]\n",
            "enabled = true\n\n",
            "[code-block-tools.tools.fakelint]\n",
            "command = [\"sh\", \"-c\", \"in=$(cat); echo lint >> runs.txt; case \\\"$in\\\" in FORMATTED*) ;; *) echo '1:1: needs formatting';; esac\"]\n\n",
            "[code-block-tools.tools.fakefmt]\n",
            "command = [\"sh\", \"-c\", \"cat >/dev/null; echo format >> runs.txt; printf 'FORMATTED\\\\n'\"]\n\n",
            "[code-block-tools.languages.python]\n",
            "lint = [\"fakelint\"]\n",
            "format = [\"fakefmt\"]\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), "#Outside\n\n```python\nx=1\n```\n").unwrap();
    dir
}

/// Only mode selects rules, not phases. Compared against the same command without
/// the flag, so the assertion is the equality rather than a sequence that could
/// drift with an unrelated change to how `fmt` schedules tool passes.
#[cfg(unix)]
#[test]
fn fmt_only_runs_the_same_tool_phases_as_plain_fmt() {
    let plain = phase_recording_project();
    let only = phase_recording_project();

    let plain_output = run(plain.path(), &["fmt", "--no-cache", "--quiet", "doc.md"]);
    let only_output = run(
        only.path(),
        &["fmt", "--only-code-block-tools", "--no-cache", "--quiet", "doc.md"],
    );

    assert!(plain_output.status.success(), "{}", output_text(&plain_output));
    assert!(only_output.status.success(), "{}", output_text(&only_output));

    let plain_runs = fs::read_to_string(plain.path().join("runs.txt")).unwrap();
    assert_eq!(
        plain_runs, "lint\nformat\nlint\n",
        "precondition: plain fmt lints, formats, then relints"
    );
    assert_eq!(
        fs::read_to_string(only.path().join("runs.txt")).unwrap(),
        plain_runs,
        "only mode drops the outer document's rules and leaves the tool phases alone"
    );

    // The outer heading separates the two: only mode leaves it as written.
    assert_eq!(
        fs::read_to_string(plain.path().join("doc.md")).unwrap(),
        "# Outside\n\n```python\nFORMATTED\n```\n"
    );
    assert_eq!(
        fs::read_to_string(only.path().join("doc.md")).unwrap(),
        "#Outside\n\n```python\nFORMATTED\n```\n"
    );
}

/// Running the lint phase is what makes a formatter's blind spot visible: the
/// finding survives the format pass and is reported unfixed, instead of the run
/// rewriting the block and exiting as though the block were clean.
#[cfg(unix)]
#[test]
fn fmt_only_reports_a_tool_finding_the_formatter_cannot_fix() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        concat!(
            "[code-block-tools]\n",
            "enabled = true\n\n",
            "[code-block-tools.tools.stubbornlint]\n",
            "command = [\"sh\", \"-c\", \"cat >/dev/null; echo '1:1: unused import'\"]\n\n",
            "[code-block-tools.tools.fakefmt]\n",
            "command = [\"sh\", \"-c\", \"cat >/dev/null; printf 'FORMATTED\\\\n'\"]\n\n",
            "[code-block-tools.languages.python]\n",
            "lint = [\"stubbornlint\"]\n",
            "format = [\"fakefmt\"]\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), "#Outside\n\n```python\nx=1\n```\n").unwrap();

    let output = run(
        dir.path(),
        &[
            "fmt",
            "--only-code-block-tools",
            "--color",
            "never",
            "--no-cache",
            "doc.md",
        ],
    );

    let text = output_text(&output);
    assert!(text.contains("doc.md:4:1: [stubbornlint] unused import"), "{text}");
    assert!(!text.contains("[fixed]"), "the formatter did not resolve it\n{text}");
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "#Outside\n\n```python\nFORMATTED\n```\n"
    );
}

/// Only mode has nothing to run when no language names a tool, so the run reads
/// every file and reports nothing. Without the warning that is byte-identical to
/// a clean check.
#[test]
fn only_mode_warns_when_no_tool_is_configured() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("doc.md"), "#Outside \n").unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--no-config",
            "--no-cache",
            "doc.md",
        ],
    );

    assert!(output.status.success(), "{}", output_text(&output));
    assert!(
        output_text(&output).contains("--only-code-block-tools: no code-block tools are configured"),
        "{}",
        output_text(&output)
    );
}

/// The negative control for the warning, and the reason it asks the resolved
/// groups: the tools live in a subdirectory config the root knows nothing about.
#[test]
fn only_mode_does_not_warn_when_a_subdirectory_configures_a_tool() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "[code-block-tools]\nenabled = false\n").unwrap();
    fs::write(
        dir.path().join("sub/.rumdl.toml"),
        concat!(
            "[code-block-tools]\n",
            "enabled = false\n\n",
            "[code-block-tools.languages.markdown]\n",
            "lint = [\"rumdl\"]\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join("sub/doc.md"), "# Title\n\n```markdown\ninside\n```\n").unwrap();

    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--no-cache", "sub/doc.md"],
    );

    assert!(output.status.success(), "{}", output_text(&output));
    assert!(
        !output_text(&output).contains("no code-block tools are configured"),
        "{}",
        output_text(&output)
    );
}

/// A language switched off carries no tools for this run, so its presence in the
/// file does not answer the question the warning asks.
#[test]
fn only_mode_warns_when_the_configured_language_is_disabled() {
    let dir = embedded_markdown_project(false, Some(false));
    fs::write(dir.path().join("doc.md"), "# Title\n\n```markdown\ninside  \n```\n").unwrap();

    let output = run(
        dir.path(),
        &["check", "--only-code-block-tools", "--no-cache", "doc.md"],
    );

    assert!(output.status.success(), "{}", output_text(&output));
    assert!(
        output_text(&output).contains("--only-code-block-tools: no code-block tools are configured"),
        "{}",
        output_text(&output)
    );
}

/// Counted like every other config warning, and suppressed by `--silent` like
/// every other config warning without ceasing to count.
#[test]
fn the_no_tool_warning_is_denied_and_silenced_like_other_config_warnings() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("doc.md"), "#Outside \n").unwrap();

    let denied = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--deny-config-warnings",
            "--no-config",
            "--no-cache",
            "doc.md",
        ],
    );
    assert_eq!(denied.status.code(), Some(2), "{}", output_text(&denied));

    let silenced = run(
        dir.path(),
        &[
            "check",
            "--only-code-block-tools",
            "--deny-config-warnings",
            "--silent",
            "--no-config",
            "--no-cache",
            "doc.md",
        ],
    );
    assert_eq!(silenced.status.code(), Some(2), "{}", output_text(&silenced));
    assert!(output_text(&silenced).is_empty(), "{}", output_text(&silenced));
}

#[cfg(unix)]
#[test]
fn check_fix_only_runs_lint_format_relint() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        concat!(
            "[code-block-tools]\n",
            "enabled = false\n\n",
            "[code-block-tools.tools.fakelint]\n",
            "command = [\"sh\", \"-c\", \"in=$(cat); echo lint >> runs.txt; case \\\"$in\\\" in FORMATTED*) ;; *) echo '1:1: needs formatting';; esac\"]\n\n",
            "[code-block-tools.tools.fakefmt]\n",
            "command = [\"sh\", \"-c\", \"cat >/dev/null; echo format >> runs.txt; printf 'FORMATTED\\\\n'\"]\n\n",
            "[code-block-tools.languages.python]\n",
            "lint = [\"fakelint\"]\n",
            "format = [\"fakefmt\"]\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join("doc.md"), "#Outside\n\n```python\nx=1\n```\n").unwrap();

    let output = run(
        dir.path(),
        &[
            "check",
            "--fix",
            "--only-code-block-tools",
            "--no-cache",
            "--quiet",
            "doc.md",
        ],
    );

    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(
        fs::read_to_string(dir.path().join("runs.txt")).unwrap(),
        "lint\nformat\nlint\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("doc.md")).unwrap(),
        "#Outside\n\n```python\nFORMATTED\n```\n"
    );
}

/// Emptying the rule set is not the same request as only mode, and the
/// difference is invisible unless a run mixes both kinds of tool.
///
/// `--disable all` empties the rule set that fenced Markdown configured with
/// `lint = ["rumdl"]` is linted with, so the one tool needing no external
/// binary goes quiet while every external tool keeps reporting. That reads as
/// "code block tools only" right up to the point where the tool that stopped
/// reporting is the built-in one.
#[cfg(unix)]
#[test]
fn disable_all_silences_the_built_in_tool_that_only_mode_keeps() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        concat!(
            "[code-block-tools]\n",
            "enabled = true\n\n",
            "[code-block-tools.tools.fakelint]\n",
            "command = [\"sh\", \"-c\", \"cat >/dev/null; echo '1:1: external tool ran'\"]\n\n",
            "[code-block-tools.languages.python]\n",
            "lint = [\"fakelint\"]\n\n",
            "[code-block-tools.languages.markdown]\n",
            "lint = [\"rumdl\"]\n",
        ),
    )
    .unwrap();
    fs::write(
        dir.path().join("doc.md"),
        "# Title\n\noutside \n\n```markdown\ninside  \n```\n\n```python\nx=1\n```\n",
    )
    .unwrap();

    // No `--enable`: an enable list survives `--disable all` by design, so naming
    // MD009 explicitly would keep the outer finding and mask what is being compared.
    let text_for = |extra: &[&str]| {
        let mut args = vec!["check", "--no-cache", "--quiet", "--color", "never"];
        args.extend_from_slice(extra);
        args.push("doc.md");
        output_text(&run(dir.path(), &args))
    };

    let disable_all = text_for(&["--disable", "all"]);
    let only = text_for(&["--only-code-block-tools"]);

    // Both drop the outer document's own finding, which is what makes them look
    // interchangeable.
    assert!(!disable_all.contains("doc.md:3:"), "{disable_all}");
    assert!(!only.contains("doc.md:3:"), "{only}");

    // Both keep the external tool, so neither run is simply doing nothing.
    assert!(disable_all.contains("[fakelint]"), "{disable_all}");
    assert!(only.contains("[fakelint]"), "{only}");

    // Only the mode flag keeps the built-in tool.
    assert!(
        !disable_all.contains("doc.md:6:"),
        "--disable all empties the embedded rule set, so the built-in tool has nothing to \
         report:\n{disable_all}"
    );
    assert!(
        only.contains("doc.md:6:"),
        "only mode must keep the rule set the built-in tool lints with:\n{only}"
    );
}
