//! When more than one rumdl-native config file lives in the same directory, the
//! lower-precedence file is silently ignored. These tests pin the user-facing
//! warning that surfaces that collision, while resolution stays unchanged (the dot
//! file still wins, matching Ruff).

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

/// `rumdl check` warns when both `.rumdl.toml` and `rumdl.toml` exist in the same
/// directory, naming the winner and the shadowed file.
#[test]
fn check_warns_when_dot_and_non_dot_config_coexist() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "line-length = 11\n").unwrap();
    fs::write(dir.path().join("rumdl.toml"), "line-length = 22\n").unwrap();
    fs::write(dir.path().join("test.md"), "# Title\n\nSome short body text here.\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("multiple rumdl config files"),
        "expected a shadowed-config warning on stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains(".rumdl.toml") && stderr.contains("rumdl.toml"),
        "warning should name both the winner and the shadowed file, got:\n{stderr}"
    );
}

/// The dot file still wins: resolution is unchanged, only a warning is added.
#[test]
fn check_still_resolves_dot_config_as_winner() {
    let dir = tempdir().unwrap();
    // `.rumdl.toml` sets a tiny line length; `rumdl.toml` a large one. If the dot
    // file wins, the long line is flagged against length 11.
    fs::write(dir.path().join(".rumdl.toml"), "line-length = 11\n").unwrap();
    fs::write(dir.path().join("rumdl.toml"), "line-length = 200\n").unwrap();
    fs::write(
        dir.path().join("test.md"),
        "# Title\n\nThis line is definitely longer than eleven characters.\n",
    )
    .unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("MD013"),
        "the dot config (line-length = 11) must win, flagging MD013, got:\n{stdout}"
    );
}

/// A single config file must NOT trigger the shadow warning.
#[test]
fn check_does_not_warn_for_a_single_config() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("rumdl.toml"), "line-length = 80\n").unwrap();
    fs::write(dir.path().join("test.md"), "# Title\n\nShort body.\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("multiple rumdl config files"),
        "a single config must not warn, got:\n{stderr}"
    );
}

/// `--silent` suppresses the shadow warning, like other config warnings.
#[test]
fn check_silent_suppresses_the_shadow_warning() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "line-length = 80\n").unwrap();
    fs::write(dir.path().join("rumdl.toml"), "line-length = 80\n").unwrap();
    fs::write(dir.path().join("test.md"), "# Title\n\nShort body.\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "--silent", "test.md"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("multiple rumdl config files"),
        "--silent must suppress the shadow warning, got:\n{stderr}"
    );
}

/// `rumdl config` also surfaces the shadow warning so `config file` / `config`
/// users see which file is winning.
#[test]
fn config_command_warns_when_configs_shadow() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "line-length = 11\n").unwrap();
    fs::write(dir.path().join("rumdl.toml"), "line-length = 22\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["config"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("multiple rumdl config files"),
        "`rumdl config` should warn about shadowed configs on stderr, got:\n{stderr}"
    );
}

/// `rumdl config file` keeps stdout to the winning path (so it stays pipeable) and
/// emits the shadow warning on stderr.
#[test]
fn config_file_warns_on_stderr_keeping_stdout_clean() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "line-length = 11\n").unwrap();
    fs::write(dir.path().join("rumdl.toml"), "line-length = 22\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["config", "file"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // stdout must be exactly the winning path (one line, no warning bleed) so it
    // stays pipeable.
    let stdout_lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        stdout_lines.len(),
        1,
        "stdout should be a single path line, got:\n{stdout}"
    );
    assert!(
        stdout_lines[0].ends_with(".rumdl.toml"),
        "stdout should be the winning `.rumdl.toml` path, got:\n{stdout}"
    );
    assert!(
        stderr.contains("multiple rumdl config files"),
        "stderr should carry the shadow warning, got:\n{stderr}"
    );
}

