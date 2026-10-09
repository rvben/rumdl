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

/// A lint tool that cannot run, beside one that runs and reports a finding.
///
/// `broken` is an executable whose interpreter does not exist, so it passes the
/// PATH lookup and then fails to spawn: a genuine tool error, reached instantly
/// and on every Unix, where a nonzero exit would only be a diagnostic.
#[cfg(unix)]
mod lint_tool_errors {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// The `json` block (fence at line 3) goes to `linter`, which reports a
    /// finding. The `yaml` block (fence at line 7) goes to `broken`.
    const DOC: &str = "# T\n\n```json\n{}\n```\n\n```yaml\nk: v\n```\n";

    /// Only the block whose tool cannot run.
    const BROKEN_ONLY_DOC: &str = "# T\n\n```yaml\nk: v\n```\n";

    fn setup_tools(on_error: &str, body: &str) -> TempDir {
        let config = format!(
            "[code-block-tools]\nenabled = true\nnormalize-language = \"exact\"\non-error = \"{on_error}\"\n\n\
             [code-block-tools.tools.linter]\ncommand = [\"linter\"]\nstdin = true\nstdout = true\n\n\
             [code-block-tools.tools.broken]\ncommand = [\"broken\"]\nstdin = true\nstdout = true\n\n\
             [code-block-tools.languages]\njson = {{ lint = [\"linter\"] }}\nyaml = {{ lint = [\"broken\"] }}\n"
        );
        let dir = setup(&config, body);

        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        for (name, script) in [
            ("linter", "#!/bin/sh\necho 'error: linter finding' >&2\nexit 1\n"),
            ("broken", "#!/nonexistent/interpreter\n"),
        ] {
            let path = bin.join(name);
            fs::write(&path, script).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    fn check(dir: &Path, extra: &[&str]) -> Output {
        let path = format!("{}:{}", dir.join("bin").display(), std::env::var("PATH").unwrap());
        Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir)
            .env("PATH", path)
            .arg("check")
            .args(extra)
            .args(["--no-cache", "t.md"])
            .output()
            .unwrap()
    }

    fn stderr_of(output: &Output) -> String {
        String::from_utf8_lossy(&output.stderr).to_string()
    }

    /// `fail` stops the document, but what the blocks before the failure already
    /// reported is still true and still owed to the user.
    #[test]
    fn fail_keeps_the_findings_of_blocks_checked_before_the_failure() {
        let dir = setup_tools("fail", DOC);
        let output = check(dir.path(), &[]);

        let stdout = stdout_of(&output);
        assert!(
            stdout.contains("t.md:3:1: [linter] error: linter finding"),
            "the json block's finding was dropped: {stdout}"
        );
        assert_eq!(output.status.code(), Some(1), "stdout: {stdout}");
    }

    #[test]
    fn fail_reports_the_failure_at_the_block_whose_tool_could_not_run() {
        let dir = setup_tools("fail", DOC);
        let output = check(dir.path(), &[]);

        let stdout = stdout_of(&output);
        assert!(
            stdout.contains("t.md:7:1: [code-block-tools]") && stdout.contains("Failed to spawn 'broken'"),
            "the failure is not reported at the yaml block: {stdout}"
        );
        assert!(
            !stdout.contains("t.md:1:1"),
            "the failure is reported at the top of the file: {stdout}"
        );
    }

    /// Every output format carries the position, not just the text one.
    #[test]
    fn fail_reports_the_failure_position_in_json_output() {
        let dir = setup_tools("fail", BROKEN_ONLY_DOC);
        let output = check(dir.path(), &["--output-format", "json"]);

        let findings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let finding = &findings[0];
        assert_eq!(finding["line"], 3, "{findings}");
        assert_eq!(finding["rule"], "code-block-tools", "{findings}");
        assert!(
            !finding["message"].as_str().unwrap().starts_with("line "),
            "the position is only in the message: {findings}"
        );
    }

    /// `warn` asks to be told and to carry on, the same as it does for `fmt`: the
    /// failure is reported on stderr and the run is otherwise clean.
    #[test]
    fn warn_reports_the_failure_on_stderr_without_failing_the_run() {
        let dir = setup_tools("warn", BROKEN_ONLY_DOC);
        let output = check(dir.path(), &[]);

        let stderr = stderr_of(&output);
        assert!(
            stderr.contains("t.md:3") && stderr.contains("Failed to spawn 'broken'"),
            "the failure was not reported: {stderr}"
        );
        assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    }

    #[test]
    fn warn_keeps_checking_the_other_blocks() {
        let dir = setup_tools("warn", DOC);
        let output = check(dir.path(), &[]);

        let stdout = stdout_of(&output);
        assert!(stdout.contains("t.md:3:1: [linter] error: linter finding"), "{stdout}");
        assert!(
            !stdout.contains("[code-block-tools]"),
            "warn made the failure a finding: {stdout}"
        );
    }

    #[test]
    fn warn_is_quiet_under_silent() {
        let dir = setup_tools("warn", BROKEN_ONLY_DOC);
        let output = check(dir.path(), &["--silent"]);

        assert!(
            !stderr_of(&output).contains("Failed to spawn"),
            "{}",
            stderr_of(&output)
        );
    }

    #[test]
    fn skip_says_nothing() {
        let dir = setup_tools("skip", BROKEN_ONLY_DOC);
        let output = check(dir.path(), &[]);

        let stderr = stderr_of(&output);
        assert!(!stderr.contains("Failed to spawn"), "{stderr}");
        assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    }
}

/// A format tool that cannot run, reported by a fixing run.
#[cfg(unix)]
mod format_tool_error_position {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn check_fix_reports_the_failure_at_the_block_in_json_output() {
        let config = "[code-block-tools]\nenabled = true\nnormalize-language = \"exact\"\n\n\
             [code-block-tools.tools.broken]\ncommand = [\"broken\"]\nstdin = true\nstdout = true\n\n\
             [code-block-tools.languages]\nyaml = { format = [\"broken\"] }\n";
        let dir = setup(config, "# T\n\nText.\n\n```yaml\nk: v\n```\n");
        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let script = bin.join("broken");
        fs::write(&script, "#!/nonexistent/interpreter\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir.path())
            .env("PATH", path)
            .args(["check", "--fix", "--output-format", "json", "--no-cache", "t.md"])
            .output()
            .unwrap();

        let findings: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let failure = findings
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["rule"] == "code-block-tools")
            .unwrap_or_else(|| panic!("no failure reported: {findings}"));
        assert_eq!(failure["line"], 5, "{findings}");
        assert_eq!(output.status.code(), Some(2), "{findings}");
    }
}

/// A formatter that exits 0 and prints nothing for a block that is not empty.
///
/// Taking that output would erase the block, so it is never applied. It is still
/// a formatter that did not do its job, which is what `on-error` governs; a run
/// that reports success over it claims a block was formatted when nothing was.
#[cfg(unix)]
mod empty_formatter_output {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Discards its input and succeeds.
    const EMPTY: &str = "#!/bin/sh\ncat >/dev/null\nexit 0\n";
    /// Uppercases its input: visibly formats, so a fallback to it can be seen.
    const UPPER: &str = "#!/bin/sh\ntr a-z A-Z\n";

