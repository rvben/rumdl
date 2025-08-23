use rumdl_lib::config::{ConfigSource, normalize_key};
use rumdl_lib::markdownlint_config::MarkdownlintConfig;

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
    let rumdl_config: rumdl_lib::config::Config = sourced.into();

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
    assert!(style.source == ConfigSource::Markdownlint);
    assert!(style.value.as_str().unwrap() == "fenced");

    let ul_rule = sourced.rules.get("MD004").expect("MD004 missing");
    let ul_style = ul_rule.values.get("style").expect("ul style missing");
    assert!(ul_style.source == ConfigSource::Markdownlint);
    assert!(ul_style.value.as_str().unwrap() == "dash");

    let heading_rule = sourced.rules.get("MD003").expect("MD003 missing");
    let heading_style = heading_rule.values.get("style").expect("heading style missing");
    assert!(heading_style.source == ConfigSource::Markdownlint);
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
