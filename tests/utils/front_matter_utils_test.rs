use rumdl::rules::front_matter_utils::{FrontMatterType, FrontMatterUtils};

#[test]
fn test_detect_front_matter_type() {
    // YAML front matter
    let yaml_content = "---\ntitle: Test Document\ndate: 2023-04-01\n---\n# Heading";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(yaml_content),
        FrontMatterType::Yaml
    );

    // TOML front matter
    let toml_content = "+++\ntitle = \"Test Document\"\ndate = 2023-04-01\n+++\n# Heading";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(toml_content),
        FrontMatterType::Toml
    );

    // JSON front matter
    let json_content = "{\n\"title\": \"Test Document\",\n\"date\": \"2023-04-01\"\n}\n# Heading";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(json_content),
        FrontMatterType::Json
    );

    // Malformed front matter
    let malformed_content1 = "- --\ntitle: Test Document\ndate: 2023-04-01\n- --\n# Heading";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(malformed_content1),
        FrontMatterType::Malformed
    );

    let malformed_content2 = "-- -\ntitle: Test Document\ndate: 2023-04-01\n-- -\n# Heading";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(malformed_content2),
        FrontMatterType::Malformed
    );

    // No front matter
    let no_front_matter = "# Heading\nThis is content.";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(no_front_matter),
        FrontMatterType::None
    );

    // Empty document
    let empty_content = "";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(empty_content),
        FrontMatterType::None
    );

    // Too short for front matter
    let short_content = "---\ntitle";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(short_content),
        FrontMatterType::None
    );
}

#[test]
fn test_is_in_front_matter() {
    // YAML front matter
    let yaml_content = "---\ntitle: Test Document\ndate: 2023-04-01\n---\n# Heading";
    assert!(FrontMatterUtils::is_in_front_matter(yaml_content, 1)); // Line with "title"
    assert!(FrontMatterUtils::is_in_front_matter(yaml_content, 2)); // Line with "date"
    assert!(!FrontMatterUtils::is_in_front_matter(yaml_content, 3)); // Closing delimiter
    assert!(!FrontMatterUtils::is_in_front_matter(yaml_content, 4)); // Line with heading

    // TOML front matter
    let toml_content = "+++\ntitle = \"Test Document\"\ndate = 2023-04-01\n+++\n# Heading";
    assert!(FrontMatterUtils::is_in_front_matter(toml_content, 1)); // Line with "title"
    assert!(FrontMatterUtils::is_in_front_matter(toml_content, 2)); // Line with "date"
    assert!(!FrontMatterUtils::is_in_front_matter(toml_content, 3)); // Closing delimiter
    assert!(!FrontMatterUtils::is_in_front_matter(toml_content, 4)); // Line with heading

    // JSON front matter
    let json_content = "{\n\"title\": \"Test Document\",\n\"date\": \"2023-04-01\"\n}\n# Heading";
    assert!(FrontMatterUtils::is_in_front_matter(json_content, 1)); // Line with "title"
    assert!(FrontMatterUtils::is_in_front_matter(json_content, 2)); // Line with "date"
    assert!(!FrontMatterUtils::is_in_front_matter(json_content, 3)); // Closing delimiter
    assert!(!FrontMatterUtils::is_in_front_matter(json_content, 4)); // Line with heading

    // Malformed front matter
    let malformed_content = "- --\ntitle: Test Document\ndate: 2023-04-01\n- --\n# Heading";
    assert!(FrontMatterUtils::is_in_front_matter(malformed_content, 1)); // Line with "title"
    assert!(FrontMatterUtils::is_in_front_matter(malformed_content, 2)); // Line with "date"
    assert!(!FrontMatterUtils::is_in_front_matter(malformed_content, 3)); // Closing delimiter
    assert!(!FrontMatterUtils::is_in_front_matter(malformed_content, 4)); // Line with heading

    // No front matter
    let no_front_matter = "# Heading\nThis is content.";
    assert!(!FrontMatterUtils::is_in_front_matter(no_front_matter, 0)); // Line with heading
    assert!(!FrontMatterUtils::is_in_front_matter(no_front_matter, 1)); // Line with content

    // Edge cases
    assert!(!FrontMatterUtils::is_in_front_matter("", 0)); // Empty document
    assert!(!FrontMatterUtils::is_in_front_matter("---\ntitle: Test", 5)); // Out of bounds
}