    /// Put `scripts` on PATH and return the directory holding them.
    fn install(dir: &Path, scripts: &[(&str, &str)]) {
        let bin = dir.join("bin");
        fs::create_dir(&bin).unwrap();
        for (name, body) in scripts {
            let path = bin.join(name);
            fs::write(&path, body).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn format_setup(on_error: Option<&str>, format: &str) -> TempDir {
        let on_error = on_error.map_or_else(String::new, |value| format!("on-error = \"{value}\"\n"));
        let config = format!(
            "[code-block-tools]\nenabled = true\nnormalize-language = \"exact\"\n{on_error}\n\
             [code-block-tools.tools.empty]\ncommand = [\"emptyfmt\"]\nstdin = true\nstdout = true\n\n\
             [code-block-tools.tools.upper]\ncommand = [\"upperfmt\"]\nstdin = true\nstdout = true\n\n\
             [code-block-tools.languages]\nyaml = {{ format = {format} }}\n"
        );
        let dir = setup(&config, YAML_DOC);
        install(dir.path(), &[("emptyfmt", EMPTY), ("upperfmt", UPPER)]);
        dir
    }

    fn run_in(dir: &Path, args: &[&str]) -> Output {
        let path = format!("{}:{}", dir.join("bin").display(), std::env::var("PATH").unwrap());
        Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir)
            .env("PATH", path)
            .args(args)
            .args(["--no-cache", "t.md"])
            .output()
            .unwrap()
    }

    fn document(dir: &Path) -> String {
        fs::read_to_string(dir.join("t.md")).unwrap()
    }

    fn stderr_of(output: &Output) -> String {
        String::from_utf8_lossy(&output.stderr).to_string()
    }

    /// The default `on-error` is `fail`.
    #[test]
    fn fmt_exits_two_under_the_default_setting_and_leaves_the_block() {
        let dir = format_setup(None, "[\"empty\"]");
        let output = run_in(dir.path(), &["fmt"]);

        let stderr = stderr_of(&output);
        assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
        assert!(
            stderr.contains("t.md:3") && stderr.contains("no output"),
            "the empty output was not reported at the block: {stderr}"
        );
        assert!(!stdout_of(&output).contains("No issues found"));
        assert_eq!(document(dir.path()), YAML_DOC);
    }

    #[test]
    fn fmt_warns_and_succeeds_under_warn() {
        let dir = format_setup(Some("warn"), "[\"empty\"]");
        let output = run_in(dir.path(), &["fmt"]);

        let stderr = stderr_of(&output);
        assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
        assert!(
            stderr.contains("Warning: t.md:3") && stderr.contains("no output"),
            "the empty output was not reported: {stderr}"
        );
        assert_eq!(document(dir.path()), YAML_DOC);
    }

    /// The block is never erased, whatever the setting.
    #[test]
    fn fmt_leaves_the_block_under_skip() {
        let dir = format_setup(Some("skip"), "[\"empty\"]");
        let output = run_in(dir.path(), &["fmt"]);

        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr_of(&output));
        assert!(!stderr_of(&output).contains("no output"), "{}", stderr_of(&output));
        assert_eq!(document(dir.path()), YAML_DOC);
    }