/// A `pyproject.toml` with `[tool.rumdl]` alongside a `.rumdl.toml` is a real
/// collision: the dedicated file wins and pyproject is named as shadowed.
#[test]
fn check_warns_when_pyproject_shadows_dedicated_config() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "line-length = 11\n").unwrap();
    fs::write(dir.path().join("pyproject.toml"), "[tool.rumdl]\nline-length = 80\n").unwrap();
    fs::write(dir.path().join("test.md"), "# Title\n\nShort body.\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("multiple rumdl config files"),
        "a [tool.rumdl] pyproject next to .rumdl.toml should warn, got:\n{stderr}"
    );
    assert!(
        stderr.contains("pyproject.toml"),
        "the warning should name the shadowed pyproject.toml, got:\n{stderr}"
    );
}

/// A `pyproject.toml` WITHOUT a `[tool.rumdl]` section is not a rumdl config source,
/// so it must NOT trigger a shadow warning next to a `.rumdl.toml`.
#[test]
fn check_does_not_warn_for_pyproject_without_tool_rumdl() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "line-length = 80\n").unwrap();
    fs::write(dir.path().join("pyproject.toml"), "[tool.black]\nline-length = 88\n").unwrap();
    fs::write(dir.path().join("test.md"), "# Title\n\nShort body.\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("multiple rumdl config files"),
        "a pyproject.toml without [tool.rumdl] must not count as a rumdl config, got:\n{stderr}"
    );
}

/// An explicit `--config <path>` is standalone: discovery is skipped, so no shadow
/// warning fires even when sibling configs exist on disk.
#[test]
fn explicit_config_does_not_trigger_shadow_warning() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "line-length = 11\n").unwrap();
    fs::write(dir.path().join("rumdl.toml"), "line-length = 22\n").unwrap();
    fs::write(dir.path().join("test.md"), "# Title\n\nShort body.\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "--config", "rumdl.toml", "test.md"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("multiple rumdl config files"),
        "explicit --config is standalone and must not warn about shadowing, got:\n{stderr}"
    );
}

/// The warning comes from config loading, which happens before the lint cache is
/// consulted, so it must still appear on a second run that hits a warm cache - not
/// only on the first cold run. Guards against the warning being hidden behind
/// cached lint results.
#[test]
fn check_warns_even_with_a_warm_cache() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "line-length = 80\n").unwrap();
    fs::write(dir.path().join("rumdl.toml"), "line-length = 80\n").unwrap();
    fs::write(dir.path().join("test.md"), "# Title\n\nShort body.\n").unwrap();

    // Caching is ON (no --no-cache); an isolated cache dir keeps the run hermetic.
    let cache_dir = dir.path().join("cache");
    let run = || {
        Command::new(rumdl_bin())
            .current_dir(dir.path())
            .env("RUMDL_CACHE_DIR", &cache_dir)
            .args(["check", "test.md"])
            .output()
            .unwrap()
    };

    let cold = run();
    let warm = run();
    let cold_err = String::from_utf8_lossy(&cold.stderr);
    let warm_err = String::from_utf8_lossy(&warm.stderr);

    assert!(
        cold_err.contains("multiple rumdl config files"),
        "cold run should warn, got:\n{cold_err}"
    );
    assert!(
        warm_err.contains("multiple rumdl config files"),
        "warm-cache run must still warn (warning precedes the cache lookup), got:\n{warm_err}"
    );
}

/// Discovery walks upward: a collision in a parent directory must warn when running
/// from a subdirectory that has no config of its own.
#[test]
fn check_warns_for_shadow_in_a_parent_directory() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap(); // project-root boundary
    fs::write(dir.path().join(".rumdl.toml"), "line-length = 80\n").unwrap();
    fs::write(dir.path().join("rumdl.toml"), "line-length = 80\n").unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("test.md"), "# Title\n\nShort body.\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(&sub)
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("multiple rumdl config files"),
        "a collision in the parent directory should warn from a subdir, got:\n{stderr}"
    );
}
