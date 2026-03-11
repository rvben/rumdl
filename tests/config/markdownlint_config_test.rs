use rumdl_lib::config::{Config, ConfigSource, SourcedConfig, normalize_key};
use rumdl_lib::markdownlint_config::MarkdownlintConfig;
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_markdownlint_config_mapping() {
    // Sample markdownlint config from https://github.com/DavidAnson/markdownlint/blob/main/.markdownlint.json
    let config_str = r#"{
        "code-block-style": { "style": "fenced" },
        "code-fence-style": { "style": "backtick" },
        "emphasis-style": { "style": "asterisk" },
        "extended-ascii": { "ascii-only": true },
        "fenced-code-language": {
            "allowed_languages": ["bash", "html", "javascript", "json", "markdown", "text"],
            "language_only": true
        },
        "heading-style": { "style": "atx" },
        "hr-style": { "style": "---" },
        "line-length": { "strict": true, "code_blocks": false },
        "link-image-style": { "collapsed": false, "shortcut": false, "url_inline": false },
        "no-duplicate-heading": { "siblings_only": true },
        "ol-prefix": { "style": "ordered" },
        "proper-names": {
            "code_blocks": false,
            "names": [
                "Cake.Markdownlint",
                "CommonMark",
                "JavaScript",
                "Markdown",
                "markdown-it",
                "markdownlint",
                "Node.js"
            ]
        },
        "reference-links-images": { "shortcut_syntax": true },
        "strong-style": { "style": "asterisk" },
        "ul-style": { "style": "dash" }
    }"#;

    // Parse as MarkdownlintConfig
    let ml_config: MarkdownlintConfig = serde_json::from_str(config_str).expect("Failed to parse markdownlint config");
    let sourced = ml_config.map_to_sourced_rumdl_config(Some("test_markdownlint.json"));
    let rumdl_config: rumdl_lib::config::Config = sourced.into_validated_unchecked().into();

    // Check that all expected rules are mapped
    let expected_rules = vec![
        ("MD046", "code-block-style"),
        ("MD048", "code-fence-style"),
        ("MD049", "emphasis-style"),
        // "extended-ascii" is not mapped (not a standard markdownlint rule)
        ("MD040", "fenced-code-language"),
        ("MD003", "heading-style"),
        ("MD035", "hr-style"),
        ("MD013", "line-length"),
        ("MD054", "link-image-style"),
        ("MD024", "no-duplicate-heading"),
        ("MD029", "ol-prefix"),
        ("MD044", "proper-names"),
        ("MD052", "reference-links-images"),
        ("MD050", "strong-style"),
        ("MD004", "ul-style"),
    ];

    for (rumdl_key, ml_key) in &expected_rules {
        assert!(
            rumdl_config.rules.contains_key(*rumdl_key),
            "Missing mapping for {rumdl_key} (from {ml_key})"
        );
    }

    // MD046: code-block-style
    let code_block_style = &rumdl_config.rules["MD046"].values["style"];
    assert!(code_block_style.as_str().unwrap() == "fenced");

    // MD048: code-fence-style
    let code_fence_style = &rumdl_config.rules["MD048"].values["style"];
    assert!(code_fence_style.as_str().unwrap() == "backtick");

    // MD049: emphasis-style
    let emphasis_style = &rumdl_config.rules["MD049"].values["style"];
    assert!(emphasis_style.as_str().unwrap() == "asterisk");

    // MD040: fenced-code-language
    let fenced_code_langs = &rumdl_config.rules["MD040"].values[&normalize_key("allowed_languages")];
    let langs = fenced_code_langs.as_array().unwrap();
    let expected_langs = ["bash", "html", "javascript", "json", "markdown", "text"];
    for (i, lang) in expected_langs.iter().enumerate() {
        assert!(langs[i].as_str().unwrap() == *lang);
    }
    let language_only = &rumdl_config.rules["MD040"].values[&normalize_key("language_only")];
    assert!(language_only.as_bool().unwrap());

    // MD003: heading-style
    let heading_style = &rumdl_config.rules["MD003"].values["style"];
    assert!(heading_style.as_str().unwrap() == "atx");

    // MD035: hr-style
    let hr_style = &rumdl_config.rules["MD035"].values["style"];
    assert!(hr_style.as_str().unwrap() == "---");

    // MD013: line-length
    let line_length_strict = &rumdl_config.rules["MD013"].values["strict"];
    assert!(line_length_strict.as_bool().unwrap());
    let line_length_code_blocks = &rumdl_config.rules["MD013"].values[&normalize_key("code_blocks")];
    assert!(!line_length_code_blocks.as_bool().unwrap());

    // MD054: link-image-style
    let link_image_collapsed = &rumdl_config.rules["MD054"].values["collapsed"];
    assert!(!link_image_collapsed.as_bool().unwrap());
    let link_image_shortcut = &rumdl_config.rules["MD054"].values["shortcut"];
    assert!(!link_image_shortcut.as_bool().unwrap());
    let link_image_url_inline = &rumdl_config.rules["MD054"].values[&normalize_key("url_inline")];
    assert!(!link_image_url_inline.as_bool().unwrap());

    // MD024: no-duplicate-heading
    let no_duplicate_heading_siblings = &rumdl_config.rules["MD024"].values[&normalize_key("siblings_only")];
    assert!(no_duplicate_heading_siblings.as_bool().unwrap());

    // MD029: ol-prefix
    let ol_prefix_style = &rumdl_config.rules["MD029"].values["style"];
    assert!(ol_prefix_style.as_str().unwrap() == "ordered");

    // MD044: proper-names
    let proper_names_code_blocks = &rumdl_config.rules["MD044"].values[&normalize_key("code_blocks")];
    assert!(!proper_names_code_blocks.as_bool().unwrap());
    let proper_names_names = &rumdl_config.rules["MD044"].values[&normalize_key("names")];
    let expected_names = [
        "Cake.Markdownlint",
        "CommonMark",
        "JavaScript",
        "Markdown",
        "markdown-it",
        "markdownlint",
        "Node.js",
    ];
    let names = proper_names_names.as_array().unwrap();
    for (i, name) in expected_names.iter().enumerate() {
        assert!(names[i].as_str().unwrap() == *name);
    }

    // MD052: reference-links-images
    let ref_links_shortcut = &rumdl_config.rules["MD052"].values[&normalize_key("shortcut_syntax")];
    assert!(ref_links_shortcut.as_bool().unwrap());

    // MD050: strong-style
    let strong_style = &rumdl_config.rules["MD050"].values["style"];
    assert!(strong_style.as_str().unwrap() == "asterisk");

    // MD004: ul-style
    let ul_style = &rumdl_config.rules["MD004"].values["style"];
    assert!(ul_style.as_str().unwrap() == "dash");

    // Check that unmapped rules are not present
    assert!(!rumdl_config.rules.contains_key("extended-ascii"));
}

