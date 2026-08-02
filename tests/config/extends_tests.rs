/// Tests for the `extends` config inheritance feature.
///
/// Covers: basic inheritance, deep chains, child override, relative path resolution,
/// home directory paths, missing base file errors, circular reference errors, and chaining.
use rumdl_lib::config::SourcedConfig;
use rumdl_lib::config::types::ConfigError;
use std::fs;
use tempfile::tempdir;

/// Load a config file from a path and return the result.
fn load_config(path: &std::path::Path) -> Result<rumdl_lib::config::Config, ConfigError> {
    SourcedConfig::load_with_discovery(Some(path.to_str().unwrap()), None, false)
        .map(|s| s.into_validated_unchecked().into())
}

/// Basic inheritance: child overrides one field, inherits the rest from base.
#[test]
fn test_extends_basic_inheritance() {
    let dir = tempdir().unwrap();

    // Base config enables MD013 with line-length 80, and sets a flavor
    let base = dir.path().join("base.rumdl.toml");
    fs::write(
        &base,
        r#"
[global]
disable = ["MD033"]

[MD013]
line-length = 80
"#,
    )
    .unwrap();

    // Child extends base and overrides line-length
    let child = dir.path().join("child.rumdl.toml");
    fs::write(
        &child,
        r#"extends = "base.rumdl.toml"

[MD013]
line-length = 120
"#,
    )
    .unwrap();

    let config = load_config(&child).unwrap();

    // Inherited from base
    assert!(
        config.global.disable.contains(&"MD033".to_string()),
        "Child should inherit disable list from base"
    );

    // Overridden in child
    let line_length = rumdl_lib::config::get_rule_config_value::<i64>(&config, "MD013", "line-length");
    assert_eq!(line_length, Some(120), "Child should override line-length from base");
}

/// Deep chain: A extends B extends C — all values from the full chain are present.
#[test]
fn test_extends_deep_chain() {
    let dir = tempdir().unwrap();

    // C is the root base
    let c = dir.path().join("c.rumdl.toml");
    fs::write(
        &c,
        r#"
[global]
disable = ["MD041"]
"#,
    )
    .unwrap();

    // B extends C and adds its own disable
    let b = dir.path().join("b.rumdl.toml");
    fs::write(
        &b,
        r#"extends = "c.rumdl.toml"

[global]
disable = ["MD033"]
"#,
    )
    .unwrap();

    // A extends B and adds another disable
    let a = dir.path().join("a.rumdl.toml");
    fs::write(
        &a,
        r#"extends = "b.rumdl.toml"

[global]
disable = ["MD013"]
"#,
    )
    .unwrap();

    let config = load_config(&a).unwrap();

    // A's disable wins (override semantics — child replaces parent for disable)
    // The final value should be the leaf node's (A's) disable list since disable uses replace semantics
    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "A's disable list should be applied"
    );
}

/// Child fully overrides: all values come from child, nothing leaks from base.
#[test]
fn test_extends_child_overrides_all() {
    let dir = tempdir().unwrap();

    let base = dir.path().join("base.rumdl.toml");
    fs::write(
        &base,
        r#"
[global]
disable = ["MD033"]
flavor = "mkdocs"
"#,
    )
    .unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(
        &child,
        r#"extends = "base.rumdl.toml"

[global]
disable = ["MD013"]
flavor = "standard"
"#,
    )
    .unwrap();

    let config = load_config(&child).unwrap();

    // Child's disable replaces base's (replace semantics)
    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "Child's disable should be present"
    );
    // Base value is replaced (not merged)
    assert!(
        !config.global.disable.contains(&"MD033".to_string()),
        "Base's disable should be replaced, not merged"
    );

    // Child's flavor replaces base's
    use rumdl_lib::config::MarkdownFlavor;
    assert_eq!(
        config.global.flavor,
        MarkdownFlavor::Standard,
        "Child flavor should override base flavor"
    );
}

/// Relative path resolution: extends path is resolved relative to the config file's directory.
#[test]
fn test_extends_relative_path_resolution() {
    let dir = tempdir().unwrap();

    // Create a subdirectory for the child config
    let sub_dir = dir.path().join("subdir");
    fs::create_dir_all(&sub_dir).unwrap();

    // Base is in the parent directory
    let base = dir.path().join("base.rumdl.toml");
    fs::write(
        &base,
        r#"
[global]
disable = ["MD001"]
"#,
    )
    .unwrap();

    // Child is in subdir and uses relative path "../base.rumdl.toml"
    let child = sub_dir.join("child.rumdl.toml");
    fs::write(
        &child,
        r#"extends = "../base.rumdl.toml"

[global]
disable = ["MD002"]
"#,
    )
    .unwrap();

    let config = load_config(&child).unwrap();

    // Child's disable list wins (override semantics)
    assert!(
        config.global.disable.contains(&"MD002".to_string()),
        "Child's disable should be applied"
    );
}

/// Absolute path resolution: extends with absolute path works.
#[test]
fn test_extends_absolute_path_resolution() {
    let dir = tempdir().unwrap();

    let base = dir.path().join("base.rumdl.toml");
    fs::write(
        &base,
        r#"
[global]
disable = ["MD041"]
"#,
    )
    .unwrap();

    // Use the absolute path directly in the child config. Written as a TOML
    // literal string (single quotes) so Windows backslashes in the absolute path
    // are not interpreted as TOML escape sequences.
    let base_absolute = base.canonicalize().unwrap();
    let child_content = format!(
        r#"extends = '{}'

[global]
disable = ["MD013"]
"#,
        base_absolute.display()
    );

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, &child_content).unwrap();

    let config = load_config(&child).unwrap();

    // The child config loaded successfully (absolute path works)
    assert!(
        config.global.disable.contains(&"MD013".to_string()),
        "Child's config should load with absolute path extends"
    );
}

/// Missing base file: clear error when extends target doesn't exist.
#[test]
fn test_extends_missing_base_file_gives_clear_error() {
    let dir = tempdir().unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(
        &child,
        r#"extends = "nonexistent_base.rumdl.toml"

[global]
disable = ["MD013"]
"#,
    )
    .unwrap();

    let result = load_config(&child);

    assert!(result.is_err(), "Loading config with missing base should fail");

    match result.unwrap_err() {
        err @ ConfigError::ExtendsNotFound { .. } => {
            let message = err.to_string();
            assert!(
                message.contains("nonexistent_base.rumdl.toml"),
                "Error should mention the missing target, got: {message}"
            );
            assert!(
                message.contains("child.rumdl.toml"),
                "Error should mention the referencing file, got: {message}"
            );
        }
        other => panic!("Expected ExtendsNotFound error, got: {other:?}"),
    }
}

