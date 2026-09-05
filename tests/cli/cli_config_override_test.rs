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

// ---------------------------------------------------------------------------
// Non-rule sections (`code-block-tools`, `per-file-ignores`, `per-file-flavor`)
//
// These live in their own fields on the config rather than in the rule map. An
// inline override used to be looked up as a rule name, so the whole section was
// filed as an unknown rule and the value was dropped with a misleading warning.
//
// Code block tools are exercised through the built-in `rumdl` tool for embedded
// markdown, so these tests need no external binary and behave the same on every
// platform. The embedded finding (`MD018` inside the fenced block) says whether
// the section is active; the outer document's `MD009` is the control saying the
// run happened at all.
// ---------------------------------------------------------------------------

/// A document whose fenced `markdown` block holds a violation, plus one of its
/// own outside the block.
const EMBEDDED_BLOCK_DOC: &str = "# Outer   \n\n```markdown\n#Embedded heading\n```\n";

/// Config enabling code block tools with the built-in embedded markdown linter.
const EMBEDDED_TOOLS_CONFIG: &str = "[global]\ndisable = [\"MD041\", \"MD040\"]\n\n[code-block-tools]\nenabled = true\n\n[code-block-tools.languages]\nmarkdown = { lint = [\"rumdl\"] }\n";

/// Run `check` in a temp dir holding `config` and [`EMBEDDED_BLOCK_DOC`],
/// returning `(stdout, stderr)`.
fn check_embedded_doc(config: &str, extra_args: &[&str]) -> (String, String) {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    fs::write(dir.path().join("doc.md"), EMBEDDED_BLOCK_DOC).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache"])
        .args(extra_args)
        .arg("doc.md")
        .output()
        .unwrap();

    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Without an override the configured tools run: the control every test below
/// is read against.
#[test]
fn code_block_tools_run_when_configured() {
    let (stdout, _) = check_embedded_doc(EMBEDDED_TOOLS_CONFIG, &[]);
    assert!(
        stdout.contains("MD018"),
        "the configured embedded markdown linter should report inside the block:\n{stdout}"
    );
    assert!(
        stdout.contains("MD009"),
        "the document's own rules should report too:\n{stdout}"
    );
}

/// `--config 'code-block-tools.enabled = false'` turns the tools off while the
/// rest of the configuration keeps applying.
#[test]
fn code_block_tools_section_override_disables_tools() {
    let (stdout, stderr) = check_embedded_doc(EMBEDDED_TOOLS_CONFIG, &["--config", "code-block-tools.enabled = false"]);
    assert!(
        !stdout.contains("MD018"),
        "the override should stop the tools from running:\n{stdout}"
    );
    assert!(
        stdout.contains("MD009"),
        "the document's own rules must still run:\n{stdout}"
    );
    assert!(
        !stderr.contains("Unknown rule"),
        "a known config section must not be reported as an unknown rule:\n{stderr}"
    );
}

/// The section is patched key by key, so overriding one setting keeps the
/// configured languages and tools. Without that, naming any key at all would
/// silently empty the tool configuration.
#[test]
fn code_block_tools_section_override_keeps_configured_languages() {
    let (stdout, _) = check_embedded_doc(
        EMBEDDED_TOOLS_CONFIG,
        &["--config", "code-block-tools.on-error = \"warn\""],
    );
    assert!(
        stdout.contains("MD018"),
        "overriding an unrelated key must leave the configured languages in place:\n{stdout}"
    );
}

/// The override works in the other direction too: a section switched off in
/// config can be switched on for one run.
#[test]
fn code_block_tools_section_override_can_enable() {
    let disabled = EMBEDDED_TOOLS_CONFIG.replace("enabled = true", "enabled = false");
    let (control, _) = check_embedded_doc(&disabled, &[]);
    assert!(
        !control.contains("MD018"),
        "control: the section is off in config, so nothing should run:\n{control}"
    );

    let (stdout, _) = check_embedded_doc(&disabled, &["--config", "code-block-tools.enabled = true"]);
    assert!(
        stdout.contains("MD018"),
        "the override should switch the configured tools on:\n{stdout}"
    );
}

/// The key may be written in either spelling; config keys normalize to
/// lowercase kebab-case before they are looked up.
#[test]
fn code_block_tools_section_override_accepts_snake_case() {
    let (stdout, stderr) = check_embedded_doc(EMBEDDED_TOOLS_CONFIG, &["--config", "code_block_tools.enabled = false"]);
    assert!(
        !stdout.contains("MD018"),
        "the snake_case spelling should reach the same section:\n{stdout}"
    );
    assert!(
        !stderr.contains("Unknown rule"),
        "the snake_case spelling must not warn as an unknown rule:\n{stderr}"
    );
}

/// `--no-code-block-tools` is the named form of the same override.
#[test]
fn no_code_block_tools_flag_skips_tools() {
    let (stdout, stderr) = check_embedded_doc(EMBEDDED_TOOLS_CONFIG, &["--no-code-block-tools"]);
    assert!(
        !stdout.contains("MD018"),
        "the flag should stop the tools from running:\n{stdout}"
    );
    assert!(
        stdout.contains("MD009"),
        "the flag must not disturb the rest of the configuration:\n{stdout}"
    );
    assert!(stderr.is_empty(), "the flag should produce no warnings:\n{stderr}");
}

/// Two command-line routes reach the same setting. The named flag is the
/// explicit one, so it decides.
#[test]
fn no_code_block_tools_flag_wins_over_inline_enable() {
    let (stdout, _) = check_embedded_doc(
        EMBEDDED_TOOLS_CONFIG,
        &["--config", "code-block-tools.enabled = true", "--no-code-block-tools"],
    );
    assert!(
        !stdout.contains("MD018"),
        "the named flag should win over the inline override:\n{stdout}"
    );
}

/// `fmt` takes the flag too, and skipping the tools leaves the code block
/// exactly as written while the document's own fixes still apply.
#[test]
fn no_code_block_tools_flag_applies_to_fmt() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), EMBEDDED_TOOLS_CONFIG).unwrap();
    let doc = dir.path().join("doc.md");

    fs::write(&doc, EMBEDDED_BLOCK_DOC).unwrap();
    let control = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["fmt", "--no-cache", "doc.md"])
        .output()
        .unwrap();
    let formatted = fs::read_to_string(&doc).unwrap();
    assert!(
        formatted.contains("# Embedded heading"),
        "control: with the tools on, fmt formats the embedded block:\n{formatted}\nstderr: {}",
        String::from_utf8_lossy(&control.stderr)
    );

    fs::write(&doc, EMBEDDED_BLOCK_DOC).unwrap();
    Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["fmt", "--no-cache", "--no-code-block-tools", "doc.md"])
        .output()
        .unwrap();
    let skipped = fs::read_to_string(&doc).unwrap();
    assert!(
        skipped.contains("#Embedded heading"),
        "the flag should leave the code block as written:\n{skipped}"
    );
    assert!(
        skipped.starts_with("# Outer\n"),
        "the document's own fixes must still apply:\n{skipped}"
    );
}

