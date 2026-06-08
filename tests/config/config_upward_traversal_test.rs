use std::fs;
// Only the Unix-gated symlink test below uses this.
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_config_upward_traversal() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create nested directory structure
    let nested_dir = project_dir.join("subdir").join("nested");
    fs::create_dir_all(&nested_dir).unwrap();

    // Create config at project root
    let config_content = r#"
[global]
line-length = 120
disable = ["MD013", "MD033"]
"#;
    let config_path = project_dir.join(".rumdl.toml");
    fs::write(&config_path, config_content).unwrap();

    // Create a test markdown file in nested directory
    let test_file = nested_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters but should not trigger MD013 due to parent config.\n").unwrap();

    // Run rumdl from nested directory
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md"])
        .current_dir(&nested_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();

    // MD013 should be disabled by parent config
    assert!(!stderr.contains("MD013"), "MD013 should be disabled by parent config");
    assert!(!stderr.contains("Line length"), "Line length warning should not appear");
}

#[test]
fn test_config_stops_at_git_boundary() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create nested directory structure with .git in middle
    let subdir = project_dir.join("subdir");
    let nested_dir = subdir.join("nested");
    fs::create_dir_all(&nested_dir).unwrap();

    // Create .git directory in subdir (boundary)
    fs::create_dir(subdir.join(".git")).unwrap();

    // Create config at project root (should not be found)
    let config_content = r#"
[global]
disable = ["MD013"]
"#;
    fs::write(project_dir.join(".rumdl.toml"), config_content).unwrap();

    // Create a test markdown file
    let test_file = nested_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters and should trigger MD013 because config is not found.\n").unwrap();

    // Run rumdl from nested directory
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md"])
        .current_dir(&nested_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Check both stdout and stderr for the MD013 message
    let combined = format!("{stdout}{stderr}");

    // MD013 should trigger because config is not found (stopped at .git)
    assert!(
        combined.contains("MD013") || combined.contains("Line length"),
        "MD013 should trigger because traversal stopped at .git boundary. Output: {combined}"
    );
}

#[test]
fn test_isolated_flag_ignores_all_configs() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create config that disables MD013
    let config_content = r#"
[global]
disable = ["MD013"]
"#;
    fs::write(project_dir.join(".rumdl.toml"), config_content).unwrap();

    // Create a test markdown file
    let test_file = project_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters and should trigger MD013 when using --isolated flag.\n").unwrap();

    // Run with --isolated flag
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md", "--isolated"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Check both stdout and stderr for the MD013 message
    let combined = format!("{stdout}{stderr}");

    // MD013 should trigger despite config because --isolated is used
    assert!(
        combined.contains("MD013") || combined.contains("Line length"),
        "MD013 should trigger with --isolated flag. Output: {combined}"
    );
}

#[test]
fn test_config_precedence_order() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create both pyproject.toml and .rumdl.toml
    let pyproject_content = r#"
[tool.rumdl]
line-length = 120
"#;
    fs::write(project_dir.join("pyproject.toml"), pyproject_content).unwrap();

    let rumdl_content = r#"
[global]
line-length = 100
"#;
    fs::write(project_dir.join(".rumdl.toml"), rumdl_content).unwrap();

    // Check which config is loaded
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["config", "file"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();

    // .rumdl.toml should take precedence
    assert!(
        stdout.contains(".rumdl.toml"),
        ".rumdl.toml should take precedence over pyproject.toml"
    );
    assert!(
        !stdout.contains("pyproject.toml"),
        "pyproject.toml should not be loaded when .rumdl.toml exists"
    );
}

#[test]
#[cfg(unix)]
fn test_symlinked_config_is_followed() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create a real config file
    let real_config_content = r#"
