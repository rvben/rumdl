use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::rules::front_matter_utils::{FrontMatterType, FrontMatterUtils};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// Pre-compiled regex for extracting JSON keys
static JSON_KEY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\s*"([^"]+)"\s*:"#).expect("Invalid JSON key regex"));

/// Configuration for MD072 (Frontmatter key sort)
///
/// This rule is disabled by default (opt-in) because alphabetical key sorting
/// is an opinionated style choice. Many projects prefer semantic ordering.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MD072Config {
    /// Whether this rule is enabled (default: false - opt-in rule)
    #[serde(default)]
    pub enabled: bool,
}

impl RuleConfig for MD072Config {
    const RULE_NAME: &'static str = "MD072";
}

/// Rule MD072: Frontmatter key sort
///
/// Ensures frontmatter keys are sorted alphabetically.
/// Supports YAML, TOML, and JSON frontmatter formats.
/// Auto-fix is only available when frontmatter contains no comments (YAML/TOML).
/// JSON frontmatter is always auto-fixable since JSON has no comments.
///
/// **Note**: This rule is disabled by default because alphabetical key sorting
/// is an opinionated style choice. Many projects prefer semantic ordering
/// (title first, date second, etc.) rather than alphabetical.
///
/// See [docs/md072.md](../../docs/md072.md) for full documentation.
#[derive(Clone, Default)]
pub struct MD072FrontmatterKeySort {
    config: MD072Config,
}

impl MD072FrontmatterKeySort {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from a config struct
    pub fn from_config_struct(config: MD072Config) -> Self {
        Self { config }
    }

    /// Check if frontmatter contains comments (YAML/TOML use #)
    fn has_comments(frontmatter_lines: &[&str]) -> bool {
        frontmatter_lines.iter().any(|line| line.trim_start().starts_with('#'))
    }

    /// Extract top-level keys from YAML frontmatter
    fn extract_yaml_keys(frontmatter_lines: &[&str]) -> Vec<(usize, String)> {
        let mut keys = Vec::new();

        for (idx, line) in frontmatter_lines.iter().enumerate() {
            // Top-level keys have no leading whitespace and contain a colon
            if !line.starts_with(' ')
                && !line.starts_with('\t')
                && let Some(colon_pos) = line.find(':')
            {
                let key = line[..colon_pos].trim();
                if !key.is_empty() && !key.starts_with('#') {
                    keys.push((idx, key.to_string()));
                }
            }
        }

        keys
    }

    /// Extract top-level keys from TOML frontmatter
    fn extract_toml_keys(frontmatter_lines: &[&str]) -> Vec<(usize, String)> {
        let mut keys = Vec::new();

        for (idx, line) in frontmatter_lines.iter().enumerate() {
            let trimmed = line.trim();
            // Skip comments and empty lines
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // Stop at table headers like [section] - everything after is nested
            if trimmed.starts_with('[') {
                break;
            }
            // Top-level keys have no leading whitespace and contain =
            if !line.starts_with(' ')
                && !line.starts_with('\t')
                && let Some(eq_pos) = line.find('=')
            {
                let key = line[..eq_pos].trim();
                if !key.is_empty() {
                    keys.push((idx, key.to_string()));
                }
            }
        }

        keys
    }

