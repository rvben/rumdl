use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::front_matter_utils::{FrontMatterType, FrontMatterUtils};

/// Rule MD072: Frontmatter key sort
///
/// Ensures frontmatter keys are sorted alphabetically.
/// Supports YAML, TOML, and JSON frontmatter formats.
/// Auto-fix is only available when frontmatter contains no comments (YAML/TOML).
/// JSON frontmatter is always auto-fixable since JSON has no comments.
///
/// See [docs/md072.md](../../docs/md072.md) for full documentation.
#[derive(Clone, Default)]
pub struct MD072FrontmatterKeySort;

impl MD072FrontmatterKeySort {
    pub fn new() -> Self {
        Self
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
            // Skip table headers like [section]
            if trimmed.starts_with('[') {
                continue;
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
        let mut keys = Vec::new();
        // Match patterns like "key": at the start of a line (possibly with leading whitespace)
        let key_pattern = regex::Regex::new(r#"^\s*"([^"]+)"\s*:"#).unwrap();

        for line in frontmatter_lines {
            if let Some(captures) = key_pattern.captures(line)
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

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD072FrontmatterKeySort)
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

    // ==================== YAML Tests ====================

    #[test]
    fn test_no_frontmatter() {
        let rule = MD072FrontmatterKeySort;
        let content = "# Heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_sorted_keys() {
        let rule = MD072FrontmatterKeySort;
        let content = "---\nauthor: John\ndate: 2024-01-01\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_unsorted_keys() {
        let rule = MD072FrontmatterKeySort;
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
        let rule = MD072FrontmatterKeySort;
        let content = "---\nAuthor: John\ndate: 2024-01-01\nTitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Author, date, Title should be considered sorted (case-insensitive)
        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_fix_sorts_keys() {
        let rule = MD072FrontmatterKeySort;
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
        let rule = MD072FrontmatterKeySort;
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
        let rule = MD072FrontmatterKeySort;
        let content = "---\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Single key is always sorted
        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_nested_keys_ignored() {
        let rule = MD072FrontmatterKeySort;
        // Only top-level keys are checked, nested keys are ignored
        let content = "---\nauthor:\n  name: John\n  email: john@example.com\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author, title are sorted
        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_fix_idempotent() {
        let rule = MD072FrontmatterKeySort;
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed_once = rule.fix(&ctx).unwrap();

        let ctx2 = LintContext::new(&fixed_once, crate::config::MarkdownFlavor::Standard, None);
        let fixed_twice = rule.fix(&ctx2).unwrap();

        assert_eq!(fixed_once, fixed_twice);
    }

    #[test]
    fn test_yaml_complex_values() {
        let rule = MD072FrontmatterKeySort;
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
        let rule = MD072FrontmatterKeySort;
        let content = "+++\nauthor = \"John\"\ndate = \"2024-01-01\"\ntitle = \"Test\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_toml_unsorted_keys() {
        let rule = MD072FrontmatterKeySort;
        let content = "+++\ntitle = \"Test\"\nauthor = \"John\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("TOML"));
        assert!(result[0].message.contains("not sorted"));
    }

    #[test]
    fn test_toml_fix_sorts_keys() {
        let rule = MD072FrontmatterKeySort;
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
        let rule = MD072FrontmatterKeySort;
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
        let rule = MD072FrontmatterKeySort;
        let content = "{\n\"author\": \"John\",\n\"title\": \"Test\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_json_unsorted_keys() {
        let rule = MD072FrontmatterKeySort;
        let content = "{\n\"title\": \"Test\",\n\"author\": \"John\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("JSON"));
        assert!(result[0].message.contains("not sorted"));
    }

    #[test]
    fn test_json_fix_sorts_keys() {
        let rule = MD072FrontmatterKeySort;
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
        let rule = MD072FrontmatterKeySort;
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
        let rule = MD072FrontmatterKeySort;
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_empty_frontmatter() {
        let rule = MD072FrontmatterKeySort;
        let content = "---\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }
}
