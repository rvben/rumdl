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

    /// Keys that must be present in the frontmatter. Each missing key is
    /// reported, matched case-insensitively against top-level keys (like
    /// `key_order`). Independent of `key_order`: you can order many keys while
    /// requiring only a few. These warnings carry no fix because rumdl cannot
    /// invent meaningful values for missing keys.
    ///
    /// Only applies to files that have frontmatter; requiring frontmatter to
    /// exist at all is out of scope for this rule.
    ///
    /// Example: `required_keys = ["title", "date"]`
    #[serde(default, alias = "required-keys")]
    pub required_keys: Vec<String>,
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
            // (searched outside a leading quoted key, which may itself
            // contain one, e.g. "og:title").
            if !line.starts_with(' ')
                && !line.starts_with('\t')
                && let Some(colon_pos) = Self::separator_pos_outside_quoted_key(line, ':')
            {
                let raw = line[..colon_pos].trim();
                if !raw.is_empty() && !raw.starts_with('#') {
                    // Sort by the key's content, not by surrounding quote
                    // characters: a quoted key like "zebra" must compare as
                    // `zebra`, not as `"zebra` (which would always sort before
                    // any unquoted key because '"' is ASCII 34).
                    let key = raw
                        .strip_prefix('"')
                        .and_then(|k| k.strip_suffix('"'))
                        .or_else(|| raw.strip_prefix('\'').and_then(|k| k.strip_suffix('\'')))
                        .unwrap_or(raw);
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
            // (searched outside a leading quoted key, which may itself
            // contain one, e.g. "a=b").
            if !line.starts_with(' ')
                && !line.starts_with('\t')
                && let Some(eq_pos) = Self::separator_pos_outside_quoted_key(line, '=')
            {
                let raw = line[..eq_pos].trim();
                if !raw.is_empty() {
                    // Compare by the key's content, not the surrounding quote
                    // characters: a TOML basic (`"key"`) or literal (`'key'`)
                    // quoted key must sort and match like its bare form ('"'
                    // is ASCII 34 and would sort before any unquoted key).
                    let key = raw
                        .strip_prefix('"')
                        .and_then(|k| k.strip_suffix('"'))
                        .or_else(|| raw.strip_prefix('\'').and_then(|k| k.strip_suffix('\'')))
                        .unwrap_or(raw);
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

            // Count braces and brackets to track nesting, skipping those inside strings
            let mut in_string = false;
            let mut prev_backslash = false;
            for ch in line.chars() {
                if in_string {
                    if ch == '"' && !prev_backslash {
                        in_string = false;
                    }
                    prev_backslash = ch == '\\' && !prev_backslash;
                } else {
                    match ch {
                        '"' => in_string = true,
                        '{' | '[' => depth += 1,
                        '}' | ']' => depth = depth.saturating_sub(1),
                        _ => {}
                    }
                    prev_backslash = false;
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
    ) -> Option<(usize, &'a str, &'a str)> {
        for i in 1..keys.len() {
            let pos_curr = Self::key_sort_position(&keys[i].1, key_order);
            let pos_prev = Self::key_sort_position(&keys[i - 1].1, key_order);
            if pos_curr < pos_prev {
                return Some((keys[i].0, &keys[i].1, &keys[i - 1].1));
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

    /// Byte position of `separator` outside a leading quoted key: a quoted
    /// key (`"og:title":`, `"a=b" =`) may contain the separator character,
    /// so the search starts after the closing quote.
    fn separator_pos_outside_quoted_key(line: &str, separator: char) -> Option<usize> {
        let after_quote = if let Some(rest) = line.strip_prefix('"') {
            rest.find('"').map(|i| i + 2)
        } else if let Some(rest) = line.strip_prefix('\'') {
            rest.find('\'').map(|i| i + 2)
        } else {
            None
        };
        match after_quote {
            Some(start) => line[start..].find(separator).map(|i| start + i),
            None => line.find(separator),
        }
    }

    /// The top-level key a raw TOML key expression defines. A quoted key is
    /// atomic (`"a.b"` defines `a.b`); otherwise the root of a dotted path
    /// (`params.seo` defines `params`).
    fn toml_root_key(raw: &str) -> &str {
        if let Some(rest) = raw.strip_prefix('"') {
            if let Some(end) = rest.find('"') {
                return &rest[..end];
            }
        } else if let Some(rest) = raw.strip_prefix('\'')
            && let Some(end) = rest.find('\'')
        {
            return &rest[..end];
        }
        raw.split('.').next().unwrap_or(raw).trim()
    }

    /// Every top-level key the TOML frontmatter defines, for the presence
    /// check. Broader than `extract_toml_keys` (which is scoped to what the
    /// sort check orders): dotted assignments count by their root
    /// (`params.seo = true` defines `params`), and table headers count too
    /// (`[taxonomies]`, `[[authors]]`, `[params.seo] # comment`). Assignments
    /// after the first table header are nested inside that table, not
    /// top-level.
    fn extract_toml_presence_keys(frontmatter_lines: &[&str]) -> Vec<String> {
        let mut keys = Vec::new();
        let mut in_tables = false;

        for line in frontmatter_lines {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with('[') {
                in_tables = true;
                // Take the bracketed expression only: a header may carry an
                // inline comment after the closing bracket.
                let inner = trimmed
                    .strip_prefix("[[")
                    .and_then(|s| s.split_once("]]").map(|(inner, _)| inner))
                    .or_else(|| {
                        trimmed
                            .strip_prefix('[')
                            .and_then(|s| s.split_once(']').map(|(inner, _)| inner))
                    });
                if let Some(inner) = inner {
                    let key = Self::toml_root_key(inner.trim());
                    if !key.is_empty() {
                        keys.push(key.to_string());
                    }
                }
                continue;
            }
            if !in_tables
                && !line.starts_with(' ')
                && !line.starts_with('\t')
                && let Some(eq_pos) = Self::separator_pos_outside_quoted_key(line, '=')
            {
                let key = Self::toml_root_key(line[..eq_pos].trim());
                if !key.is_empty() {
                    keys.push(key.to_string());
                }
            }
        }

        keys
    }

    /// Parse the JSON frontmatter and return its top-level keys, in no
    /// particular order (`serde_json::Map` sorts them). `None` when the JSON
    /// does not parse as an object.
    fn parse_json_top_level_keys(frontmatter_lines: &[&str]) -> Option<Vec<String>> {
        let json_content = format!("{{{}}}", frontmatter_lines.join("\n"));
        match serde_json::from_str::<serde_json::Value>(&json_content) {
            Ok(serde_json::Value::Object(map)) => Some(map.keys().cloned().collect()),
            _ => None,
        }
    }

    /// Warnings for configured required keys absent from the frontmatter.
    ///
    /// Matching is case-insensitive against top-level keys, consistent with how
    /// `key_order` matches. The warning spans the whole frontmatter block (the
    /// opening fence through the closing fence): the absence belongs to the
    /// block, not to any line in it, and the range lets an inline disable
    /// comment anywhere inside the frontmatter suppress the warning, like it
    /// does for the sort warnings. No fix is attached because rumdl cannot
    /// invent a meaningful value.
    fn missing_required_key_warnings(
        &self,
        present_keys: &[String],
        format: &str,
        fence_len: usize,
        fm_end_line: usize,
    ) -> Vec<LintWarning> {
        if self.config.required_keys.is_empty() {
            return Vec::new();
        }

        let present: Vec<String> = present_keys.iter().map(|k| k.to_lowercase()).collect();
        self.config
            .required_keys
            .iter()
            .filter(|required| !present.contains(&required.to_lowercase()))
            .map(|required| LintWarning {
                rule_name: Some(self.name().to_string()),
                message: format!("{format} frontmatter is missing required key '{required}'"),
                line: 1,
                column: 1,
                end_line: fm_end_line.max(1),
                end_column: fence_len + 1,
                severity: Severity::Warning,
                fix: None,
            })
            .collect()
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
                let keys = Self::extract_yaml_keys(&frontmatter_lines);

                let key_names: Vec<String> = keys.iter().map(|(_, key)| key.clone()).collect();
                warnings.extend(self.missing_required_key_warnings(&key_names, "YAML", 3, ctx.front_matter_end_line()));

                if frontmatter_lines.is_empty() {
                    return Ok(warnings);
                }

                let key_order = self.config.key_order.as_deref();
                let Some((key_idx, out_of_place, should_come_after)) =
                    Self::find_first_unsorted_indexed_pair(&keys, key_order)
                else {
                    return Ok(warnings);
                };
                // key_idx is relative to frontmatter_lines; +2 for 1-indexing and the opening ---
                let key_line = key_idx + 2;

                let has_comments = Self::has_comments(&frontmatter_lines);

                let fix = if has_comments {
                    None
                } else {
                    // Compute the actual fix: full content replacement
                    let fixed_content = self.fix_yaml(content, ctx.front_matter_end_line());
                    if fixed_content != content {
                        Some(Fix::new(0..content.len(), fixed_content))
                    } else {
                        None
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

                // out_of_place has surrounding quotes stripped for sorting, so
                // span the raw key (quotes included) as it appears on the line.
                let end_column = frontmatter_lines
                    .get(key_idx)
                    .and_then(|line| {
                        Self::separator_pos_outside_quoted_key(line, ':')
                            .map(|pos| line[..pos].trim().chars().count() + 1)
                    })
                    .unwrap_or(out_of_place.chars().count() + 1);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message,
                    line: key_line,
                    column: 1,
                    end_line: key_line,
                    end_column,
                    severity: Severity::Warning,
                    fix,
                });
            }
            FrontMatterType::Toml => {
                let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
                let keys = Self::extract_toml_keys(&frontmatter_lines);

                // Presence uses a broader extraction than the sort check:
                // table headers and the roots of dotted assignments define
                // top-level keys too.
                let key_names = Self::extract_toml_presence_keys(&frontmatter_lines);
                warnings.extend(self.missing_required_key_warnings(&key_names, "TOML", 3, ctx.front_matter_end_line()));

                if frontmatter_lines.is_empty() {
                    return Ok(warnings);
                }

                let key_order = self.config.key_order.as_deref();
                let Some((key_idx, out_of_place, should_come_after)) =
                    Self::find_first_unsorted_indexed_pair(&keys, key_order)
                else {
                    return Ok(warnings);
                };
                let key_line = key_idx + 2;

                let has_comments = Self::has_comments(&frontmatter_lines);

                let fix = if has_comments {
                    None
                } else {
                    // Compute the actual fix: full content replacement
                    let fixed_content = self.fix_toml(content, ctx.front_matter_end_line());
                    if fixed_content != content {
                        Some(Fix::new(0..content.len(), fixed_content))
                    } else {
                        None
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

                // out_of_place has surrounding quotes stripped for sorting, so
                // span the raw key (quotes included) as it appears on the line.
                let end_column = frontmatter_lines
                    .get(key_idx)
                    .and_then(|line| {
                        Self::separator_pos_outside_quoted_key(line, '=')
                            .map(|pos| line[..pos].trim().chars().count() + 1)
                    })
                    .unwrap_or(out_of_place.chars().count() + 1);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message,
                    line: key_line,
                    column: 1,
                    end_line: key_line,
                    end_column,
                    severity: Severity::Warning,
                    fix,
                });
            }
            FrontMatterType::Json => {
                let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
                let keys = Self::extract_json_keys(&frontmatter_lines);

                // Presence is checked against a real JSON parse: the line-based
                // extractor preserves document order for the sort check but
                // captures only the first key per line, and a key it misses
                // would be a false "missing required key". Order is irrelevant
                // for presence, so the parsed object is authoritative; fall
                // back to the line-based list when the JSON does not parse.
                let parsed_keys = Self::parse_json_top_level_keys(&frontmatter_lines);
                warnings.extend(self.missing_required_key_warnings(
                    parsed_keys.as_deref().unwrap_or(&keys),
                    "JSON",
                    1,
                    ctx.front_matter_end_line(),
                ));

                if frontmatter_lines.is_empty() {
                    return Ok(warnings);
                }

                let key_order = self.config.key_order.as_deref();
                let Some((out_of_place, should_come_after)) = Self::find_first_unsorted_pair(&keys, key_order) else {
                    return Ok(warnings);
                };

                // Compute the actual fix: full content replacement
                let fixed_content = self.fix_json(content, ctx.front_matter_end_line());
                let fix = if fixed_content != content {
                    Some(Fix::new(0..content.len(), fixed_content))
                } else {
                    None
                };

                let message = format!(
                    "JSON frontmatter keys are not sorted alphabetically: '{out_of_place}' should come before '{should_come_after}'"
                );

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message,
                    line: 2,
                    column: 1,
                    end_line: 2,
                    end_column: out_of_place.chars().count() + 1,
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

        // Skip fix if rule is disabled via inline config at the frontmatter region (line 2)
        if ctx.is_rule_disabled(self.name(), 2) {
            return Ok(content.to_string());
        }

        let fm_type = FrontMatterUtils::detect_front_matter_type(content);

        let fm_end = ctx.front_matter_end_line();
        Ok(match fm_type {
            FrontMatterType::Yaml => self.fix_yaml(content, fm_end),
            FrontMatterType::Toml => self.fix_toml(content, fm_end),
            FrontMatterType::Json => self.fix_json(content, fm_end),
            _ => content.to_string(),
        })
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::FrontMatter
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty()
            || !ctx.content.starts_with("---") && !ctx.content.starts_with("+++") && !ctx.content.starts_with('{')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD072Config, nullable);
}

impl MD072FrontmatterKeySort {
    /// Restore the original document's trailing newline. The fix functions
    /// rebuild content via `lines()` + `join("\n")`, which never re-emits a
    /// final newline, so without this a file ending in `\n` would lose it on
    /// every fix (a dirty, non-idempotent diff).
    fn preserve_trailing_newline(original: &str, mut result: String) -> String {
        if original.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }
        result
    }

    fn fix_yaml(&self, content: &str, fm_end: usize) -> String {
        let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
        if frontmatter_lines.is_empty() {
            return content.to_string();
        }

        // Cannot fix if comments present
        if Self::has_comments(&frontmatter_lines) {
            return content.to_string();
        }

        let keys = Self::extract_yaml_keys(&frontmatter_lines);
        let key_order = self.config.key_order.as_deref();
        if Self::are_indexed_keys_sorted(&keys, key_order) {
            return content.to_string();
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

        Self::preserve_trailing_newline(content, result)
    }

    fn fix_toml(&self, content: &str, fm_end: usize) -> String {
        let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
        if frontmatter_lines.is_empty() {
            return content.to_string();
        }

        // Cannot fix if comments present
        if Self::has_comments(&frontmatter_lines) {
            return content.to_string();
        }

        let keys = Self::extract_toml_keys(&frontmatter_lines);
        let key_order = self.config.key_order.as_deref();
        if Self::are_indexed_keys_sorted(&keys, key_order) {
            return content.to_string();
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

        Self::preserve_trailing_newline(content, result)
    }

    fn fix_json(&self, content: &str, fm_end: usize) -> String {
        let frontmatter_lines = FrontMatterUtils::extract_front_matter(content);
        if frontmatter_lines.is_empty() {
            return content.to_string();
        }

        let keys = Self::extract_json_keys(&frontmatter_lines);
        let key_order = self.config.key_order.as_deref();

        if keys.is_empty() || Self::are_keys_sorted(&keys, key_order) {
            return content.to_string();
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

                        // The pretty-printed JSON includes the outer braces
                        // We need to format it properly for frontmatter
                        let mut result = String::new();
                        result.push_str(&sorted_json);

                        if fm_end < lines.len() {
                            result.push('\n');
                            result.push_str(&lines[fm_end..].join("\n"));
                        }

                        Self::preserve_trailing_newline(content, result)
                    }
                    Err(_) => content.to_string(),
                }
            }
            _ => content.to_string(),
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
            ..Default::default()
        })
    }

    /// Create an enabled rule with custom key order for testing
    fn create_rule_with_key_order(keys: Vec<&str>) -> MD072FrontmatterKeySort {
        MD072FrontmatterKeySort::from_config_struct(MD072Config {
            enabled: true,
            key_order: Some(keys.into_iter().map(String::from).collect()),
            ..Default::default()
        })
    }

    /// Create an enabled rule with required keys for testing
    fn create_rule_with_required_keys(keys: Vec<&str>) -> MD072FrontmatterKeySort {
        MD072FrontmatterKeySort::from_config_struct(MD072Config {
            enabled: true,
            required_keys: keys.into_iter().map(String::from).collect(),
            ..Default::default()
        })
    }

    // ==================== Config Tests ====================

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
    fn test_yaml_fix_preserves_trailing_newline() {
        let rule = create_enabled_rule();
        // Content ends with a trailing newline; fix must not strip it.
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.ends_with('\n'),
            "trailing newline must be preserved, got {fixed:?}"
        );

        // And the fix is idempotent on trailing-newline content.
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let fixed_twice = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed, fixed_twice);
    }

    #[test]
    fn test_yaml_fix_whole_file_frontmatter_preserves_trailing_newline() {
        let rule = create_enabled_rule();
        // Frontmatter is the entire file (no body after the closing fence).
        let content = "---\ntitle: Test\nauthor: John\n---\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.ends_with('\n'),
            "trailing newline must be preserved, got {fixed:?}"
        );
    }

    #[test]
    fn test_yaml_quoted_keys_sort_by_content() {
        let rule = create_enabled_rule();
        // A quoted key must sort by its unquoted content, not by the leading
        // quote char. "zebra" before apple is out of order alphabetically.
        let content = "---\n\"zebra\": 1\napple: 2\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "quoted key out of order must be flagged");
        assert!(result[0].message.contains("'apple' should come before 'zebra'"));
    }

    #[test]
    fn test_yaml_quoted_key_warning_span_covers_quotes() {
        let rule = create_enabled_rule();
        // "apple" is out of order (should come before banana). Its quotes are
        // stripped for sorting, but the diagnostic span must still cover the
        // raw key as written, including the quotes.
        let content = "---\nbanana: 1\n\"apple\": 2\n---\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        let w = &result[0];
        assert_eq!(w.line, 3);
        assert_eq!(w.column, 1);
        // Raw key `"apple"` is 7 chars, so end_column is 8 (not 6 for `apple`).
        assert_eq!(w.end_column, 8, "diagnostic span must cover the quoted key");
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
        let parsed: Result<serde_yaml::Value, _> = serde_yaml::from_str(&fm_content);
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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

    // ==================== JSON braces in string values ====================

    #[test]
    fn test_json_braces_in_string_values_extracts_all_keys() {
        // Braces inside JSON string values should not affect depth tracking.
        // The key "author" (on the line after the brace-containing value) must be extracted.
        // Content is already sorted, so no warnings expected.
        let rule = create_enabled_rule();
        let content = "{\n\"author\": \"Someone\",\n\"description\": \"Use { to open\",\n\"tags\": [\"a\"],\n\"title\": \"My Post\"\n}\n\nContent here.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // If all 4 keys are extracted, they are already sorted: author, description, tags, title
        assert!(
            result.is_empty(),
            "All keys should be extracted and recognized as sorted. Got: {result:?}"
        );
    }

    #[test]
    fn test_json_braces_in_string_key_after_brace_value_detected() {
        // Specifically verify that a key appearing AFTER a line with unbalanced braces in a string is extracted
        let rule = create_enabled_rule();
        // "description" has an unbalanced `{` in its value
        // "author" comes on the next line and must be detected as a top-level key
        let content = "{\n\"description\": \"Use { to open\",\n\"author\": \"Someone\"\n}\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author < description alphabetically, but description comes first => unsorted
        // The warning should mention 'author' should come before 'description'
        assert_eq!(
            result.len(),
            1,
            "Should detect unsorted keys after brace-containing string value"
        );
        assert!(
            result[0].message.contains("'author' should come before 'description'"),
            "Should report author before description. Got: {}",
            result[0].message
        );
    }

    #[test]
    fn test_json_brackets_in_string_values() {
        // Brackets inside JSON string values should not affect depth tracking
        let rule = create_enabled_rule();
        let content = "{\n\"description\": \"My [Post]\",\n\"author\": \"Someone\"\n}\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author < description, but description comes first => unsorted
        assert_eq!(
            result.len(),
            1,
            "Should detect unsorted keys despite brackets in string values"
        );
        assert!(
            result[0].message.contains("'author' should come before 'description'"),
            "Got: {}",
            result[0].message
        );
    }

    #[test]
    fn test_json_escaped_quotes_in_values() {
        // Escaped quotes inside values should not break string tracking
        let rule = create_enabled_rule();
        let content = "{\n\"title\": \"He said \\\"hello {world}\\\"\",\n\"author\": \"Someone\"\n}\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author < title, title comes first => unsorted
        assert_eq!(result.len(), 1, "Should handle escaped quotes with braces in values");
        assert!(
            result[0].message.contains("'author' should come before 'title'"),
            "Got: {}",
            result[0].message
        );
    }

    #[test]
    fn test_json_multiple_braces_in_string() {
        // Multiple unbalanced braces in string values
        let rule = create_enabled_rule();
        let content = "{\n\"pattern\": \"{{{}}\",\n\"author\": \"Someone\"\n}\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // author < pattern, but pattern comes first => unsorted
        assert_eq!(result.len(), 1, "Should handle multiple braces in string values");
        assert!(
            result[0].message.contains("'author' should come before 'pattern'"),
            "Got: {}",
            result[0].message
        );
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

    // ==================== Required Keys Tests ====================

    #[test]
    fn test_required_keys_yaml_missing_key_warns_without_fix() {
        let rule = create_rule_with_required_keys(vec!["title", "date"]);
        let content = "---\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing required key 'date'"));
        assert!(result[0].message.contains("YAML"));
        assert!(result[0].fix.is_none(), "missing keys must not be auto-fixable");
        // The warning spans the opening fence on line 1.
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 1);
        assert_eq!(result[0].end_column, 4);
    }

    #[test]
    fn test_required_keys_all_present_no_warning() {
        let rule = create_rule_with_required_keys(vec!["author", "title"]);
        let content = "---\nauthor: John\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_required_keys_one_warning_per_missing_key() {
        let rule = create_rule_with_required_keys(vec!["title", "date", "author"]);
        let content = "---\ntags: [a, b]\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3);
        let messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("'title'")));
        assert!(messages.iter().any(|m| m.contains("'date'")));
        assert!(messages.iter().any(|m| m.contains("'author'")));
    }

    #[test]
    fn test_required_keys_case_insensitive_match() {
        // Matching is case-insensitive, consistent with key_order matching.
        let rule = create_rule_with_required_keys(vec!["Title"]);
        let content = "---\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_required_keys_missing_and_unsorted_both_reported() {
        let rule = MD072FrontmatterKeySort::from_config_struct(MD072Config {
            enabled: true,
            required_keys: vec!["date".to_string()],
            ..Default::default()
        });
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("missing required key 'date'"));
        assert!(result[1].message.contains("'author' should come before 'title'"));
    }

    #[test]
    fn test_required_keys_toml_missing_key() {
        let rule = create_rule_with_required_keys(vec!["title", "date"]);
        let content = "+++\ntitle = \"Test\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(
            result[0]
                .message
                .contains("TOML frontmatter is missing required key 'date'")
        );
        assert!(result[0].fix.is_none());
    }

    #[test]
    fn test_required_keys_json_missing_key() {
        let rule = create_rule_with_required_keys(vec!["title", "date"]);
        let content = "{\n\"title\": \"Test\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(
            result[0]
                .message
                .contains("JSON frontmatter is missing required key 'date'")
        );
        // JSON's opening fence is `{`, so the span is a single character.
        assert_eq!(result[0].end_column, 2);
    }

    #[test]
    fn test_required_keys_no_frontmatter_no_warning() {
        // Whether frontmatter must exist at all is out of scope for MD072;
        // required keys only apply to files that have frontmatter.
        let rule = create_rule_with_required_keys(vec!["title"]);
        let content = "# Heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_required_keys_empty_frontmatter_warns() {
        // An empty (but present) frontmatter block is missing every required key.
        let rule = create_rule_with_required_keys(vec!["title", "date"]);
        let content = "---\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|w| w.message.contains("missing required key")));
    }

    #[test]
    fn test_required_keys_nested_key_does_not_satisfy() {
        // Only top-level keys count, consistent with the sorting checks.
        let rule = create_rule_with_required_keys(vec!["title"]);
        let content = "---\nmeta:\n  title: Nested\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing required key 'title'"));
    }

    #[test]
    fn test_required_keys_quoted_yaml_key_satisfies() {
        // Quoted keys are matched by their content, like the sorting checks.
        let rule = create_rule_with_required_keys(vec!["title"]);
        let content = "---\n\"title\": Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_required_keys_fix_does_not_insert_keys() {
        // fix() must leave content unchanged when the only issue is a missing
        // required key: there is no meaningful value to insert.
        let rule = create_rule_with_required_keys(vec!["date"]);
        let content = "---\nauthor: John\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, content);
    }

    #[test]
    fn test_required_keys_with_key_order_subset() {
        // required_keys can be a subset of key_order: ordering covers many
        // keys, existence is enforced for a few.
        let rule = MD072FrontmatterKeySort::from_config_struct(MD072Config {
            enabled: true,
            key_order: Some(vec![
                "title".to_string(),
                "date".to_string(),
                "author".to_string(),
                "tags".to_string(),
            ]),
            required_keys: vec!["title".to_string(), "date".to_string()],
        });

        // Ordered correctly but missing 'date': exactly one warning.
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing required key 'date'"));

        // All required keys present and ordered: clean.
        let content = "---\ntitle: Test\ndate: 2024-01-01\ntags: [a]\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_required_keys_unsorted_fix_still_applies_without_inserting() {
        // When keys are both unsorted and one is missing, the sort fix applies
        // and the missing key stays missing (and keeps warning afterwards).
        let rule = create_rule_with_required_keys(vec!["date"]);
        let content = "---\ntitle: Test\nauthor: John\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let author_pos = fixed.find("author:").unwrap();
        let title_pos = fixed.find("title:").unwrap();
        assert!(author_pos < title_pos, "sort fix must still apply");
        assert!(!fixed.contains("date"), "fix must not insert the missing key");

        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx2).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing required key 'date'"));
    }

    #[test]
    fn test_required_keys_warning_spans_the_frontmatter_block() {
        // The absence belongs to the block, so the warning covers line 1
        // through the closing fence. The range also makes an inline disable
        // comment anywhere inside the frontmatter suppress the warning.
        let rule = create_rule_with_required_keys(vec!["date"]);
        let content = "---\ntitle: Test\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 1);
        assert_eq!(result[0].end_line, 3, "span must reach the closing fence line");
        assert_eq!(result[0].end_column, 4);
    }

    #[test]
    fn test_required_keys_suppressed_by_inline_disable_in_frontmatter() {
        // A `# <!-- rumdl-disable MD072 -->` comment inside the frontmatter
        // suppresses sort warnings; missing-key warnings must honor it too.
        // Goes through the production `lint` path, where inline-config
        // filtering happens.
        let rule = create_rule_with_required_keys(vec!["date"]);
        let content = "---\n# <!-- rumdl-disable MD072 -->\ntitle: Test\n---\n\n# Heading\n";
        let warnings = crate::lint(
            content,
            &[Box::new(rule) as Box<dyn Rule>],
            false,
            crate::config::MarkdownFlavor::Standard,
            None,
            None,
        )
        .unwrap();

        assert!(
            warnings.is_empty(),
            "inline disable inside the frontmatter must suppress missing-key warnings, got: {warnings:?}"
        );
    }

    #[test]
    fn test_required_keys_reported_through_lint_without_disable() {
        // Counterpart to the suppression test: the same content without the
        // disable comment must report through the production `lint` path.
        let rule = create_rule_with_required_keys(vec!["date"]);
        let content = "---\ntitle: Test\n---\n\n# Heading\n";
        let warnings = crate::lint(
            content,
            &[Box::new(rule) as Box<dyn Rule>],
            false,
            crate::config::MarkdownFlavor::Standard,
            None,
            None,
        )
        .unwrap();

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("missing required key 'date'"));
    }

    #[test]
    fn test_required_keys_quoted_toml_key_satisfies() {
        // TOML basic and literal quoted keys are matched by their content.
        let rule = create_rule_with_required_keys(vec!["title", "date"]);
        let content = "+++\n\"date\" = \"2024-01-01\"\n'title' = \"Test\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "quoted TOML keys must satisfy required-keys, got: {result:?}"
        );
    }

    #[test]
    fn test_toml_quoted_keys_sort_by_content() {
        // A quoted TOML key must sort by its unquoted content, not by the
        // leading quote char ('"' is ASCII 34 and would sort before any
        // unquoted key). Mirrors the YAML behavior.
        let rule = create_enabled_rule();
        let content = "+++\n\"zebra\" = 1\napple = 2\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "quoted TOML key out of order must be flagged");
        assert!(result[0].message.contains("'apple' should come before 'zebra'"));
    }

    #[test]
    fn test_toml_quoted_key_warning_span_covers_quotes() {
        // "apple" is out of order. Its quotes are stripped for sorting, but
        // the diagnostic span must still cover the raw key as written.
        let rule = create_enabled_rule();
        let content = "+++\nbanana = 1\n\"apple\" = 2\n+++\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        let w = &result[0];
        assert_eq!(w.line, 3);
        assert_eq!(w.column, 1);
        // Raw key `"apple"` is 7 chars, so end_column is 8 (not 6 for `apple`).
        assert_eq!(w.end_column, 8, "diagnostic span must cover the quoted key");
    }

    #[test]
    fn test_required_keys_json_multiple_keys_on_one_line() {
        // The line-based extractor captures only the first key per line (it
        // exists for the order check); presence must see every key, so it is
        // checked against a real JSON parse.
        let rule = create_rule_with_required_keys(vec!["title", "date"]);
        let content = "{\n\"title\": \"Test\", \"date\": \"2024-01-01\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "all keys on one JSON line must satisfy required-keys, got: {result:?}"
        );
    }

    #[test]
    fn test_required_keys_json_multiple_keys_on_one_line_missing_still_reported() {
        // Same-line keys must not mask a genuinely missing key.
        let rule = create_rule_with_required_keys(vec!["title", "date", "author"]);
        let content = "{\n\"title\": \"Test\", \"date\": \"2024-01-01\"\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing required key 'author'"));
    }

    #[test]
    fn test_required_keys_json_invalid_falls_back_to_line_based_keys() {
        // Unparseable JSON falls back to the line-based extraction so a key
        // that is visibly present is not reported missing.
        let rule = create_rule_with_required_keys(vec!["title"]);
        let content = "{\n\"title\": unquoted-invalid\n}\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "invalid JSON must fall back to line-based key extraction, got: {result:?}"
        );
    }

    #[test]
    fn test_required_keys_toml_table_header_satisfies() {
        // A TOML table header defines a top-level key: required `taxonomies`
        // is satisfied by a `[taxonomies]` section. The sort check ignores
        // tables, but presence must see them.
        let rule = create_rule_with_required_keys(vec!["title", "taxonomies"]);
        let content = "+++\ntitle = \"Test\"\n\n[taxonomies]\ntags = [\"a\"]\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "a TOML table header must satisfy required-keys, got: {result:?}"
        );
    }

    #[test]
    fn test_required_keys_toml_array_of_tables_satisfies() {
        // `[[authors]]` defines the top-level key `authors`.
        let rule = create_rule_with_required_keys(vec!["authors"]);
        let content = "+++\ntitle = \"Test\"\n\n[[authors]]\nname = \"John\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "a TOML array-of-tables header must satisfy required-keys, got: {result:?}"
        );
    }

    #[test]
    fn test_required_keys_toml_dotted_table_header_satisfies_root() {
        // `[params.seo]` defines the top-level key `params`.
        let rule = create_rule_with_required_keys(vec!["params"]);
        let content = "+++\ntitle = \"Test\"\n\n[params.seo]\nnoindex = true\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "a dotted TOML table header must satisfy its root key, got: {result:?}"
        );
    }

    #[test]
    fn test_required_keys_toml_missing_despite_other_tables() {
        // Table headers must not mask a genuinely missing key.
        let rule = create_rule_with_required_keys(vec!["date"]);
        let content = "+++\ntitle = \"Test\"\n\n[taxonomies]\ntags = [\"a\"]\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing required key 'date'"));
    }

    #[test]
    fn test_required_keys_toml_dotted_assignment_satisfies_root() {
        // `params.seo = true` defines the top-level key `params`.
        let rule = create_rule_with_required_keys(vec!["params"]);
        let content = "+++\nparams.seo = true\ntitle = \"Test\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "a dotted TOML assignment must satisfy its root key, got: {result:?}"
        );
    }

    #[test]
    fn test_required_keys_toml_quoted_dotted_key_is_atomic() {
        // `"a.b" = 1` defines the literal top-level key `a.b`, not `a`.
        let rule = create_rule_with_required_keys(vec!["a.b"]);
        let content = "+++\n\"a.b\" = 1\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "quoted dotted key must match literally, got: {result:?}"
        );

        let rule = create_rule_with_required_keys(vec!["a"]);
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "quoted dotted key must NOT satisfy its first segment");
        assert!(result[0].message.contains("missing required key 'a'"));
    }

    #[test]
    fn test_required_keys_toml_table_header_with_inline_comment() {
        // A valid TOML header can carry an inline comment.
        let rule = create_rule_with_required_keys(vec!["taxonomies"]);
        let content = "+++\ntitle = \"Test\"\n\n[taxonomies] # used by Hugo\ntags = [\"a\"]\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "a table header with an inline comment must satisfy required-keys, got: {result:?}"
        );
    }

    #[test]
    fn test_required_keys_toml_assignment_inside_table_does_not_satisfy() {
        // An assignment under a table header is nested, not top-level.
        let rule = create_rule_with_required_keys(vec!["date"]);
        let content = "+++\ntitle = \"Test\"\n\n[params]\ndate = \"2024-01-01\"\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing required key 'date'"));
    }

    #[test]
    fn test_required_keys_yaml_quoted_key_with_colon_satisfies() {
        // A quoted YAML key may contain ':' (e.g. OpenGraph names); the
        // key/value separator is the colon outside the quotes.
        let rule = create_rule_with_required_keys(vec!["og:title"]);
        let content = "---\n\"og:title\": My post\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "a quoted YAML key containing a colon must satisfy required-keys, got: {result:?}"
        );
    }

    #[test]
    fn test_yaml_quoted_key_with_colon_sorts_by_full_content() {
        // The sort check must also see `og:title`, not a truncated `"og`.
        let rule = create_enabled_rule();
        let content = "---\n\"og:title\": My post\nalpha: 1\n---\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.contains("'alpha' should come before 'og:title'"),
            "sorting must use the full quoted key, got: {}",
            result[0].message
        );
    }

    #[test]
    fn test_required_keys_toml_quoted_key_with_equals_satisfies() {
        // A quoted TOML key may contain '='; the assignment separator is the
        // '=' outside the quotes.
        let rule = create_rule_with_required_keys(vec!["a=b"]);
        let content = "+++\n\"a=b\" = 1\n+++\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "a quoted TOML key containing '=' must satisfy required-keys, got: {result:?}"
        );
    }
}