/// Circular extends: A extends B extends A → error with cycle info.
#[test]
fn test_extends_circular_reference_is_detected() {
    let dir = tempdir().unwrap();

    let a = dir.path().join("a.rumdl.toml");
    let b = dir.path().join("b.rumdl.toml");

    fs::write(&a, r#"extends = "b.rumdl.toml""#).unwrap();
    fs::write(&b, r#"extends = "a.rumdl.toml""#).unwrap();

    let result = load_config(&a);

    assert!(result.is_err(), "Circular extends should produce an error");

    match result.unwrap_err() {
        ConfigError::CircularExtends { path, .. } => {
            // The path in the error should reference one of the two files in the cycle
            assert!(
                path.contains("a.rumdl.toml") || path.contains("b.rumdl.toml"),
                "Error should mention a file in the cycle, got: {path}"
            );
        }
        other => panic!("Expected CircularExtends error, got: {other:?}"),
    }
}

/// Self-referential extends: a file that extends itself.
#[test]
fn test_extends_self_reference_is_detected() {
    let dir = tempdir().unwrap();

    let config = dir.path().join("self.rumdl.toml");
    fs::write(&config, r#"extends = "self.rumdl.toml""#).unwrap();

    let result = load_config(&config);
    assert!(result.is_err(), "Self-referential extends should produce an error");

    match result.unwrap_err() {
        ConfigError::CircularExtends { .. } => {} // expected
        other => panic!("Expected CircularExtends error, got: {other:?}"),
    }
}

/// Extends in base is itself respected (chaining).
/// A extends B, B extends C — A sees values from C.
#[test]
fn test_extends_chain_propagation() {
    let dir = tempdir().unwrap();

    // C has a rule config
    let c = dir.path().join("c.rumdl.toml");
    fs::write(
        &c,
        r#"
[MD007]
indent = 4
"#,
    )
    .unwrap();

    // B extends C, adds its own setting
    let b = dir.path().join("b.rumdl.toml");
    fs::write(
        &b,
        r#"extends = "c.rumdl.toml"

[MD003]
style = "atx"
"#,
    )
    .unwrap();

    // A extends B but doesn't override MD007 or MD003
    let a = dir.path().join("a.rumdl.toml");
    fs::write(&a, r#"extends = "b.rumdl.toml""#).unwrap();

    let config = load_config(&a).unwrap();

    // A inherits MD007.indent from C (via B)
    let indent = rumdl_lib::config::get_rule_config_value::<i64>(&config, "MD007", "indent");
    assert_eq!(indent, Some(4), "A should inherit MD007.indent from C via the chain");

    // A inherits MD003.style from B
    let style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD003", "style");
    assert_eq!(style, Some("atx".to_string()), "A should inherit MD003.style from B");
}

/// Child rule config overrides base rule config when both set the same key.
#[test]
fn test_extends_rule_config_child_overrides_base() {
    let dir = tempdir().unwrap();

    let base = dir.path().join("base.rumdl.toml");
    fs::write(
        &base,
        r#"
[MD013]
line-length = 80

[MD003]
style = "atx"
"#,
    )
    .unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(
        &child,
        r#"extends = "base.rumdl.toml"

[MD013]
line-length = 100
"#,
    )
    .unwrap();

    let config = load_config(&child).unwrap();

    // Child overrides line-length
    let line_length = rumdl_lib::config::get_rule_config_value::<i64>(&config, "MD013", "line-length");
    assert_eq!(line_length, Some(100), "Child should override MD013.line-length");

    // MD003 style is inherited from base (child doesn't set it)
    let style = rumdl_lib::config::get_rule_config_value::<String>(&config, "MD003", "style");
    assert_eq!(
        style,
        Some("atx".to_string()),
        "MD003.style should be inherited from base"
    );
}

/// Loaded files list includes both the child and the base.
#[test]
fn test_extends_loaded_files_tracks_chain() {
    let dir = tempdir().unwrap();

    let base = dir.path().join("base.rumdl.toml");
    fs::write(&base, "").unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, r#"extends = "base.rumdl.toml""#).unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child.to_str().unwrap()), None, false).unwrap();

    assert!(
        sourced.loaded_files.len() >= 2,
        "Both base and child should appear in loaded_files, got: {:?}",
        sourced.loaded_files
    );

    let has_base = sourced.loaded_files.iter().any(|f| f.contains("base.rumdl.toml"));
    let has_child = sourced.loaded_files.iter().any(|f| f.contains("child.rumdl.toml"));
    assert!(has_base, "base.rumdl.toml should be in loaded_files");
    assert!(has_child, "child.rumdl.toml should be in loaded_files");
}

/// Extend-enable uses union semantics across the extends chain.
#[test]
fn test_extends_extend_enable_union_semantics() {
    let dir = tempdir().unwrap();

    let base = dir.path().join("base.rumdl.toml");
    fs::write(
        &base,
        r#"
[global]
extend-enable = ["MD060"]
"#,
    )
    .unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(
        &child,
        r#"extends = "base.rumdl.toml"

[global]
extend-enable = ["MD063"]
"#,
    )
    .unwrap();

    let config = load_config(&child).unwrap();

    // Both should be in extend_enable (union semantics)
    assert!(
        config.global.extend_enable.contains(&"MD060".to_string()),
        "Base's extend-enable should be present"
    );
    assert!(
        config.global.extend_enable.contains(&"MD063".to_string()),
        "Child's extend-enable should be present"
    );
}

/// Deep chain replace semantics: disable uses replace, so the leaf node's value wins entirely.
#[test]
fn test_extends_deep_chain_replace_semantics() {
    let dir = tempdir().unwrap();

    let c = dir.path().join("c.rumdl.toml");
    fs::write(&c, "[global]\ndisable = [\"MD041\"]\n").unwrap();

    let b = dir.path().join("b.rumdl.toml");
    fs::write(&b, "extends = \"c.rumdl.toml\"\n[global]\ndisable = [\"MD033\"]\n").unwrap();

    let a = dir.path().join("a.rumdl.toml");
    fs::write(&a, "extends = \"b.rumdl.toml\"\n[global]\ndisable = [\"MD013\"]\n").unwrap();

    let config = load_config(&a).unwrap();

    // disable uses replace semantics: only A's value survives
    assert_eq!(
        config.global.disable,
        vec!["MD013".to_string()],
        "disable should contain only A's value (replace semantics)"
    );
    assert!(
        !config.global.disable.contains(&"MD033".to_string()),
        "B's disable should not leak into A"
    );
    assert!(
        !config.global.disable.contains(&"MD041".to_string()),
        "C's disable should not leak into A"
    );
}

/// per_file_ignores inheritance: child inherits base's per_file_ignores when not overriding.
#[test]
fn test_extends_per_file_ignores_inherited() {
    let dir = tempdir().unwrap();

    let base = dir.path().join("base.rumdl.toml");
    fs::write(
        &base,
        r#"
[per-file-ignores]
"docs/*.md" = ["MD013"]
"README.md" = ["MD041"]
"#,
    )
    .unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(
        &child,
        r#"extends = "base.rumdl.toml"

[global]
line-length = 100
"#,
    )
    .unwrap();

    let config = load_config(&child).unwrap();

    assert!(
        config.per_file_ignores.contains_key("docs/*.md"),
        "Child should inherit per_file_ignores from base"
    );
    assert!(
        config.per_file_ignores.contains_key("README.md"),
        "Child should inherit all per_file_ignores patterns from base"
    );
}

/// per_file_ignores replacement: child's per_file_ignores fully replaces base's.
#[test]
fn test_extends_per_file_ignores_replaced_by_child() {
    let dir = tempdir().unwrap();

    let base = dir.path().join("base.rumdl.toml");
    fs::write(
        &base,
        r#"
[per-file-ignores]
"docs/*.md" = ["MD013"]
"#,
    )
    .unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(
        &child,
        r#"extends = "base.rumdl.toml"

[per-file-ignores]
"src/*.md" = ["MD041"]
"#,
    )
    .unwrap();

    let config = load_config(&child).unwrap();

    // Child's per_file_ignores replaces base's entirely (replace semantics)
    assert!(
        config.per_file_ignores.contains_key("src/*.md"),
        "Child's per_file_ignores should be present"
    );
    assert!(
        !config.per_file_ignores.contains_key("docs/*.md"),
        "Base's per_file_ignores should be replaced by child's"
    );
}

/// Provenance: each value's origin names the file in the extends chain that
/// actually set it, so `rumdl config` can attribute base-config values to the
/// base file and overrides to the child.
#[test]
fn test_extends_origin_attribution() {
    let dir = tempdir().unwrap();
    let base = dir.path().join("base.rumdl.toml");
    let child = dir.path().join(".rumdl.toml");

    fs::write(
        &base,
        r#"exclude = ["drafts"]

[MD013]
line-length = 100
"#,
    )
    .unwrap();
    fs::write(
        &child,
        r#"extends = "base.rumdl.toml"
enable = ["MD001", "MD013"]

[MD013]
line-length = 120
"#,
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(child.to_str().unwrap()), None, false).unwrap();

    let origin_of = |origin: &Option<String>| -> String {
        origin
            .as_deref()
            .and_then(|f| std::path::Path::new(f).file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    };

    // Inherited value: attributed to the base file.
    assert_eq!(origin_of(&sourced.global.exclude.origin), "base.rumdl.toml");
    // Value only the child sets: attributed to the child.
    assert_eq!(origin_of(&sourced.global.enable.origin), ".rumdl.toml");
    // Value the child overrides: the child wins the attribution.
    let md013 = sourced.rules.get("MD013").expect("MD013 config present");
    let line_length = md013.values.get("line-length").expect("line-length set");
    assert_eq!(toml::Value::Integer(120), line_length.value);
    assert_eq!(origin_of(&line_length.origin), ".rumdl.toml");
}

// --- Env-var expansion in `extends` paths (issue #667) ----------------------
//
// These tests mutate the real process environment, so they carry
// `#[serial_test::serial]` (the codebase convention for global-state tests) and
// clean up via an RAII guard. They drive the production loader (`load_config`
// -> `SourcedConfig::load_with_discovery`), not a re-implementation of the
// resolver, so the full parse -> expand -> resolve -> merge path is exercised.

/// Sets an env var for the duration of a test and removes it on drop.
struct EnvVarGuard {
    key: String,
}

impl EnvVarGuard {
    fn set(key: &str, value: &std::path::Path) -> Self {
        // SAFETY: rumdl's test runner is nextest (`make test` / CI), which executes
        // each test in its own process, so this `set_var` cannot race a concurrent
        // `std::env::var` in another test. `#[serial_test::serial]` additionally
        // serializes this against the other global-state (env/cwd) tests, which
        // covers shared-process runners that serialize on the same lock. The guard
        // restores the environment on drop.
        unsafe { std::env::set_var(key, value) };
        Self { key: key.to_string() }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // SAFETY: see `EnvVarGuard::set`.
        unsafe { std::env::remove_var(&self.key) };
    }
}

/// Bare `$VAR` form: a child references its base through an environment variable
/// and inherits the base while keeping its own override.
#[test]
#[serial_test::serial]
fn test_extends_env_var_expansion() {
    let dir = tempdir().unwrap();

    let base = dir.path().join("base.rumdl.toml");
    fs::write(
        &base,
        r#"
[global]
disable = ["MD033"]

[MD013]
line-length = 80
"#,
    )
    .unwrap();

    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_BASE_DIR", dir.path());

    // `extends` written as a TOML literal string ('...') so Windows backslashes
    // in the expanded path are not treated as escapes.
    let child = dir.path().join("child.rumdl.toml");
    fs::write(
        &child,
        "extends = '$RUMDL_TEST_EXTENDS_BASE_DIR/base.rumdl.toml'\n\n[MD013]\nline-length = 120\n",
    )
    .unwrap();

    let config = load_config(&child).unwrap();

    assert!(
        config.global.disable.contains(&"MD033".to_string()),
        "child should inherit the base via an env-var-expanded extends path"
    );
    let line_length = rumdl_lib::config::get_rule_config_value::<i64>(&config, "MD013", "line-length");
    assert_eq!(line_length, Some(120), "child override should still apply");
}

/// Braced `${VAR}` form resolves the same way.
#[test]
#[serial_test::serial]
fn test_extends_env_var_braced_form() {
    let dir = tempdir().unwrap();

    let base = dir.path().join("base.rumdl.toml");
    fs::write(&base, "[global]\ndisable = [\"MD041\"]\n").unwrap();

    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_BRACED_DIR", dir.path());

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '${RUMDL_TEST_EXTENDS_BRACED_DIR}/base.rumdl.toml'\n").unwrap();

    let config = load_config(&child).unwrap();
    assert!(
        config.global.disable.contains(&"MD041".to_string()),
        "braced ${{VAR}} form should resolve and inherit the base"
    );
}

/// An `extends` path that references an unset variable fails with a clear,
/// dedicated error (not a confusing "file not found" on a half-expanded path).
#[test]
#[serial_test::serial]
fn test_extends_undefined_env_var_gives_clear_error() {
    let dir = tempdir().unwrap();

    // Make sure the variable really is unset.
    // SAFETY: per-test-process isolation under nextest + `#[serial_test::serial]`;
    // see `EnvVarGuard::set`.
    unsafe { std::env::remove_var("RUMDL_TEST_DEFINITELY_UNSET_667") };

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_DEFINITELY_UNSET_667/base.rumdl.toml'\n").unwrap();

    match load_config(&child).unwrap_err() {
        ConfigError::ExtendsUndefinedVar { var, from } => {
            assert_eq!(
                var, "$RUMDL_TEST_DEFINITELY_UNSET_667",
                "error should name the missing variable"
            );
            assert!(
                from.contains("child.rumdl.toml"),
                "error should name the referencing file, got from: {from}"
            );
        }
        other => panic!("Expected ExtendsUndefinedVar error, got: {other:?}"),
    }
}

/// Cycle detection still fires when the cycle is formed through an env-expanded
/// path: the expanded path must canonicalize into the visited set.
#[test]
#[serial_test::serial]
fn test_extends_env_var_cycle_is_detected() {
    let dir = tempdir().unwrap();

    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_SELF_DIR", dir.path());

    // The config extends itself via an env-var-expanded path.
    let cfg = dir.path().join("self.rumdl.toml");
    fs::write(&cfg, "extends = '$RUMDL_TEST_EXTENDS_SELF_DIR/self.rumdl.toml'\n").unwrap();

    match load_config(&cfg).unwrap_err() {
        ConfigError::CircularExtends { .. } => {} // expected: env-expanded path hit the cycle guard
        other => panic!("Expected CircularExtends via env-expanded path, got: {other:?}"),
    }
}

// --- What an `extends` failure may say about the file it reached -------------
//
// An `extends` value is expanded from the environment and then points wherever
// it points, so a message about the file it reaches must not print the expanded
// path (that prints environment variable values into whatever reads the error,
// which under CI is the build log) and must not quote the file's own text (the
// target need not be a config file at all). It still has to say enough to be
// acted on, so each test below pairs the disclosure assertion with a positive
// control that the reference is named as written.
//
// These tests set an environment variable, so they follow the convention above:
// `#[serial_test::serial]` plus the RAII guard.

/// A value distinctive enough that finding it in a message can only mean the
/// expanded path was printed.
const SECRET: &str = "s3cr3t-value-do-not-leak";

/// Asserts a message keeps the secret out while still naming the reference.
#[track_caller]
fn assert_reference_named_without_secret(message: &str, written_as: &str) {
    assert!(
        !message.contains(SECRET),
        "message disclosed the expanded environment variable: {message}"
    );
    // Positive control: suppressing the path must not leave an error nobody can
    // act on. A message naming neither the value nor the file would pass the
    // assertion above while being useless.
    assert!(
        message.contains(written_as),
        "message should name the extends value as written ({written_as}), got: {message}"
    );
    assert!(
        message.contains("child.rumdl.toml"),
        "message should name the referencing config, got: {message}"
    );
}

/// The target does not exist: the "not found" error names the reference, not the
/// path the environment variable expanded to.
#[test]
#[serial_test::serial]
fn test_extends_missing_target_does_not_disclose_expanded_path() {
    let dir = tempdir().unwrap();
    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_SECRET_DIR", &dir.path().join(SECRET));

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_SECRET_DIR/base.toml'\n").unwrap();

    match load_config(&child).unwrap_err() {
        err @ ConfigError::ExtendsNotFound { .. } => {
            let message = err.to_string();
            assert_reference_named_without_secret(&message, "$RUMDL_TEST_EXTENDS_SECRET_DIR/base.toml");
            assert!(
                message.contains("RUMDL_TEST_EXTENDS_SECRET_DIR"),
                "naming the substituted variable is what makes the error diagnosable, got: {message}"
            );
        }
        other => panic!("Expected ExtendsNotFound, got: {other:?}"),
    }
}

/// The target exists but is not readable as a file: the I/O error names the
/// reference, not the expanded path.
#[test]
#[serial_test::serial]
fn test_extends_unreadable_target_does_not_disclose_expanded_path() {
    let dir = tempdir().unwrap();
    // A directory: `read_to_string` fails on it on every platform.
    let target = dir.path().join(SECRET);
    fs::create_dir(&target).unwrap();
    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_SECRET_TARGET", &target);

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_SECRET_TARGET'\n").unwrap();

    let message = load_config(&child).unwrap_err().to_string();
    assert_reference_named_without_secret(&message, "$RUMDL_TEST_EXTENDS_SECRET_TARGET");
}

/// The target is not TOML at all: the parse error locates the problem by line
/// and column and does not quote the line, which belongs to a file the user
/// never asked rumdl to read.
#[test]
#[serial_test::serial]
fn test_extends_parse_error_does_not_quote_target_contents() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("private-key.pem");
    fs::write(&target, format!("-----BEGIN PRIVATE KEY-----\n{SECRET}\n")).unwrap();
    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_SECRET_FILE", &target);

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_SECRET_FILE'\n").unwrap();

    let message = load_config(&child).unwrap_err().to_string();
    assert!(
        !message.contains("BEGIN PRIVATE KEY"),
        "parse error quoted a line of the target file: {message}"
    );
    assert_reference_named_without_secret(&message, "$RUMDL_TEST_EXTENDS_SECRET_FILE");
    assert!(
        message.contains("line 1"),
        "the position still has to be reported so the owner of the file can find it, got: {message}"
    );
}

/// Negative control for the rule above: a config the user named themselves is
/// still rendered in full, with the offending line quoted. Suppressing that
/// everywhere would make every syntax error harder to fix.
#[test]
fn test_direct_config_parse_error_still_quotes_the_line() {
    let dir = tempdir().unwrap();
    let config = dir.path().join("broken.rumdl.toml");
    fs::write(&config, "[global]\nthis line is not toml\n").unwrap();

    let message = load_config(&config).unwrap_err().to_string();
    assert!(
        message.contains("this line is not toml"),
        "a directly named config should still have its offending line quoted, got: {message}"
    );
}

/// A cycle formed through an expanded path: the chain in the error is described
/// by how each file was reached, not by the paths it resolved to.
#[test]
#[serial_test::serial]
fn test_extends_cycle_does_not_disclose_expanded_paths() {
    let dir = tempdir().unwrap();
    let secret_dir = dir.path().join(SECRET);
    fs::create_dir(&secret_dir).unwrap();
    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_CYCLE_DIR", &secret_dir);

    // child -> base (through the secret directory) -> back to child.
    let base = secret_dir.join("base.rumdl.toml");
    fs::write(&base, "extends = '../child.rumdl.toml'\n").unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_CYCLE_DIR/base.rumdl.toml'\n").unwrap();

    let message = load_config(&child).unwrap_err().to_string();
    assert!(
        !message.contains(SECRET),
        "the cycle report disclosed the expanded environment variable: {message}"
    );
    assert!(
        message.contains("$RUMDL_TEST_EXTENDS_CYCLE_DIR/base.rumdl.toml"),
        "the cycle report should still name the reference as written, got: {message}"
    );
}

/// The `[config warning]` messages a plain `rumdl check` prints for this config.
fn validation_warnings(config: &std::path::Path) -> Vec<String> {
    let sourced = SourcedConfig::load_with_discovery(Some(config.to_str().unwrap()), None, false).unwrap();
    let rules = rumdl_lib::all_rules(&rumdl_lib::config::Config::default());
    let registry = rumdl_lib::config::RuleRegistry::from_rules(&rules);
    rumdl_lib::config::validate_config_sourced(&sourced, &registry)
        .into_iter()
        .map(|w| w.message)
        .collect()
}

/// Validation warnings travel the same channel as errors, and it is the channel
/// a user gets without asking: `[config warning]` is printed by a bare
/// `rumdl check`. An unknown key in a file reached through `extends` must not
/// name the expanded path, and must not repeat the key either. rumdl did not
/// recognize the key, so it is text out of a file rumdl was merely pointed at.
#[test]
#[serial_test::serial]
fn test_extends_unknown_key_warning_does_not_disclose_expanded_path() {
    let dir = tempdir().unwrap();
    let secret_dir = dir.path().join(SECRET);
    fs::create_dir(&secret_dir).unwrap();
    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_WARN_DIR", &secret_dir);

    let base = secret_dir.join("base.rumdl.toml");
    fs::write(&base, "[global]\nprod-database-password = true\n").unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_WARN_DIR/base.rumdl.toml'\n").unwrap();

    let messages = validation_warnings(&child);
    let warning = messages
        .iter()
        .find(|m| m.contains("Unknown global option"))
        .unwrap_or_else(|| panic!("expected a warning for the unknown key, got: {messages:?}"));

    assert!(
        !warning.contains("prod-database-password"),
        "the warning repeated a key read out of the extends target: {warning}"
    );
    // Positive control: withholding must be visible. A warning that quietly
    // dropped the key would satisfy the assertion above and tell the user less
    // than it knows.
    assert!(
        warning.contains("<withheld>"),
        "the warning should say a key was withheld, got: {warning}"
    );
    assert_reference_named_without_secret(warning, "$RUMDL_TEST_EXTENDS_WARN_DIR/base.rumdl.toml");
}

/// A section name is read out of the file the same way a key is.
#[test]
#[serial_test::serial]
fn test_extends_unknown_section_warning_withholds_the_section_name() {
    let dir = tempdir().unwrap();
    let secret_dir = dir.path().join(SECRET);
    fs::create_dir(&secret_dir).unwrap();
    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_SECTION_DIR", &secret_dir);

    let base = secret_dir.join("base.rumdl.toml");
    fs::write(&base, "[prod-signing-key-id]\nenabled = true\n").unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_SECTION_DIR/base.rumdl.toml'\n").unwrap();

    let messages = validation_warnings(&child);
    let warning = messages
        .iter()
        .find(|m| m.starts_with("Unknown rule in"))
        .unwrap_or_else(|| panic!("expected a warning for the unknown section, got: {messages:?}"));

    assert!(
        !warning.contains("prod-signing-key-id"),
        "the warning repeated a section name read out of the extends target: {warning}"
    );
    assert!(
        warning.contains("<withheld>"),
        "the warning should say a section name was withheld, got: {warning}"
    );
    assert_reference_named_without_secret(warning, "$RUMDL_TEST_EXTENDS_SECTION_DIR/base.rumdl.toml");
}

/// An option of a rule rumdl *does* know reaches the validator by a different
/// route: the parser stores it in the config map, and the validator reports
/// whatever key it finds there. So an unrecognized option in a `[MD013]` table
/// has to be withheld where it is parsed, not where it is reported.
#[test]
#[serial_test::serial]
fn test_extends_unknown_rule_option_is_withheld() {
    let dir = tempdir().unwrap();
    let secret_dir = dir.path().join(SECRET);
    fs::create_dir(&secret_dir).unwrap();
    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_OPTION_DIR", &secret_dir);

    let base = secret_dir.join("base.rumdl.toml");
    fs::write(&base, "[MD013]\nline-length = 80\nprod-database-password = true\n").unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_OPTION_DIR/base.rumdl.toml'\n").unwrap();

    let messages = validation_warnings(&child);
    let warning = messages
        .iter()
        .find(|m| m.contains("Unknown option for rule MD013"))
        .unwrap_or_else(|| panic!("expected a warning for the unknown option, got: {messages:?}"));

    assert!(
        !warning.contains("prod-database-password"),
        "the warning repeated an option key read out of the extends target: {warning}"
    );
    assert!(
        warning.contains("<withheld>"),
        "the warning should say an option was withheld, got: {warning}"
    );
    assert_reference_named_without_secret(warning, "$RUMDL_TEST_EXTENDS_OPTION_DIR/base.rumdl.toml");

    // Withholding must not cost the rest of the table: the recognized option in
    // the same section still applies, so this is a redaction and not a refusal
    // to read the file.
    let config = load_config(&child).unwrap();
    assert_eq!(
        rumdl_lib::config::get_rule_config_value::<i64>(&config, "MD013", "line-length"),
        Some(80),
        "the recognized option in the same table should still be inherited"
    );
}

/// Negative control for the three above: a config the user named themselves has
/// its unknown key, section and rule option quoted back, which is what makes
/// those warnings worth printing. Withholding everywhere would trade one bug
/// for another.
#[test]
fn test_direct_config_unknown_key_is_still_named() {
    let dir = tempdir().unwrap();
    let config = dir.path().join("named.rumdl.toml");
    fs::write(
        &config,
        "[global]\nnot-a-global-option = true\nenable = [\"not-a-rule-name\"]\n\n[MD9999]\nenabled = true\n\n[MD013]\nnot-an-md013-option = 1\n",
    )
    .unwrap();

    let messages = validation_warnings(&config);
    assert!(
        messages.iter().any(|m| m.contains("not-a-rule-name")),
        "a directly named config should have its unknown rule name quoted back, got: {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("not-a-global-option")),
        "a directly named config should have its unknown key quoted back, got: {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("MD9999")),
        "a directly named config should have its unknown section quoted back, got: {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("not-an-md013-option")),
        "a directly named config should have its unknown rule option quoted back, got: {messages:?}"
    );
    assert!(
        messages.iter().all(|m| !m.contains("<withheld>")),
        "nothing should be withheld for a config the user named, got: {messages:?}"
    );
}

/// The warnings that repeat a config *value* go through `log`, so they need
/// `RUST_LOG` to be visible and this test drives the binary rather than the
/// library. An invalid value in a file reached through `extends` is reported
/// without echoing what the file said.
#[test]
fn test_extends_invalid_value_warning_does_not_echo_the_value() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("secrets");
    fs::create_dir(&target_dir).unwrap();

    let base = target_dir.join("base.rumdl.toml");
    fs::write(&base, format!("[per-file-flavor]\n\"*.md\" = \"{SECRET}\"\n")).unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_FLAVOR_DIR/base.rumdl.toml'\n").unwrap();

    let doc = dir.path().join("doc.md");
    fs::write(&doc, "# Title\n").unwrap();

    // The environment variable is set on the child process only, so this test
    // needs no guard and does not have to be serialized against the others.
    let run = |config: &std::path::Path| -> String {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["check", "--no-cache", "--config"])
            .arg(config)
            .arg(&doc)
            .env("RUST_LOG", "warn")
            .env("RUMDL_TEST_EXTENDS_FLAVOR_DIR", &target_dir)
            .output()
            .expect("failed to run the rumdl binary");
        String::from_utf8_lossy(&output.stderr).into_owned()
    };

    let through_extends = run(&child);
    assert!(
        !through_extends.contains(SECRET),
        "the warning echoed a value read out of the extends target: {through_extends}"
    );
    assert!(
        through_extends.contains("Invalid flavor"),
        "the problem still has to be reported, got: {through_extends}"
    );

    // Positive control, and proof this harness can see the value at all: the
    // same file named directly still has its invalid value quoted back.
    let directly = run(&base);
    assert!(
        directly.contains(SECRET),
        "a directly named config should still name the invalid value, got: {directly}"
    );
}

/// A rule name rumdl does not recognize is text out of the file, and the
/// validator prints one back verbatim on the always-on channel. From a file
/// reached through `extends` it is withheld, while the names rumdl does
/// recognize keep working.
#[test]
#[serial_test::serial]
fn test_extends_unknown_rule_name_in_global_list_is_withheld() {
    let dir = tempdir().unwrap();
    let secret_dir = dir.path().join(SECRET);
    fs::create_dir(&secret_dir).unwrap();
    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_LIST_DIR", &secret_dir);

    let base = secret_dir.join("base.rumdl.toml");
    fs::write(&base, format!("[global]\nenable = [\"MD013\", \"{SECRET}\"]\n")).unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_LIST_DIR/base.rumdl.toml'\n").unwrap();

    let messages = validation_warnings(&child);
    let warning = messages
        .iter()
        .find(|m| m.contains("Unknown rule in global.enable"))
        .unwrap_or_else(|| panic!("expected a warning for the unknown rule name, got: {messages:?}"));

    assert!(
        !warning.contains(SECRET),
        "the warning repeated a rule name read out of the extends target: {warning}"
    );
    assert!(
        warning.contains("<withheld>"),
        "withholding has to stay visible, got: {warning}"
    );

    // Positive control: withholding one entry is a redaction of that entry, not
    // a refusal of the list. MD013 sat beside it and still selects.
    let config = load_config(&child).unwrap();
    assert_eq!(
        config.global.enable.iter().filter(|r| *r == "MD013").count(),
        1,
        "the recognized rule name should survive, got: {:?}",
        config.global.enable
    );
}

/// The glob caches print a pattern they cannot compile, and they run far from
/// the config file with no idea where the pattern came from. One out of an
/// `extends` target is dropped while the origin is still known, and the warning
/// it would have raised is raised here instead.
#[test]
fn test_extends_invalid_glob_pattern_is_withheld() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("secrets");
    fs::create_dir(&target_dir).unwrap();

    let base = target_dir.join("base.rumdl.toml");
    fs::write(
        &base,
        format!("[global]\nenable = [\"MD013\"]\n\n[per-file-ignores]\n\"[{SECRET}\" = [\"MD013\"]\n"),
    )
    .unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_GLOB_DIR/base.rumdl.toml'\n").unwrap();

    let doc = dir.path().join("doc.md");
    fs::write(&doc, format!("# Title\n\n{}\n", ["word"; 40].join(" "))).unwrap();

    let run = |config: &std::path::Path| -> String {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["check", "--no-cache", "--config"])
            .arg(config)
            .arg(&doc)
            .env("RUST_LOG", "warn")
            .env("RUMDL_TEST_EXTENDS_GLOB_DIR", &target_dir)
            .output()
            .expect("failed to run the rumdl binary");
        String::from_utf8_lossy(&output.stderr).into_owned()
    };

    let through_extends = run(&child);
    assert!(
        !through_extends.contains(SECRET),
        "the warning echoed a pattern read out of the extends target: {through_extends}"
    );
    assert!(
        through_extends.contains("Invalid glob pattern in per-file-ignores"),
        "the problem still has to be reported, got: {through_extends}"
    );

    // Positive control, and proof this harness can see the pattern at all: the
    // same file named directly still has its invalid pattern printed.
    let directly = run(&base);
    assert!(
        directly.contains(SECRET),
        "a directly named config should still name the invalid pattern, got: {directly}"
    );
}

