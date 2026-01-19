//! Inline configuration comment handling for markdownlint compatibility
//!
//! Supports:
//! - `<!-- markdownlint-disable -->` - Disable all rules from this point
//! - `<!-- markdownlint-enable -->` - Re-enable all rules from this point
//! - `<!-- markdownlint-disable MD001 MD002 -->` - Disable specific rules
//! - `<!-- markdownlint-enable MD001 MD002 -->` - Re-enable specific rules
//! - `<!-- markdownlint-disable-line MD001 -->` - Disable rules for current line
//! - `<!-- markdownlint-disable-next-line MD001 -->` - Disable rules for next line
//! - `<!-- markdownlint-capture -->` - Capture current configuration state
//! - `<!-- markdownlint-restore -->` - Restore captured configuration state
//! - `<!-- markdownlint-disable-file -->` - Disable all rules for entire file
//! - `<!-- markdownlint-enable-file -->` - Re-enable all rules for entire file
//! - `<!-- markdownlint-disable-file MD001 MD002 -->` - Disable specific rules for entire file
//! - `<!-- markdownlint-enable-file MD001 MD002 -->` - Re-enable specific rules for entire file
//! - `<!-- markdownlint-configure-file { "MD013": { "line_length": 120 } } -->` - Configure rules for entire file
//! - `<!-- prettier-ignore -->` - Disable all rules for next line (compatibility with prettier)
//!
//! Also supports rumdl-specific syntax with same semantics.

use crate::markdownlint_config::markdownlint_to_rumdl_rule_key;
use crate::utils::code_block_utils::CodeBlockUtils;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};

/// Normalize a rule name to its canonical form (e.g., "line-length" -> "MD013").
/// If the rule name is not recognized, returns it uppercase (for forward compatibility).
fn normalize_rule_name(rule: &str) -> String {
    markdownlint_to_rumdl_rule_key(rule)
        .map(|s| s.to_string())
        .unwrap_or_else(|| rule.to_uppercase())
}

fn has_inline_config_markers(content: &str) -> bool {
    if !content.contains("<!--") {
        return false;
    }
    content.contains("markdownlint") || content.contains("rumdl") || content.contains("prettier-ignore")
}

#[derive(Debug, Clone)]
pub struct InlineConfig {
    /// Rules that are disabled at each line (1-indexed line -> set of disabled rules)
    disabled_at_line: HashMap<usize, HashSet<String>>,
    /// Rules that are explicitly enabled when all rules are disabled (1-indexed line -> set of enabled rules)
    /// Only used when "*" is in disabled_at_line
    enabled_at_line: HashMap<usize, HashSet<String>>,
    /// Rules disabled for specific lines via disable-line (1-indexed)
    line_disabled_rules: HashMap<usize, HashSet<String>>,
    /// Rules disabled for the entire file
    file_disabled_rules: HashSet<String>,
    /// Rules explicitly enabled for the entire file (used when all rules are disabled)
    file_enabled_rules: HashSet<String>,
    /// Configuration overrides for specific rules from configure-file comments
    /// Maps rule name to configuration JSON value
    file_rule_config: HashMap<String, JsonValue>,
}

impl Default for InlineConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineConfig {
    pub fn new() -> Self {
        Self {
            disabled_at_line: HashMap::new(),
            enabled_at_line: HashMap::new(),
            line_disabled_rules: HashMap::new(),
            file_disabled_rules: HashSet::new(),
            file_enabled_rules: HashSet::new(),
            file_rule_config: HashMap::new(),
        }
    }

    /// Process all inline comments in the content and return the configuration state
    pub fn from_content(content: &str) -> Self {
        if !has_inline_config_markers(content) {
            return Self::new();
        }

        let code_blocks = CodeBlockUtils::detect_code_blocks(content);
        Self::from_content_with_code_blocks_internal(content, &code_blocks)
    }

    /// Process all inline comments in the content with precomputed code blocks.
    pub fn from_content_with_code_blocks(content: &str, code_blocks: &[(usize, usize)]) -> Self {
        if !has_inline_config_markers(content) {
            return Self::new();
        }

        Self::from_content_with_code_blocks_internal(content, code_blocks)
    }