[global]
disable = ["MD013", "MD033"]
"#;
    let real_config_path = project_dir.join("real-config.toml");
    fs::write(&real_config_path, real_config_content).unwrap();

    // Create a symlink to it
    let symlink_path = project_dir.join(".rumdl.toml");
    unix_fs::symlink(&real_config_path, &symlink_path).unwrap();

    // Create a test markdown file
    let test_file = project_dir.join("test.md");
    fs::write(&test_file, "# Test\n\nThis is a very long line that exceeds 80 characters and MD013 should be disabled by symlinked config.\n").unwrap();

    // Run rumdl - should follow the symlink
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "test.md"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let combined = format!("{stdout}{stderr}");

    // MD013 should be disabled by symlinked config (following Ruff's behavior)
    assert!(
        !combined.contains("MD013"),
        "MD013 should be disabled by symlinked config"
    );
    assert!(
        !combined.contains("Line length"),
        "Line length warning should not appear"
    );
}

#[test]
fn test_markdownlint_yaml_upward_traversal() {
    // Issue #193: .markdownlint.yaml should be discovered via upward traversal
    // just like .rumdl.toml
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // Create nested directory structure
    let nested_dir = project_dir.join("path").join("to");
    fs::create_dir_all(&nested_dir).unwrap();

    // Create .markdownlint.yaml at project root (not in nested dir)
    let config_content = r#"
MD013:
  line_length: 200
  code_blocks: false
"#;
    fs::write(project_dir.join(".markdownlint.yaml"), config_content).unwrap();

    // Create a test markdown file in nested directory
    let test_file = nested_dir.join("file.md");
    fs::write(
        &test_file,
        "# Test\n\nThis is a line that is about 100 characters long and should not trigger MD013 due to parent config setting line_length to 200.\n",
    )
    .unwrap();

    // Run rumdl from nested directory, checking the file
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "file.md"])
        .current_dir(&nested_dir)
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let combined = format!("{stdout}{stderr}");

    // MD013 should NOT trigger because line_length=200 from parent config
    assert!(
        !combined.contains("MD013"),
        "MD013 should not trigger - .markdownlint.yaml at repo root should be discovered. Output: {combined}"
    );
}

#[test]
fn test_multi_path_global_config_not_seeded_from_first_path() {
    // Regression: with multiple paths spanning several config scopes, the
    // *global* config must be discovered from the project root (cwd), not from
    // the first path's directory. Otherwise whichever file sorts first decides
    // the baseline for every file, so a nested `extend-disable` silently
    // disables that rule for files in other directories that inherit the root
    // config (e.g. `rumdl check .claude/a.md d/b.md` would drop MD013 on d/b.md
    // because `.claude/.rumdl.toml` extend-disables it).
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // `.git` marks the project root so config discovery treats `project_dir`
    // (not `dir_a`) as the project root for the inheriting files.
    fs::create_dir(project_dir.join(".git")).unwrap();

    // Project-root config: MD013 enabled.
    fs::write(project_dir.join(".rumdl.toml"), "[MD013]\nline-length = 120\n").unwrap();

    // dir_a: nested config that extend-disables MD013.
    let dir_a = project_dir.join("dir_a");
    fs::create_dir_all(&dir_a).unwrap();
    fs::write(
        dir_a.join(".rumdl.toml"),
        "extends = \"../.rumdl.toml\"\n\n[global]\nextend-disable = [\"MD013\"]\n",
    )
    .unwrap();
    fs::write(dir_a.join("a.md"), "# A\n\nshort line.\n").unwrap();

    // dir_b: no own config, inherits the root config, so its long line must fire MD013.
    let dir_b = project_dir.join("dir_b");
    fs::create_dir_all(&dir_b).unwrap();
    let long_line = "This is a deliberately long line in dir_b which inherits the root config and clearly exceeds one hundred twenty characters so MD013 must fire.";
    fs::write(dir_b.join("b.md"), format!("# B\n\n{long_line}\n")).unwrap();

    // Pass the dir_a file FIRST: a regression seeds the global config from
    // dir_a (MD013 disabled) and wrongly suppresses MD013 for dir_b/b.md.
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "dir_a/a.md", "dir_b/b.md"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{stdout}{stderr}");

    // Only dir_b/b.md can produce MD013 (dir_a disables it and a.md is short),
    // so the rule firing at all proves dir_b kept the root config's MD013.
    assert!(
        combined.contains("MD013"),
        "MD013 must fire for dir_b/b.md (inherits root config) even though the \
         first path is in dir_a whose config extend-disables MD013. Output: {combined}"
    );
}