    /// Extract top-level keys from JSON frontmatter in order of appearance
    fn extract_json_keys(frontmatter_lines: &[&str]) -> Vec<String> {
        // Extract keys from raw JSON text to preserve original order
        // serde_json::Map uses BTreeMap which sorts keys, so we parse manually
        // Only extract keys at depth 0 relative to the content (top-level inside the outer object)
        // Note: frontmatter_lines excludes the opening `{`, so we start at depth 0
        let mut keys = Vec::new();
        let mut depth: usize = 0;

        for line in frontmatter_lines {
            // Track depth before checking for keys on this line
            let line_start_depth = depth;

            // Count braces and brackets to track nesting
            for ch in line.chars() {
                match ch {
                    '{' | '[' => depth += 1,
                    '}' | ']' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }

            // Only extract keys at depth 0 (top-level, since opening brace is excluded)
            if line_start_depth == 0
                && let Some(captures) = JSON_KEY_PATTERN.captures(line)
                && let Some(key_match) = captures.get(1)
            {
                keys.push(key_match.as_str().to_string());
            }
        }

        keys
    }

    /// Check if keys are sorted alphabetically (case-insensitive)
    fn are_keys_sorted(keys: &[String]) -> bool {
        if keys.len() <= 1 {
            return true;
        }

        for i in 1..keys.len() {
            if keys[i].to_lowercase() < keys[i - 1].to_lowercase() {
                return false;
            }
        }

        true
    }

    /// Check if indexed keys are sorted alphabetically (case-insensitive)
    fn are_indexed_keys_sorted(keys: &[(usize, String)]) -> bool {
        if keys.len() <= 1 {
            return true;
        }

        for i in 1..keys.len() {
            if keys[i].1.to_lowercase() < keys[i - 1].1.to_lowercase() {
                return false;
            }
        }

        true
    }

    /// Get the expected order of keys
    fn get_sorted_keys(keys: &[String]) -> Vec<String> {
        let mut sorted = keys.to_vec();
        sorted.sort_by_key(|a| a.to_lowercase());
        sorted
    }

    /// Get the expected order of indexed keys
    fn get_sorted_indexed_keys(keys: &[(usize, String)]) -> Vec<String> {
        let mut sorted: Vec<String> = keys.iter().map(|(_, k)| k.clone()).collect();
        sorted.sort_by_key(|a| a.to_lowercase());
        sorted
    }
}

impl Rule for MD072FrontmatterKeySort {
    fn name(&self) -> &'static str {
        "MD072"
    }

    fn description(&self) -> &'static str {
        "Frontmatter keys should be sorted alphabetically"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let content = ctx.content;
        let mut warnings = Vec::new();

        if content.is_empty() {
            return Ok(warnings);
        }

        let fm_type = FrontMatterUtils::detect_front_matter_type(content);

