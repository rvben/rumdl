//! Exit codes and summary lines when a code-block tool cannot run.
//!
//! The lint path reports a tool that could not run as a violation, so `check`
//! exits 1. The format path (`fmt`, `check --fix`) has no violation to report,
//! so it must surface the same fact as a tool error: exit 2, and no "No issues
//! found" summary. Without that, `on-error` and the `on-missing-*` settings are
//! inert in the format path and a run that formatted nothing reports success.
//!
//! The tools here are synthetic on purpose: a name that is not on PATH for the
//! missing-binary cases, and a script that exits nonzero for the tool-error
//! cases. Nothing in this file depends on a real formatter being installed.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

/// A tool name no machine has installed.
const ABSENT_TOOL: &str = "rumdl-absent-formatter";

/// Write `.rumdl.toml` and a markdown document that is clean apart from the
/// code block, so a "No issues found" summary can only come from the tool path.
fn setup(config: &str, body: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    fs::write(dir.path().join("t.md"), body).unwrap();
    dir
}

/// A config whose only configured tool is a binary that does not exist.
fn absent_tool_config() -> String {
    format!(
        "[code-block-tools]\nenabled = true\nnormalize-language = \"exact\"\n\n\
         [code-block-tools.tools.absent]\ncommand = [\"{ABSENT_TOOL}\", \"-\"]\nstdin = true\nstdout = true\n\n\
         [code-block-tools.languages]\nyaml = {{ format = [\"absent\"] }}\n"
    )
}

const YAML_DOC: &str = "# T\n\n```yaml\nkey: value\n```\n";

/// Run rumdl in `dir`. `--no-cache` because a second run of the same content
/// would otherwise be answered from `.rumdl_cache` without consulting a tool.
fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .arg("--no-cache")
        .arg("t.md")
        .output()
        .unwrap()
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn fmt_exits_two_when_the_tool_binary_is_missing_and_the_setting_is_fail() {
    let dir = setup(&absent_tool_config(), YAML_DOC);
    let output = run(
        dir.path(),
        &["fmt", "--config", "code-block-tools.on-missing-tool-binary = \"fail\""],
    );

    assert_eq!(output.status.code(), Some(2), "stdout: {}", stdout_of(&output));
}

#[test]
fn fmt_does_not_report_success_when_the_tool_binary_is_missing() {
    let dir = setup(&absent_tool_config(), YAML_DOC);
    let output = run(
        dir.path(),
        &["fmt", "--config", "code-block-tools.on-missing-tool-binary = \"fail\""],
    );

    let stdout = stdout_of(&output);
    assert!(
        !stdout.contains("No issues found"),
        "a run that formatted nothing reported success: {stdout}"
    );
    assert!(
        stdout.contains("the run was incomplete"),
        "the summary did not say the run was incomplete: {stdout}"
    );
}

#[test]
fn check_fix_exits_two_when_the_tool_binary_is_missing_and_the_setting_is_fail() {
    let dir = setup(&absent_tool_config(), YAML_DOC);
    let output = run(
        dir.path(),
        &[
            "check",
            "--fix",
            "--config",
            "code-block-tools.on-missing-tool-binary = \"fail\"",
        ],
    );

    assert_eq!(output.status.code(), Some(2), "stdout: {}", stdout_of(&output));
}

#[test]
fn diff_mode_exits_two_when_the_tool_binary_is_missing_and_the_setting_is_fail() {
    let dir = setup(&absent_tool_config(), YAML_DOC);
    let output = run(
        dir.path(),
        &[
            "check",
            "--diff",
            "--config",
            "code-block-tools.on-missing-tool-binary = \"fail\"",
        ],
    );

    assert_eq!(output.status.code(), Some(2), "stdout: {}", stdout_of(&output));
}

/// The default. A missing binary is skipped silently, so the run is complete as
/// far as rumdl was asked to go and exit 0 is correct.
#[test]
fn fmt_exits_zero_when_the_tool_binary_is_missing_and_the_setting_is_ignore() {
    let dir = setup(&absent_tool_config(), YAML_DOC);
    let output = run(dir.path(), &["fmt"]);

    assert_eq!(output.status.code(), Some(0), "stdout: {}", stdout_of(&output));
    assert!(stdout_of(&output).contains("No issues found"));
}

#[test]
fn fmt_exits_two_when_a_language_has_no_tools_and_the_setting_is_fail() {
    let dir = setup(&absent_tool_config(), "# T\n\n```python\nx = 1\n```\n");
    let output = run(
        dir.path(),
        &[
            "fmt",
            "--config",
            "code-block-tools.on-missing-language-definition = \"fail\"",
        ],
    );

    assert_eq!(output.status.code(), Some(2), "stdout: {}", stdout_of(&output));
}

