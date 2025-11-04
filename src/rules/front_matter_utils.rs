use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

// Standard front matter delimiter (three dashes)
static STANDARD_FRONT_MATTER_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^---\s*$").unwrap());
static STANDARD_FRONT_MATTER_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^---\s*$").unwrap());

// TOML front matter delimiter (three plus signs)
static TOML_FRONT_MATTER_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\+\+\+\s*$").unwrap());
static TOML_FRONT_MATTER_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\+\+\+\s*$").unwrap());

// JSON front matter delimiter (curly braces)
static JSON_FRONT_MATTER_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\{\s*$").unwrap());
static JSON_FRONT_MATTER_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\}\s*$").unwrap());

// Common malformed front matter (dash space dash dash)
static MALFORMED_FRONT_MATTER_START1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^- --\s*$").unwrap());
static MALFORMED_FRONT_MATTER_END1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^- --\s*$").unwrap());

// Alternate malformed front matter (dash dash space dash)
static MALFORMED_FRONT_MATTER_START2: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-- -\s*$").unwrap());
static MALFORMED_FRONT_MATTER_END2: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-- -\s*$").unwrap());

// Front matter field pattern
static FRONT_MATTER_FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([^:]+):\s*(.*)$").unwrap());

// TOML field pattern
static TOML_FIELD_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^([^=]+)\s*=\s*"?([^"]*)"?$"#).unwrap());

/// Represents the type of front matter found in a document
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FrontMatterType {
    /// YAML front matter (---)
    Yaml,
    /// TOML front matter (+++)
    Toml,
    /// JSON front matter ({})
    Json,
    /// Malformed front matter
    Malformed,
    /// No front matter
    None,
}

/// Utility functions for detecting and handling front matter in Markdown documents
pub struct FrontMatterUtils;

impl FrontMatterUtils {
    /// Check if a line is inside front matter content
    pub fn is_in_front_matter(content: &str, line_num: usize) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        if line_num >= lines.len() {
            return false;
        }

        let mut in_standard_front_matter = false;
        let mut in_toml_front_matter = false;
        let mut in_json_front_matter = false;
        let mut in_malformed_front_matter1 = false;
        let mut in_malformed_front_matter2 = false;

        for (i, line) in lines.iter().enumerate() {
            if i > line_num {
                break;
            }

            // Check if current line is a closing delimiter before updating state
            if i == line_num
                && i > 0
                && ((in_standard_front_matter && STANDARD_FRONT_MATTER_END.is_match(line))
                    || (in_toml_front_matter && TOML_FRONT_MATTER_END.is_match(line))
                    || (in_json_front_matter && JSON_FRONT_MATTER_END.is_match(line))
                    || (in_malformed_front_matter1 && MALFORMED_FRONT_MATTER_END1.is_match(line))
                    || (in_malformed_front_matter2 && MALFORMED_FRONT_MATTER_END2.is_match(line)))
            {
                return false; // Closing delimiter is not part of front matter content
            }

            // Standard YAML front matter handling
            if i == 0 && STANDARD_FRONT_MATTER_START.is_match(line) {
                in_standard_front_matter = true;
            } else if STANDARD_FRONT_MATTER_END.is_match(line) && in_standard_front_matter && i > 0 {
                in_standard_front_matter = false;
            }
            // TOML front matter handling
            else if i == 0 && TOML_FRONT_MATTER_START.is_match(line) {
                in_toml_front_matter = true;
            } else if TOML_FRONT_MATTER_END.is_match(line) && in_toml_front_matter && i > 0 {
                in_toml_front_matter = false;
            }
            // JSON front matter handling
            else if i == 0 && JSON_FRONT_MATTER_START.is_match(line) {
                in_json_front_matter = true;
            } else if JSON_FRONT_MATTER_END.is_match(line) && in_json_front_matter && i > 0 {
                in_json_front_matter = false;
            }
            // Malformed front matter type 1 (- --)
            else if i == 0 && MALFORMED_FRONT_MATTER_START1.is_match(line) {
                in_malformed_front_matter1 = true;
            } else if MALFORMED_FRONT_MATTER_END1.is_match(line) && in_malformed_front_matter1 && i > 0 {
                in_malformed_front_matter1 = false;
            }
            // Malformed front matter type 2 (-- -)
            else if i == 0 && MALFORMED_FRONT_MATTER_START2.is_match(line) {
                in_malformed_front_matter2 = true;
            } else if MALFORMED_FRONT_MATTER_END2.is_match(line) && in_malformed_front_matter2 && i > 0 {
                in_malformed_front_matter2 = false;
            }
        }