#[test]
fn test_extract_front_matter() {
    // YAML front matter
    let yaml_content = "---\ntitle: Test Document\ndate: 2023-04-01\n---\n# Heading";
    let yaml_fm = FrontMatterUtils::extract_front_matter(yaml_content);
    assert_eq!(yaml_fm.len(), 2);
    assert_eq!(yaml_fm[0], "title: Test Document");
    assert_eq!(yaml_fm[1], "date: 2023-04-01");

    // TOML front matter
    let toml_content = "+++\ntitle = \"Test Document\"\ndate = 2023-04-01\n+++\n# Heading";
    let toml_fm = FrontMatterUtils::extract_front_matter(toml_content);
    assert_eq!(toml_fm.len(), 2);
    assert_eq!(toml_fm[0], "title = \"Test Document\"");
    assert_eq!(toml_fm[1], "date = 2023-04-01");

    // JSON front matter
    let json_content = "{\n\"title\": \"Test Document\",\n\"date\": \"2023-04-01\"\n}\n# Heading";
    let json_fm = FrontMatterUtils::extract_front_matter(json_content);
    assert_eq!(json_fm.len(), 2);
    assert_eq!(json_fm[0], "\"title\": \"Test Document\",");
    assert_eq!(json_fm[1], "\"date\": \"2023-04-01\"");

    // Malformed front matter
    let malformed_content = "- --\ntitle: Test Document\ndate: 2023-04-01\n- --\n# Heading";
    let malformed_fm = FrontMatterUtils::extract_front_matter(malformed_content);
    assert_eq!(malformed_fm.len(), 2);
    assert_eq!(malformed_fm[0], "title: Test Document");
    assert_eq!(malformed_fm[1], "date: 2023-04-01");

    // No front matter
    let no_front_matter = "# Heading\nThis is content.";
    let no_fm = FrontMatterUtils::extract_front_matter(no_front_matter);
    assert!(no_fm.is_empty());

    // Edge cases
    let empty_content = "";
    assert!(FrontMatterUtils::extract_front_matter(empty_content).is_empty());

    let short_content = "---\ntitle";
    assert!(FrontMatterUtils::extract_front_matter(short_content).is_empty());
}

#[test]
fn test_has_front_matter_field() {
    // YAML front matter with field
    let yaml_content = "---\ntitle: Test Document\ndate: 2023-04-01\n---\n# Heading";
    assert!(FrontMatterUtils::has_front_matter_field(yaml_content, "title"));
    assert!(FrontMatterUtils::has_front_matter_field(yaml_content, "date"));
    assert!(!FrontMatterUtils::has_front_matter_field(yaml_content, "author"));

    // TOML front matter with field
    let toml_content = "+++\ntitle = \"Test Document\"\ndate = 2023-04-01\n+++\n# Heading";
    assert!(FrontMatterUtils::has_front_matter_field(toml_content, "title"));
    assert!(FrontMatterUtils::has_front_matter_field(toml_content, "date"));
    assert!(!FrontMatterUtils::has_front_matter_field(toml_content, "author"));

    // No front matter
    let no_front_matter = "# Heading\nThis is content.";
    assert!(!FrontMatterUtils::has_front_matter_field(no_front_matter, "title"));

    // Edge cases
    let empty_content = "";
    assert!(!FrontMatterUtils::has_front_matter_field(empty_content, "title"));

    let short_content = "---\ntitle";
    assert!(!FrontMatterUtils::has_front_matter_field(short_content, "title"));
}

#[test]
fn test_get_front_matter_field_value() {
    // YAML front matter
    let yaml_content = "---\ntitle: Test Document\ndate: 2023-04-01\n---\n# Heading";
    assert_eq!(
        FrontMatterUtils::get_front_matter_field_value(yaml_content, "title"),
        Some("Test Document")
    );
    assert_eq!(
        FrontMatterUtils::get_front_matter_field_value(yaml_content, "date"),
        Some("2023-04-01")
    );
    assert_eq!(
        FrontMatterUtils::get_front_matter_field_value(yaml_content, "author"),
        None
    );

    // TOML front matter
    let toml_content = "+++\ntitle = \"Test Document\"\ndate = 2023-04-01\n+++\n# Heading";
    assert_eq!(
        FrontMatterUtils::get_front_matter_field_value(toml_content, "title"),
        Some("Test Document")
    );
    assert_eq!(
        FrontMatterUtils::get_front_matter_field_value(toml_content, "date"),
        Some("2023-04-01")
    );
    assert_eq!(
        FrontMatterUtils::get_front_matter_field_value(toml_content, "author"),
        None
    );

    // No front matter
    let no_front_matter = "# Heading\nThis is content.";
    assert_eq!(
        FrontMatterUtils::get_front_matter_field_value(no_front_matter, "title"),
        None
    );

    // Edge cases
    let empty_content = "";
    assert_eq!(
        FrontMatterUtils::get_front_matter_field_value(empty_content, "title"),
        None
    );

    let short_content = "---\ntitle";
    assert_eq!(
        FrontMatterUtils::get_front_matter_field_value(short_content, "title"),
        None
    );
}