    /// Past a formatter that failed, `warn` and `skip` try the next one in the list.
    #[test]
    fn fmt_falls_back_to_the_next_formatter_under_skip() {
        let dir = format_setup(Some("skip"), "[\"empty\", \"upper\"]");
        let output = run_in(dir.path(), &["fmt"]);

        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr_of(&output));
        assert_eq!(document(dir.path()), "# T\n\n```yaml\nKEY: VALUE\n```\n");
    }

    /// A built-in formatter in a `lint` slot answers by formatting and comparing.
    /// An empty answer is not "formatted" and not "not formatted"; it is a tool
    /// that failed, and `check` reports it like any other.
    #[test]
    fn check_reports_a_builtin_formatter_that_printed_nothing() {
        let config = "[code-block-tools]\nenabled = true\nnormalize-language = \"exact\"\n\n\
             [code-block-tools.languages]\nsh = { lint = [\"shfmt\"] }\n";
        let dir = setup(config, "# T\n\n```sh\necho hi\n```\n");
        install(dir.path(), &[("shfmt", EMPTY)]);
        let output = run_in(dir.path(), &["check"]);

        let stdout = stdout_of(&output);
        assert!(
            stdout.contains("t.md:3:1: [code-block-tools]") && stdout.contains("no output"),
            "the empty output was not reported: {stdout}"
        );
        assert_eq!(output.status.code(), Some(1), "stdout: {stdout}");
    }
}

/// The lint cache and the state of the tools a cached result came from.
///
/// A code-block tool's verdict depends on which binary runs, not only on the
/// document and the config, so a cached result is only valid for the binaries
/// that produced it. These runs deliberately leave the cache on.
#[cfg(unix)]
mod cache_and_tool_state {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const JSON_DOC: &str = "# T\n\n```json\n{}\n```\n";

