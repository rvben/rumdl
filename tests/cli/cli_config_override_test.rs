//! Tests for inline `--config KEY=VALUE` overrides on the CLI.
//!
//! Mirrors Ruff's `--config` flag behavior: the same flag accepts either a
//! file path or a TOML `KEY = VALUE` snippet that overrides config options
//! without touching the config file on disk.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

/// Markdown sample whose body line is >20 chars but <200 chars. The leading
/// H1 satisfies MD041 so test failures only ever come from MD013.
const LONG_LINE: &str =
    "# Heading\n\nThis line is intentionally longer than twenty characters but shorter than two hundred.\n";

/// `--config 'MD013.line_length=20'` must shrink the limit even with no config file.
#[test]
fn inline_override_lowers_md013_line_length() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.md");
    fs::write(&file, LONG_LINE).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "--config", "MD013.line_length=20", "a.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("MD013") || stderr.contains("MD013"),
        "expected MD013 violation when line_length=20 via --config, got:\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// `--config 'MD013.line_length=200'` must raise the limit so a long-but-not-huge line passes.
#[test]
fn inline_override_raises_md013_line_length() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.md");
    fs::write(&file, LONG_LINE).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "--config", "MD013.line_length=200", "a.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "rumdl should exit 0 (no violations) with line_length=200 override, got code {:?}\nstdout: {stdout}\nstderr: {stderr}",
        output.status.code()
    );
    assert!(
        !stdout.contains("MD013"),
        "did not expect MD013 violation when line_length=200 via --config, got:\n{stdout}"
    );
}

/// CLI `--config` overrides must beat values set in `.rumdl.toml`.
#[test]
fn inline_override_beats_config_file() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), "[MD013]\nline-length = 200\n").unwrap();
    fs::write(dir.path().join("a.md"), LONG_LINE).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--config", "MD013.line_length=20", "a.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("MD013"),
        "CLI override line_length=20 should beat .rumdl.toml line-length=200, got:\n{stdout}"
    );
}

/// Multiple `--config` entries must combine: one file path plus inline overrides.
#[test]
fn inline_override_combines_with_config_file_path() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("custom.toml");
    fs::write(&cfg, "[MD013]\nline-length = 200\n").unwrap();
    fs::write(dir.path().join("a.md"), LONG_LINE).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--config",
            "MD013.line_length=20",
            "a.md",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("MD013"),
        "inline override should win over the file path passed in the same --config series, got:\n{stdout}"
    );
}

/// Two inline overrides for different rules must both apply.
#[test]
fn multiple_inline_overrides_apply() {
    let dir = tempdir().unwrap();
    // File that only triggers MD013 if line_length is small AND only triggers MD041
    // if first-line-h1 is enforced. We craft content that fails both when overrides apply.
    let content = "Not a heading and this line is moderately long for the test\n";
    fs::write(dir.path().join("a.md"), content).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args([
            "check",
            "--no-config",
            "--config",
            "MD013.line_length=10",
            "--config",
            "MD041.level=1",
            "a.md",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("MD013"),
        "MD013 should fire after override, got:\n{stdout}"
    );
    assert!(
        stdout.contains("MD041"),
        "MD041 should fire after override, got:\n{stdout}"
    );
}

/// `--config 'MD013.reflow=true'` must enable reflow without a config file
/// (this is the exact use case from discussion #592).
#[test]
fn inline_override_enables_md013_reflow() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.md");
    let content =
        "This is a very long line that definitely exceeds forty characters and should be reflowed when reflow is on.\n";
    fs::write(&file, content).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args([
            "check",
            "--fix",
            "--no-config",
            "--config",
            "MD013.line_length=40",
            "--config",
            "MD013.reflow=true",
            "a.md",
        ])
        .output()
        .unwrap();

    let exit_code = output.status.code().unwrap_or(-1);
    assert!(
        exit_code == 0 || exit_code == 1,
        "expected exit 0 or 1, got {exit_code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let fixed = fs::read_to_string(&file).unwrap();
    let max = fixed.lines().map(str::len).max().unwrap_or(0);
    assert!(
        max <= 60,
        "reflow should have wrapped lines (max line was {max} chars):\n{fixed}"
    );
    let original_max = content.lines().map(str::len).max().unwrap_or(0);
    assert!(
        max < original_max,
        "post-fix max line ({max}) should be shorter than original ({original_max})"
    );
}

/// Invalid TOML in `--config` must produce a clean error, not a panic or silent ignore.
#[test]
fn invalid_inline_override_errors_clearly() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "# H\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "--config", "this is not valid toml = =", "a.md"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit for invalid --config value"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("toml")
            || stderr.to_lowercase().contains("must either be a path")
            || stderr.to_lowercase().contains("key = value")
            || stderr.to_lowercase().contains("key=value"),
        "stderr should explain the --config value is neither a path nor inline TOML, got:\n{stderr}"
    );
}

/// Lowercase rule IDs should normalize to their canonical form.
#[test]
fn inline_override_accepts_lowercase_rule_id() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.md");
    fs::write(&file, LONG_LINE).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "--config", "md013.line_length=20", "a.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("MD013"),
        "lowercase md013 should normalize to MD013, got:\n{stdout}"
    );
}