        match fm_type {
            FrontMatterType::Yaml => {
                let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
                if frontmatter_lines.is_empty() {
                    return Ok(warnings);
                }

                let keys = Self::extract_yaml_keys(&frontmatter_lines);
                if Self::are_indexed_keys_sorted(&keys) {
                    return Ok(warnings);
                }

                let has_comments = Self::has_comments(&frontmatter_lines);
                let sorted_keys = Self::get_sorted_indexed_keys(&keys);
                let current_order = keys.iter().map(|(_, k)| k.as_str()).collect::<Vec<_>>().join(", ");
                let expected_order = sorted_keys.join(", ");

                let fix = if has_comments {
                    None
                } else {
                    Some(Fix {
                        range: 0..0,
                        replacement: String::new(),
                    })
                };

                let message = if has_comments {
                    format!(
                        "YAML frontmatter keys are not sorted alphabetically. Expected order: [{expected_order}]. Current order: [{current_order}]. Auto-fix unavailable: frontmatter contains comments."
                    )
                } else {
                    format!(
                        "YAML frontmatter keys are not sorted alphabetically. Expected order: [{expected_order}]. Current order: [{current_order}]"
                    )
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message,
                    line: 2, // First line after opening ---
                    column: 1,
                    end_line: 2,
                    end_column: 1,
                    severity: Severity::Warning,
                    fix,
                });
            }
            FrontMatterType::Toml => {
                let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
                if frontmatter_lines.is_empty() {
                    return Ok(warnings);
                }

                let keys = Self::extract_toml_keys(&frontmatter_lines);
                if Self::are_indexed_keys_sorted(&keys) {
                    return Ok(warnings);
                }

                let has_comments = Self::has_comments(&frontmatter_lines);
                let sorted_keys = Self::get_sorted_indexed_keys(&keys);
                let current_order = keys.iter().map(|(_, k)| k.as_str()).collect::<Vec<_>>().join(", ");
                let expected_order = sorted_keys.join(", ");

                let fix = if has_comments {
                    None
                } else {
                    Some(Fix {
                        range: 0..0,
                        replacement: String::new(),
                    })
                };

                let message = if has_comments {
                    format!(
                        "TOML frontmatter keys are not sorted alphabetically. Expected order: [{expected_order}]. Current order: [{current_order}]. Auto-fix unavailable: frontmatter contains comments."
                    )
                } else {
                    format!(
                        "TOML frontmatter keys are not sorted alphabetically. Expected order: [{expected_order}]. Current order: [{current_order}]"
                    )
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message,
                    line: 2, // First line after opening +++
                    column: 1,
                    end_line: 2,
                    end_column: 1,
                    severity: Severity::Warning,
                    fix,
                });
            }
            FrontMatterType::Json => {
                let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
                if frontmatter_lines.is_empty() {
                    return Ok(warnings);
                }

                let keys = Self::extract_json_keys(&frontmatter_lines);

                if keys.is_empty() || Self::are_keys_sorted(&keys) {
                    return Ok(warnings);
                }

                let sorted_keys = Self::get_sorted_keys(&keys);
                let current_order = keys.join(", ");
                let expected_order = sorted_keys.join(", ");

                // JSON has no comments, always fixable
                let fix = Some(Fix {
                    range: 0..0,
                    replacement: String::new(),
                });

                let message = format!(
                    "JSON frontmatter keys are not sorted alphabetically. Expected order: [{expected_order}]. Current order: [{current_order}]"
                );

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message,
                    line: 2, // First line after opening {
                    column: 1,
                    end_line: 2,
                    end_column: 1,
                    severity: Severity::Warning,
                    fix,
                });
            }
            _ => {
                // No frontmatter or malformed - skip
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if !self.config.enabled {
            return Ok(ctx.content.to_string());
        }

        let content = ctx.content;

        let fm_type = FrontMatterUtils::detect_front_matter_type(content);

        match fm_type {
            FrontMatterType::Yaml => self.fix_yaml(content),
            FrontMatterType::Toml => self.fix_toml(content),
            FrontMatterType::Json => self.fix_json(content),
            _ => Ok(content.to_string()),
        }
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::FrontMatter
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD072Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

impl MD072FrontmatterKeySort {
    fn fix_yaml(&self, content: &str) -> Result<String, LintError> {
        let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
        if frontmatter_lines.is_empty() {
            return Ok(content.to_string());
        }

        // Cannot fix if comments present
        if Self::has_comments(&frontmatter_lines) {
            return Ok(content.to_string());
        }

        let keys = Self::extract_yaml_keys(&frontmatter_lines);
        if Self::are_indexed_keys_sorted(&keys) {
            return Ok(content.to_string());
        }

        // Parse and re-serialize with sorted keys
        let fm_content = frontmatter_lines.join("\n");

        match serde_yml::from_str::<serde_yml::Value>(&fm_content) {
            Ok(value) => {
                if let serde_yml::Value::Mapping(map) = value {
                    let mut sorted_map = serde_yml::Mapping::new();
                    let mut keys: Vec<_> = map.keys().cloned().collect();
                    keys.sort_by_key(|a| a.as_str().unwrap_or("").to_lowercase());

                    for key in keys {
                        if let Some(value) = map.get(&key) {
                            sorted_map.insert(key, value.clone());
                        }
                    }

                    match serde_yml::to_string(&sorted_map) {
                        Ok(sorted_yaml) => {
                            let lines: Vec<&str> = content.lines().collect();
                            let fm_end = FrontMatterUtils::get_front_matter_end_line(content);

                            let mut result = String::new();
                            result.push_str("---\n");
                            result.push_str(sorted_yaml.trim_end());
                            result.push_str("\n---");

                            if fm_end < lines.len() {
                                result.push('\n');
                                result.push_str(&lines[fm_end..].join("\n"));
                            }

                            Ok(result)
                        }
                        Err(_) => Ok(content.to_string()),
                    }
                } else {
                    Ok(content.to_string())
                }
            }
            Err(_) => Ok(content.to_string()),
        }
    }

    fn fix_toml(&self, content: &str) -> Result<String, LintError> {
        let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
        if frontmatter_lines.is_empty() {
            return Ok(content.to_string());
        }

        // Cannot fix if comments present
        if Self::has_comments(&frontmatter_lines) {
            return Ok(content.to_string());
        }

        let keys = Self::extract_toml_keys(&frontmatter_lines);
        if Self::are_indexed_keys_sorted(&keys) {
            return Ok(content.to_string());
        }

        // Parse and re-serialize with sorted keys
        let fm_content = frontmatter_lines.join("\n");

        match toml::from_str::<toml::Value>(&fm_content) {
            Ok(value) => {
                if let toml::Value::Table(table) = value {
                    // toml crate's Table is already a BTreeMap which is sorted
                    // But we need case-insensitive sorting
                    let mut sorted_table = toml::map::Map::new();
                    let mut keys: Vec<_> = table.keys().cloned().collect();
                    keys.sort_by_key(|a| a.to_lowercase());

                    for key in keys {
                        if let Some(value) = table.get(&key) {
                            sorted_table.insert(key, value.clone());
                        }
                    }

                    match toml::to_string_pretty(&toml::Value::Table(sorted_table)) {
                        Ok(sorted_toml) => {
                            let lines: Vec<&str> = content.lines().collect();
                            let fm_end = FrontMatterUtils::get_front_matter_end_line(content);

                            let mut result = String::new();
                            result.push_str("+++\n");
                            result.push_str(sorted_toml.trim_end());
                            result.push_str("\n+++");

                            if fm_end < lines.len() {
                                result.push('\n');
                                result.push_str(&lines[fm_end..].join("\n"));
                            }

                            Ok(result)
                        }
                        Err(_) => Ok(content.to_string()),
                    }
                } else {
                    Ok(content.to_string())
                }
            }
            Err(_) => Ok(content.to_string()),
        }
    }

    fn fix_json(&self, content: &str) -> Result<String, LintError> {
        let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
        if frontmatter_lines.is_empty() {
            return Ok(content.to_string());
        }

        let keys = Self::extract_json_keys(&frontmatter_lines);

        if keys.is_empty() || Self::are_keys_sorted(&keys) {
            return Ok(content.to_string());
        }

        // Reconstruct JSON content including braces for parsing
        let json_content = format!("{{{}}}", frontmatter_lines.join("\n"));

        // Parse and re-serialize with sorted keys
        match serde_json::from_str::<serde_json::Value>(&json_content) {
            Ok(serde_json::Value::Object(map)) => {
                // serde_json::Map preserves insertion order, so we need to rebuild
                let mut sorted_map = serde_json::Map::new();
                let mut keys: Vec<_> = map.keys().cloned().collect();
                keys.sort_by_key(|a| a.to_lowercase());

                for key in keys {
                    if let Some(value) = map.get(&key) {
                        sorted_map.insert(key, value.clone());
                    }
                }

                match serde_json::to_string_pretty(&serde_json::Value::Object(sorted_map)) {
                    Ok(sorted_json) => {
                        let lines: Vec<&str> = content.lines().collect();
                        let fm_end = FrontMatterUtils::get_front_matter_end_line(content);

                        // The pretty-printed JSON includes the outer braces
                        // We need to format it properly for frontmatter
                        let mut result = String::new();
                        result.push_str(&sorted_json);

                        if fm_end < lines.len() {
                            result.push('\n');
                            result.push_str(&lines[fm_end..].join("\n"));
                        }

                        Ok(result)
                    }
                    Err(_) => Ok(content.to_string()),
                }
            }
            _ => Ok(content.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    /// Create an enabled rule for testing
    fn create_enabled_rule() -> MD072FrontmatterKeySort {
        MD072FrontmatterKeySort::from_config_struct(MD072Config { enabled: true })
    }

    // ==================== Config Tests ====================

    #[test]
    fn test_disabled_by_default() {
        let rule = MD072FrontmatterKeySort::new();
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Disabled by default, should return no warnings
        assert!(result.is_empty());
    }

    #[test]
    fn test_enabled_via_config() {
        let rule = create_enabled_rule();
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Enabled, should detect unsorted keys
        assert_eq!(result.len(), 1);
    }

    // ==================== YAML Tests ====================

    #[test]
    fn test_no_frontmatter() {
        let rule = create_enabled_rule();
        let content = "# Heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_sorted_keys() {
        let rule = create_enabled_rule();
        let content = "---\nauthor: John\ndate: 2024-01-01\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_unsorted_keys() {
        let rule = create_enabled_rule();
        let content = "---\ntitle: Test\nauthor: John\ndate: 2024-01-01\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("YAML"));
        assert!(result[0].message.contains("not sorted"));
        assert!(result[0].message.contains("author, date, title"));
    }

    #[test]
    fn test_yaml_case_insensitive_sort() {
        let rule = create_enabled_rule();
        let content = "---\nAuthor: John\ndate: 2024-01-01\nTitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Author, date, Title should be considered sorted (case-insensitive)
        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_fix_sorts_keys() {
        let rule = create_enabled_rule();
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Keys should be sorted
        let author_pos = fixed.find("author:").unwrap();
        let title_pos = fixed.find("title:").unwrap();
        assert!(author_pos < title_pos);
    }

    #[test]
    fn test_yaml_no_fix_with_comments() {
        let rule = create_enabled_rule();
        let content = "---\ntitle: Test\n# This is a comment\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Auto-fix unavailable"));
        assert!(result[0].fix.is_none());

        // Fix should not modify content
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_yaml_single_key() {
        let rule = create_enabled_rule();
        let content = "---\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Single key is always sorted
        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_nested_keys_ignored() {
        let rule = create_enabled_rule();
        // Only top-level keys are checked, nested keys are ignored
        let content = "---\nauthor:\n  name: John\n  email: john@example.com\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author, title are sorted
        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_fix_idempotent() {
        let rule = create_enabled_rule();
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed_once = rule.fix(&ctx).unwrap();

        let ctx2 = LintContext::new(&fixed_once, crate::config::MarkdownFlavor::Standard, None);
        let fixed_twice = rule.fix(&ctx2).unwrap();

        assert_eq!(fixed_once, fixed_twice);
    }

    #[test]
    fn test_yaml_complex_values() {
        let rule = create_enabled_rule();
        // Keys in sorted order: author, tags, title
        let content =
            "---\nauthor: John Doe\ntags:\n  - rust\n  - markdown\ntitle: \"Test: A Complex Title\"\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author, tags, title - sorted
        assert!(result.is_empty());
    }

    // ==================== TOML Tests ====================

    #[test]
    fn test_toml_sorted_keys() {
        let rule = create_enabled_rule();
        let content = "+++\nauthor = \"John\"\ndate = \"2024-01-01\"\ntitle = \"Test\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_toml_unsorted_keys() {
        let rule = create_enabled_rule();
        let content = "+++\ntitle = \"Test\"\nauthor = \"John\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("TOML"));
        assert!(result[0].message.contains("not sorted"));
    }

    #[test]
    fn test_toml_fix_sorts_keys() {
        let rule = create_enabled_rule();
        let content = "+++\ntitle = \"Test\"\nauthor = \"John\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Keys should be sorted
        let author_pos = fixed.find("author").unwrap();
        let title_pos = fixed.find("title").unwrap();
        assert!(author_pos < title_pos);
    }

    #[test]
    fn test_toml_no_fix_with_comments() {
        let rule = create_enabled_rule();
        let content = "+++\ntitle = \"Test\"\n# This is a comment\nauthor = \"John\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Auto-fix unavailable"));

        // Fix should not modify content
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    // ==================== JSON Tests ====================

    #[test]
    fn test_json_sorted_keys() {
        let rule = create_enabled_rule();
        let content = "{\n\"author\": \"John\",\n\"title\": \"Test\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_json_unsorted_keys() {
        let rule = create_enabled_rule();
        let content = "{\n\"title\": \"Test\",\n\"author\": \"John\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("JSON"));
        assert!(result[0].message.contains("not sorted"));
    }

    #[test]
    fn test_json_fix_sorts_keys() {
        let rule = create_enabled_rule();
        let content = "{\n\"title\": \"Test\",\n\"author\": \"John\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Keys should be sorted
        let author_pos = fixed.find("author").unwrap();
        let title_pos = fixed.find("title").unwrap();
        assert!(author_pos < title_pos);
    }

    #[test]
    fn test_json_always_fixable() {
        let rule = create_enabled_rule();
        // JSON has no comments, so should always be fixable
        let content = "{\n\"title\": \"Test\",\n\"author\": \"John\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].fix.is_some()); // Always fixable
        assert!(!result[0].message.contains("Auto-fix unavailable"));
    }

    // ==================== General Tests ====================

    #[test]
    fn test_empty_content() {
        let rule = create_enabled_rule();
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_empty_frontmatter() {
        let rule = create_enabled_rule();
        let content = "---\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_toml_nested_tables_ignored() {
        // Keys inside [extra] or [taxonomies] should NOT be checked
        let rule = create_enabled_rule();
        let content = "+++\ntitle = \"Programming\"\nsort_by = \"weight\"\n\n[extra]\nwe_have_extra = \"variables\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only top-level keys (title, sort_by) should be checked, not we_have_extra
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("sort_by, title"));
        assert!(!result[0].message.contains("we_have_extra"));
    }

    #[test]
    fn test_toml_nested_taxonomies_ignored() {
        // Keys inside [taxonomies] should NOT be checked
        let rule = create_enabled_rule();
        let content = "+++\ntitle = \"Test\"\ndate = \"2024-01-01\"\n\n[taxonomies]\ncategories = [\"test\"]\ntags = [\"foo\"]\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only top-level keys (title, date) should be checked
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("date, title"));
        assert!(!result[0].message.contains("categories"));
        assert!(!result[0].message.contains("tags"));
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_yaml_unicode_keys() {
        let rule = create_enabled_rule();
        // Japanese keys should sort correctly
        let content = "---\nタイトル: Test\nあいう: Value\n日本語: Content\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should detect unsorted keys (あいう < タイトル < 日本語 in Unicode order)
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_yaml_keys_with_special_characters() {
        let rule = create_enabled_rule();
        // Keys with dashes and underscores
        let content = "---\nmy-key: value1\nmy_key: value2\nmykey: value3\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // my-key, my_key, mykey - should be sorted
        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_keys_with_numbers() {
        let rule = create_enabled_rule();
        let content = "---\nkey1: value\nkey10: value\nkey2: value\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // key1, key10, key2 - lexicographic order (1 < 10 < 2)
        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_multiline_string_block_literal() {
        let rule = create_enabled_rule();
        let content =
            "---\ndescription: |\n  This is a\n  multiline literal\ntitle: Test\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author, description, title - not sorted
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("author, description, title"));
    }

    #[test]
    fn test_yaml_multiline_string_folded() {
        let rule = create_enabled_rule();
        let content = "---\ndescription: >\n  This is a\n  folded string\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author, description - not sorted
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_yaml_fix_preserves_multiline_values() {
        let rule = create_enabled_rule();
        let content = "---\ntitle: Test\ndescription: |\n  Line 1\n  Line 2\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // description should come before title
        let desc_pos = fixed.find("description").unwrap();
        let title_pos = fixed.find("title").unwrap();
        assert!(desc_pos < title_pos);
    }

    #[test]
    fn test_yaml_quoted_keys() {
        let rule = create_enabled_rule();
        let content = "---\n\"quoted-key\": value1\nunquoted: value2\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // quoted-key should sort before unquoted
        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_duplicate_keys() {
        // YAML allows duplicate keys (last one wins), but we should still sort
        let rule = create_enabled_rule();
        let content = "---\ntitle: First\nauthor: John\ntitle: Second\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should still check sorting (title, author, title is not sorted)
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_toml_inline_table() {
        let rule = create_enabled_rule();
        let content =
            "+++\nauthor = { name = \"John\", email = \"john@example.com\" }\ntitle = \"Test\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author, title - sorted
        assert!(result.is_empty());
    }

    #[test]
    fn test_toml_array_of_tables() {
        let rule = create_enabled_rule();
        let content = "+++\ntitle = \"Test\"\ndate = \"2024-01-01\"\n\n[[authors]]\nname = \"John\"\n\n[[authors]]\nname = \"Jane\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only top-level keys (title, date) checked - date < title, so unsorted
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("date, title"));
    }

    #[test]
    fn test_json_nested_objects() {
        let rule = create_enabled_rule();
        let content = "{\n\"author\": {\n  \"name\": \"John\",\n  \"email\": \"john@example.com\"\n},\n\"title\": \"Test\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only top-level keys (author, title) checked - sorted
        assert!(result.is_empty());
    }

    #[test]
    fn test_json_arrays() {
        let rule = create_enabled_rule();
        let content = "{\n\"tags\": [\"rust\", \"markdown\"],\n\"author\": \"John\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author, tags - not sorted (tags comes first)
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_fix_preserves_content_after_frontmatter() {
        let rule = create_enabled_rule();
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading\n\nParagraph 1.\n\n- List item\n- Another item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Verify content after frontmatter is preserved
        assert!(fixed.contains("# Heading"));
        assert!(fixed.contains("Paragraph 1."));
        assert!(fixed.contains("- List item"));
        assert!(fixed.contains("- Another item"));
    }

    #[test]
    fn test_fix_yaml_produces_valid_yaml() {
        let rule = create_enabled_rule();
        let content = "---\ntitle: \"Test: A Title\"\nauthor: John Doe\ndate: 2024-01-15\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The fixed output should be parseable as YAML
        // Extract frontmatter lines
        let lines: Vec<&str> = fixed.lines().collect();
        let fm_end = lines.iter().skip(1).position(|l| *l == "---").unwrap() + 1;
        let fm_content: String = lines[1..fm_end].join("\n");

        // Should parse without error
        let parsed: Result<serde_yml::Value, _> = serde_yml::from_str(&fm_content);
        assert!(parsed.is_ok(), "Fixed YAML should be valid: {fm_content}");
    }

    #[test]
    fn test_fix_toml_produces_valid_toml() {
        let rule = create_enabled_rule();
        let content = "+++\ntitle = \"Test\"\nauthor = \"John Doe\"\ndate = 2024-01-15\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Extract frontmatter
        let lines: Vec<&str> = fixed.lines().collect();
        let fm_end = lines.iter().skip(1).position(|l| *l == "+++").unwrap() + 1;
        let fm_content: String = lines[1..fm_end].join("\n");

        // Should parse without error
        let parsed: Result<toml::Value, _> = toml::from_str(&fm_content);
        assert!(parsed.is_ok(), "Fixed TOML should be valid: {fm_content}");
    }

    #[test]
    fn test_fix_json_produces_valid_json() {
        let rule = create_enabled_rule();
        let content = "{\n\"title\": \"Test\",\n\"author\": \"John\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Extract JSON frontmatter (everything up to blank line)
        let json_end = fixed.find("\n\n").unwrap();
        let json_content = &fixed[..json_end];

        // Should parse without error
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_content);
        assert!(parsed.is_ok(), "Fixed JSON should be valid: {json_content}");
    }

    #[test]
    fn test_many_keys_performance() {
        let rule = create_enabled_rule();
        // Generate frontmatter with 100 keys
        let mut keys: Vec<String> = (0..100).map(|i| format!("key{i:03}: value{i}")).collect();
        keys.reverse(); // Make them unsorted
        let content = format!("---\n{}\n---\n\n# Heading", keys.join("\n"));

        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should detect unsorted keys
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_yaml_empty_value() {
        let rule = create_enabled_rule();
        let content = "---\ntitle:\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author, title - not sorted
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_yaml_null_value() {
        let rule = create_enabled_rule();
        let content = "---\ntitle: null\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_yaml_boolean_values() {
        let rule = create_enabled_rule();
        let content = "---\ndraft: true\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author, draft - not sorted
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_toml_boolean_values() {
        let rule = create_enabled_rule();
        let content = "+++\ndraft = true\nauthor = \"John\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_yaml_list_at_top_level() {
        let rule = create_enabled_rule();
        let content = "---\ntags:\n  - rust\n  - markdown\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author, tags - not sorted (tags comes first)
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_three_keys_all_orderings() {
        let rule = create_enabled_rule();

        // Test all 6 permutations of a, b, c
        let orderings = [
            ("a, b, c", "---\na: 1\nb: 2\nc: 3\n---\n\n# H", true),  // sorted
            ("a, c, b", "---\na: 1\nc: 3\nb: 2\n---\n\n# H", false), // unsorted
            ("b, a, c", "---\nb: 2\na: 1\nc: 3\n---\n\n# H", false), // unsorted
            ("b, c, a", "---\nb: 2\nc: 3\na: 1\n---\n\n# H", false), // unsorted
            ("c, a, b", "---\nc: 3\na: 1\nb: 2\n---\n\n# H", false), // unsorted
            ("c, b, a", "---\nc: 3\nb: 2\na: 1\n---\n\n# H", false), // unsorted
        ];

        for (name, content, should_pass) in orderings {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.is_empty(),
                should_pass,
                "Ordering {name} should {} pass",
                if should_pass { "" } else { "not" }
            );
        }
    }

    #[test]
    fn test_crlf_line_endings() {
        let rule = create_enabled_rule();
        let content = "---\r\ntitle: Test\r\nauthor: John\r\n---\r\n\r\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should detect unsorted keys with CRLF
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_json_escaped_quotes_in_keys() {
        let rule = create_enabled_rule();
        // This is technically invalid JSON but tests regex robustness
        let content = "{\n\"normal\": \"value\",\n\"key\": \"with \\\"quotes\\\"\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // key, normal - not sorted
        assert_eq!(result.len(), 1);
    }
}
