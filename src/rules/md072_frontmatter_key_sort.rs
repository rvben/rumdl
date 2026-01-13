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
/// This rule is disabled by default (opt-in) because key sorting
/// is an opinionated style choice. Many projects prefer semantic ordering.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MD072Config {
    /// Whether this rule is enabled (default: false - opt-in rule)
    #[serde(default)]
    pub enabled: bool,

    /// Custom key order. Keys listed here will be sorted in this order.
    /// Keys not in this list will be sorted alphabetically after the specified keys.
    /// If not set, all keys are sorted alphabetically (case-insensitive).
    ///
    /// Example: `key_order = ["title", "date", "author", "tags"]`
    #[serde(default, alias = "key-order")]
    pub key_order: Option<Vec<String>>,
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

    /// Get the sort position for a key based on custom key_order or alphabetical fallback.
    /// Keys in key_order get their index (0, 1, 2...), keys not in key_order get
    /// a high value so they sort after, with alphabetical sub-sorting.
    fn key_sort_position(key: &str, key_order: Option<&[String]>) -> (usize, String) {
        if let Some(order) = key_order {
            // Find position in custom order (case-insensitive match)
            let key_lower = key.to_lowercase();
            for (idx, ordered_key) in order.iter().enumerate() {
                if ordered_key.to_lowercase() == key_lower {
                    return (idx, key_lower);
                }
            }
            // Not in custom order - sort after with alphabetical
            (usize::MAX, key_lower)
        } else {
            // No custom order - pure alphabetical
            (0, key.to_lowercase())
        }
    }

    /// Find the first pair of keys that are out of order
    /// Returns (out_of_place_key, should_come_after_key)
    fn find_first_unsorted_pair<'a>(keys: &'a [String], key_order: Option<&[String]>) -> Option<(&'a str, &'a str)> {
        for i in 1..keys.len() {
            let pos_curr = Self::key_sort_position(&keys[i], key_order);
            let pos_prev = Self::key_sort_position(&keys[i - 1], key_order);
            if pos_curr < pos_prev {
                return Some((&keys[i], &keys[i - 1]));
            }
        }
        None
    }

    /// Find the first pair of indexed keys that are out of order
    /// Returns (out_of_place_key, should_come_after_key)
    fn find_first_unsorted_indexed_pair<'a>(
        keys: &'a [(usize, String)],
        key_order: Option<&[String]>,
    ) -> Option<(&'a str, &'a str)> {
        for i in 1..keys.len() {
            let pos_curr = Self::key_sort_position(&keys[i].1, key_order);
            let pos_prev = Self::key_sort_position(&keys[i - 1].1, key_order);
            if pos_curr < pos_prev {
                return Some((&keys[i].1, &keys[i - 1].1));
            }
        }
        None
    }

    /// Check if keys are sorted according to key_order (or alphabetically if None)
    fn are_keys_sorted(keys: &[String], key_order: Option<&[String]>) -> bool {
        Self::find_first_unsorted_pair(keys, key_order).is_none()
    }

    /// Check if indexed keys are sorted according to key_order (or alphabetically if None)
    fn are_indexed_keys_sorted(keys: &[(usize, String)], key_order: Option<&[String]>) -> bool {
        Self::find_first_unsorted_indexed_pair(keys, key_order).is_none()
    }

    /// Sort keys according to key_order, with alphabetical fallback for unlisted keys
    fn sort_keys_by_order(keys: &mut [(String, Vec<&str>)], key_order: Option<&[String]>) {
        keys.sort_by(|a, b| {
            let pos_a = Self::key_sort_position(&a.0, key_order);
            let pos_b = Self::key_sort_position(&b.0, key_order);
            pos_a.cmp(&pos_b)
        });
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
                let key_order = self.config.key_order.as_deref();
                let Some((out_of_place, should_come_after)) = Self::find_first_unsorted_indexed_pair(&keys, key_order)
                else {
                    return Ok(warnings);
                };

                let has_comments = Self::has_comments(&frontmatter_lines);

                let fix = if has_comments {
                    None
                } else {
                    // Compute the actual fix: full content replacement
                    match self.fix_yaml(content) {
                        Ok(fixed_content) if fixed_content != content => Some(Fix {
                            range: 0..content.len(),
                            replacement: fixed_content,
                        }),
                        _ => None,
                    }
                };

                let message = if has_comments {
                    format!(
                        "YAML frontmatter keys are not sorted alphabetically: '{out_of_place}' should come before '{should_come_after}' (auto-fix unavailable: contains comments)"
                    )
                } else {
                    format!(
                        "YAML frontmatter keys are not sorted alphabetically: '{out_of_place}' should come before '{should_come_after}'"
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
                let key_order = self.config.key_order.as_deref();
                let Some((out_of_place, should_come_after)) = Self::find_first_unsorted_indexed_pair(&keys, key_order)
                else {
                    return Ok(warnings);
                };

                let has_comments = Self::has_comments(&frontmatter_lines);

                let fix = if has_comments {
                    None
                } else {
                    // Compute the actual fix: full content replacement
                    match self.fix_toml(content) {
                        Ok(fixed_content) if fixed_content != content => Some(Fix {
                            range: 0..content.len(),
                            replacement: fixed_content,
                        }),
                        _ => None,
                    }
                };

                let message = if has_comments {
                    format!(
                        "TOML frontmatter keys are not sorted alphabetically: '{out_of_place}' should come before '{should_come_after}' (auto-fix unavailable: contains comments)"
                    )
                } else {
                    format!(
                        "TOML frontmatter keys are not sorted alphabetically: '{out_of_place}' should come before '{should_come_after}'"
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
                let key_order = self.config.key_order.as_deref();
                let Some((out_of_place, should_come_after)) = Self::find_first_unsorted_pair(&keys, key_order) else {
                    return Ok(warnings);
                };

                // Compute the actual fix: full content replacement
                let fix = match self.fix_json(content) {
                    Ok(fixed_content) if fixed_content != content => Some(Fix {
                        range: 0..content.len(),
                        replacement: fixed_content,
                    }),
                    _ => None,
                };

                let message = format!(
                    "JSON frontmatter keys are not sorted alphabetically: '{out_of_place}' should come before '{should_come_after}'"
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

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD072Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD072Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                // For opt-in rules, we need to explicitly declare the 'enabled' key
                let mut table = toml::map::Map::new();
                table.insert("enabled".to_string(), toml::Value::Boolean(false));
                Some((MD072Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            }
        } else {
            None
        }
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
        let key_order = self.config.key_order.as_deref();
        if Self::are_indexed_keys_sorted(&keys, key_order) {
            return Ok(content.to_string());
        }

        // Line-based reordering to preserve original formatting (indentation, etc.)
        // Each key owns all lines until the next top-level key
        let mut key_blocks: Vec<(String, Vec<&str>)> = Vec::new();

        for (i, (line_idx, key)) in keys.iter().enumerate() {
            let start = *line_idx;
            let end = if i + 1 < keys.len() {
                keys[i + 1].0
            } else {
                frontmatter_lines.len()
            };

            let block_lines: Vec<&str> = frontmatter_lines[start..end].to_vec();
            key_blocks.push((key.clone(), block_lines));
        }

        // Sort by key_order, with alphabetical fallback for unlisted keys
        Self::sort_keys_by_order(&mut key_blocks, key_order);

        // Reassemble frontmatter
        let content_lines: Vec<&str> = content.lines().collect();
        let fm_end = FrontMatterUtils::get_front_matter_end_line(content);

        let mut result = String::new();
        result.push_str("---\n");
        for (_, lines) in &key_blocks {
            for line in lines {
                result.push_str(line);
                result.push('\n');
            }
        }
        result.push_str("---");

        if fm_end < content_lines.len() {
            result.push('\n');
            result.push_str(&content_lines[fm_end..].join("\n"));
        }

        Ok(result)
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
        let key_order = self.config.key_order.as_deref();
        if Self::are_indexed_keys_sorted(&keys, key_order) {
            return Ok(content.to_string());
        }

        // Line-based reordering to preserve original formatting
        // Each key owns all lines until the next top-level key
        let mut key_blocks: Vec<(String, Vec<&str>)> = Vec::new();

        for (i, (line_idx, key)) in keys.iter().enumerate() {
            let start = *line_idx;
            let end = if i + 1 < keys.len() {
                keys[i + 1].0
            } else {
                frontmatter_lines.len()
            };

            let block_lines: Vec<&str> = frontmatter_lines[start..end].to_vec();
            key_blocks.push((key.clone(), block_lines));
        }

        // Sort by key_order, with alphabetical fallback for unlisted keys
        Self::sort_keys_by_order(&mut key_blocks, key_order);

        // Reassemble frontmatter
        let content_lines: Vec<&str> = content.lines().collect();
        let fm_end = FrontMatterUtils::get_front_matter_end_line(content);

        let mut result = String::new();
        result.push_str("+++\n");
        for (_, lines) in &key_blocks {
            for line in lines {
                result.push_str(line);
                result.push('\n');
            }
        }
        result.push_str("+++");

        if fm_end < content_lines.len() {
            result.push('\n');
            result.push_str(&content_lines[fm_end..].join("\n"));
        }

        Ok(result)
    }

    fn fix_json(&self, content: &str) -> Result<String, LintError> {
        let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
        if frontmatter_lines.is_empty() {
            return Ok(content.to_string());
        }

        let keys = Self::extract_json_keys(&frontmatter_lines);
        let key_order = self.config.key_order.as_deref();

        if keys.is_empty() || Self::are_keys_sorted(&keys, key_order) {
            return Ok(content.to_string());
        }

        // Reconstruct JSON content including braces for parsing
        let json_content = format!("{{{}}}", frontmatter_lines.join("\n"));

        // Parse and re-serialize with sorted keys
        match serde_json::from_str::<serde_json::Value>(&json_content) {
            Ok(serde_json::Value::Object(map)) => {
                // Sort keys according to key_order, with alphabetical fallback
                let mut sorted_map = serde_json::Map::new();
                let mut keys: Vec<_> = map.keys().cloned().collect();
                keys.sort_by(|a, b| {
                    let pos_a = Self::key_sort_position(a, key_order);
                    let pos_b = Self::key_sort_position(b, key_order);
                    pos_a.cmp(&pos_b)
                });

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

    /// Create an enabled rule for testing (alphabetical sort)
    fn create_enabled_rule() -> MD072FrontmatterKeySort {
        MD072FrontmatterKeySort::from_config_struct(MD072Config {
            enabled: true,
            key_order: None,
        })
    }

    /// Create an enabled rule with custom key order for testing
    fn create_rule_with_key_order(keys: Vec<&str>) -> MD072FrontmatterKeySort {
        MD072FrontmatterKeySort::from_config_struct(MD072Config {
            enabled: true,
            key_order: Some(keys.into_iter().map(String::from).collect()),
        })
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
        // Message shows first out-of-order pair: 'author' should come before 'title'
        assert!(result[0].message.contains("'author' should come before 'title'"));
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
        assert!(result[0].message.contains("auto-fix unavailable"));
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
        assert!(result[0].message.contains("auto-fix unavailable"));

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
        // Message shows first out-of-order pair: 'sort_by' should come before 'title'
        assert!(result[0].message.contains("'sort_by' should come before 'title'"));
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
        // Message shows first out-of-order pair: 'date' should come before 'title'
        assert!(result[0].message.contains("'date' should come before 'title'"));
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

        // description, title, author - first out-of-order: 'author' should come before 'title'
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'author' should come before 'title'"));
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
        // Message shows first out-of-order pair: 'date' should come before 'title'
        assert!(result[0].message.contains("'date' should come before 'title'"));
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

    // ==================== Warning-based Fix Tests (LSP Path) ====================

    #[test]
    fn test_warning_fix_yaml_sorts_keys() {
        let rule = create_enabled_rule();
        let content = "---\nbbb: 123\naaa:\n  - hello\n  - world\n---\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].fix.is_some(), "Warning should have a fix attached for LSP");

        let fix = warnings[0].fix.as_ref().unwrap();
        assert_eq!(fix.range, 0..content.len(), "Fix should replace entire content");

        // Apply the fix using the warning-based fix utility (LSP path)
        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify keys are sorted
        let aaa_pos = fixed.find("aaa:").expect("aaa should exist");
        let bbb_pos = fixed.find("bbb:").expect("bbb should exist");
        assert!(aaa_pos < bbb_pos, "aaa should come before bbb after sorting");
    }

    #[test]
    fn test_warning_fix_preserves_yaml_list_indentation() {
        let rule = create_enabled_rule();
        let content = "---\nbbb: 123\naaa:\n  - hello\n  - world\n---\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify list items retain their 2-space indentation
        assert!(
            fixed.contains("  - hello"),
            "List indentation should be preserved: {fixed}"
        );
        assert!(
            fixed.contains("  - world"),
            "List indentation should be preserved: {fixed}"
        );
    }

    #[test]
    fn test_warning_fix_preserves_nested_object_indentation() {
        let rule = create_enabled_rule();
        let content = "---\nzzzz: value\naaaa:\n  nested_key: nested_value\n  another: 123\n---\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 1);
        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify aaaa comes before zzzz
        let aaaa_pos = fixed.find("aaaa:").expect("aaaa should exist");
        let zzzz_pos = fixed.find("zzzz:").expect("zzzz should exist");
        assert!(aaaa_pos < zzzz_pos, "aaaa should come before zzzz");

        // Verify nested keys retain their 2-space indentation
        assert!(
            fixed.contains("  nested_key: nested_value"),
            "Nested object indentation should be preserved: {fixed}"
        );
        assert!(
            fixed.contains("  another: 123"),
            "Nested object indentation should be preserved: {fixed}"
        );
    }

    #[test]
    fn test_warning_fix_preserves_deeply_nested_structure() {
        let rule = create_enabled_rule();
        let content = "---\nzzz: top\naaa:\n  level1:\n    level2:\n      - item1\n      - item2\n---\n\n# Content\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify sorting
        let aaa_pos = fixed.find("aaa:").expect("aaa should exist");
        let zzz_pos = fixed.find("zzz:").expect("zzz should exist");
        assert!(aaa_pos < zzz_pos, "aaa should come before zzz");

        // Verify all indentation levels are preserved
        assert!(fixed.contains("  level1:"), "2-space indent should be preserved");
        assert!(fixed.contains("    level2:"), "4-space indent should be preserved");
        assert!(fixed.contains("      - item1"), "6-space indent should be preserved");
        assert!(fixed.contains("      - item2"), "6-space indent should be preserved");
    }

    #[test]
    fn test_warning_fix_toml_sorts_keys() {
        let rule = create_enabled_rule();
        let content = "+++\ntitle = \"Test\"\nauthor = \"John\"\n+++\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].fix.is_some(), "TOML warning should have a fix");

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify keys are sorted
        let author_pos = fixed.find("author").expect("author should exist");
        let title_pos = fixed.find("title").expect("title should exist");
        assert!(author_pos < title_pos, "author should come before title");
    }

    #[test]
    fn test_warning_fix_json_sorts_keys() {
        let rule = create_enabled_rule();
        let content = "{\n\"title\": \"Test\",\n\"author\": \"John\"\n}\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].fix.is_some(), "JSON warning should have a fix");

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify keys are sorted
        let author_pos = fixed.find("author").expect("author should exist");
        let title_pos = fixed.find("title").expect("title should exist");
        assert!(author_pos < title_pos, "author should come before title");
    }

    #[test]
    fn test_warning_fix_no_fix_when_comments_present() {
        let rule = create_enabled_rule();
        let content = "---\ntitle: Test\n# This is a comment\nauthor: John\n---\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].fix.is_none(),
            "Warning should NOT have a fix when comments are present"
        );
        assert!(
            warnings[0].message.contains("auto-fix unavailable"),
            "Message should indicate auto-fix is unavailable"
        );
    }

    #[test]
    fn test_warning_fix_preserves_content_after_frontmatter() {
        let rule = create_enabled_rule();
        let content = "---\nzzz: last\naaa: first\n---\n\n# Heading\n\nParagraph with content.\n\n- List item\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify content after frontmatter is preserved
        assert!(fixed.contains("# Heading"), "Heading should be preserved");
        assert!(
            fixed.contains("Paragraph with content."),
            "Paragraph should be preserved"
        );
        assert!(fixed.contains("- List item"), "List item should be preserved");
    }

    #[test]
    fn test_warning_fix_idempotent() {
        let rule = create_enabled_rule();
        let content = "---\nbbb: 2\naaa: 1\n---\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        let fixed_once = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Apply again - should produce no warnings
        let ctx2 = LintContext::new(&fixed_once, crate::config::MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap();

        assert!(
            warnings2.is_empty(),
            "After fixing, no more warnings should be produced"
        );
    }

    #[test]
    fn test_warning_fix_preserves_multiline_block_literal() {
        let rule = create_enabled_rule();
        let content = "---\nzzz: simple\naaa: |\n  Line 1 of block\n  Line 2 of block\n---\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify block literal is preserved with indentation
        assert!(fixed.contains("aaa: |"), "Block literal marker should be preserved");
        assert!(
            fixed.contains("  Line 1 of block"),
            "Block literal line 1 should be preserved with indent"
        );
        assert!(
            fixed.contains("  Line 2 of block"),
            "Block literal line 2 should be preserved with indent"
        );
    }

    #[test]
    fn test_warning_fix_preserves_folded_string() {
        let rule = create_enabled_rule();
        let content = "---\nzzz: simple\naaa: >\n  Folded line 1\n  Folded line 2\n---\n\n# Content\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify folded string is preserved
        assert!(fixed.contains("aaa: >"), "Folded string marker should be preserved");
        assert!(
            fixed.contains("  Folded line 1"),
            "Folded line 1 should be preserved with indent"
        );
        assert!(
            fixed.contains("  Folded line 2"),
            "Folded line 2 should be preserved with indent"
        );
    }

    #[test]
    fn test_warning_fix_preserves_4_space_indentation() {
        let rule = create_enabled_rule();
        // Some projects use 4-space indentation
        let content = "---\nzzz: value\naaa:\n    nested: with_4_spaces\n    another: value\n---\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify 4-space indentation is preserved exactly
        assert!(
            fixed.contains("    nested: with_4_spaces"),
            "4-space indentation should be preserved: {fixed}"
        );
        assert!(
            fixed.contains("    another: value"),
            "4-space indentation should be preserved: {fixed}"
        );
    }

    #[test]
    fn test_warning_fix_preserves_tab_indentation() {
        let rule = create_enabled_rule();
        // Some projects use tabs
        let content = "---\nzzz: value\naaa:\n\tnested: with_tab\n\tanother: value\n---\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify tab indentation is preserved exactly
        assert!(
            fixed.contains("\tnested: with_tab"),
            "Tab indentation should be preserved: {fixed}"
        );
        assert!(
            fixed.contains("\tanother: value"),
            "Tab indentation should be preserved: {fixed}"
        );
    }

    #[test]
    fn test_warning_fix_preserves_inline_list() {
        let rule = create_enabled_rule();
        // Inline YAML lists should be preserved
        let content = "---\nzzz: value\naaa: [one, two, three]\n---\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify inline list format is preserved
        assert!(
            fixed.contains("aaa: [one, two, three]"),
            "Inline list should be preserved exactly: {fixed}"
        );
    }

    #[test]
    fn test_warning_fix_preserves_quoted_strings() {
        let rule = create_enabled_rule();
        // Quoted strings with special chars
        let content = "---\nzzz: simple\naaa: \"value with: colon\"\nbbb: 'single quotes'\n---\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).expect("Fix should apply");

        // Verify quoted strings are preserved exactly
        assert!(
            fixed.contains("aaa: \"value with: colon\""),
            "Double-quoted string should be preserved: {fixed}"
        );
        assert!(
            fixed.contains("bbb: 'single quotes'"),
            "Single-quoted string should be preserved: {fixed}"
        );
    }

    // ==================== Custom Key Order Tests ====================

    #[test]
    fn test_yaml_custom_key_order_sorted() {
        // Keys match the custom order: title, date, author
        let rule = create_rule_with_key_order(vec!["title", "date", "author"]);
        let content = "---\ntitle: Test\ndate: 2024-01-01\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Keys are in the custom order, should be considered sorted
        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_custom_key_order_unsorted() {
        // Keys NOT in the custom order: should report author before date
        let rule = create_rule_with_key_order(vec!["title", "date", "author"]);
        let content = "---\ntitle: Test\nauthor: John\ndate: 2024-01-01\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        // 'date' should come before 'author' according to custom order
        assert!(result[0].message.contains("'date' should come before 'author'"));
    }

    #[test]
    fn test_yaml_custom_key_order_unlisted_keys_alphabetical() {
        // unlisted keys should come after specified keys, sorted alphabetically
        let rule = create_rule_with_key_order(vec!["title"]);
        let content = "---\ntitle: Test\nauthor: John\ndate: 2024-01-01\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // title is specified, author and date are not - they should be alphabetically after title
        // author < date alphabetically, so this is sorted
        assert!(result.is_empty());
    }

    #[test]
    fn test_yaml_custom_key_order_unlisted_keys_unsorted() {
        // unlisted keys out of alphabetical order
        let rule = create_rule_with_key_order(vec!["title"]);
        let content = "---\ntitle: Test\nzebra: Zoo\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // zebra and author are unlisted, author < zebra alphabetically
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'author' should come before 'zebra'"));
    }

    #[test]
    fn test_yaml_custom_key_order_fix() {
        let rule = create_rule_with_key_order(vec!["title", "date", "author"]);
        let content = "---\nauthor: John\ndate: 2024-01-01\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Keys should be in custom order: title, date, author
        let title_pos = fixed.find("title:").unwrap();
        let date_pos = fixed.find("date:").unwrap();
        let author_pos = fixed.find("author:").unwrap();
        assert!(
            title_pos < date_pos && date_pos < author_pos,
            "Fixed YAML should have keys in custom order: title, date, author. Got:\n{fixed}"
        );
    }

    #[test]
    fn test_yaml_custom_key_order_fix_with_unlisted() {
        // Mix of listed and unlisted keys
        let rule = create_rule_with_key_order(vec!["title", "author"]);
        let content = "---\nzebra: Zoo\nauthor: John\ntitle: Test\naardvark: Ant\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Order should be: title, author (specified), then aardvark, zebra (alphabetical)
        let title_pos = fixed.find("title:").unwrap();
        let author_pos = fixed.find("author:").unwrap();
        let aardvark_pos = fixed.find("aardvark:").unwrap();
        let zebra_pos = fixed.find("zebra:").unwrap();

        assert!(
            title_pos < author_pos && author_pos < aardvark_pos && aardvark_pos < zebra_pos,
            "Fixed YAML should have specified keys first, then unlisted alphabetically. Got:\n{fixed}"
        );
    }

    #[test]
    fn test_toml_custom_key_order_sorted() {
        let rule = create_rule_with_key_order(vec!["title", "date", "author"]);
        let content = "+++\ntitle = \"Test\"\ndate = \"2024-01-01\"\nauthor = \"John\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_toml_custom_key_order_unsorted() {
        let rule = create_rule_with_key_order(vec!["title", "date", "author"]);
        let content = "+++\nauthor = \"John\"\ntitle = \"Test\"\ndate = \"2024-01-01\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("TOML"));
    }

    #[test]
    fn test_json_custom_key_order_sorted() {
        let rule = create_rule_with_key_order(vec!["title", "date", "author"]);
        let content = "{\n  \"title\": \"Test\",\n  \"date\": \"2024-01-01\",\n  \"author\": \"John\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_json_custom_key_order_unsorted() {
        let rule = create_rule_with_key_order(vec!["title", "date", "author"]);
        let content = "{\n  \"author\": \"John\",\n  \"title\": \"Test\",\n  \"date\": \"2024-01-01\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("JSON"));
    }

    #[test]
    fn test_key_order_case_insensitive_match() {
        // Key order should match case-insensitively
        let rule = create_rule_with_key_order(vec!["Title", "Date", "Author"]);
        let content = "---\ntitle: Test\ndate: 2024-01-01\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Keys match the custom order (case-insensitive)
        assert!(result.is_empty());
    }

    #[test]
    fn test_key_order_partial_match() {
        // Some keys specified, some not
        let rule = create_rule_with_key_order(vec!["title"]);
        let content = "---\ntitle: Test\ndate: 2024-01-01\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only 'title' is specified, so it comes first
        // 'author' and 'date' are unlisted and sorted alphabetically: author < date
        // But current order is date, author - WRONG
        // Wait, content has: title, date, author
        // title is specified (pos 0)
        // date is unlisted (pos MAX, "date")
        // author is unlisted (pos MAX, "author")
        // Since both unlisted, compare alphabetically: author < date
        // So author should come before date, but date comes before author in content
        // This IS unsorted!
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'author' should come before 'date'"));
    }

    // ==================== Key Order Edge Cases ====================

    #[test]
    fn test_key_order_empty_array_falls_back_to_alphabetical() {
        // Empty key_order should behave like alphabetical sorting
        let rule = MD072FrontmatterKeySort::from_config_struct(MD072Config {
            enabled: true,
            key_order: Some(vec![]),
        });
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // With empty key_order, all keys are unlisted → alphabetical
        // author < title, but title comes first in content → unsorted
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'author' should come before 'title'"));
    }

    #[test]
    fn test_key_order_single_key() {
        // key_order with only one key
        let rule = create_rule_with_key_order(vec!["title"]);
        let content = "---\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_key_order_all_keys_specified() {
        // All document keys are in key_order
        let rule = create_rule_with_key_order(vec!["title", "author", "date"]);
        let content = "---\ntitle: Test\nauthor: John\ndate: 2024-01-01\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_key_order_no_keys_match() {
        // None of the document keys are in key_order
        let rule = create_rule_with_key_order(vec!["foo", "bar", "baz"]);
        let content = "---\nauthor: John\ndate: 2024-01-01\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // All keys are unlisted, so they sort alphabetically: author, date, title
        // Current order is author, date, title - which IS sorted
        assert!(result.is_empty());
    }

    #[test]
    fn test_key_order_no_keys_match_unsorted() {
        // None of the document keys are in key_order, and they're out of alphabetical order
        let rule = create_rule_with_key_order(vec!["foo", "bar", "baz"]);
        let content = "---\ntitle: Test\ndate: 2024-01-01\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // All unlisted → alphabetical: author < date < title
        // Current: title, date, author → unsorted
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_key_order_duplicate_keys_in_config() {
        // Duplicate keys in key_order (should use first occurrence)
        let rule = MD072FrontmatterKeySort::from_config_struct(MD072Config {
            enabled: true,
            key_order: Some(vec![
                "title".to_string(),
                "author".to_string(),
                "title".to_string(), // duplicate
            ]),
        });
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // title (pos 0), author (pos 1) → sorted
        assert!(result.is_empty());
    }

    #[test]
    fn test_key_order_with_comments_still_skips_fix() {
        // key_order should not affect the comment-skipping behavior
        let rule = create_rule_with_key_order(vec!["title", "author"]);
        let content = "---\n# This is a comment\nauthor: John\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should detect unsorted AND indicate no auto-fix due to comments
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("auto-fix unavailable"));
        assert!(result[0].fix.is_none());
    }

    #[test]
    fn test_toml_custom_key_order_fix() {
        let rule = create_rule_with_key_order(vec!["title", "date", "author"]);
        let content = "+++\nauthor = \"John\"\ndate = \"2024-01-01\"\ntitle = \"Test\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Keys should be in custom order: title, date, author
        let title_pos = fixed.find("title").unwrap();
        let date_pos = fixed.find("date").unwrap();
        let author_pos = fixed.find("author").unwrap();
        assert!(
            title_pos < date_pos && date_pos < author_pos,
            "Fixed TOML should have keys in custom order. Got:\n{fixed}"
        );
    }

    #[test]
    fn test_json_custom_key_order_fix() {
        let rule = create_rule_with_key_order(vec!["title", "date", "author"]);
        let content = "{\n  \"author\": \"John\",\n  \"date\": \"2024-01-01\",\n  \"title\": \"Test\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Keys should be in custom order: title, date, author
        let title_pos = fixed.find("\"title\"").unwrap();
        let date_pos = fixed.find("\"date\"").unwrap();
        let author_pos = fixed.find("\"author\"").unwrap();
        assert!(
            title_pos < date_pos && date_pos < author_pos,
            "Fixed JSON should have keys in custom order. Got:\n{fixed}"
        );
    }

    #[test]
    fn test_key_order_unicode_keys() {
        // Unicode keys in key_order
        let rule = MD072FrontmatterKeySort::from_config_struct(MD072Config {
            enabled: true,
            key_order: Some(vec!["タイトル".to_string(), "著者".to_string()]),
        });
        let content = "---\nタイトル: テスト\n著者: 山田太郎\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Keys match the custom order
        assert!(result.is_empty());
    }

    #[test]
    fn test_key_order_mixed_specified_and_unlisted_boundary() {
        // Test the boundary between specified and unlisted keys
        let rule = create_rule_with_key_order(vec!["z_last_specified"]);
        let content = "---\nz_last_specified: value\na_first_unlisted: value\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // z_last_specified (pos 0) should come before a_first_unlisted (pos MAX)
        // even though 'a' < 'z' alphabetically
        assert!(result.is_empty());
    }

    #[test]
    fn test_key_order_fix_preserves_values() {
        // Ensure fix preserves complex values when reordering with key_order
        let rule = create_rule_with_key_order(vec!["title", "tags"]);
        let content = "---\ntags:\n  - rust\n  - markdown\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // title should come before tags
        let title_pos = fixed.find("title:").unwrap();
        let tags_pos = fixed.find("tags:").unwrap();
        assert!(title_pos < tags_pos, "title should come before tags");

        // Nested list should be preserved
        assert!(fixed.contains("- rust"), "List items should be preserved");
        assert!(fixed.contains("- markdown"), "List items should be preserved");
    }

    #[test]
    fn test_key_order_idempotent_fix() {
        // Fixing twice should produce the same result
        let rule = create_rule_with_key_order(vec!["title", "date", "author"]);
        let content = "---\nauthor: John\ndate: 2024-01-01\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed_once = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed_once, crate::config::MarkdownFlavor::Standard, None);
        let fixed_twice = rule.fix(&ctx2).unwrap();

        assert_eq!(fixed_once, fixed_twice, "Fix should be idempotent");
    }

    #[test]
    fn test_key_order_respects_later_position_over_alphabetical() {
        // If key_order says "z" comes before "a", that should be respected
        let rule = create_rule_with_key_order(vec!["zebra", "aardvark"]);
        let content = "---\nzebra: Zoo\naardvark: Ant\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // zebra (pos 0), aardvark (pos 1) → sorted according to key_order
        assert!(result.is_empty());
    }

    #[test]
    fn test_key_order_detects_wrong_custom_order() {
        // Document has aardvark before zebra, but key_order says zebra first
        let rule = create_rule_with_key_order(vec!["zebra", "aardvark"]);
        let content = "---\naardvark: Ant\nzebra: Zoo\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'zebra' should come before 'aardvark'"));
    }
}