/// Two `--config` file paths should error (Ruff parity).
#[test]
fn two_file_paths_error() {
    let dir = tempdir().unwrap();
    let cfg1 = dir.path().join("a.toml");
    let cfg2 = dir.path().join("b.toml");
    fs::write(&cfg1, "").unwrap();
    fs::write(&cfg2, "").unwrap();
    fs::write(dir.path().join("x.md"), "# H\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args([
            "check",
            "--config",
            cfg1.to_str().unwrap(),
            "--config",
            cfg2.to_str().unwrap(),
            "x.md",
        ])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit when two file paths are passed via --config"
    );
}

/// Top-level `line-length` should set the global option, not be silently dropped.
/// MD013 falls back to the global `line-length` when no rule-level value is set.
#[test]
fn inline_override_sets_global_line_length() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.md");
    fs::write(&file, LONG_LINE).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "--config", "line-length=20", "a.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("MD013") || stderr.contains("MD013"),
        "global line-length=20 should propagate to MD013 and trigger violation, got:\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// Explicit `[global]` table syntax should also work, mirroring the file format.
#[test]
fn inline_override_explicit_global_table() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.md");
    fs::write(&file, LONG_LINE).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "--config", "global.line-length=20", "a.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("MD013") || stderr.contains("MD013"),
        "[global] line-length=20 should trigger MD013, got:\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// Setting `disable` at the top level should turn rules off — MD013 disabled means no violation.
#[test]
fn inline_override_global_disable() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.md");
    // Line >80 chars with spaces so MD013 considers it wrappable and fires by default.
    let content = format!("# H\n\n{}\n", vec!["word"; 30].join(" "));
    fs::write(&file, content).unwrap();

    // Sanity check: without the disable override, MD013 should fire on this content.
    let baseline = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "a.md"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&baseline.stdout).contains("MD013"),
        "test premise broken: MD013 should fire on long sentence by default"
    );

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "--config", r#"disable=["MD013"]"#, "a.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "rumdl should exit 0 when MD013 disabled via --config disable=[\"MD013\"], got code {:?}\nstdout: {stdout}",
        output.status.code()
    );
    assert!(
        !stdout.contains("MD013"),
        "MD013 should be suppressed by global disable=[\"MD013\"], got:\n{stdout}"
    );
}

/// String-typed rule option (e.g. `MD003.style`) must round-trip correctly.
#[test]
fn inline_override_string_value() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a.md");
    // Mixed atx and setext: MD003 with style="atx" should flag the setext heading.
    let content = "# ATX\n\nSetext\n======\n\nMore text.\n";
    fs::write(&file, content).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "--config", r#"MD003.style="atx""#, "a.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("MD003"),
        "MD003 with style=\"atx\" should flag setext heading, got:\n{stdout}"
    );
}

/// Unknown rule ID via --config must surface a config warning, not silently apply.
#[test]
fn inline_override_unknown_rule_warns() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "# H\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "--config", "MD9999.foo=1", "a.md"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("unknown rule") && stderr.contains("MD9999"),
        "expected 'Unknown rule' warning for MD9999, got:\nstderr: {stderr}"
    );
}

/// Unknown option key for a real rule must produce a per-rule warning.
#[test]
fn inline_override_unknown_option_warns() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "# H\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "--config", "MD013.no_such_option=1", "a.md"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("unknown option") && stderr.contains("MD013"),
        "expected 'Unknown option for rule MD013' warning, got:\nstderr: {stderr}"
    );
}

/// Unknown TOP-LEVEL key (not a rule, not a known global) must warn as global.
#[test]
fn inline_override_unknown_global_warns() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "# H\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-config", "--config", "totally_bogus_key=1", "a.md"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("unknown global option") && stderr.contains("totally_bogus_key"),
        "expected 'Unknown global option' warning for top-level key, got:\nstderr: {stderr}"
    );
}

/// Issue #841: MD022 accepts a six-entry array, so validation must not infer the
/// scalar default's integer type as the only valid representation.
#[test]
fn md022_per_level_inline_override_is_accepted_and_applied() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "# Title\n\n## Section\n\nBody.\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args([
            "check",
            "--no-config",
            "--no-cache",
            "--deny-config-warnings",
            "--config",
            "MD022.lines-above=[1,3,1,1,1,1]",
            "a.md",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the valid config should run and report the MD022 finding, not fail validation:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("[MD022] Expected 3 blank lines above heading"),
        "the h2-specific value should be applied:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !stderr.contains("[config warning]"),
        "a documented MD022 array must not produce a config warning:\n{stderr}"
    );
}

/// The file-backed form follows the same validation path and supports `-1` in
/// either per-level array, as documented by MD022.
#[test]
fn md022_per_level_config_file_is_accepted_and_applied() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("custom.toml"),
        "[MD022]\nlines-above = [-1, 3, 1, 1, 1, 1]\nlines-below = [1, 1, 3, 1, 1, 1]\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("a.md"),
        "# Title\n\n## Section\n\n### Subsection\n\nBody.\n",
    )
    .unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args([
            "check",
            "--no-cache",
            "--deny-config-warnings",
            "--config",
            "custom.toml",
            "a.md",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the valid config should run and report findings, not fail validation:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("[MD022] Expected 3 blank lines above heading"),
        "the h2-specific lines-above value should be applied:\n{stdout}"
    );
    assert!(
        stdout.contains("[MD022] Expected 3 blank lines below heading"),
        "the h3-specific lines-below value should be applied:\n{stdout}"
    );
    assert!(
        !stderr.contains("[config warning]"),
        "documented MD022 arrays must not produce config warnings:\n{stderr}"
    );
}