    fn setup_linter(extra_config: &str) -> TempDir {
        let config = format!(
            "[code-block-tools]\nenabled = true\nnormalize-language = \"exact\"\n{extra_config}\n\
             [code-block-tools.tools.linter]\ncommand = [\"linter\"]\nstdin = true\nstdout = true\n\n\
             [code-block-tools.languages]\njson = {{ lint = [\"linter\"] }}\n"
        );
        let dir = setup(&config, JSON_DOC);
        fs::create_dir(dir.path().join("bin")).unwrap();
        dir
    }

    fn install_linter(dir: &Path, script: &str) {
        let path = dir.join("bin").join("linter");
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn check_cached(dir: &Path) -> Output {
        let path = format!("{}:{}", dir.join("bin").display(), std::env::var("PATH").unwrap());
        Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir)
            .env("PATH", path)
            .args(["check", "t.md"])
            .output()
            .unwrap()
    }

    #[test]
    fn a_tool_installed_after_a_cached_run_is_run() {
        let dir = setup_linter("");
        let first = check_cached(dir.path());
        assert_eq!(first.status.code(), Some(0), "{}", stdout_of(&first));

        install_linter(dir.path(), "#!/bin/sh\necho 'error: from the linter' >&2\nexit 1\n");
        let second = check_cached(dir.path());

        let stdout = stdout_of(&second);
        assert!(
            stdout.contains("t.md:3:1: [linter] error: from the linter"),
            "the result from before the tool was installed was replayed: {stdout}"
        );
    }

    #[test]
    fn a_replaced_tool_binary_is_run_again() {
        let dir = setup_linter("");
        install_linter(dir.path(), "#!/bin/sh\ncat >/dev/null\nexit 0\n");
        let first = check_cached(dir.path());
        assert_eq!(first.status.code(), Some(0), "{}", stdout_of(&first));

        install_linter(
            dir.path(),
            "#!/bin/sh\necho 'error: from the new version of the linter' >&2\nexit 1\n",
        );
        let second = check_cached(dir.path());

        let stdout = stdout_of(&second);
        assert!(
            stdout.contains("error: from the new version of the linter"),
            "the old binary's result was replayed: {stdout}"
        );
    }

    /// A tool that could not run said nothing about the block. Under `skip` the
    /// run is clean, but that is not a verdict worth keeping: the next run has to
    /// try the tool again, even though nothing about the binary changed.
    #[test]
    fn a_result_in_which_a_tool_failed_is_not_cached() {
        let dir = setup_linter("on-error = \"skip\"\ntimeout = 300\n");
        // Hangs while the marker exists, reports a finding once it is gone.
        install_linter(
            dir.path(),
            "#!/bin/sh\ncat >/dev/null\nif [ -e hang ]; then sleep 10; fi\n\
             echo 'error: from the linter' >&2\nexit 1\n",
        );
        fs::write(dir.path().join("hang"), "").unwrap();
        let first = check_cached(dir.path());
        assert_eq!(first.status.code(), Some(0), "{}", stdout_of(&first));

        fs::remove_file(dir.path().join("hang")).unwrap();
        let second = check_cached(dir.path());

        let stdout = stdout_of(&second);
        assert!(
            stdout.contains("t.md:3:1: [linter] error: from the linter"),
            "the result of the run in which the tool timed out was replayed: {stdout}"
        );
    }