/// Dropping an invalid pattern must not drop the valid ones beside it.
#[test]
#[serial_test::serial]
fn test_extends_valid_glob_pattern_survives_a_withheld_one() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("secrets");
    fs::create_dir(&target_dir).unwrap();
    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_GLOB2_DIR", &target_dir);

    let base = target_dir.join("base.rumdl.toml");
    fs::write(
        &base,
        format!("[per-file-ignores]\n\"[{SECRET}\" = [\"MD013\"]\n\"docs/*.md\" = [\"MD041\"]\n"),
    )
    .unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_GLOB2_DIR/base.rumdl.toml'\n").unwrap();

    let config = load_config(&child).unwrap();
    assert_eq!(
        config.per_file_ignores.keys().collect::<Vec<_>>(),
        vec!["docs/*.md"],
        "only the pattern that does not compile should go, got: {:?}",
        config.per_file_ignores
    );
}

/// The file walk prints an `include`/`exclude` pattern it cannot compile, on a
/// channel no logging level gates. A pattern out of an extends target has to
/// reach neither.
#[test]
fn test_extends_invalid_walk_patterns_are_withheld() {
    for setting in ["include", "exclude"] {
        let dir = tempdir().unwrap();
        let target_dir = dir.path().join("secrets");
        fs::create_dir(&target_dir).unwrap();

        let base = target_dir.join("base.rumdl.toml");
        fs::write(&base, format!("[global]\n{setting} = [\"[{SECRET}\"]\n")).unwrap();

        let child = dir.path().join("child.rumdl.toml");
        fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_WALK_DIR/base.rumdl.toml'\n").unwrap();

        fs::write(dir.path().join("doc.md"), "# Title\n").unwrap();

        // Discovery mode, since a config `include` only filters a walk rumdl
        // started itself: named a file outright, it never builds the override
        // that reports the pattern.
        let run = |config: &std::path::Path| -> String {
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .args(["check", "--no-cache", "--config"])
                .arg(config)
                .current_dir(dir.path())
                .env("RUMDL_TEST_EXTENDS_WALK_DIR", &target_dir)
                .output()
                .expect("failed to run the rumdl binary");
            String::from_utf8_lossy(&output.stderr).into_owned()
        };

        // No RUST_LOG here on purpose: the message the walk would have printed is
        // always on, so the one standing in for it has to be too.
        let through_extends = run(&child);
        assert!(
            !through_extends.contains(SECRET),
            "the {setting} warning echoed a pattern read out of the extends target: {through_extends}"
        );
        assert!(
            through_extends.contains(&format!("Invalid {setting} pattern in")),
            "the problem still has to be reported, got: {through_extends}"
        );

        // Positive control, and proof this harness can see the pattern at all: the
        // same file named directly still has its invalid pattern printed.
        let directly = run(&base);
        assert!(
            directly.contains(SECRET),
            "a directly named config should still name the invalid {setting} pattern, got: {directly}"
        );
    }
}