#[test]
fn test_markdownlint_config_provenance() {
    let config_str = r#"{
        "code-block-style": { "style": "fenced" },
        "ul-style": { "style": "dash" },
        "heading-style": { "style": "atx" }
    }"#;
    let fake_path = "test_markdownlint.json";
    let ml_config = load_markdownlint_config_from_str(config_str).expect("Failed to parse markdownlint config");
    let sourced = ml_config.map_to_sourced_rumdl_config(Some(fake_path));

    // Check loaded_files
    assert!(sourced.loaded_files == vec![fake_path.to_string()]);

    // Check that rules are present and provenance is correct
    let rule = sourced.rules.get("MD046").expect("MD046 missing");
    let style = rule.values.get("style").expect("style missing");
    assert!(style.source == ConfigSource::ProjectConfig);
    assert!(style.value.as_str().unwrap() == "fenced");

    let ul_rule = sourced.rules.get("MD004").expect("MD004 missing");
    let ul_style = ul_rule.values.get("style").expect("ul style missing");
    assert!(ul_style.source == ConfigSource::ProjectConfig);
    assert!(ul_style.value.as_str().unwrap() == "dash");

    let heading_rule = sourced.rules.get("MD003").expect("MD003 missing");
    let heading_style = heading_rule.values.get("style").expect("heading style missing");
    assert!(heading_style.source == ConfigSource::ProjectConfig);
    assert!(heading_style.value.as_str().unwrap() == "atx");
}

#[test]
fn test_markdownlint_config_provenance_debug_output() {
    let config_str = r#"{
        "code-block-style": { "style": "fenced" },
        "ul-style": { "style": "dash" }
    }"#;
    let fake_path = "test_markdownlint.json";
    let ml_config = load_markdownlint_config_from_str(config_str).expect("Failed to parse markdownlint config");
    let sourced = ml_config.map_to_sourced_rumdl_config(Some(fake_path));

    // No debug prints; just check presence for completeness
    let _ = sourced.rules.get("MD004").and_then(|r| r.values.get("style"));
    let _ = sourced.rules.get("MD046").and_then(|r| r.values.get("style"));
}

// Helper to parse from string for test
fn load_markdownlint_config_from_str(s: &str) -> Result<rumdl_lib::markdownlint_config::MarkdownlintConfig, String> {
    serde_json::from_str(s).map_err(|e| format!("Failed to parse JSON: {e}"))
}