#[test]
fn test_extract_front_matter_fields() {
    // YAML front matter
    let yaml_content = "---\ntitle: Test Document\ndate: 2023-04-01\n---\n# Heading";
    let yaml_fields = FrontMatterUtils::extract_front_matter_fields(yaml_content);
    assert_eq!(yaml_fields.len(), 2);
    assert_eq!(yaml_fields.get("title"), Some(&"Test Document".to_string()));
    assert_eq!(yaml_fields.get("date"), Some(&"2023-04-01".to_string()));

    // TOML front matter
    let toml_content = "+++\ntitle = \"Test Document\"\ndate = 2023-04-01\n+++\n# Heading";
    let toml_fields = FrontMatterUtils::extract_front_matter_fields(toml_content);
    assert_eq!(toml_fields.len(), 2);
    assert_eq!(toml_fields.get("title"), Some(&"Test Document".to_string()));
    assert_eq!(toml_fields.get("date"), Some(&"2023-04-01".to_string()));

    // Front matter with nested fields
    let nested_content =
        "---\ntitle: Test Document\nmetadata:\n  date: 2023-04-01\n  author: Test Author\n---\n# Heading";
    let nested_fields = FrontMatterUtils::extract_front_matter_fields(nested_content);
    assert!(!nested_fields.is_empty());
    assert_eq!(nested_fields.get("title"), Some(&"Test Document".to_string()));

    // No front matter
    let no_front_matter = "# Heading\nThis is content.";
    let no_fields = FrontMatterUtils::extract_front_matter_fields(no_front_matter);
    assert!(no_fields.is_empty());

    // Edge cases
    let empty_content = "";
    assert!(FrontMatterUtils::extract_front_matter_fields(empty_content).is_empty());

    let short_content = "---\ntitle";
    assert!(FrontMatterUtils::extract_front_matter_fields(short_content).is_empty());
}

#[test]
fn test_complex_front_matter_scenarios() {
    // Front matter with complex values (lists, nested objects)
    let complex_yaml = "---\ntitle: Test Document\ntags: [rust, markdown, testing]\nmetadata:\n  date: 2023-04-01\n  author: Test Author\n---\n# Heading";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(complex_yaml),
        FrontMatterType::Yaml
    );
    assert!(FrontMatterUtils::has_front_matter_field(complex_yaml, "tags"));
    assert!(FrontMatterUtils::has_front_matter_field(complex_yaml, "metadata"));

    // Front matter with indentation
    let indented_yaml = "---\ntitle: Test Document\n  subtitle: With indentation\n---\n# Heading";
    let fields = FrontMatterUtils::extract_front_matter_fields(indented_yaml);
    assert_eq!(fields.get("title"), Some(&"Test Document".to_string()));

    // Front matter with colons in values
    let colons_yaml = "---\ntitle: Test: With Colon\nurl: https://example.com\n---\n# Heading";
    assert!(FrontMatterUtils::has_front_matter_field(colons_yaml, "url"));

    // Front matter with empty values
    let empty_values = "---\ntitle:\ndate:\n---\n# Heading";
    let fields = FrontMatterUtils::extract_front_matter_fields(empty_values);
    assert_eq!(fields.get("title"), Some(&"".to_string()));
    assert_eq!(fields.get("date"), Some(&"".to_string()));
}