/// Dropping an invalid walk pattern must not drop the valid ones beside it, and
/// only `exclude` is dropped at all: an `include` pattern is rewritten relative
/// to the project root before the walk compiles it, and this does not know that
/// root, so judging one here would discard patterns the walk can use.
#[test]
#[serial_test::serial]
fn test_extends_valid_walk_patterns_survive_a_withheld_one() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("secrets");
    fs::create_dir(&target_dir).unwrap();
    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_WALK2_DIR", &target_dir);

    let base = target_dir.join("base.rumdl.toml");
    fs::write(
        &base,
        format!("[global]\ninclude = [\"[{SECRET}\", \"docs/**\"]\nexclude = [\"[{SECRET}\", \"drafts\"]\n"),
    )
    .unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_WALK2_DIR/base.rumdl.toml'\n").unwrap();

    let config = load_config(&child).unwrap();
    assert_eq!(
        config.global.include,
        vec![format!("[{SECRET}"), "docs/**".to_string()],
        "the include patterns should reach the walk as written"
    );
    let withheld = config.global.include_withheld.as_deref().unwrap_or_default();
    assert!(
        withheld.starts_with("'$RUMDL_TEST_EXTENDS_WALK2_DIR/base.rumdl.toml' (referenced from ")
            && !withheld.contains(SECRET),
        "the walk needs the origin to report a pattern it cannot use without quoting it, got: {withheld}"
    );
    assert_eq!(
        config.global.exclude,
        vec!["drafts".to_string()],
        "only the exclude pattern that does not compile should go"
    );
}