#[test]
fn test_multi_path_global_config_anchored_at_project_root_from_subdir_cwd() {
    // Regression: the global config for a multi-path run must be discovered from
    // the *project root*, not the current working directory. When `cwd` is itself
    // a configured subdirectory whose `.rumdl.toml` `extend-disable`s a rule, that
    // config must not leak into files in *other* directories that inherit the
    // project-root config. Seeding the global config from cwd (instead of the
    // project root) reintroduces the same silent gate-bypass for any run launched
    // from such a subdirectory, e.g. `cd dir_a && rumdl check a.md ../dir_b/b.md`.
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // `.git` marks the project root.
    fs::create_dir(project_dir.join(".git")).unwrap();

    // Project-root config: MD013 enabled.
    fs::write(project_dir.join(".rumdl.toml"), "[MD013]\nline-length = 120\n").unwrap();

    // dir_a: nested config that extend-disables MD013. This is also the cwd.
    let dir_a = project_dir.join("dir_a");
    fs::create_dir_all(&dir_a).unwrap();
    fs::write(
        dir_a.join(".rumdl.toml"),
        "extends = \"../.rumdl.toml\"\n\n[global]\nextend-disable = [\"MD013\"]\n",
    )
    .unwrap();
    fs::write(dir_a.join("a.md"), "# A\n\nshort line.\n").unwrap();

    // dir_b: no own config, inherits the root config, so its long line must fire MD013.
    let dir_b = project_dir.join("dir_b");
    fs::create_dir_all(&dir_b).unwrap();
    let long_line = "This is a deliberately long line in dir_b which inherits the root config and clearly exceeds one hundred twenty characters so MD013 must fire.";
    fs::write(dir_b.join("b.md"), format!("# B\n\n{long_line}\n")).unwrap();

    // Run FROM dir_a (the configured subdir). The global config must still be the
    // project-root config (MD013 enabled), not dir_a's (MD013 disabled), so
    // ../dir_b/b.md keeps MD013.
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "a.md", "../dir_b/b.md"])
        .current_dir(&dir_a)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{stdout}{stderr}");

    // Only ../dir_b/b.md can produce MD013 (dir_a disables it and a.md is short).
    assert!(
        combined.contains("MD013"),
        "MD013 must fire for ../dir_b/b.md (inherits root config) even though the \
         run is launched from dir_a whose config extend-disables MD013. Output: {combined}"
    );
}

#[test]
fn test_multi_path_distinct_subdir_configs_apply_independently() {
    // Two files in two differently-configured subdirectories, checked in one run,
    // must each use their own config. dir_a disables MD013 (its long line stays
    // clean); dir_c tightens the limit to 40 (its medium line trips MD013). A bug
    // that collapses everything onto one baseline would get at least one wrong.
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    fs::create_dir(project_dir.join(".git")).unwrap();
    // Root: MD013 enabled with a generous limit.
    fs::write(project_dir.join(".rumdl.toml"), "[MD013]\nline-length = 120\n").unwrap();

    // dir_a disables MD013.
    let dir_a = project_dir.join("dir_a");
    fs::create_dir_all(&dir_a).unwrap();
    fs::write(
        dir_a.join(".rumdl.toml"),
        "extends = \"../.rumdl.toml\"\n\n[global]\nextend-disable = [\"MD013\"]\n",
    )
    .unwrap();
    let long_line = "This line is well over forty characters long but under one hundred twenty.";
    fs::write(dir_a.join("a.md"), format!("# A\n\n{long_line}\n")).unwrap();

    // dir_c keeps MD013 but tightens the limit to 40.
    let dir_c = project_dir.join("dir_c");
    fs::create_dir_all(&dir_c).unwrap();
    fs::write(
        dir_c.join(".rumdl.toml"),
        "extends = \"../.rumdl.toml\"\n\n[MD013]\nline-length = 40\n",
    )
    .unwrap();
    // Same medium line: clean under root's 120, but trips dir_c's 40.
    fs::write(dir_c.join("c.md"), format!("# C\n\n{long_line}\n")).unwrap();

    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "dir_a/a.md", "dir_c/c.md"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.lines().any(|l| l.contains("c.md") && l.contains("MD013")),
        "MD013 must fire for dir_c/c.md (its config tightens line-length to 40). Output: {combined}"
    );
    assert!(
        !combined.lines().any(|l| l.contains("a.md") && l.contains("MD013")),
        "MD013 must NOT fire for dir_a/a.md (its config disables MD013). Output: {combined}"
    );
}