    fn from_content_with_code_blocks_internal(content: &str, code_blocks: &[(usize, usize)]) -> Self {
        let mut config = Self::new();
        let lines: Vec<&str> = content.lines().collect();

        // Pre-compute line positions for checking if a line is in a code block
        let mut line_positions = Vec::with_capacity(lines.len());
        let mut pos = 0;
        for line in &lines {
            line_positions.push(pos);
            pos += line.len() + 1; // +1 for newline
        }

        // Track current state of disabled rules
        let mut currently_disabled = HashSet::new();
        let mut currently_enabled = HashSet::new(); // For when all rules are disabled
        let mut capture_stack: Vec<(HashSet<String>, HashSet<String>)> = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1; // 1-indexed

            // Store the current state for this line BEFORE processing comments
            // This way, comments on a line don't affect that same line
            config.disabled_at_line.insert(line_num, currently_disabled.clone());
            config.enabled_at_line.insert(line_num, currently_enabled.clone());

            // Skip processing if this line is inside a code block
            let line_start = line_positions[idx];
            let line_end = line_start + line.len();
            let in_code_block = code_blocks
                .iter()
                .any(|&(block_start, block_end)| line_start >= block_start && line_end <= block_end);

            if in_code_block {
                continue;
            }

            // Process file-wide comments first as they affect the entire file
            // Check for disable-file
            if let Some(rules) = parse_disable_file_comment(line) {
                if rules.is_empty() {
                    // Disable all rules for entire file
                    config.file_disabled_rules.clear();
                    config.file_disabled_rules.insert("*".to_string());
                } else {
                    // Disable specific rules for entire file
                    if config.file_disabled_rules.contains("*") {
                        // All rules are disabled, so remove from enabled list
                        for rule in rules {
                            config.file_enabled_rules.remove(&normalize_rule_name(rule));
                        }
                    } else {
                        // Normal case: add to disabled list
                        for rule in rules {
                            config.file_disabled_rules.insert(normalize_rule_name(rule));
                        }
                    }
                }
            }

            // Check for enable-file
            if let Some(rules) = parse_enable_file_comment(line) {
                if rules.is_empty() {
                    // Enable all rules for entire file
                    config.file_disabled_rules.clear();
                    config.file_enabled_rules.clear();
                } else {
                    // Enable specific rules for entire file
                    if config.file_disabled_rules.contains("*") {
                        // All rules are disabled, so add to enabled list
                        for rule in rules {
                            config.file_enabled_rules.insert(normalize_rule_name(rule));
                        }
                    } else {
                        // Normal case: remove from disabled list
                        for rule in rules {
                            config.file_disabled_rules.remove(&normalize_rule_name(rule));
                        }
                    }
                }
            }

            // Check for configure-file
            if let Some(json_config) = parse_configure_file_comment(line) {
                // Process the JSON configuration
                if let Some(obj) = json_config.as_object() {
                    for (rule_name, rule_config) in obj {
                        config.file_rule_config.insert(rule_name.clone(), rule_config.clone());
                    }
                }
            }

            // Process comments - handle multiple comment types on same line
            // Process line-specific comments first (they don't affect state)

            // Check for disable-next-line
            if let Some(rules) = parse_disable_next_line_comment(line) {
                let next_line = line_num + 1;
                let line_rules = config.line_disabled_rules.entry(next_line).or_default();
                if rules.is_empty() {
                    // Disable all rules for next line
                    line_rules.insert("*".to_string());
                } else {
                    for rule in rules {
                        line_rules.insert(normalize_rule_name(rule));
                    }
                }
            }

            // Check for prettier-ignore (disables all rules for next line)
            if line.contains("<!-- prettier-ignore -->") {
                let next_line = line_num + 1;
                let line_rules = config.line_disabled_rules.entry(next_line).or_default();
                line_rules.insert("*".to_string());
            }

            // Check for disable-line
            if let Some(rules) = parse_disable_line_comment(line) {
                let line_rules = config.line_disabled_rules.entry(line_num).or_default();
                if rules.is_empty() {
                    // Disable all rules for current line
                    line_rules.insert("*".to_string());
                } else {
                    for rule in rules {
                        line_rules.insert(normalize_rule_name(rule));
                    }
                }
            }

            // Process state-changing comments in the order they appear
            // This handles multiple comments on the same line correctly
            let mut processed_capture = false;
            let mut processed_restore = false;

            // Find all comments on this line and process them in order
            let mut comment_positions = Vec::new();

            if let Some(pos) = line.find("<!-- markdownlint-disable")
                && !line[pos..].contains("<!-- markdownlint-disable-line")
                && !line[pos..].contains("<!-- markdownlint-disable-next-line")
            {
                comment_positions.push((pos, "disable"));
            }
            if let Some(pos) = line.find("<!-- rumdl-disable")
                && !line[pos..].contains("<!-- rumdl-disable-line")
                && !line[pos..].contains("<!-- rumdl-disable-next-line")
            {
                comment_positions.push((pos, "disable"));
            }

            if let Some(pos) = line.find("<!-- markdownlint-enable") {
                comment_positions.push((pos, "enable"));
            }
            if let Some(pos) = line.find("<!-- rumdl-enable") {
                comment_positions.push((pos, "enable"));
            }

            if let Some(pos) = line.find("<!-- markdownlint-capture") {
                comment_positions.push((pos, "capture"));
            }
            if let Some(pos) = line.find("<!-- rumdl-capture") {
                comment_positions.push((pos, "capture"));
            }

            if let Some(pos) = line.find("<!-- markdownlint-restore") {
                comment_positions.push((pos, "restore"));
            }
            if let Some(pos) = line.find("<!-- rumdl-restore") {
                comment_positions.push((pos, "restore"));
            }

            // Sort by position to process in order
            comment_positions.sort_by_key(|&(pos, _)| pos);

            // Process each comment in order
            for (_, comment_type) in comment_positions {
                match comment_type {
                    "disable" => {
                        if let Some(rules) = parse_disable_comment(line) {
                            if rules.is_empty() {
                                // Disable all rules
                                currently_disabled.clear();
                                currently_disabled.insert("*".to_string());
                                currently_enabled.clear(); // Reset enabled list
                            } else {
                                // Disable specific rules
                                if currently_disabled.contains("*") {
                                    // All rules are disabled, so remove from enabled list
                                    for rule in rules {
                                        currently_enabled.remove(&normalize_rule_name(rule));
                                    }
                                } else {
                                    // Normal case: add to disabled list
                                    for rule in rules {
                                        currently_disabled.insert(normalize_rule_name(rule));
                                    }
                                }
                            }
                        }
                    }
                    "enable" => {
                        if let Some(rules) = parse_enable_comment(line) {
                            if rules.is_empty() {
                                // Enable all rules
                                currently_disabled.clear();
                                currently_enabled.clear();
                            } else {
                                // Enable specific rules
                                if currently_disabled.contains("*") {
                                    // All rules are disabled, so add to enabled list
                                    for rule in rules {
                                        currently_enabled.insert(normalize_rule_name(rule));
                                    }
                                } else {
                                    // Normal case: remove from disabled list
                                    for rule in rules {
                                        currently_disabled.remove(&normalize_rule_name(rule));
                                    }
                                }
                            }
                        }
                    }
                    "capture" => {
                        if !processed_capture && is_capture_comment(line) {
                            capture_stack.push((currently_disabled.clone(), currently_enabled.clone()));
                            processed_capture = true;
                        }
                    }
                    "restore" => {
                        if !processed_restore && is_restore_comment(line) {
                            if let Some((disabled, enabled)) = capture_stack.pop() {
                                currently_disabled = disabled;
                                currently_enabled = enabled;
                            }
                            processed_restore = true;
                        }
                    }
                    _ => {}
                }
            }
        }