/// The rewrite that makes an `include` pattern unjudgeable at parse time, from
/// the outside: an absolute pattern loses the project root's own name, and that
/// name can hold glob syntax the pattern never has to satisfy. `[2019-2021]`
/// reads as a character class spanning a descending range, so the pattern does
/// not compile until the prefix holding it comes off. Dropping it where it is
/// parsed silently skips every file it selects.
#[test]
fn test_extends_absolute_include_survives_a_project_root_name() {
    let dir = tempdir().unwrap();
    // The project root must be the directory holding the config, so each arm
    // gets its own `.git` marker rather than relying on there being none above.
    let mut arms = Vec::new();
    for arm in ["notes [2019-2021]", "named [2019-2021]"] {
        let project = dir.path().join(arm);
        fs::create_dir_all(project.join("docs")).unwrap();
        fs::create_dir(project.join(".git")).unwrap();
        fs::write(project.join("docs/note.md.jinja"), "no heading here\n").unwrap();
        // The pattern is written the way the running process sees the directory,
        // which is the canonical form on both platforms rumdl normalizes for
        // (macOS resolves `/var`, Windows reports an 8.3 short name). A path in a
        // TOML literal string, so a Windows separator is not an escape.
        let canonical = rumdl_lib::discovery::canonicalize_for_matching(&project).unwrap();
        let include = format!("include = ['{}/docs/*.md.jinja']\n", canonical.display()).replace('\\', "/");
        arms.push((project, include));
    }

    let (extending, extending_include) = &arms[0];
    let base_dir = dir.path().join("shared");
    fs::create_dir(&base_dir).unwrap();
    let base = base_dir.join("base.rumdl.toml");
    fs::write(&base, format!("[global]\n{extending_include}")).unwrap();
    fs::write(
        extending.join(".rumdl.toml"),
        "extends = '$RUMDL_TEST_EXTENDS_ABSINC_DIR/base.rumdl.toml'\n",
    )
    .unwrap();

    let (direct, direct_include) = &arms[1];
    fs::write(direct.join(".rumdl.toml"), format!("[global]\n{direct_include}")).unwrap();

    // Discovery mode with no `--config`: naming the config would move the project
    // root to the config's own directory and the prefix would never be stripped.
    let run = |project: &std::path::Path| -> String {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["check", "--no-cache"])
            .current_dir(project)
            .env("RUMDL_TEST_EXTENDS_ABSINC_DIR", &base_dir)
            .output()
            .expect("failed to run the rumdl binary");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    // The control runs first: it fixes what the answer is, so a shared mistake in
    // the layout fails here rather than being read as a passing assertion below.
    let directly = run(direct);
    assert!(
        directly.contains("note.md.jinja"),
        "the absolute include should select the file when the config is the project's own, got: {directly}"
    );

    let through_extends = run(extending);
    assert!(
        through_extends.contains("note.md.jinja"),
        "the same include reached through extends should select the same file, got: {through_extends}"
    );
}