        // Return true if we're in any type of front matter
        in_standard_front_matter
            || in_toml_front_matter
            || in_json_front_matter
            || in_malformed_front_matter1
            || in_malformed_front_matter2
    }

    /// Check if a content contains front matter with a specific field
    pub fn has_front_matter_field(content: &str, field_prefix: &str) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 3 {
            return false;
        }

        let front_matter_type = Self::detect_front_matter_type(content);
        if front_matter_type == FrontMatterType::None {
            return false;
        }

        let front_matter = Self::extract_front_matter(content);
        for line in front_matter {
            if line.trim().starts_with(field_prefix) {
                return true;
            }
        }

        false
    }

    /// Get the value of a specific front matter field
    pub fn get_front_matter_field_value<'a>(content: &'a str, field_name: &str) -> Option<&'a str> {
        let lines: Vec<&'a str> = content.lines().collect();
        if lines.len() < 3 {
            return None;
        }

        let front_matter_type = Self::detect_front_matter_type(content);
        if front_matter_type == FrontMatterType::None {
            return None;
        }

        let front_matter = Self::extract_front_matter(content);
        for line in front_matter {
            let line = line.trim();
            match front_matter_type {
                FrontMatterType::Toml => {
                    // Handle TOML-style fields (key = value)
                    if let Some(captures) = TOML_FIELD_PATTERN.captures(line) {
                        let key = captures.get(1).unwrap().as_str().trim();
                        if key == field_name {
                            let value = captures.get(2).unwrap().as_str();
                            return Some(value);
                        }
                    }
                }
                _ => {
                    // Handle YAML/JSON-style fields (key: value)
                    if let Some(captures) = FRONT_MATTER_FIELD.captures(line) {
                        let mut key = captures.get(1).unwrap().as_str().trim();

                        // Strip quotes from the key if present (for JSON-style fields in any format)
                        if key.starts_with('"') && key.ends_with('"') && key.len() >= 2 {
                            key = &key[1..key.len() - 1];
                        }

                        if key == field_name {
                            let value = captures.get(2).unwrap().as_str().trim();
                            // Strip quotes if present
                            if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                                return Some(&value[1..value.len() - 1]);
                            }
                            return Some(value);
                        }
                    }
                }
            }
        }

        None
    }

    /// Extract all front matter fields as a HashMap
    pub fn extract_front_matter_fields(content: &str) -> HashMap<String, String> {
        let mut fields = HashMap::new();

        let front_matter_type = Self::detect_front_matter_type(content);
        if front_matter_type == FrontMatterType::None {
            return fields;
        }

        let front_matter = Self::extract_front_matter(content);
        let mut current_prefix = String::new();
        let mut indent_level = 0;

        for line in front_matter {
            let line_indent = line.chars().take_while(|c| c.is_whitespace()).count();
            let line = line.trim();

            // Handle indentation changes for nested fields
            match line_indent.cmp(&indent_level) {
                std::cmp::Ordering::Greater => {
                    // Going deeper
                    indent_level = line_indent;
                }
                std::cmp::Ordering::Less => {
                    // Going back up
                    indent_level = line_indent;
                    // Remove last nested level from prefix
                    if let Some(last_dot) = current_prefix.rfind('.') {
                        current_prefix.truncate(last_dot);
                    } else {
                        current_prefix.clear();
                    }
                }
                std::cmp::Ordering::Equal => {}
            }

            match front_matter_type {
                FrontMatterType::Toml => {
                    // Handle TOML-style fields
                    if let Some(captures) = TOML_FIELD_PATTERN.captures(line) {
                        let key = captures.get(1).unwrap().as_str().trim();
                        let value = captures.get(2).unwrap().as_str();
                        let full_key = if current_prefix.is_empty() {
                            key.to_string()
                        } else {
                            format!("{current_prefix}.{key}")
                        };
                        fields.insert(full_key, value.to_string());
                    }
                }
                _ => {
                    // Handle YAML/JSON-style fields
                    if let Some(captures) = FRONT_MATTER_FIELD.captures(line) {
                        let mut key = captures.get(1).unwrap().as_str().trim();
                        let value = captures.get(2).unwrap().as_str().trim();

                        // Strip quotes from the key if present (for JSON-style fields in any format)
                        if key.starts_with('"') && key.ends_with('"') && key.len() >= 2 {
                            key = &key[1..key.len() - 1];
                        }

                        if let Some(stripped) = key.strip_suffix(':') {
                            // This is a nested field marker
                            if current_prefix.is_empty() {
                                current_prefix = stripped.to_string();
                            } else {
                                current_prefix = format!("{current_prefix}.{stripped}");
                            }
                        } else {
                            // This is a field with a value
                            let full_key = if current_prefix.is_empty() {
                                key.to_string()
                            } else {
                                format!("{current_prefix}.{key}")
                            };
                            // Strip quotes if present
                            let value = value
                                .strip_prefix('"')
                                .and_then(|v| v.strip_suffix('"'))
                                .unwrap_or(value);
                            fields.insert(full_key, value.to_string());
                        }
                    }
                }
            }
        }

        fields
    }

    /// Extract the front matter content as a vector of lines
    pub fn extract_front_matter<'a>(content: &'a str) -> Vec<&'a str> {
        let lines: Vec<&'a str> = content.lines().collect();
        if lines.len() < 3 {
            return Vec::new();
        }

        let front_matter_type = Self::detect_front_matter_type(content);
        if front_matter_type == FrontMatterType::None {
            return Vec::new();
        }

        let mut front_matter = Vec::new();
        let mut in_front_matter = false;

        for (i, line) in lines.iter().enumerate() {
            match front_matter_type {
                FrontMatterType::Yaml => {
                    if i == 0 && STANDARD_FRONT_MATTER_START.is_match(line) {
                        in_front_matter = true;
                        continue;
                    } else if STANDARD_FRONT_MATTER_END.is_match(line) && in_front_matter && i > 0 {
                        break;
                    }
                }
                FrontMatterType::Toml => {
                    if i == 0 && TOML_FRONT_MATTER_START.is_match(line) {
                        in_front_matter = true;
                        continue;
                    } else if TOML_FRONT_MATTER_END.is_match(line) && in_front_matter && i > 0 {
                        break;
                    }
                }
                FrontMatterType::Json => {
                    if i == 0 && JSON_FRONT_MATTER_START.is_match(line) {
                        in_front_matter = true;
                        continue;
                    } else if JSON_FRONT_MATTER_END.is_match(line) && in_front_matter && i > 0 {
                        break;
                    }
                }
                FrontMatterType::Malformed => {
                    if i == 0
                        && (MALFORMED_FRONT_MATTER_START1.is_match(line)
                            || MALFORMED_FRONT_MATTER_START2.is_match(line))
                    {
                        in_front_matter = true;
                        continue;
                    } else if (MALFORMED_FRONT_MATTER_END1.is_match(line) || MALFORMED_FRONT_MATTER_END2.is_match(line))
                        && in_front_matter
                        && i > 0
                    {
                        break;
                    }
                }
                FrontMatterType::None => break,
            }

            if in_front_matter {
                front_matter.push(*line);
            }
        }

        front_matter
    }

    /// Detect the type of front matter in the content
    pub fn detect_front_matter_type(content: &str) -> FrontMatterType {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return FrontMatterType::None;
        }

        let first_line = lines[0];

        if STANDARD_FRONT_MATTER_START.is_match(first_line) {
            // Check if there's a closing marker
            for line in lines.iter().skip(1) {
                if STANDARD_FRONT_MATTER_END.is_match(line) {
                    return FrontMatterType::Yaml;
                }
            }
        } else if TOML_FRONT_MATTER_START.is_match(first_line) {
            // Check if there's a closing marker
            for line in lines.iter().skip(1) {
                if TOML_FRONT_MATTER_END.is_match(line) {
                    return FrontMatterType::Toml;
                }
            }
        } else if JSON_FRONT_MATTER_START.is_match(first_line) {
            // Check if there's a closing marker
            for line in lines.iter().skip(1) {
                if JSON_FRONT_MATTER_END.is_match(line) {
                    return FrontMatterType::Json;
                }
            }
        } else if MALFORMED_FRONT_MATTER_START1.is_match(first_line)
            || MALFORMED_FRONT_MATTER_START2.is_match(first_line)
        {
            // Check if there's a closing marker
            for line in lines.iter().skip(1) {
                if MALFORMED_FRONT_MATTER_END1.is_match(line) || MALFORMED_FRONT_MATTER_END2.is_match(line) {
                    return FrontMatterType::Malformed;
                }
            }
        }

        FrontMatterType::None
    }

    /// Get the line number where front matter ends (or 0 if no front matter)
    pub fn get_front_matter_end_line(content: &str) -> usize {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 3 {
            return 0;
        }

        let front_matter_type = Self::detect_front_matter_type(content);
        if front_matter_type == FrontMatterType::None {
            return 0;
        }

        let mut in_front_matter = false;

        for (i, line) in lines.iter().enumerate() {
            match front_matter_type {
                FrontMatterType::Yaml => {
                    if i == 0 && STANDARD_FRONT_MATTER_START.is_match(line) {
                        in_front_matter = true;
                    } else if STANDARD_FRONT_MATTER_END.is_match(line) && in_front_matter && i > 0 {
                        return i + 1;
                    }
                }
                FrontMatterType::Toml => {
                    if i == 0 && TOML_FRONT_MATTER_START.is_match(line) {
                        in_front_matter = true;
                    } else if TOML_FRONT_MATTER_END.is_match(line) && in_front_matter && i > 0 {
                        return i + 1;
                    }
                }
                FrontMatterType::Json => {
                    if i == 0 && JSON_FRONT_MATTER_START.is_match(line) {
                        in_front_matter = true;
                    } else if JSON_FRONT_MATTER_END.is_match(line) && in_front_matter && i > 0 {
                        return i + 1;
                    }
                }
                FrontMatterType::Malformed => {
                    if i == 0
                        && (MALFORMED_FRONT_MATTER_START1.is_match(line)
                            || MALFORMED_FRONT_MATTER_START2.is_match(line))
                    {
                        in_front_matter = true;
                    } else if (MALFORMED_FRONT_MATTER_END1.is_match(line) || MALFORMED_FRONT_MATTER_END2.is_match(line))
                        && in_front_matter
                        && i > 0
                    {
                        return i + 1;
                    }
                }
                FrontMatterType::None => return 0,
            }
        }

        0
    }

    /// Fix malformed front matter
    pub fn fix_malformed_front_matter(content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 3 {
            return content.to_string();
        }

        let mut result = Vec::new();
        let mut in_front_matter = false;
        let mut is_malformed = false;

        for (i, line) in lines.iter().enumerate() {
            // Handle front matter start
            if i == 0 {
                if STANDARD_FRONT_MATTER_START.is_match(line) {
                    // Standard front matter - keep as is
                    in_front_matter = true;
                    result.push(line.to_string());
                } else if MALFORMED_FRONT_MATTER_START1.is_match(line) || MALFORMED_FRONT_MATTER_START2.is_match(line) {
                    // Malformed front matter - fix it
                    in_front_matter = true;
                    is_malformed = true;
                    result.push("---".to_string());
                } else {
                    // Regular line
                    result.push(line.to_string());
                }
                continue;
            }

            // Handle front matter end
            if in_front_matter {
                if STANDARD_FRONT_MATTER_END.is_match(line) {
                    // Standard front matter end - keep as is
                    in_front_matter = false;
                    result.push(line.to_string());
                } else if (MALFORMED_FRONT_MATTER_END1.is_match(line) || MALFORMED_FRONT_MATTER_END2.is_match(line))
                    && is_malformed
                {
                    // Malformed front matter end - fix it
                    in_front_matter = false;
                    result.push("---".to_string());
                } else {
                    // Content inside front matter
                    result.push(line.to_string());
                }
                continue;
            }

            // Regular line
            result.push(line.to_string());
        }

        result.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_front_matter_type_enum() {
        assert_eq!(FrontMatterType::Yaml, FrontMatterType::Yaml);
        assert_eq!(FrontMatterType::Toml, FrontMatterType::Toml);
        assert_eq!(FrontMatterType::Json, FrontMatterType::Json);
        assert_eq!(FrontMatterType::Malformed, FrontMatterType::Malformed);
        assert_eq!(FrontMatterType::None, FrontMatterType::None);
        assert_ne!(FrontMatterType::Yaml, FrontMatterType::Toml);
    }

    #[test]
    fn test_detect_front_matter_type() {
        // YAML front matter
        let yaml_content = "---\ntitle: Test\n---\nContent";
        assert_eq!(
            FrontMatterUtils::detect_front_matter_type(yaml_content),
            FrontMatterType::Yaml
        );

        // TOML front matter
        let toml_content = "+++\ntitle = \"Test\"\n+++\nContent";
        assert_eq!(
            FrontMatterUtils::detect_front_matter_type(toml_content),
            FrontMatterType::Toml
        );

        // JSON front matter
        let json_content = "{\n\"title\": \"Test\"\n}\nContent";
        assert_eq!(
            FrontMatterUtils::detect_front_matter_type(json_content),
            FrontMatterType::Json
        );

        // Malformed front matter
        let malformed1 = "- --\ntitle: Test\n- --\nContent";
        assert_eq!(
            FrontMatterUtils::detect_front_matter_type(malformed1),
            FrontMatterType::Malformed
        );

        let malformed2 = "-- -\ntitle: Test\n-- -\nContent";
        assert_eq!(
            FrontMatterUtils::detect_front_matter_type(malformed2),
            FrontMatterType::Malformed
        );

        // No front matter
        assert_eq!(
            FrontMatterUtils::detect_front_matter_type("# Regular content"),
            FrontMatterType::None
        );
        assert_eq!(FrontMatterUtils::detect_front_matter_type(""), FrontMatterType::None);

        // Incomplete front matter (no closing marker)
        assert_eq!(
            FrontMatterUtils::detect_front_matter_type("---\ntitle: Test"),
            FrontMatterType::None
        );
    }

    #[test]
    fn test_is_in_front_matter() {
        let content = "---\ntitle: Test\nauthor: Me\n---\nContent here";

        // The implementation considers opening delimiter and content lines as inside front matter
        // but the closing delimiter triggers the exit from front matter state
        assert!(FrontMatterUtils::is_in_front_matter(content, 0)); // Opening ---
        assert!(FrontMatterUtils::is_in_front_matter(content, 1)); // title line
        assert!(FrontMatterUtils::is_in_front_matter(content, 2)); // author line
        assert!(!FrontMatterUtils::is_in_front_matter(content, 3)); // Closing --- (not part of front matter)
        assert!(!FrontMatterUtils::is_in_front_matter(content, 4)); // Content

        // Out of bounds
        assert!(!FrontMatterUtils::is_in_front_matter(content, 100));
    }

    #[test]
    fn test_extract_front_matter() {
        let content = "---\ntitle: Test\nauthor: Me\n---\nContent";
        let front_matter = FrontMatterUtils::extract_front_matter(content);

        assert_eq!(front_matter.len(), 2);
        assert_eq!(front_matter[0], "title: Test");
        assert_eq!(front_matter[1], "author: Me");

        // No front matter
        let no_fm = FrontMatterUtils::extract_front_matter("Regular content");
        assert!(no_fm.is_empty());

        // Too short content
        let short = FrontMatterUtils::extract_front_matter("---\n---");
        assert!(short.is_empty());
    }

    #[test]
    fn test_has_front_matter_field() {
        let content = "---\ntitle: Test\nauthor: Me\n---\nContent";

        assert!(FrontMatterUtils::has_front_matter_field(content, "title"));
        assert!(FrontMatterUtils::has_front_matter_field(content, "author"));
        assert!(!FrontMatterUtils::has_front_matter_field(content, "date"));

        // No front matter
        assert!(!FrontMatterUtils::has_front_matter_field("Regular content", "title"));

        // Too short content
        assert!(!FrontMatterUtils::has_front_matter_field("--", "title"));
    }

    #[test]
    fn test_get_front_matter_field_value() {
        // YAML front matter
        let yaml_content = "---\ntitle: Test Title\nauthor: \"John Doe\"\n---\nContent";
        assert_eq!(
            FrontMatterUtils::get_front_matter_field_value(yaml_content, "title"),
            Some("Test Title")
        );
        assert_eq!(
            FrontMatterUtils::get_front_matter_field_value(yaml_content, "author"),
            Some("John Doe")
        );
        assert_eq!(
            FrontMatterUtils::get_front_matter_field_value(yaml_content, "nonexistent"),
            None
        );

        // TOML front matter
        let toml_content = "+++\ntitle = \"Test Title\"\nauthor = \"John Doe\"\n+++\nContent";
        assert_eq!(
            FrontMatterUtils::get_front_matter_field_value(toml_content, "title"),
            Some("Test Title")
        );
        assert_eq!(
            FrontMatterUtils::get_front_matter_field_value(toml_content, "author"),
            Some("John Doe")
        );

        // JSON-style fields in YAML front matter - keys should not include quotes
        let json_style_yaml = "---\n\"title\": \"Test Title\"\n---\nContent";
        assert_eq!(
            FrontMatterUtils::get_front_matter_field_value(json_style_yaml, "title"),
            Some("Test Title")
        );

        // Actual JSON front matter
        let json_fm = "{\n\"title\": \"Test Title\"\n}\nContent";
        assert_eq!(
            FrontMatterUtils::get_front_matter_field_value(json_fm, "title"),
            Some("Test Title")
        );

        // No front matter
        assert_eq!(
            FrontMatterUtils::get_front_matter_field_value("Regular content", "title"),
            None
        );

        // Too short content
        assert_eq!(FrontMatterUtils::get_front_matter_field_value("--", "title"), None);
    }

    #[test]
    fn test_extract_front_matter_fields() {
        // Simple YAML front matter
        let yaml_content = "---\ntitle: Test\nauthor: Me\n---\nContent";
        let fields = FrontMatterUtils::extract_front_matter_fields(yaml_content);

        assert_eq!(fields.get("title"), Some(&"Test".to_string()));
        assert_eq!(fields.get("author"), Some(&"Me".to_string()));

        // TOML front matter
        let toml_content = "+++\ntitle = \"Test\"\nauthor = \"Me\"\n+++\nContent";
        let toml_fields = FrontMatterUtils::extract_front_matter_fields(toml_content);

        assert_eq!(toml_fields.get("title"), Some(&"Test".to_string()));
        assert_eq!(toml_fields.get("author"), Some(&"Me".to_string()));

        // No front matter
        let no_fields = FrontMatterUtils::extract_front_matter_fields("Regular content");
        assert!(no_fields.is_empty());
    }

    #[test]
    fn test_get_front_matter_end_line() {
        let content = "---\ntitle: Test\n---\nContent";
        assert_eq!(FrontMatterUtils::get_front_matter_end_line(content), 3);

        // TOML
        let toml_content = "+++\ntitle = \"Test\"\n+++\nContent";
        assert_eq!(FrontMatterUtils::get_front_matter_end_line(toml_content), 3);

        // No front matter
        assert_eq!(FrontMatterUtils::get_front_matter_end_line("Regular content"), 0);

        // Too short
        assert_eq!(FrontMatterUtils::get_front_matter_end_line("--"), 0);
    }

    #[test]
    fn test_fix_malformed_front_matter() {
        // Fix malformed type 1
        let malformed1 = "- --\ntitle: Test\n- --\nContent";
        let fixed1 = FrontMatterUtils::fix_malformed_front_matter(malformed1);
        assert!(fixed1.starts_with("---\ntitle: Test\n---"));

        // Fix malformed type 2
        let malformed2 = "-- -\ntitle: Test\n-- -\nContent";
        let fixed2 = FrontMatterUtils::fix_malformed_front_matter(malformed2);
        assert!(fixed2.starts_with("---\ntitle: Test\n---"));

        // Don't change valid front matter
        let valid = "---\ntitle: Test\n---\nContent";
        let unchanged = FrontMatterUtils::fix_malformed_front_matter(valid);
        assert_eq!(unchanged, valid);

        // No front matter
        let no_fm = "# Regular content";
        assert_eq!(FrontMatterUtils::fix_malformed_front_matter(no_fm), no_fm);

        // Too short
        let short = "--";
        assert_eq!(FrontMatterUtils::fix_malformed_front_matter(short), short);
    }

    #[test]
    fn test_nested_yaml_fields() {
        let content = "---
title: Test
author:
  name: John Doe
  email: john@example.com
---
Content";

        let fields = FrontMatterUtils::extract_front_matter_fields(content);

        // Note: The current implementation doesn't fully handle nested YAML
        // This test documents the current behavior
        assert!(fields.contains_key("title"));
        // Nested fields handling would need enhancement
    }

    #[test]
    fn test_edge_cases() {
        // Empty content
        assert_eq!(FrontMatterUtils::detect_front_matter_type(""), FrontMatterType::None);
        assert!(FrontMatterUtils::extract_front_matter("").is_empty());
        assert_eq!(FrontMatterUtils::get_front_matter_end_line(""), 0);

        // Only delimiters
        let only_delim = "---\n---";
        assert!(FrontMatterUtils::extract_front_matter(only_delim).is_empty());

        // Multiple front matter sections (only first should be detected)
        let multiple = "---\ntitle: First\n---\n---\ntitle: Second\n---";
        let fm_type = FrontMatterUtils::detect_front_matter_type(multiple);
        assert_eq!(fm_type, FrontMatterType::Yaml);
        let fields = FrontMatterUtils::extract_front_matter_fields(multiple);
        assert_eq!(fields.get("title"), Some(&"First".to_string()));
    }

    #[test]
    fn test_unicode_content() {
        let content = "---\ntitle: 你好世界\nauthor: José\n---\nContent";

        assert_eq!(
            FrontMatterUtils::detect_front_matter_type(content),
            FrontMatterType::Yaml
        );
        assert_eq!(
            FrontMatterUtils::get_front_matter_field_value(content, "title"),
            Some("你好世界")
        );
        assert_eq!(
            FrontMatterUtils::get_front_matter_field_value(content, "author"),
            Some("José")
        );
    }
}