#[test]
fn test_multi_path_subdir_reenable_does_not_leak_to_root_group() {
    // Opposite polarity: the root DISABLES a rule and a subdir re-ENABLES it (via
    // `enable`, the replace-semantic that overrides an inherited `disable`). The
    // subdir file must get the rule; a sibling file that inherits the root config
    // must NOT. The sibling staying clean proves the root group uses the
    // project-root config (rule disabled), not defaults (rule enabled) or the
    // subdir's config (rule enabled).
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    fs::create_dir(project_dir.join(".git")).unwrap();
    // Root disables MD013.
    fs::write(project_dir.join(".rumdl.toml"), "[global]\ndisable = [\"MD013\"]\n").unwrap();

    // dir_e re-enables MD013 and tightens the limit. `enable` (not `extend-enable`)
    // is what removes a rule from an inherited `disable` list.
    let dir_e = project_dir.join("dir_e");
    fs::create_dir_all(&dir_e).unwrap();
    fs::write(
        dir_e.join(".rumdl.toml"),
        "extends = \"../.rumdl.toml\"\n\n[global]\nenable = [\"MD013\"]\n\n[MD013]\nline-length = 40\n",
    )
    .unwrap();
    let long_line = "This line is well over forty characters long but under one hundred twenty.";
    fs::write(dir_e.join("e.md"), format!("# E\n\n{long_line}\n")).unwrap();

    // dir_f has no config: inherits the root (MD013 disabled), so its long line must stay clean.
    let dir_f = project_dir.join("dir_f");
    fs::create_dir_all(&dir_f).unwrap();
    fs::write(dir_f.join("f.md"), format!("# F\n\n{long_line}\n")).unwrap();

    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "dir_e/e.md", "dir_f/f.md"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.lines().any(|l| l.contains("e.md") && l.contains("MD013")),
        "MD013 must fire for dir_e/e.md (its config extend-enables MD013). Output: {combined}"
    );
    assert!(
        !combined.lines().any(|l| l.contains("f.md") && l.contains("MD013")),
        "MD013 must NOT fire for dir_f/f.md (inherits the root config that disables MD013). Output: {combined}"
    );
}

#[test]
fn test_single_path_into_configured_subdir_uses_subdir_config() {
    // Regression guard for the single-path branch (unchanged by the multi-path
    // fix): a lone path inside a configured subdir must use that subdir's config,
    // even when run from the project root.
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    fs::create_dir(project_dir.join(".git")).unwrap();
    fs::write(project_dir.join(".rumdl.toml"), "[MD013]\nline-length = 120\n").unwrap();

    let dir_a = project_dir.join("dir_a");
    fs::create_dir_all(&dir_a).unwrap();
    fs::write(
        dir_a.join(".rumdl.toml"),
        "extends = \"../.rumdl.toml\"\n\n[global]\nextend-disable = [\"MD013\"]\n",
    )
    .unwrap();
    let long_line = "This is a deliberately long line that clearly exceeds one hundred twenty characters so MD013 would fire under the root config but not dir_a.";
    fs::write(dir_a.join("a.md"), format!("# A\n\n{long_line}\n")).unwrap();

    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "dir_a/a.md"])
        .current_dir(project_dir)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{stdout}{stderr}");

    assert!(
        !combined.contains("MD013"),
        "MD013 must NOT fire: a single path into dir_a uses dir_a's config, which disables it. Output: {combined}"
    );
}