/// The empty-run notice names an `include` pattern that selected nothing, which
/// is as much the extends target's own text as one that does not compile. It is
/// named by the file it came from instead.
#[test]
fn test_extends_unmatched_include_is_not_quoted_in_the_empty_run_notice() {
    let dir = tempdir().unwrap();
    let base_dir = dir.path().join("shared");
    fs::create_dir(&base_dir).unwrap();
    let base = base_dir.join("base.rumdl.toml");
    // A pattern that compiles and matches nothing, beside one that does match,
    // so the run is a normal one rather than a diagnosis of a broken config.
    fs::write(&base, format!("[global]\ninclude = [\"docs/{SECRET}/*.md\"]\n")).unwrap();

    let project = dir.path().join("project");
    fs::create_dir_all(project.join("docs")).unwrap();
    fs::write(project.join("docs/note.md"), "no heading here\n").unwrap();
    fs::write(
        project.join(".rumdl.toml"),
        "extends = '$RUMDL_TEST_EXTENDS_NOMATCH_DIR/base.rumdl.toml'\n",
    )
    .unwrap();

    let run = |cwd: &std::path::Path| -> String {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["check", "--no-cache"])
            .current_dir(cwd)
            .env("RUMDL_TEST_EXTENDS_NOMATCH_DIR", &base_dir)
            .output()
            .expect("failed to run the rumdl binary");
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    };

    let through_extends = run(&project);
    assert!(
        !through_extends.contains(SECRET),
        "the empty-run notice quoted a pattern read out of the extends target: {through_extends}"
    );
    assert!(
        through_extends.contains("1 include pattern in '$RUMDL_TEST_EXTENDS_NOMATCH_DIR/base.rumdl.toml'")
            && through_extends.contains("matches no file"),
        "the notice still has to say an include selected nothing and where it came from, got: {through_extends}"
    );

    // Positive control: the same include in the project's own config is quoted,
    // which is what makes the notice worth printing.
    let named = dir.path().join("named");
    fs::create_dir_all(named.join("docs")).unwrap();
    fs::write(named.join("docs/note.md"), "no heading here\n").unwrap();
    fs::write(
        named.join(".rumdl.toml"),
        format!("[global]\ninclude = [\"docs/{SECRET}/*.md\"]\n"),
    )
    .unwrap();
    let directly = run(&named);
    assert!(
        directly.contains(&format!("include pattern 'docs/{SECRET}/*.md' matches no file")),
        "a directly named config should still have its unmatched pattern quoted, got: {directly}"
    );
}

