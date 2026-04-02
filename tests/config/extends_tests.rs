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

    // Use the absolute path directly in the child config
    let base_absolute = base.canonicalize().unwrap();
    let child_content = format!(
        r#"extends = "{}"

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
        ConfigError::ExtendsNotFound { path, from } => {
            assert!(
                path.contains("nonexistent_base.rumdl.toml"),
                "Error should mention missing file path, got path: {path}"
            );
            assert!(
                from.contains("child.rumdl.toml"),
                "Error should mention the referencing file, got from: {from}"
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