/// `[per-file-ignores]` is reachable from the command line, and only the named
/// rule is dropped.
#[test]
fn per_file_ignores_section_override_applies() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("doc.md"), "#Title\n\nsome text   \n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args([
            "check",
            "--no-cache",
            "--no-config",
            "--config",
            "per-file-ignores.\"doc.md\" = [\"MD018\"]",
            "doc.md",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains("MD018"),
        "the ignored rule should not be reported for the matching file:\n{stdout}"
    );
    assert!(
        stdout.contains("MD009"),
        "every other rule must still report:\n{stdout}"
    );
    assert!(
        !stderr.contains("Unknown rule"),
        "a known config section must not be reported as an unknown rule:\n{stderr}"
    );
}

/// `[per-file-flavor]` is reachable too, and picking a flavor changes how the
/// file parses: MkDocs reads the indented block as admonition content, standard
/// Markdown reads it as an indented code block.
#[test]
fn per_file_flavor_section_override_applies() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("doc.md"), "# T\n\n!!! note\n\n    indented content\n").unwrap();

    let run = |extra: &[&str]| {
        let output = Command::new(rumdl_bin())
            .current_dir(dir.path())
            .args([
                "check",
                "--no-cache",
                "--no-config",
                "--enable",
                "MD046",
                "--config",
                "MD046.style = \"fenced\"",
            ])
            .args(extra)
            .arg("doc.md")
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    let control = run(&[]);
    assert!(
        control.contains("MD046"),
        "control: under standard Markdown the indented block is a code block:\n{control}"
    );

    let stdout = run(&["--config", "per-file-flavor.\"doc.md\" = \"mkdocs\""]);
    assert!(
        !stdout.contains("MD046"),
        "the flavor override should apply to the matching file:\n{stdout}"
    );
}