/// An `include` pattern that does not compile in any form is judged where it is
/// used. The walk reports it, and from an `extends` target it reports it without
/// quoting it.
#[test]
fn test_extends_uncompilable_include_is_reported_without_quoting_it() {
    let dir = tempdir().unwrap();
    let base_dir = dir.path().join("shared");
    fs::create_dir(&base_dir).unwrap();
    let base = base_dir.join("base.rumdl.toml");
    fs::write(&base, format!("[global]\ninclude = [\"docs/[{SECRET}/*.md\"]\n")).unwrap();

    let project = dir.path().join("project");
    fs::create_dir_all(project.join("docs")).unwrap();
    fs::write(project.join("docs/note.md"), "no heading here\n").unwrap();
    fs::write(
        project.join(".rumdl.toml"),
        "extends = '$RUMDL_TEST_EXTENDS_BADINC_DIR/base.rumdl.toml'\n",
    )
    .unwrap();

    // No RUST_LOG: the message this replaces is printed unconditionally, so this
    // one has to be too. stdout comes along because the empty-run notice that
    // lists unmatched include patterns is printed there.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache"])
        .current_dir(&project)
        .env("RUMDL_TEST_EXTENDS_BADINC_DIR", &base_dir)
        .output()
        .expect("failed to run the rumdl binary");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let combined = format!("{stderr}{}", String::from_utf8_lossy(&output.stdout));

    assert!(
        !combined.contains(SECRET),
        "the walk echoed a pattern read out of the extends target: {combined}"
    );
    assert!(
        stderr.contains("Invalid include pattern in") && stderr.contains("<withheld>"),
        "the problem still has to be reported, got: {stderr}"
    );

    // Positive control, and proof this harness can see the pattern at all: the
    // same patterns written in the project's own config are quoted back.
    let named = dir.path().join("named");
    fs::create_dir_all(named.join("docs")).unwrap();
    fs::write(named.join("docs/note.md"), "no heading here\n").unwrap();
    fs::write(
        named.join(".rumdl.toml"),
        format!("[global]\ninclude = [\"docs/[{SECRET}/*.md\"]\n"),
    )
    .unwrap();
    let directly = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache"])
        .current_dir(&named)
        .output()
        .expect("failed to run the rumdl binary");
    assert!(
        String::from_utf8_lossy(&directly.stderr).contains(SECRET),
        "a directly named config should still name the invalid include pattern, got: {}",
        String::from_utf8_lossy(&directly.stderr)
    );
}

/// An option key a rule recognizes can still hold a value the rule cannot read,
/// and the rule quotes that value back when it deserializes its section. Only
/// the rule knows what it accepts, so the value cannot be checked at parse time
/// the way a rule name or a glob can: it is marked there instead, and the rule
/// leaves it out of the message.
#[test]
fn test_extends_invalid_rule_option_value_is_withheld() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("secrets");
    fs::create_dir(&target_dir).unwrap();

    let base = target_dir.join("base.rumdl.toml");
    fs::write(&base, format!("[MD003]\nstyle = \"{SECRET}\"\n")).unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_VALUE_DIR/base.rumdl.toml'\n").unwrap();

    let doc = dir.path().join("doc.md");
    fs::write(&doc, "# Title\n").unwrap();

    // No RUST_LOG: the rule prints straight to stderr, so the message standing
    // in for it has to be as visible as the one it replaces.
    let run = |config: &std::path::Path| -> String {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["check", "--no-cache", "--config"])
            .arg(config)
            .arg(&doc)
            .env("RUMDL_TEST_EXTENDS_VALUE_DIR", &target_dir)
            .output()
            .expect("failed to run the rumdl binary");
        String::from_utf8_lossy(&output.stderr).into_owned()
    };

    let through_extends = run(&child);
    assert!(
        !through_extends.contains(SECRET),
        "the warning echoed a rule option value read out of the extends target: {through_extends}"
    );
    assert!(
        through_extends.contains("Invalid configuration for rule MD003: <withheld>"),
        "the problem still has to be reported: {through_extends}"
    );

    // Positive control, and proof this harness can see the value at all: the
    // same file named directly still has its invalid value quoted back.
    let directly = run(&base);
    assert!(
        directly.contains(SECRET),
        "a directly named config should still name the invalid value, got: {directly}"
    );
}

/// Marking a rule option's value is not dropping it: the rule still reads what
/// the extended file set.
#[test]
#[serial_test::serial]
fn test_extends_valid_rule_option_values_still_apply() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("secrets");
    fs::create_dir(&target_dir).unwrap();
    let _guard = EnvVarGuard::set("RUMDL_TEST_EXTENDS_VALUE2_DIR", &target_dir);

    let base = target_dir.join("base.rumdl.toml");
    fs::write(&base, "[MD003]\nstyle = \"setext\"\n").unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_VALUE2_DIR/base.rumdl.toml'\n").unwrap();

    let config = load_config(&child).unwrap();
    assert_eq!(
        config.rules["MD003"].values["style"].as_str(),
        Some("setext"),
        "the extended file's value should survive being marked"
    );
}

/// The mark travels with the value, so a config naming the value itself gets it
/// quoted back: what may be said about a value follows whichever file supplied
/// the one that won.
#[test]
fn test_rule_option_value_named_by_the_extending_config_is_shown() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("secrets");
    fs::create_dir(&target_dir).unwrap();

    let base = target_dir.join("base.rumdl.toml");
    fs::write(&base, format!("[MD003]\nstyle = \"{SECRET}\"\n")).unwrap();

    // The child overrides the same key, so the base's value never reaches the
    // rule and the value the rule does report is one this project wrote.
    let child = dir.path().join("child.rumdl.toml");
    fs::write(
        &child,
        "extends = '$RUMDL_TEST_EXTENDS_VALUE3_DIR/base.rumdl.toml'\n[MD003]\nstyle = \"child-typo\"\n",
    )
    .unwrap();

    let doc = dir.path().join("doc.md");
    fs::write(&doc, "# Title\n").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--config"])
        .arg(&child)
        .arg(&doc)
        .env("RUMDL_TEST_EXTENDS_VALUE3_DIR", &target_dir)
        .output()
        .expect("failed to run the rumdl binary");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("child-typo"),
        "the extending config's own value should be quoted back, got: {stderr}"
    );
    assert!(
        !stderr.contains(SECRET),
        "the overridden value should not appear at all, got: {stderr}"
    );
}

/// Some option values deserialize cleanly and only fail when the rule tries to
/// use them. Those are reported by the rule itself, long past the point where the
/// section was read, so the rule has to be told what it may quote.
#[test]
fn test_extends_uncompilable_rule_pattern_is_withheld() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("secrets");
    fs::create_dir(&target_dir).unwrap();

    // A well-formed string, and not a regex: MD051 accepts the value and only
    // finds out it cannot compile when it builds its filter.
    let base = target_dir.join("base.rumdl.toml");
    fs::write(&base, format!("[MD051]\nignored-pattern = \"[{SECRET}\"\n")).unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_REGEX_DIR/base.rumdl.toml'\n").unwrap();

    let doc = dir.path().join("doc.md");
    fs::write(&doc, "# Title\n").unwrap();

    let run = |config: &std::path::Path| -> String {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["check", "--no-cache", "--config"])
            .arg(config)
            .arg(&doc)
            .env("RUST_LOG", "warn")
            .env("RUMDL_TEST_EXTENDS_REGEX_DIR", &target_dir)
            .output()
            .expect("failed to run the rumdl binary");
        String::from_utf8_lossy(&output.stderr).into_owned()
    };

    let through_extends = run(&child);
    assert!(
        !through_extends.contains(SECRET),
        "the warning echoed a pattern read out of the extends target: {through_extends}"
    );
    assert!(
        through_extends.contains("Invalid ignored-pattern for MD051: <withheld>"),
        "the problem still has to be reported: {through_extends}"
    );

    // Positive control, and proof this harness can see the pattern at all: the
    // same file named directly still has its invalid pattern quoted back.
    let directly = run(&base);
    assert!(
        directly.contains(SECRET),
        "a directly named config should still name the invalid pattern, got: {directly}"
    );
}