        config
    }

    /// Check if a rule is disabled at a specific line
    pub fn is_rule_disabled(&self, rule_name: &str, line_number: usize) -> bool {
        // Check file-wide disables first (highest priority)
        if self.file_disabled_rules.contains("*") {
            // All rules are disabled for the file, check if this rule is explicitly enabled
            return !self.file_enabled_rules.contains(rule_name);
        } else if self.file_disabled_rules.contains(rule_name) {
            return true;
        }

        // Check line-specific disables (disable-line, disable-next-line)
        if let Some(line_rules) = self.line_disabled_rules.get(&line_number)
            && (line_rules.contains("*") || line_rules.contains(rule_name))
        {
            return true;
        }

        // Check persistent disables at this line
        if let Some(disabled_set) = self.disabled_at_line.get(&line_number) {
            if disabled_set.contains("*") {
                // All rules are disabled, check if this rule is explicitly enabled
                if let Some(enabled_set) = self.enabled_at_line.get(&line_number) {
                    return !enabled_set.contains(rule_name);
                }
                return true; // All disabled and not explicitly enabled
            } else {
                return disabled_set.contains(rule_name);
            }
        }

        false
    }

    /// Get all disabled rules at a specific line
    pub fn get_disabled_rules(&self, line_number: usize) -> HashSet<String> {
        let mut disabled = HashSet::new();

        // Add persistent disables
        if let Some(disabled_set) = self.disabled_at_line.get(&line_number) {
            if disabled_set.contains("*") {
                // All rules are disabled except those explicitly enabled
                disabled.insert("*".to_string());
                // We could subtract enabled rules here, but that would require knowing all rules
                // For now, we'll just return "*" to indicate all rules are disabled
            } else {
                for rule in disabled_set {
                    disabled.insert(rule.clone());
                }
            }
        }

        // Add line-specific disables
        if let Some(line_rules) = self.line_disabled_rules.get(&line_number) {
            for rule in line_rules {
                disabled.insert(rule.clone());
            }
        }

        disabled
    }

    /// Get configuration overrides for a specific rule from configure-file comments
    pub fn get_rule_config(&self, rule_name: &str) -> Option<&JsonValue> {
        self.file_rule_config.get(rule_name)
    }

    /// Get all configuration overrides from configure-file comments
    pub fn get_all_rule_configs(&self) -> &HashMap<String, JsonValue> {
        &self.file_rule_config
    }

    /// Export the disabled rules data for storage in FileIndex
    ///
    /// Returns (file_disabled_rules, line_disabled_rules) for use in cross-file checks.
    /// Merges both persistent disables and line-specific disables into a single map.
    pub fn export_for_file_index(&self) -> (HashSet<String>, HashMap<usize, HashSet<String>>) {
        let file_disabled = self.file_disabled_rules.clone();

        // Merge disabled_at_line and line_disabled_rules into a single map
        let mut line_disabled: HashMap<usize, HashSet<String>> = HashMap::new();

        for (line, rules) in &self.disabled_at_line {
            line_disabled.entry(*line).or_default().extend(rules.clone());
        }
        for (line, rules) in &self.line_disabled_rules {
            line_disabled.entry(*line).or_default().extend(rules.clone());
        }

        (file_disabled, line_disabled)
    }
}