/// Run `check` on a trivial document with the given extra arguments, with no
/// `RUST_LOG` in the environment: a warning about what the user typed has to
/// reach them on the run they typed it on, without being asked for.
fn check_with_stderr(extra: &[&str]) -> (String, std::process::Output) {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("doc.md"), "# T\n").unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .env_remove("RUST_LOG")
        .args(["check", "--no-cache", "--no-config"])
        .args(extra)
        .arg("doc.md")
        .output()
        .unwrap();

    (String::from_utf8_lossy(&output.stderr).into_owned(), output)
}

/// An invalid flavor is reported rather than silently ignored, and the name of
/// the offending pattern is in the message.
#[test]
fn per_file_flavor_section_override_reports_invalid_flavor() {
    let (stderr, _) = check_with_stderr(&["--config", "per-file-flavor.\"doc.md\" = \"nonsense\""]);
    assert!(
        stderr.contains("[config warning]")
            && stderr.contains("invalid flavor 'nonsense'")
            && stderr.contains("doc.md"),
        "an unusable flavor name should be reported:\n{stderr}"
    );
}

/// A value of the wrong shape names the pattern it came from, so the message
/// says which of several overrides to fix.
#[test]
fn per_file_flavor_section_override_reports_a_value_that_is_not_a_name() {
    let (stderr, _) = check_with_stderr(&["--config", "per-file-flavor.\"doc.md\" = 3"]);
    assert!(
        stderr.contains("[config warning]") && stderr.contains("expected a flavor name") && stderr.contains("doc.md"),
        "a non-string flavor should be reported:\n{stderr}"
    );
}

/// The rules of a `per-file-ignores` pattern are an array; anything else is a
/// mistake worth a word rather than a silently skipped pattern.
#[test]
fn per_file_ignores_section_override_reports_a_value_that_is_not_an_array() {
    let (stderr, _) = check_with_stderr(&["--config", "per-file-ignores.\"doc.md\" = \"MD013\""]);
    assert!(
        stderr.contains("[config warning]")
            && stderr.contains("expected an array of rule names")
            && stderr.contains("doc.md"),
        "a non-array rule list should be reported:\n{stderr}"
    );
}

/// A value the section cannot hold is reported with the deserializer's own
/// message, the way a config file carrying the same mistake is.
#[test]
fn code_block_tools_section_override_reports_an_unusable_value() {
    let (stderr, _) = check_with_stderr(&["--config", "code-block-tools.enabled = \"nope\""]);
    assert!(
        stderr.contains("[config warning]")
            && stderr.contains("[code-block-tools]")
            && stderr.contains("expected a boolean"),
        "an unusable value should be reported:\n{stderr}"
    );

    let (clean, _) = check_with_stderr(&["--config", "code-block-tools.enabled = false"]);
    assert!(
        !clean.contains("[config warning]"),
        "control: a usable value warns about nothing:\n{clean}"
    );
}

/// The reference documents this message; it is the one a mistyped global value
/// produces, and it reaches stderr without `RUST_LOG`.
#[test]
fn global_override_reports_a_type_mismatch() {
    let (stderr, _) = check_with_stderr(&["--config", "line-length = \"huge\""]);
    assert!(
        stderr.contains("[config warning]") && stderr.contains("expected integer for global key 'line-length'"),
        "a mistyped global value should be reported:\n{stderr}"
    );
}

/// A warning about the command line is a config warning like any other, so a
/// run that refuses to proceed on one refuses to proceed on this.
#[test]
fn an_unusable_override_fails_deny_config_warnings() {
    let (_, output) = check_with_stderr(&["--deny-config-warnings", "--config", "line-length = \"huge\""]);
    assert!(
        !output.status.success(),
        "--deny-config-warnings should exit non-zero on an unusable override"
    );

    let (_, control) = check_with_stderr(&["--deny-config-warnings", "--config", "line-length = 120"]);
    assert!(
        control.status.success(),
        "control: a usable override leaves the run alone:\n{}",
        String::from_utf8_lossy(&control.stderr)
    );
}