#[test]
fn test_advanced_front_matter_handling() {
    // TOML with various value types
    let toml_with_types = "+++\n\
title = \"Test Document\"\n\
date = 2023-04-01\n\
published = true\n\
rating = 4.5\n\
tags = [\"markdown\", \"toml\", \"test\"]\n\
+++\n\
# Heading";

    // Test detection and extraction
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(toml_with_types),
        FrontMatterType::Toml
    );
    let toml_fields = FrontMatterUtils::extract_front_matter_fields(toml_with_types);

    // Verify various value types are extracted correctly
    assert_eq!(toml_fields.get("title"), Some(&"Test Document".to_string()));
    assert_eq!(toml_fields.get("date"), Some(&"2023-04-01".to_string()));
    assert_eq!(toml_fields.get("published"), Some(&"true".to_string()));
    assert_eq!(toml_fields.get("rating"), Some(&"4.5".to_string()));

    // TOML with nested tables
    let toml_nested = "+++\n\
title = \"Test Document\"\n\
[author]\n\
name = \"Test Author\"\n\
email = \"test@example.com\"\n\
[metadata]\n\
version = \"1.0.0\"\n\
+++\n\
# Heading";

    let nested_toml_fields = FrontMatterUtils::extract_front_matter_fields(toml_nested);
    assert_eq!(nested_toml_fields.get("title"), Some(&"Test Document".to_string()));

    // YAML with complex nested structures
    let complex_nested_yaml = "---\n\
title: Advanced YAML Test\n\
author:\n\
  name: Test Author\n\
  contact:\n\
    email: test@example.com\n\
    phone: 555-1234\n\
metadata:\n\
  tags:\n\
    - markdown\n\
    - yaml\n\
    - test\n\
  categories:\n\
    - documentation\n\
    - testing\n\
---\n\
# Heading";

    let complex_fields = FrontMatterUtils::extract_front_matter_fields(complex_nested_yaml);
    assert_eq!(complex_fields.get("title"), Some(&"Advanced YAML Test".to_string()));

    // The current implementation doesn't fully support deep nesting with dot notation
    // but it should at least extract the top-level "author" field
    assert!(complex_fields.contains_key("author"));

    // Edge case: YAML with quotes
    let yaml_with_quotes = "---\n\
title: \"Quoted Title\"\n\
description: 'Single quoted description'\n\
complex: \"Title with \\\"nested\\\" quotes\"\n\
---\n\
# Heading";

    let quoted_fields = FrontMatterUtils::extract_front_matter_fields(yaml_with_quotes);
    assert_eq!(quoted_fields.get("title"), Some(&"Quoted Title".to_string()));
    assert_eq!(
        quoted_fields.get("description"),
        Some(&"'Single quoted description'".to_string())
    );

    // Edge case: empty front matter
    let empty_frontmatter = "---\n---\n# Heading";
    let empty_fields = FrontMatterUtils::extract_front_matter_fields(empty_frontmatter);
    assert!(empty_fields.is_empty());

    // Edge case: front matter with special characters
    let special_chars = "---\n\
special: !@#$%^&*()\n\
path: /usr/local/bin\n\
regex: \\d+\\.\\d+\n\
---\n\
# Heading";

    let special_fields = FrontMatterUtils::extract_front_matter_fields(special_chars);
    assert_eq!(special_fields.get("special"), Some(&"!@#$%^&*()".to_string()));
    assert_eq!(special_fields.get("path"), Some(&"/usr/local/bin".to_string()));
    assert_eq!(special_fields.get("regex"), Some(&"\\d+\\.\\d+".to_string()));

    // Mixed delimiter edge case
    let mixed_delimiters = "---\n\
title: YAML Content\n\
+++\n\
# Not really front matter";

    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(mixed_delimiters),
        FrontMatterType::None
    );
}

#[test]
fn test_front_matter_distinction() {
    // Test how YAML and TOML are distinguished

    // Standard YAML
    let standard_yaml = "---\ntitle: YAML Document\n---\n# Content";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(standard_yaml),
        FrontMatterType::Yaml
    );

    // Standard TOML
    let standard_toml = "+++\ntitle = \"TOML Document\"\n+++\n# Content";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(standard_toml),
        FrontMatterType::Toml
    );

    // YAML with +++ content (should still be YAML)
    let yaml_with_plus = "---\ntitle: Document with +++\n---\n# Content";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(yaml_with_plus),
        FrontMatterType::Yaml
    );

    // TOML with --- content (should still be TOML)
    let toml_with_dash = "+++\ntitle = \"Document with ---\"\n+++\n# Content";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(toml_with_dash),
        FrontMatterType::Toml
    );

    // Ambiguous syntax (first delimiter wins)
    let yaml_first = "---\ntitle: YAML\n+++\nMore content\n---\n# Content";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(yaml_first),
        FrontMatterType::Yaml
    );

    let toml_first = "+++\ntitle = \"TOML\"\n---\nMore content\n+++\n# Content";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(toml_first),
        FrontMatterType::Toml
    );

    // Whitespace before front matter (should be no front matter)
    let space_before_yaml = " ---\ntitle: YAML\n---\n# Content";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(space_before_yaml),
        FrontMatterType::None
    );

    let space_before_toml = " +++\ntitle = \"TOML\"\n+++\n# Content";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(space_before_toml),
        FrontMatterType::None
    );

    // Missing closing delimiter
    let unclosed_yaml = "---\ntitle: Unclosed YAML\n# Content";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(unclosed_yaml),
        FrontMatterType::None
    );

    let unclosed_toml = "+++\ntitle = \"Unclosed TOML\"\n# Content";
    assert_eq!(
        FrontMatterUtils::detect_front_matter_type(unclosed_toml),
        FrontMatterType::None
    );
}