/// Parse a disable comment and return the list of rules (empty vec means all rules)
pub fn parse_disable_comment(line: &str) -> Option<Vec<&str>> {
    // Check for both rumdl-disable and markdownlint-disable
    for prefix in &["<!-- rumdl-disable", "<!-- markdownlint-disable"] {
        if let Some(start) = line.find(prefix) {
            let after_prefix = &line[start + prefix.len()..];

            // Skip more specific variants (disable-line, disable-next-line, disable-file)
            if after_prefix.starts_with('-') {
                continue;
            }

            // Global disable: <!-- markdownlint-disable -->
            if after_prefix.trim_start().starts_with("-->") {
                return Some(Vec::new()); // Empty vec means all rules
            }

            // Rule-specific disable: <!-- markdownlint-disable MD001 MD002 -->
            if let Some(end) = after_prefix.find("-->") {
                let rules_str = after_prefix[..end].trim();
                if !rules_str.is_empty() {
                    let rules: Vec<&str> = rules_str.split_whitespace().collect();
                    return Some(rules);
                }
            }
        }
    }

    None
}

/// Parse an enable comment and return the list of rules (empty vec means all rules)
pub fn parse_enable_comment(line: &str) -> Option<Vec<&str>> {
    // Check for both rumdl-enable and markdownlint-enable
    for prefix in &["<!-- rumdl-enable", "<!-- markdownlint-enable"] {
        if let Some(start) = line.find(prefix) {
            let after_prefix = &line[start + prefix.len()..];

            // Skip more specific variants (enable-file)
            if after_prefix.starts_with('-') {
                continue;
            }

            // Global enable: <!-- markdownlint-enable -->
            if after_prefix.trim_start().starts_with("-->") {
                return Some(Vec::new()); // Empty vec means all rules
            }

            // Rule-specific enable: <!-- markdownlint-enable MD001 MD002 -->
            if let Some(end) = after_prefix.find("-->") {
                let rules_str = after_prefix[..end].trim();
                if !rules_str.is_empty() {
                    let rules: Vec<&str> = rules_str.split_whitespace().collect();
                    return Some(rules);
                }
            }
        }
    }

    None
}