/// Withholding what may be said about a pattern must not stop it working. A
/// pattern that does compile still filters what the rule reports.
#[test]
fn test_extends_compilable_rule_pattern_still_applies() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("secrets");
    fs::create_dir(&target_dir).unwrap();

    let base = target_dir.join("base.rumdl.toml");
    fs::write(&base, "[MD051]\nignored-pattern = \".*\"\n").unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_REGEX2_DIR/base.rumdl.toml'\n").unwrap();

    let doc = dir.path().join("doc.md");
    fs::write(&doc, "# Title\n\n[link](#no-such-anchor)\n").unwrap();

    let run = |config: Option<&std::path::Path>| -> String {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
        command.args(["check", "--no-cache"]);
        match config {
            Some(path) => {
                command.arg("--config").arg(path);
            }
            None => {
                command.arg("--no-config");
            }
        }
        let output = command
            .arg(&doc)
            .env("RUMDL_TEST_EXTENDS_REGEX2_DIR", &target_dir)
            .output()
            .expect("failed to run the rumdl binary");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    // Negative control first: without the pattern the fragment is reported, so
    // the run below has something to suppress.
    let without = run(None);
    assert!(
        without.contains("MD051"),
        "the fragment should be reported without the pattern, got: {without}"
    );

    let with = run(Some(&child));
    assert!(
        !with.contains("MD051"),
        "the extended file's pattern should still filter, got: {with}"
    );
}

/// `[code-block-tools]` names tools the registry resolves while a document is
/// being linted, and it reports an id it cannot resolve. That id is text out of
/// whichever file supplied the section.
#[test]
fn test_extends_unknown_code_block_tool_is_withheld() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("secrets");
    fs::create_dir(&target_dir).unwrap();

    let base = target_dir.join("base.rumdl.toml");
    fs::write(
        &base,
        format!("[code-block-tools]\nenabled = true\n\n[code-block-tools.languages.python]\nlint = [\"{SECRET}\"]\n"),
    )
    .unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = '$RUMDL_TEST_EXTENDS_TOOL_DIR/base.rumdl.toml'\n").unwrap();

    let doc = dir.path().join("doc.md");
    fs::write(&doc, "# Title\n\n```python\nx = 1\n```\n").unwrap();

    let run = |config: &std::path::Path| -> String {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["check", "--no-cache", "--config"])
            .arg(config)
            .arg(&doc)
            .env("RUST_LOG", "warn")
            .env("RUMDL_TEST_EXTENDS_TOOL_DIR", &target_dir)
            .output()
            .expect("failed to run the rumdl binary");
        String::from_utf8_lossy(&output.stderr).into_owned()
    };

    let through_extends = run(&child);
    assert!(
        !through_extends.contains(SECRET),
        "the warning echoed a tool id read out of the extends target: {through_extends}"
    );
    assert!(
        through_extends.contains("Unknown tool <withheld> configured for language <withheld>"),
        "the problem still has to be reported: {through_extends}"
    );

    // Positive control, and proof this harness can see the id at all: the same
    // file named directly still has its unknown tool quoted back.
    let directly = run(&base);
    assert!(
        directly.contains(SECRET),
        "a directly named config should still name the unknown tool, got: {directly}"
    );
}

/// An `extends` value is a line of the file that wrote it, so a file that was
/// itself reached through `extends` does not get that one line quoted while the
/// rest of it is withheld. Here the reference fails to resolve.
#[test]
fn test_extends_nested_reference_is_withheld() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("shared");
    fs::create_dir(&target_dir).unwrap();

    let base = target_dir.join("base.rumdl.toml");
    fs::write(&base, format!("extends = 'missing-{SECRET}/further.rumdl.toml'\n")).unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = 'shared/base.rumdl.toml'\n").unwrap();

    match load_config(&child).unwrap_err() {
        err @ ConfigError::ExtendsNotFound { .. } => {
            let message = err.to_string();
            assert!(
                !message.contains(SECRET),
                "the error quoted an extends value read out of the extends target: {message}"
            );
            assert!(
                message.contains("<withheld>"),
                "withholding must be visible rather than leaving an empty name, got: {message}"
            );
            assert!(
                message.contains("'shared/base.rumdl.toml'"),
                "the file holding the unusable reference is what makes this fixable, got: {message}"
            );
        }
        other => panic!("Expected ExtendsNotFound, got: {other:?}"),
    }

    // Positive control: the same reference written by the config the user named
    // is quoted in full, so this harness can see such a value at all and depth is
    // what changes the answer.
    let direct = dir.path().join("direct.rumdl.toml");
    fs::write(&direct, format!("extends = 'missing-{SECRET}/further.rumdl.toml'\n")).unwrap();
    let message = load_config(&direct).unwrap_err().to_string();
    assert!(
        message.contains(SECRET),
        "a directly named config should still have its own reference quoted, got: {message}"
    );
}

/// The name every message uses for a file at depth two is written in the file at
/// depth one, so a warning about the deeper file cannot quote its name either.
/// This is the ordinary path rather than an error path: the run succeeds and
/// prints a `[config warning]`.
#[test]
fn test_extends_warning_about_a_nested_target_withholds_its_name() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("shared");
    fs::create_dir(&target_dir).unwrap();

    let deep = target_dir.join(format!("{SECRET}.rumdl.toml"));
    fs::write(&deep, "[global]\nprod-database-password = true\n").unwrap();

    let base = target_dir.join("base.rumdl.toml");
    fs::write(&base, format!("extends = '{SECRET}.rumdl.toml'\n")).unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = 'shared/base.rumdl.toml'\n").unwrap();

    let unknown_key = |config: &std::path::Path| -> String {
        let messages = validation_warnings(config);
        messages
            .iter()
            .find(|m| m.contains("Unknown global option"))
            .unwrap_or_else(|| panic!("expected a warning for the unknown key, got: {messages:?}"))
            .clone()
    };

    let deep_warning = unknown_key(&child);
    assert!(
        !deep_warning.contains(SECRET),
        "the warning named the deeper file with text out of the file that reached for it: {deep_warning}"
    );
    assert!(
        deep_warning.contains("(referenced from 'shared/base.rumdl.toml')"),
        "which file reached for it is the extending config's own text and still has to be said, got: {deep_warning}"
    );

    // Positive control: at depth one the same name is the child's own text, and
    // the child is a config the user named. It is quoted, which is also what
    // proves this harness can see the name at all.
    let shallow = dir.path().join("shallow.rumdl.toml");
    fs::write(&shallow, format!("extends = 'shared/{SECRET}.rumdl.toml'\n")).unwrap();
    let shallow_warning = unknown_key(&shallow);
    assert!(
        shallow_warning.contains(SECRET),
        "a reference the named config wrote itself should still be quoted, got: {shallow_warning}"
    );
}

/// The variable name in a nested `extends` value is part of that value, so a set
/// and an unset variable disclose the same amount: nothing.
#[test]
#[serial_test::serial]
fn test_extends_nested_undefined_variable_is_withheld() {
    // SAFETY: per-test-process isolation under nextest + `#[serial_test::serial]`;
    // see `EnvVarGuard::set`.
    unsafe { std::env::remove_var("RUMDL_TEST_UNSET_CUSTOMER_ACME_INTERNAL") };

    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("shared");
    fs::create_dir(&target_dir).unwrap();

    let base = target_dir.join("base.rumdl.toml");
    fs::write(
        &base,
        "extends = '$RUMDL_TEST_UNSET_CUSTOMER_ACME_INTERNAL/base.toml'\n",
    )
    .unwrap();

    let child = dir.path().join("child.rumdl.toml");
    fs::write(&child, "extends = 'shared/base.rumdl.toml'\n").unwrap();

    match load_config(&child).unwrap_err() {
        ConfigError::ExtendsUndefinedVar { var, from } => {
            assert_eq!(var, "<withheld>", "the variable name is text out of the extends target");
            assert!(
                from.contains("base.rumdl.toml"),
                "the file that referenced it still has to be named, got from: {from}"
            );
        }
        other => panic!("Expected ExtendsUndefinedVar, got: {other:?}"),
    }

    // Positive control: the same value in a config the user named still names the
    // variable, which is what makes that error fixable without opening a file.
    let direct = dir.path().join("direct.rumdl.toml");
    fs::write(
        &direct,
        "extends = '$RUMDL_TEST_UNSET_CUSTOMER_ACME_INTERNAL/base.toml'\n",
    )
    .unwrap();
    match load_config(&direct).unwrap_err() {
        ConfigError::ExtendsUndefinedVar { var, .. } => {
            assert_eq!(var, "$RUMDL_TEST_UNSET_CUSTOMER_ACME_INTERNAL");
        }
        other => panic!("Expected ExtendsUndefinedVar, got: {other:?}"),
    }
}