#[test]
fn test_multi_path_standalone_subdir_configs_without_root_config() {
    // A project with a `.git` root that has NO config of its own, only standalone
    // subdirectory configs, must still apply each file's nearest config on a
    // multi-path run. Anchoring discovery at the config-less project root must not
    // disable per-directory grouping: each file keeps its own subdir config.
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path();

    // `.git` marks the project root, but the root has no rumdl config.
    fs::create_dir(project_dir.join(".git")).unwrap();

    // dir_a: standalone config that disables MD013 (no root to extend).
    let dir_a = project_dir.join("dir_a");
    fs::create_dir_all(&dir_a).unwrap();
    fs::write(dir_a.join(".rumdl.toml"), "[global]\ndisable = [\"MD013\"]\n").unwrap();
    // Long line (>80, would trip default MD013) that dir_a's config silences.
    let very_long_line = "This dir_a line is comfortably over eighty characters long so default MD013 would flag it, but dir_a disables MD013.";
    fs::write(dir_a.join("a.md"), format!("# A\n\n{very_long_line}\n")).unwrap();

    // dir_b: standalone config that tightens MD013 to 40.
    let dir_b = project_dir.join("dir_b");
    fs::create_dir_all(&dir_b).unwrap();
    fs::write(dir_b.join(".rumdl.toml"), "[MD013]\nline-length = 40\n").unwrap();
    // Medium line (60 chars) with breakable whitespace past column 40: under the
    // default 80 (so defaults would NOT flag it) but over dir_b's 40.
    let medium_line = "word word word word word word word word word over forty now.";
    fs::write(dir_b.join("b.md"), format!("# B\n\n{medium_line}\n")).unwrap();

    // Run from dir_a with a bare-filename first path, the case that anchors
    // discovery at the config-less `.git` root.
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "a.md", "../dir_b/b.md"])
        .current_dir(&dir_a)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{stdout}{stderr}");

    // dir_b's tighter limit must apply (proves dir_b's config was discovered);
    // under defaults the medium line would be clean.
    assert!(
        combined.lines().any(|l| l.contains("b.md") && l.contains("MD013")),
        "MD013 must fire for ../dir_b/b.md (its standalone config tightens line-length to 40). Output: {combined}"
    );
    // dir_a's disable must apply (proves dir_a's config was discovered); under
    // defaults the long line would trip MD013.
    assert!(
        !combined.lines().any(|l| l.contains("a.md") && l.contains("MD013")),
        "MD013 must NOT fire for a.md (its standalone config disables MD013). Output: {combined}"
    );
}

#[test]
fn test_multi_path_no_git_first_subdir_config_does_not_leak_to_sibling() {
    // No `.git` anywhere: the global baseline must come from the common ancestor
    // of the target paths, not the first path's own directory. Otherwise a multi-
    // path run whose first path lives in a configured subdir leaks that subdir's
    // config onto sibling files that should keep the (default) baseline.
    let temp_dir = tempdir().unwrap();
    let tree = temp_dir.path();
    // Deliberately NO `.git` directory.

    // dir_a: standalone config that disables MD013.
    let dir_a = tree.join("dir_a");
    fs::create_dir_all(&dir_a).unwrap();
    fs::write(dir_a.join(".rumdl.toml"), "[global]\ndisable = [\"MD013\"]\n").unwrap();
    let long_line = "word word word word word word word word word over eighty characters long here now today.";
    fs::write(dir_a.join("a.md"), format!("# A\n\n{long_line}\n")).unwrap();

    // dir_b: no config of its own, so its long line must keep the default MD013.
    let dir_b = tree.join("dir_b");
    fs::create_dir_all(&dir_b).unwrap();
    fs::write(dir_b.join("b.md"), format!("# B\n\n{long_line}\n")).unwrap();

    // First path is under dir_a (the configured subdir).
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "dir_a/a.md", "dir_b/b.md"])
        .current_dir(tree)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{stdout}{stderr}");

    // dir_b inherits the default baseline (MD013 enabled), so its long line fires.
    assert!(
        combined.lines().any(|l| l.contains("b.md") && l.contains("MD013")),
        "MD013 must fire for dir_b/b.md (default baseline) and not inherit dir_a's disable. Output: {combined}"
    );
    // dir_a's own config still disables MD013 for its file.
    assert!(
        !combined.lines().any(|l| l.contains("a.md") && l.contains("MD013")),
        "MD013 must NOT fire for dir_a/a.md (its config disables MD013). Output: {combined}"
    );
}