/// Parse a disable-line comment
pub fn parse_disable_line_comment(line: &str) -> Option<Vec<&str>> {
    // Check for both rumdl and markdownlint variants
    for prefix in &["<!-- rumdl-disable-line", "<!-- markdownlint-disable-line"] {
        if let Some(start) = line.find(prefix) {
            let after_prefix = &line[start + prefix.len()..];

            // Global disable-line: <!-- markdownlint-disable-line -->
            if after_prefix.trim_start().starts_with("-->") {
                return Some(Vec::new()); // Empty vec means all rules
            }

            // Rule-specific disable-line: <!-- markdownlint-disable-line MD001 MD002 -->
            if let Some(end) = after_prefix.find("-->") {
                let rules_str = after_prefix[..end].trim();
                if !rules_str.is_empty() {
                    let rules: Vec<&str> = rules_str.split_whitespace().collect();
                    return Some(rules);
                }
            }
        }
    }

    None
}

/// Parse a disable-next-line comment
pub fn parse_disable_next_line_comment(line: &str) -> Option<Vec<&str>> {
    // Check for both rumdl and markdownlint variants
    for prefix in &["<!-- rumdl-disable-next-line", "<!-- markdownlint-disable-next-line"] {
        if let Some(start) = line.find(prefix) {
            let after_prefix = &line[start + prefix.len()..];

            // Global disable-next-line: <!-- markdownlint-disable-next-line -->
            if after_prefix.trim_start().starts_with("-->") {
                return Some(Vec::new()); // Empty vec means all rules
            }

            // Rule-specific disable-next-line: <!-- markdownlint-disable-next-line MD001 MD002 -->
            if let Some(end) = after_prefix.find("-->") {
                let rules_str = after_prefix[..end].trim();
                if !rules_str.is_empty() {
                    let rules: Vec<&str> = rules_str.split_whitespace().collect();
                    return Some(rules);
                }
            }
        }
    }

    None
}

/// Check if line contains a capture comment
pub fn is_capture_comment(line: &str) -> bool {
    line.contains("<!-- markdownlint-capture -->") || line.contains("<!-- rumdl-capture -->")
}

/// Check if line contains a restore comment
pub fn is_restore_comment(line: &str) -> bool {
    line.contains("<!-- markdownlint-restore -->") || line.contains("<!-- rumdl-restore -->")
}

/// Parse a disable-file comment and return the list of rules (empty vec means all rules)
pub fn parse_disable_file_comment(line: &str) -> Option<Vec<&str>> {
    // Check for both rumdl and markdownlint variants
    for prefix in &["<!-- rumdl-disable-file", "<!-- markdownlint-disable-file"] {
        if let Some(start) = line.find(prefix) {
            let after_prefix = &line[start + prefix.len()..];

            // Global disable-file: <!-- markdownlint-disable-file -->
            if after_prefix.trim_start().starts_with("-->") {
                return Some(Vec::new()); // Empty vec means all rules
            }

            // Rule-specific disable-file: <!-- markdownlint-disable-file MD001 MD002 -->
            if let Some(end) = after_prefix.find("-->") {
                let rules_str = after_prefix[..end].trim();
                if !rules_str.is_empty() {
                    let rules: Vec<&str> = rules_str.split_whitespace().collect();
                    return Some(rules);
                }
            }
        }
    }

    None
}

/// Parse an enable-file comment and return the list of rules (empty vec means all rules)
pub fn parse_enable_file_comment(line: &str) -> Option<Vec<&str>> {
    // Check for both rumdl and markdownlint variants
    for prefix in &["<!-- rumdl-enable-file", "<!-- markdownlint-enable-file"] {
        if let Some(start) = line.find(prefix) {
            let after_prefix = &line[start + prefix.len()..];

            // Global enable-file: <!-- markdownlint-enable-file -->
            if after_prefix.trim_start().starts_with("-->") {
                return Some(Vec::new()); // Empty vec means all rules
            }

            // Rule-specific enable-file: <!-- markdownlint-enable-file MD001 MD002 -->
            if let Some(end) = after_prefix.find("-->") {
                let rules_str = after_prefix[..end].trim();
                if !rules_str.is_empty() {
                    let rules: Vec<&str> = rules_str.split_whitespace().collect();
                    return Some(rules);
                }
            }
        }
    }

    None
}