/// A tool that runs and fails, rather than one that is absent. Needs a real
/// executable, so these are Unix-only.
#[cfg(unix)]
mod tool_errors {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Put a script that always exits 3 on PATH under the name `crashtool`.
    fn setup_crashtool(on_error: Option<&str>) -> TempDir {
        let on_error = on_error.map_or_else(String::new, |value| format!("on-error = \"{value}\"\n"));
        let config = format!(
            "[code-block-tools]\nenabled = true\nnormalize-language = \"exact\"\n{on_error}\n\
             [code-block-tools.tools.crashtool]\ncommand = [\"crashtool\", \"-\"]\nstdin = true\nstdout = true\n\n\
             [code-block-tools.languages]\nyaml = {{ format = [\"crashtool\"] }}\n"
        );
        let dir = setup(&config, YAML_DOC);

        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let script = bin.join("crashtool");
        fs::write(&script, "#!/bin/sh\necho 'crashtool: internal error' >&2\nexit 3\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        dir
    }

    fn run_with_crashtool(dir: &Path, args: &[&str]) -> Output {
        let path = format!("{}:{}", dir.join("bin").display(), std::env::var("PATH").unwrap());
        Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir)
            .env("PATH", path)
            .args(args)
            .arg("--no-cache")
            .arg("t.md")
            .output()
            .unwrap()
    }

    #[test]
    fn fmt_exits_two_when_a_tool_fails_and_on_error_is_fail() {
        let dir = setup_crashtool(Some("fail"));
        let output = run_with_crashtool(dir.path(), &["fmt"]);

        assert_eq!(output.status.code(), Some(2), "stdout: {}", stdout_of(&output));
        assert!(!stdout_of(&output).contains("No issues found"));
    }

    /// `warn` asks to be told and to carry on. It must not start failing builds
    /// now that the format path can report a tool error at all.
    #[test]
    fn fmt_exits_zero_when_a_tool_fails_and_on_error_is_warn() {
        let dir = setup_crashtool(Some("warn"));
        let output = run_with_crashtool(dir.path(), &["fmt"]);

        assert_eq!(output.status.code(), Some(0), "stdout: {}", stdout_of(&output));
        assert!(stdout_of(&output).contains("No issues found"));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("crashtool: internal error"),
            "the warning itself was not reported"
        );
    }

    #[test]
    fn fmt_exits_zero_when_a_tool_fails_and_on_error_is_skip() {
        let dir = setup_crashtool(Some("skip"));
        let output = run_with_crashtool(dir.path(), &["fmt"]);

        assert_eq!(output.status.code(), Some(0), "stdout: {}", stdout_of(&output));
    }
}

/// A tool that exits before reading its input, over a block too large for the pipe
/// buffer.
///
/// The executor writes the block to the tool's stdin and treats a broken pipe as
/// normal, since a tool is free to exit without consuming its input. Reaching that
/// error at all takes ignoring SIGPIPE, which rumdl otherwise leaves at its default
/// disposition so that piping its own output into `head` ends quietly. Without that,
/// the write kills rumdl: no output, no exit code, and `on-error` never consulted.
///
/// The block is deliberately larger than a pipe buffer (64KB on Linux and macOS).
/// A small one usually reaches the buffer before the tool exits, which is why the
/// same defect showed up in CI only as an occasional signal death.
#[cfg(unix)]
mod broken_pipe {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn setup_quitting_tool() -> TempDir {
        let config = "[code-block-tools]\nenabled = true\nnormalize-language = \"exact\"\n\
             on-error = \"skip\"\n\n\
             [code-block-tools.tools.quitter]\ncommand = [\"quitter\"]\nstdin = true\nstdout = true\n\n\
             [code-block-tools.languages]\nyaml = { format = [\"quitter\"] }\n";

        let block: String = (0..20_000).map(|i| format!("key{i}: value{i}\n")).collect();
        let dir = setup(config, &format!("# T\n\n```yaml\n{block}```\n"));
        assert!(
            fs::metadata(dir.path().join("t.md")).unwrap().len() > 64 * 1024,
            "the block has to exceed the pipe buffer for the write to block"
        );

        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let script = bin.join("quitter");
        fs::write(&script, "#!/bin/sh\nexit 3\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        dir
    }

    fn run_quitting(dir: &Path, subcommand: &str) -> Output {
        let path = format!("{}:{}", dir.join("bin").display(), std::env::var("PATH").unwrap());
        Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir)
            .env("PATH", path)
            .args([subcommand, "--no-cache", "t.md"])
            .output()
            .unwrap()
    }

    #[test]
    fn a_tool_that_exits_without_reading_its_input_does_not_kill_rumdl() {
        let dir = setup_quitting_tool();

        for subcommand in ["fmt", "check"] {
            let output = run_quitting(dir.path(), subcommand);
            assert_eq!(
                output.status.code(),
                Some(0),
                "{subcommand} did not exit on its own terms; stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