#[test]
fn test_multiple_disabled_rules_migration() {
    // Test case from issue #64 - multiple rules set to false should all be in the disable list
    let config_str = r#"
MD007:
  indent: 4
MD033: false
MD041: false
MD013: false
MD014: false
MD024:
  siblings_only: true
MD046: false
MD060: false
"#;

    let config_map: HashMap<String, serde_yml::Value> = serde_yml::from_str(config_str).expect("Failed to parse YAML");
    let ml_config = MarkdownlintConfig(config_map);
    let fragment = ml_config.map_to_sourced_rumdl_config_fragment(Some("test.yaml"));

    // Check that all disabled rules are in the disable list
    let expected_disabled = vec!["MD013", "MD014", "MD033", "MD041", "MD046", "MD060"];
    let disabled = &fragment.global.disable.value;

    // Sort both for easier comparison
    let mut expected_sorted = expected_disabled.clone();
    expected_sorted.sort();
    let mut disabled_sorted: Vec<String> = disabled.clone();
    disabled_sorted.sort();

    assert_eq!(
        disabled_sorted, expected_sorted,
        "Expected disabled rules {expected_sorted:?} but got {disabled_sorted:?}"
    );

    // Check that MD007 and MD024 configs are preserved
    assert!(fragment.rules.contains_key("MD007"));
    assert_eq!(fragment.rules["MD007"].values["indent"].value.as_integer(), Some(4));

    assert!(fragment.rules.contains_key("MD024"));
    assert_eq!(
        fragment.rules["MD024"].values["siblings-only"].value.as_bool(),
        Some(true)
    );
}

#[test]
fn test_default_true_boolean_rules_dont_create_allowlist() {
    // Issue #389: When default: true + MD001: true + MD013: { line_length: 120 },
    // MD013 should still be active (no exclusive enable list created).
    let temp = tempdir().unwrap();
    let config_path = temp.path().join(".markdownlint.yaml");
    fs::write(
        &config_path,
        r#"default: true
MD001: true
MD013:
  line_length: 120
"#,
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
        .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    // No enable list should be set — boolean true is no-op when default: true
    assert!(
        config.global.enable.is_empty(),
        "Enable list should be empty when default: true"
    );
    // No disable list
    assert!(config.global.disable.is_empty(), "No rules should be disabled");
    // MD013 config should be preserved
    assert!(config.rules.contains_key("MD013"), "MD013 should be configured");
    let md013 = &config.rules["MD013"];
    assert_eq!(
        md013.values.get("line-length").and_then(|v| v.as_integer()),
        Some(120),
        "MD013 line-length should be 120"
    );
}

#[test]
fn test_default_false_enables_configured_rules() {
    // When default: false, only explicitly enabled/configured rules should be active
    let temp = tempdir().unwrap();
    let config_path = temp.path().join(".markdownlint.yaml");
    fs::write(
        &config_path,
        r#"default: false
MD001: true
MD013:
  line_length: 120
"#,
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
        .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    // Both MD001 (boolean true) and MD013 (object config) should be in the enable list
    let mut enabled = config.global.enable.clone();
    enabled.sort();
    assert_eq!(
        enabled,
        vec!["MD001", "MD013"],
        "Both boolean-true and config-object rules should be enabled"
    );
    // No rules disabled
    assert!(
        config.global.disable.is_empty(),
        "No rules should be disabled when default: false"
    );
    // MD013 config should be preserved
    assert!(config.rules.contains_key("MD013"), "MD013 should be configured");
    let md013 = &config.rules["MD013"];
    assert_eq!(
        md013.values.get("line-length").and_then(|v| v.as_integer()),
        Some(120),
        "MD013 line-length should be 120"
    );
}

#[test]
fn test_default_absent_same_as_default_true() {
    // When `default` key is absent, it should behave the same as default: true
    let temp = tempdir().unwrap();
    let config_path = temp.path().join(".markdownlint.yaml");
    fs::write(
        &config_path,
        r#"MD001: true
MD009: false
MD013:
  line_length: 80
"#,
    )
    .unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
        .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    // No enable list: boolean true is no-op
    assert!(
        config.global.enable.is_empty(),
        "Enable list should be empty when default absent"
    );
    // enable_is_explicit should be false when default is absent
    assert!(
        !config.global.enable_is_explicit,
        "Enable should not be explicit when default absent"
    );
    // MD009 should be disabled
    assert!(
        config.global.disable.contains(&"MD009".to_string()),
        "MD009 should be disabled"
    );
    // MD013 config should be preserved
    assert!(config.rules.contains_key("MD013"));
}

#[test]
fn test_default_false_no_rules_disables_all() {
    // default: false with no other rules → enable list is empty but explicit,
    // which means no rules should run
    let temp = tempdir().unwrap();
    let config_path = temp.path().join(".markdownlint.yaml");
    fs::write(&config_path, "default: false\n").unwrap();

    let sourced = SourcedConfig::load_with_discovery(Some(config_path.to_str().unwrap()), None, true)
        .expect("Should load config");
    let config: Config = sourced.into_validated_unchecked().into();

    assert!(config.global.enable.is_empty(), "Enable list should be empty");
    assert!(
        config.global.enable_is_explicit,
        "Enable should be explicit when default: false"
    );
}