/// Parse a configure-file comment and return the JSON configuration
pub fn parse_configure_file_comment(line: &str) -> Option<JsonValue> {
    // Check for both rumdl and markdownlint variants
    for prefix in &["<!-- rumdl-configure-file", "<!-- markdownlint-configure-file"] {
        if let Some(start) = line.find(prefix) {
            let after_prefix = &line[start + prefix.len()..];

            // Find the JSON content between the prefix and -->
            if let Some(end) = after_prefix.find("-->") {
                let json_str = after_prefix[..end].trim();
                if !json_str.is_empty() {
                    // Try to parse as JSON
                    if let Ok(value) = serde_json::from_str(json_str) {
                        return Some(value);
                    }
                }
            }
        }
    }

    None
}

/// Warning about unknown rules in inline config comments
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineConfigWarning {
    /// The line number where the warning occurred (1-indexed)
    pub line_number: usize,
    /// The rule name that was not recognized
    pub rule_name: String,
    /// The type of inline config comment
    pub comment_type: String,
    /// Optional suggestion for similar rule names
    pub suggestion: Option<String>,
}

impl InlineConfigWarning {
    /// Format the warning message
    pub fn format_message(&self) -> String {
        if let Some(ref suggestion) = self.suggestion {
            format!(
                "Unknown rule in inline {} comment: {} (did you mean: {}?)",
                self.comment_type, self.rule_name, suggestion
            )
        } else {
            format!(
                "Unknown rule in inline {} comment: {}",
                self.comment_type, self.rule_name
            )
        }
    }

    /// Print the warning to stderr with file context
    pub fn print_warning(&self, file_path: &str) {
        eprintln!(
            "\x1b[33m[inline config warning]\x1b[0m {}:{}: {}",
            file_path,
            self.line_number,
            self.format_message()
        );
    }
}