#[test]
#[cfg(unix)]
fn test_multi_path_synthesized_root_does_not_cross_home_boundary() {
    // When a multi-path run spans the home tree and an outside tree with no
    // project config, the synthesized project root must not sit at or above the
    // home directory. Otherwise per-directory discovery walks up through `$HOME`
    // and treats `~/.rumdl.toml` as a *project* config, overriding the
    // higher-precedence platform user config only for the home-side files. The
    // home dotfile must reach the loader only via the user-config fallback, so a
    // user config that disables a rule wins for every file.
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path();

    // Layout: <root>/home is $HOME; <root>/other is a sibling tree. Their common
    // ancestor is <root>, which is above $HOME.
    let home = root.join("home");
    let other = root.join("other");
    let proj = home.join("proj");
    let xdg = root.join("xdg");
    fs::create_dir_all(&proj).unwrap();
    fs::create_dir_all(&other).unwrap();
    fs::create_dir_all(xdg.join("rumdl")).unwrap();

    // Home dotfile: tightens MD013 to 40 (would fire on the lines below).
    fs::write(home.join(".rumdl.toml"), "[MD013]\nline-length = 40\n").unwrap();
    // Platform user config (higher precedence than the home dotfile): disables MD013.
    fs::write(
        xdg.join("rumdl").join("rumdl.toml"),
        "[global]\ndisable = [\"MD013\"]\n",
    )
    .unwrap();

    let long_line = "word word word word word word word word word over forty characters now today here.";
    fs::write(proj.join("a.md"), format!("# A\n\n{long_line}\n")).unwrap();
    fs::write(other.join("b.md"), format!("# B\n\n{long_line}\n")).unwrap();

    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(rumdl_exe)
        .args(["check", "--no-cache"])
        .arg(proj.join("a.md"))
        .arg(other.join("b.md"))
        .current_dir(root)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &xdg)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{stdout}{stderr}");

    // The platform user config disables MD013 for every file; `~/.rumdl.toml` must
    // not be promoted to a project config for the home-side file.
    assert!(
        !combined.contains("MD013"),
        "MD013 must NOT fire: the platform user config disables it and ~/.rumdl.toml must \
         not be treated as a project config across the home boundary. Output: {combined}"
    );
}

#[test]
fn test_multi_path_isolated_does_not_synthesize_project_root() {
    // `--isolated` deliberately skips config/project discovery and leaves the
    // project root unset, so a relative cache dir resolves against the cwd. A
    // multi-path run must not synthesize a project root from the common ancestor
    // in this mode, which would move `.rumdl_cache` under the targets instead.
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path();
    let sub = root.join("sub");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("a.md"), "# A\n\ntext.\n").unwrap();
    fs::write(sub.join("b.md"), "# B\n\ntext.\n").unwrap();

    // Run from the project root with both paths under `sub`; their common ancestor
    // is `sub`. With synthesis, the cache would be created at `sub/.rumdl_cache`.
    let rumdl_exe = env!("CARGO_BIN_EXE_rumdl");
    Command::new(rumdl_exe)
        .args(["check", "--isolated", "sub/a.md", "sub/b.md"])
        .current_dir(root)
        .output()
        .expect("Failed to execute command");

    // Isolated mode anchors the relative cache at the cwd, not the common ancestor.
    assert!(
        !sub.join(".rumdl_cache").exists(),
        "isolated multi-path run must not create .rumdl_cache under the common ancestor (sub/)"
    );
    assert!(
        root.join(".rumdl_cache").exists(),
        "isolated multi-path run should anchor the relative cache at the cwd"
    );
}