    /// The positive control: with nothing changed, the second run is served from
    /// the cache, so the tests above are about invalidation and not a disabled
    /// cache.
    #[test]
    fn an_unchanged_tool_is_answered_from_the_cache() {
        let dir = setup_linter("");
        let counter = dir.path().join("runs");
        install_linter(
            dir.path(),
            &format!(
                "#!/bin/sh\ncat >/dev/null\necho run >> '{}'\necho 'error: from the linter' >&2\nexit 1\n",
                counter.display()
            ),
        );
        let first = check_cached(dir.path());
        let second = check_cached(dir.path());

        for output in [&first, &second] {
            assert!(
                stdout_of(output).contains("t.md:3:1: [linter] error: from the linter"),
                "{}",
                stdout_of(output)
            );
        }
        assert_eq!(
            fs::read_to_string(&counter).unwrap().lines().count(),
            1,
            "the tool ran again although nothing changed"
        );
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

#[cfg(unix)]
mod native_format_checks {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn project(tool: &str, version: &str, check: &str, error_policy: &str, clean: bool) -> TempDir {
        let language = if tool == "djlint" { "html" } else { "shell" };
        let id = if tool == "djlint" {
            "djlint:html:format-check"
        } else {
            "shuck:format-check"
        };
        let code = if clean { "formatted" } else { "unformatted" };
        let config = format!(
            "[code-block-tools]\nenabled = true\nnormalize-language = 'exact'\non-error = '{error_policy}'\n[code-block-tools.languages]\n{language} = {{ lint = ['{id}'] }}\n"
        );
        let dir = setup(&config, &format!("# T\n\n```{language}\n{code}\n```\n"));
        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let script = bin.join(tool);
        fs::write(&script, format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> calls\ncase \"$*\" in\n  --version) printf '%s\\n' '{version}'; exit 0;;\n  *--check*) cat >/dev/null; {check};;\n  *) cat >/dev/null; printf 'formatted\\n'; exit 0;;\nesac\n")).unwrap();
        fs::set_permissions(script, fs::Permissions::from_mode(0o755)).unwrap();
        dir
    }

    fn check(dir: &Path) -> Output {
        let mut paths = vec![dir.join("bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .args(["check", "--no-cache", "t.md"])
            .output()
            .unwrap()
    }

    #[test]
    fn modern_djlint_and_shuck_use_native_stdin_checks_without_rewriting() {
        for (tool, version, dirty) in [
            ("djlint", "djlint, version 1.46.4", "printf 'formatted\\n'; exit 1"),
            ("shuck", "shuck 0.2.3", "exit 1"),
        ] {
            for clean in [false, true] {
                let dir = project(tool, version, if clean { "exit 0" } else { dirty }, "fail", clean);
                let before = fs::read(dir.path().join("t.md")).unwrap();
                let output = check(dir.path());
                assert_eq!(output.status.code(), Some(if clean { 0 } else { 1 }), "{output:?}");
                assert_eq!(
                    stdout_of(&output).contains("Code block is not formatted"),
                    !clean,
                    "{output:?}"
                );
                assert_eq!(fs::read(dir.path().join("t.md")).unwrap(), before);
                let calls = fs::read_to_string(dir.path().join("calls")).unwrap();
                assert!(calls.contains("--check"), "{calls}");
                assert!(!calls.contains("--reformat"), "{calls}");
            }
        }
    }

    #[test]
    fn old_unknown_and_prerelease_versions_keep_comparison() {
        for (tool, version) in [
            ("djlint", "djlint, version 1.39.4"),
            ("djlint", "unknown"),
            ("djlint", "djlint, version 1.46.4rc1"),
            ("shuck", "shuck 0.1.0"),
            ("shuck", "unknown"),
        ] {
            let dir = project(tool, version, "exit 0", "fail", false);
            let output = check(dir.path());
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(stdout_of(&output).contains("Code block is not formatted"));
            let calls = fs::read_to_string(dir.path().join("calls")).unwrap();
            assert!(!calls.contains("--check"), "{calls}");
        }
    }

    #[test]
    fn a_native_check_crash_remains_a_tool_error_under_each_policy() {
        for tool in ["djlint", "shuck"] {
            for policy in ["fail", "warn", "skip"] {
                let version = if tool == "djlint" {
                    "djlint, version 1.46.4"
                } else {
                    "shuck 0.2.3"
                };
                let dir = project(
                    tool,
                    version,
                    "printf 'partial output\\n'; printf 'internal formatter crash\\n' >&2; exit 1",
                    policy,
                    false,
                );
                let before = fs::read(dir.path().join("t.md")).unwrap();
                let output = check(dir.path());
                assert!(
                    !stdout_of(&output).contains("Code block is not formatted"),
                    "{output:?}"
                );
                let text = format!("{}{}", stdout_of(&output), String::from_utf8_lossy(&output.stderr));
                if policy != "skip" {
                    assert!(text.contains("internal formatter crash"), "{text}");
                }
                assert_eq!(output.status.success(), matches!(policy, "warn" | "skip"), "{output:?}");
                assert_eq!(fs::read(dir.path().join("t.md")).unwrap(), before);
            }
        }
    }

    #[test]
    fn parse_errors_and_ambiguous_native_exits_are_errors() {
        for (tool, check_script) in [
            ("shuck", "printf '<stdin>:1:3: parse error expected command\\n'; exit 2"),
            ("shuck", "printf 'unexpected report\\n'; exit 1"),
            ("djlint", "printf 'unformatted\\n'; exit 1"),
            ("djlint", "exit 1"),
        ] {
            let version = if tool == "djlint" {
                "djlint, version 1.46.4"
            } else {
                "shuck 0.2.3"
            };
            let dir = project(tool, version, check_script, "fail", false);
            let output = check(dir.path());
            assert!(!output.status.success(), "{output:?}");
            assert!(
                !stdout_of(&output).contains("Code block is not formatted"),
                "{output:?}"
            );
            assert!(stdout_of(&output).contains("Exit code"), "{output:?}");
        }
    }

    #[test]
    fn native_version_probe_is_cached_across_files() {
        let dir = project("shuck", "shuck 0.2.2", "exit 0", "fail", true);
        fs::copy(dir.path().join("t.md"), dir.path().join("other.md")).unwrap();
        let mut paths = vec![dir.path().join("bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir.path())
            .env("PATH", std::env::join_paths(paths).unwrap())
            .args(["check", "--no-cache", "t.md", "other.md"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let calls = fs::read_to_string(dir.path().join("calls")).unwrap();
        assert_eq!(calls.lines().filter(|line| *line == "--version").count(), 1, "{calls}");
        assert_eq!(
            calls.lines().filter(|line| line.contains("--check")).count(),
            2,
            "{calls}"
        );
    }

    #[test]
    fn a_timed_out_version_probe_falls_back_to_bounded_comparison() {
        let dir = project("shuck", "shuck 0.2.2", "exit 0", "fail", false);
        let script = dir.path().join("bin/shuck");
        let text = fs::read_to_string(&script)
            .unwrap()
            .replace("printf '%s\\n' 'shuck 0.2.2'; exit 0", "exec /bin/sleep 30");
        assert!(text.contains("exec /bin/sleep 30"));
        fs::write(script, text).unwrap();
        let config = dir.path().join(".rumdl.toml");
        let text = fs::read_to_string(&config)
            .unwrap()
            .replace("enabled = true", "enabled = true\ntimeout = 1000");
        fs::write(config, text).unwrap();
        let started = std::time::Instant::now();
        let output = check(dir.path());
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(stdout_of(&output).contains("Code block is not formatted"), "{output:?}");
        let calls = fs::read_to_string(dir.path().join("calls")).unwrap();
        assert_eq!(calls.lines().filter(|line| *line == "--version").count(), 1, "{calls}");
        assert!(!calls.contains("--check"), "{calls}");
    }

    #[test]
    fn a_custom_override_of_the_native_id_keeps_its_command() {
        let dir = project("djlint", "djlint, version 1.46.4", "exit 0", "fail", true);
        let config = dir.path().join(".rumdl.toml");
        let mut text = fs::read_to_string(&config).unwrap();
        text.push_str("\n[code-block-tools.tools.'djlint:html:format-check']\ncommand = ['djlint', 'custom-mode']\nstdin = true\nstdout = true\n");
        fs::write(config, text).unwrap();
        let _ = check(dir.path());
        let calls = fs::read_to_string(dir.path().join("calls")).unwrap();
        assert_eq!(calls.trim(), "custom-mode");
    }
}