/// The findings as `file rule` pairs, so a test can state exactly which rule
/// fired for which file rather than searching the whole output for a rule name
/// that may belong to another file.
fn findings(stdout: &str) -> std::collections::BTreeSet<String> {
    stdout
        .lines()
        .filter_map(|line| {
            let (file, rest) = line.split_once(':')?;
            let rule = rest.split_once('[')?.1.split_once(']')?.0;
            Some(format!("{} {rule}", file.replace('\\', "/")))
        })
        .collect()
}

/// The map is one setting's value, so an override replaces it whole rather
/// than patching the pattern it names into the configured map. This is what a
/// higher-precedence config file does with the same section, and what ruff's
/// `--config` does with `lint.per-file-ignores`.
#[test]
fn per_file_ignores_section_override_replaces_the_configured_map() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[per-file-ignores]\n\"a.md\" = [\"MD018\"]\n\"b.md\" = [\"MD009\"]\n",
    )
    .unwrap();
    fs::write(dir.path().join("a.md"), "#A\n\ntrailing   \n").unwrap();
    fs::write(dir.path().join("b.md"), "#B\n\ntrailing   \n").unwrap();

    let run = |extra: &[&str]| {
        let output = Command::new(rumdl_bin())
            .current_dir(dir.path())
            .args(["check", "--no-cache", "--enable", "MD009,MD018"])
            .args(extra)
            .arg(".")
            .output()
            .unwrap();
        findings(&String::from_utf8_lossy(&output.stdout))
    };

    assert_eq!(
        run(&[]),
        ["a.md MD009", "b.md MD018"].map(String::from).into_iter().collect(),
        "control: each file's configured pattern silences its own rule"
    );

    assert_eq!(
        run(&["--config", "per-file-ignores.\"a.md\" = [\"MD009\"]"]),
        ["a.md MD018", "b.md MD009", "b.md MD018"]
            .map(String::from)
            .into_iter()
            .collect(),
        "the override is the whole map: a.md takes the rules given here and b.md has no pattern left"
    );
}

/// The same for `[per-file-flavor]`, where replacing the map also settles the
/// ordering question the section carries: a file takes the flavor of the first
/// pattern it matches, and after an override the only patterns to match are the
/// ones it named. A broader configured pattern cannot answer first because it
/// is no longer there.
#[test]
fn per_file_flavor_section_override_replaces_the_configured_map() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    fs::write(
        dir.path().join(".rumdl.toml"),
        "[per-file-flavor]\n\"docs/*.md\" = \"mkdocs\"\n\n[MD046]\nstyle = \"fenced\"\n",
    )
    .unwrap();
    for name in ["one.md", "two.md"] {
        fs::write(
            dir.path().join("docs").join(name),
            "# T\n\n!!! note\n\n    indented content\n",
        )
        .unwrap();
    }

    let run = |extra: &[&str]| {
        let output = Command::new(rumdl_bin())
            .current_dir(dir.path())
            .args(["check", "--no-cache", "--enable", "MD046"])
            .args(extra)
            .arg(".")
            .output()
            .unwrap();
        findings(&String::from_utf8_lossy(&output.stdout))
    };

    assert!(
        run(&[]).is_empty(),
        "control: under the configured mkdocs flavor the indented block is admonition content, not a code block"
    );

    assert_eq!(
        run(&["--config", "per-file-flavor.\"docs/one.md\" = \"standard\""]),
        ["docs/one.md MD046", "docs/two.md MD046"]
            .map(String::from)
            .into_iter()
            .collect(),
        "the named file takes the flavor given here, and the configured pattern that covered the other file is gone"
    );
}

/// The mirror of `no_code_block_tools_flag_wins_over_inline_enable`: the docs
/// promise the named flags decide in both directions, so both directions are
/// pinned.
#[test]
fn only_code_block_tools_flag_wins_over_inline_disable() {
    let (stdout, _) = check_embedded_doc(
        EMBEDDED_TOOLS_CONFIG,
        &[
            "--config",
            "code-block-tools.enabled = false",
            "--only-code-block-tools",
        ],
    );
    assert!(
        stdout.contains("MD018"),
        "the named flag should win over the inline override:\n{stdout}"
    );
    assert!(
        !stdout.contains("MD009"),
        "only-mode runs the tools and no markdown rules:\n{stdout}"
    );
}