/// Validate all inline config comments in content and return warnings for unknown rules.
///
/// This function extracts rule names from all types of inline config comments
/// (disable, enable, disable-line, disable-next-line, disable-file, enable-file)
/// and validates them against the known rule alias map.
pub fn validate_inline_config_rules(content: &str) -> Vec<InlineConfigWarning> {
    use crate::config::{RULE_ALIAS_MAP, is_valid_rule_name, suggest_similar_key};

    let mut warnings = Vec::new();
    let all_rule_names: Vec<String> = RULE_ALIAS_MAP.keys().map(|s| s.to_string()).collect();

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;

        // Collect all rule names from various comment types
        let mut rule_entries: Vec<(&str, &str)> = Vec::new();

        // Check each comment type and collect rules with their type names
        if let Some(rules) = parse_disable_comment(line) {
            for rule in rules {
                rule_entries.push((rule, "disable"));
            }
        }
        if let Some(rules) = parse_enable_comment(line) {
            for rule in rules {
                rule_entries.push((rule, "enable"));
            }
        }
        if let Some(rules) = parse_disable_line_comment(line) {
            for rule in rules {
                rule_entries.push((rule, "disable-line"));
            }
        }
        if let Some(rules) = parse_disable_next_line_comment(line) {
            for rule in rules {
                rule_entries.push((rule, "disable-next-line"));
            }
        }
        if let Some(rules) = parse_disable_file_comment(line) {
            for rule in rules {
                rule_entries.push((rule, "disable-file"));
            }
        }
        if let Some(rules) = parse_enable_file_comment(line) {
            for rule in rules {
                rule_entries.push((rule, "enable-file"));
            }
        }

        // Check configure-file comments - rule names are JSON keys
        if let Some(json_config) = parse_configure_file_comment(line)
            && let Some(obj) = json_config.as_object()
        {
            for rule_name in obj.keys() {
                if !is_valid_rule_name(rule_name) {
                    let suggestion = suggest_similar_key(rule_name, &all_rule_names)
                        .map(|s| if s.starts_with("MD") { s } else { s.to_lowercase() });
                    warnings.push(InlineConfigWarning {
                        line_number: line_num,
                        rule_name: rule_name.to_string(),
                        comment_type: "configure-file".to_string(),
                        suggestion,
                    });
                }
            }
        }

        // Validate each rule name
        for (rule_name, comment_type) in rule_entries {
            if !is_valid_rule_name(rule_name) {
                let suggestion = suggest_similar_key(rule_name, &all_rule_names)
                    .map(|s| if s.starts_with("MD") { s } else { s.to_lowercase() });
                warnings.push(InlineConfigWarning {
                    line_number: line_num,
                    rule_name: rule_name.to_string(),
                    comment_type: comment_type.to_string(),
                    suggestion,
                });
            }
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_disable_comment() {
        // Global disable
        assert_eq!(parse_disable_comment("<!-- markdownlint-disable -->"), Some(vec![]));
        assert_eq!(parse_disable_comment("<!-- rumdl-disable -->"), Some(vec![]));

        // Specific rules
        assert_eq!(
            parse_disable_comment("<!-- markdownlint-disable MD001 MD002 -->"),
            Some(vec!["MD001", "MD002"])
        );

        // No comment
        assert_eq!(parse_disable_comment("Some regular text"), None);
    }

    #[test]
    fn test_parse_disable_line_comment() {
        // Global disable-line
        assert_eq!(
            parse_disable_line_comment("<!-- markdownlint-disable-line -->"),
            Some(vec![])
        );

        // Specific rules
        assert_eq!(
            parse_disable_line_comment("<!-- markdownlint-disable-line MD013 -->"),
            Some(vec!["MD013"])
        );

        // No comment
        assert_eq!(parse_disable_line_comment("Some regular text"), None);
    }

    #[test]
    fn test_inline_config_from_content() {
        let content = r#"# Test Document

<!-- markdownlint-disable MD013 -->
This is a very long line that would normally trigger MD013 but it's disabled

<!-- markdownlint-enable MD013 -->
This line will be checked again

<!-- markdownlint-disable-next-line MD001 -->
# This heading will not be checked for MD001
## But this one will

Some text <!-- markdownlint-disable-line MD013 -->

<!-- markdownlint-capture -->
<!-- markdownlint-disable MD001 MD002 -->
# Heading with MD001 disabled
<!-- markdownlint-restore -->
# Heading with MD001 enabled again
"#;

        let config = InlineConfig::from_content(content);

        // Line 4 should have MD013 disabled (line after disable comment on line 3)
        assert!(config.is_rule_disabled("MD013", 4));

        // Line 7 should have MD013 enabled (line after enable comment on line 6)
        assert!(!config.is_rule_disabled("MD013", 7));

        // Line 10 should have MD001 disabled (from disable-next-line on line 9)
        assert!(config.is_rule_disabled("MD001", 10));

        // Line 11 should not have MD001 disabled
        assert!(!config.is_rule_disabled("MD001", 11));

        // Line 13 should have MD013 disabled (from disable-line)
        assert!(config.is_rule_disabled("MD013", 13));

        // After restore (line 18), MD001 should be enabled again on line 19
        assert!(!config.is_rule_disabled("MD001", 19));
    }

    #[test]
    fn test_capture_restore() {
        let content = r#"<!-- markdownlint-disable MD001 -->
<!-- markdownlint-capture -->
<!-- markdownlint-disable MD002 MD003 -->
<!-- markdownlint-restore -->
Some content after restore
"#;

        let config = InlineConfig::from_content(content);

        // After restore (line 4), line 5 should only have MD001 disabled
        assert!(config.is_rule_disabled("MD001", 5));
        assert!(!config.is_rule_disabled("MD002", 5));
        assert!(!config.is_rule_disabled("MD003", 5));
    }

    #[test]
    fn test_validate_inline_config_rules_unknown_rule() {
        let content = "<!-- rumdl-disable abc -->\nSome content";
        let warnings = validate_inline_config_rules(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line_number, 1);
        assert_eq!(warnings[0].rule_name, "abc");
        assert_eq!(warnings[0].comment_type, "disable");
    }

    #[test]
    fn test_validate_inline_config_rules_valid_rule() {
        let content = "<!-- rumdl-disable MD001 -->\nSome content";
        let warnings = validate_inline_config_rules(content);
        assert!(
            warnings.is_empty(),
            "MD001 is a valid rule, should not produce warnings"
        );
    }

    #[test]
    fn test_validate_inline_config_rules_alias() {
        let content = "<!-- rumdl-disable heading-increment -->\nSome content";
        let warnings = validate_inline_config_rules(content);
        assert!(warnings.is_empty(), "heading-increment is a valid alias for MD001");
    }

    #[test]
    fn test_validate_inline_config_rules_multiple_unknown() {
        let content = r#"<!-- rumdl-disable abc xyz -->
<!-- rumdl-disable-line foo -->
<!-- markdownlint-disable-next-line bar -->
"#;
        let warnings = validate_inline_config_rules(content);
        assert_eq!(warnings.len(), 4);
        assert_eq!(warnings[0].rule_name, "abc");
        assert_eq!(warnings[1].rule_name, "xyz");
        assert_eq!(warnings[2].rule_name, "foo");
        assert_eq!(warnings[3].rule_name, "bar");
    }

    #[test]
    fn test_validate_inline_config_rules_suggestion() {
        // "MD00" should suggest "MD001" (or similar)
        let content = "<!-- rumdl-disable MD00 -->\n";
        let warnings = validate_inline_config_rules(content);
        assert_eq!(warnings.len(), 1);
        // Should have a suggestion since "MD00" is close to "MD001"
        assert!(warnings[0].suggestion.is_some());
    }

    #[test]
    fn test_validate_inline_config_rules_file_comments() {
        let content = "<!-- rumdl-disable-file nonexistent -->\n<!-- markdownlint-enable-file another_fake -->";
        let warnings = validate_inline_config_rules(content);
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].comment_type, "disable-file");
        assert_eq!(warnings[1].comment_type, "enable-file");
    }

    #[test]
    fn test_validate_inline_config_rules_global_disable() {
        // Global disable (no specific rules) should not produce warnings
        let content = "<!-- rumdl-disable -->\n<!-- markdownlint-enable -->";
        let warnings = validate_inline_config_rules(content);
        assert!(warnings.is_empty(), "Global disable/enable should not produce warnings");
    }

    #[test]
    fn test_validate_inline_config_rules_mixed_valid_invalid() {
        // Use MD001 and MD003 which are valid rules; abc and xyz are invalid
        let content = "<!-- rumdl-disable MD001 abc MD003 xyz -->";
        let warnings = validate_inline_config_rules(content);
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].rule_name, "abc");
        assert_eq!(warnings[1].rule_name, "xyz");
    }

    #[test]
    fn test_validate_inline_config_rules_configure_file() {
        // configure-file comments contain rule names as JSON keys
        let content =
            r#"<!-- rumdl-configure-file { "MD013": { "line_length": 120 }, "nonexistent": { "foo": true } } -->"#;
        let warnings = validate_inline_config_rules(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].rule_name, "nonexistent");
        assert_eq!(warnings[0].comment_type, "configure-file");
    }

    #[test]
    fn test_validate_inline_config_rules_markdownlint_variants() {
        // Test markdownlint-* variants (not just rumdl-*)
        let content = r#"<!-- markdownlint-disable unknown_rule -->
<!-- markdownlint-enable another_fake -->
<!-- markdownlint-disable-line bad_rule -->
<!-- markdownlint-disable-next-line fake_rule -->
<!-- markdownlint-disable-file missing_rule -->
<!-- markdownlint-enable-file nonexistent -->
"#;
        let warnings = validate_inline_config_rules(content);
        assert_eq!(warnings.len(), 6);
        assert_eq!(warnings[0].rule_name, "unknown_rule");
        assert_eq!(warnings[1].rule_name, "another_fake");
        assert_eq!(warnings[2].rule_name, "bad_rule");
        assert_eq!(warnings[3].rule_name, "fake_rule");
        assert_eq!(warnings[4].rule_name, "missing_rule");
        assert_eq!(warnings[5].rule_name, "nonexistent");
    }

    #[test]
    fn test_validate_inline_config_rules_markdownlint_configure_file() {
        let content = r#"<!-- markdownlint-configure-file { "fake_rule": {} } -->"#;
        let warnings = validate_inline_config_rules(content);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].rule_name, "fake_rule");
        assert_eq!(warnings[0].comment_type, "configure-file");
    }

    #[test]
    fn test_get_rule_config_from_configure_file() {
        let content = r#"<!-- markdownlint-configure-file {"MD013": {"line_length": 50}} -->

This is a test line."#;

        let inline_config = InlineConfig::from_content(content);
        let config_override = inline_config.get_rule_config("MD013");

        assert!(config_override.is_some(), "MD013 config should be found");
        let json = config_override.unwrap();
        assert!(json.is_object(), "Config should be an object");
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("line_length"), "Should have line_length key");
        assert_eq!(obj.get("line_length").unwrap().as_u64().unwrap(), 50);
    }

    #[test]
    fn test_get_rule_config_tables_false() {
        // Test that tables=false inline config is correctly parsed
        let content = r#"<!-- markdownlint-configure-file {"MD013": {"tables": false}} -->"#;

        let inline_config = InlineConfig::from_content(content);
        let config_override = inline_config.get_rule_config("MD013");

        assert!(config_override.is_some(), "MD013 config should be found");
        let json = config_override.unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("tables"), "Should have tables key");
        assert!(!obj.get("tables").unwrap().as_bool().unwrap());
    }
}
