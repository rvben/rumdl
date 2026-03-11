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

/// Type alias for the export_for_file_index return type:
/// (file_disabled_rules, persistent_transitions, line_disabled_rules)
pub type FileIndexExport = (
    HashSet<String>,
    Vec<(usize, HashSet<String>, HashSet<String>)>,
    HashMap<usize, HashSet<String>>,
);

/// A state transition recording which rules are disabled/enabled starting at a given line.
/// Transitions are stored in ascending line order. The state at any line is determined by
/// the most recent transition at or before that line.
#[derive(Debug, Clone)]
struct StateTransition {
    /// The 1-indexed line number where this state takes effect
    line: usize,
    /// The set of disabled rules at this point ("*" means all rules disabled)
    disabled: HashSet<String>,
    /// The set of explicitly enabled rules (only meaningful when disabled contains "*")
    enabled: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct InlineConfig {
    /// State transitions for persistent disable/enable directives, sorted by line number.
    /// Only stores entries where the state actually changes, not for every line.
    transitions: Vec<StateTransition>,
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
            transitions: Vec::new(),
            line_disabled_rules: HashMap::new(),
            file_disabled_rules: HashSet::new(),
            file_enabled_rules: HashSet::new(),
            file_rule_config: HashMap::new(),
        }
    }

    /// Find the state transition that applies to the given line number.
    /// Uses binary search to find the last transition at or before the given line.
    fn find_transition(&self, line_number: usize) -> Option<&StateTransition> {
        if self.transitions.is_empty() {
            return None;
        }
        // Binary search for the rightmost transition with line <= line_number
        match self.transitions.binary_search_by_key(&line_number, |t| t.line) {
            Ok(idx) => Some(&self.transitions[idx]),
            Err(idx) => {
                if idx > 0 {
                    Some(&self.transitions[idx - 1])
                } else {
                    None
                }
            }
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
        let mut currently_disabled: HashSet<String> = HashSet::new();
        let mut currently_enabled: HashSet<String> = HashSet::new();
        let mut capture_stack: Vec<(HashSet<String>, HashSet<String>)> = Vec::new();

        // Track the previously recorded transition state to detect changes
        let mut prev_disabled: HashSet<String> = HashSet::new();
        let mut prev_enabled: HashSet<String> = HashSet::new();

        // Record initial state (line 1: nothing disabled)
        config.transitions.push(StateTransition {
            line: 1,
            disabled: HashSet::new(),
            enabled: HashSet::new(),
        });

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1; // 1-indexed

            // Record a transition only if state changed since last recorded transition.
            // State for this line is the state BEFORE processing comments on this line.
            if currently_disabled != prev_disabled || currently_enabled != prev_enabled {
                config.transitions.push(StateTransition {
                    line: line_num,
                    disabled: currently_disabled.clone(),
                    enabled: currently_enabled.clone(),
                });
                prev_disabled.clone_from(&currently_disabled);
                prev_enabled.clone_from(&currently_enabled);
            }

            // Skip processing if this line is inside a code block
            let line_start = line_positions[idx];
            let line_end = line_start + line.len();
            let in_code_block = code_blocks
                .iter()
                .any(|&(block_start, block_end)| line_start >= block_start && line_end <= block_end);

            if in_code_block {
                continue;
            }

            // Parse all directives on this line once via the unified parser.
            // Directives come back in left-to-right order with correct disambiguation.
            let directives = parse_inline_directives(line);

            // Also check for prettier-ignore (not part of the rumdl/markdownlint format)
            let has_prettier_ignore = line.contains("<!-- prettier-ignore -->");

            // Pass 1: file-wide directives (affect the entire file, not state-tracked)
            for directive in &directives {
                match directive.kind {
                    DirectiveKind::DisableFile => {
                        if directive.rules.is_empty() {
                            config.file_disabled_rules.clear();
                            config.file_disabled_rules.insert("*".to_string());
                        } else if config.file_disabled_rules.contains("*") {
                            for rule in &directive.rules {
                                config.file_enabled_rules.remove(&normalize_rule_name(rule));
                            }
                        } else {
                            for rule in &directive.rules {
                                config.file_disabled_rules.insert(normalize_rule_name(rule));
                            }
                        }
                    }
                    DirectiveKind::EnableFile => {
                        if directive.rules.is_empty() {
                            config.file_disabled_rules.clear();
                            config.file_enabled_rules.clear();
                        } else if config.file_disabled_rules.contains("*") {
                            for rule in &directive.rules {
                                config.file_enabled_rules.insert(normalize_rule_name(rule));
                            }
                        } else {
                            for rule in &directive.rules {
                                config.file_disabled_rules.remove(&normalize_rule_name(rule));
                            }
                        }
                    }
                    DirectiveKind::ConfigureFile => {
                        if let Some(json_config) = parse_configure_file_comment(line) {
                            if let Some(obj) = json_config.as_object() {
                                for (rule_name, rule_config) in obj {
                                    config.file_rule_config.insert(rule_name.clone(), rule_config.clone());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Pass 2: line-specific and state-changing directives (in document order)
            for directive in &directives {
                match directive.kind {
                    DirectiveKind::DisableNextLine => {
                        let next_line = line_num + 1;
                        let line_rules = config.line_disabled_rules.entry(next_line).or_default();
                        if directive.rules.is_empty() {
                            line_rules.insert("*".to_string());
                        } else {
                            for rule in &directive.rules {
                                line_rules.insert(normalize_rule_name(rule));
                            }
                        }
                    }
                    DirectiveKind::DisableLine => {
                        let line_rules = config.line_disabled_rules.entry(line_num).or_default();
                        if directive.rules.is_empty() {
                            line_rules.insert("*".to_string());
                        } else {
                            for rule in &directive.rules {
                                line_rules.insert(normalize_rule_name(rule));
                            }
                        }
                    }
                    DirectiveKind::Disable => {
                        if directive.rules.is_empty() {
                            currently_disabled.clear();
                            currently_disabled.insert("*".to_string());
                            currently_enabled.clear();
                        } else if currently_disabled.contains("*") {
                            for rule in &directive.rules {
                                currently_enabled.remove(&normalize_rule_name(rule));
                            }
                        } else {
                            for rule in &directive.rules {
                                currently_disabled.insert(normalize_rule_name(rule));
                            }
                        }
                    }
                    DirectiveKind::Enable => {
                        if directive.rules.is_empty() {
                            currently_disabled.clear();
                            currently_enabled.clear();
                        } else if currently_disabled.contains("*") {
                            for rule in &directive.rules {
                                currently_enabled.insert(normalize_rule_name(rule));
                            }
                        } else {
                            for rule in &directive.rules {
                                currently_disabled.remove(&normalize_rule_name(rule));
                            }
                        }
                    }
                    DirectiveKind::Capture => {
                        capture_stack.push((currently_disabled.clone(), currently_enabled.clone()));
                    }
                    DirectiveKind::Restore => {
                        if let Some((disabled, enabled)) = capture_stack.pop() {
                            currently_disabled = disabled;
                            currently_enabled = enabled;
                        }
                    }
                    // File-wide directives already handled in pass 1
                    DirectiveKind::DisableFile | DirectiveKind::EnableFile | DirectiveKind::ConfigureFile => {}
                }
            }

            // prettier-ignore: disables all rules for next line
            if has_prettier_ignore {
                let next_line = line_num + 1;
                let line_rules = config.line_disabled_rules.entry(next_line).or_default();
                line_rules.insert("*".to_string());
            }
        }

        // Record final transition if state changed after the last line was processed
        if currently_disabled != prev_disabled || currently_enabled != prev_enabled {
            config.transitions.push(StateTransition {
                line: lines.len() + 1,
                disabled: currently_disabled,
                enabled: currently_enabled,
            });
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

        // Check persistent disables via state transitions (binary search)
        if let Some(transition) = self.find_transition(line_number) {
            if transition.disabled.contains("*") {
                return !transition.enabled.contains(rule_name);
            } else {
                return transition.disabled.contains(rule_name);
            }
        }

        false
    }

    /// Get all disabled rules at a specific line
    pub fn get_disabled_rules(&self, line_number: usize) -> HashSet<String> {
        let mut disabled = HashSet::new();

        // Add persistent disables via state transitions (binary search)
        if let Some(transition) = self.find_transition(line_number) {
            if transition.disabled.contains("*") {
                disabled.insert("*".to_string());
            } else {
                for rule in &transition.disabled {
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

    /// Export the disabled rules data for storage in FileIndex.
    ///
    /// Returns (file_disabled_rules, persistent_transitions, line_disabled_rules).
    pub fn export_for_file_index(&self) -> FileIndexExport {
        let file_disabled = self.file_disabled_rules.clone();

        let persistent_transitions: Vec<(usize, HashSet<String>, HashSet<String>)> = self
            .transitions
            .iter()
            .map(|t| (t.line, t.disabled.clone(), t.enabled.clone()))
            .collect();

        let line_disabled = self.line_disabled_rules.clone();

        (file_disabled, persistent_transitions, line_disabled)
    }
}

// ── Unified inline directive parser ──────────────────────────────────────────
//
// All inline config comments follow one pattern:
//   <!-- (rumdl|markdownlint)-KEYWORD [RULES...] -->
//
// Disambiguation (e.g., "disable" vs "disable-line" vs "disable-next-line")
// is handled ONCE here by matching the longest keyword first.

/// The type of an inline configuration directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectiveKind {
    Disable,
    DisableLine,
    DisableNextLine,
    DisableFile,
    Enable,
    EnableFile,
    Capture,
    Restore,
    ConfigureFile,
}

/// A parsed inline configuration directive.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineDirective<'a> {
    pub kind: DirectiveKind,
    pub rules: Vec<&'a str>,
}

/// Tool prefixes recognized in inline config comments.
const TOOL_PREFIXES: &[&str] = &["rumdl-", "markdownlint-"];

/// Directive keywords ordered so that more-specific prefixes come first.
/// "disable-next-line" before "disable-line" before "disable-file" before "disable";
/// "enable-file" before "enable". This ensures longest-match-first disambiguation.
const DIRECTIVE_KEYWORDS: &[(DirectiveKind, &str)] = &[
    (DirectiveKind::DisableNextLine, "disable-next-line"),
    (DirectiveKind::DisableLine, "disable-line"),
    (DirectiveKind::DisableFile, "disable-file"),
    (DirectiveKind::Disable, "disable"),
    (DirectiveKind::EnableFile, "enable-file"),
    (DirectiveKind::Enable, "enable"),
    (DirectiveKind::ConfigureFile, "configure-file"),
    (DirectiveKind::Capture, "capture"),
    (DirectiveKind::Restore, "restore"),
];

/// Try to parse a single directive from text immediately after `<!-- `.
/// Returns the directive and the number of bytes consumed (from `s` onward)
/// so the caller can advance past `-->`.
fn try_parse_directive(s: &str) -> Option<(InlineDirective<'_>, usize)> {
    for tool in TOOL_PREFIXES {
        if !s.starts_with(tool) {
            continue;
        }
        let after_tool = &s[tool.len()..];

        for &(kind, keyword) in DIRECTIVE_KEYWORDS {
            if !after_tool.starts_with(keyword) {
                continue;
            }
            let after_kw = &after_tool[keyword.len()..];

            // Word boundary: the keyword must be followed by whitespace, `-->`, or end-of-string.
            // This prevents "disablefoo" from matching "disable".
            if !after_kw.is_empty() && !after_kw.starts_with(char::is_whitespace) && !after_kw.starts_with("-->") {
                continue;
            }

            // Find closing -->
            let close_offset = after_kw.find("-->")?;

            let rules_str = after_kw[..close_offset].trim();
            let rules = if rules_str.is_empty() {
                Vec::new()
            } else {
                rules_str.split_whitespace().collect()
            };

            let consumed = tool.len() + keyword.len() + close_offset + 3; // 3 for "-->"
            return Some((InlineDirective { kind, rules }, consumed));
        }

        // Tool prefix matched but no keyword — not a directive we recognize.
        return None;
    }
    None
}

/// Parse all inline configuration directives from a line, in left-to-right order.
///
/// Each directive is a typed `InlineDirective` with its kind and rule list.
/// Disambiguation between overlapping prefixes (e.g., `disable` vs `disable-line`)
/// is handled by matching the longest keyword first — no ad-hoc guards needed.
pub fn parse_inline_directives(line: &str) -> Vec<InlineDirective<'_>> {
    let mut results = Vec::new();
    let mut pos = 0;

    while pos < line.len() {
        let remaining = &line[pos..];
        let Some(open_offset) = remaining.find("<!-- ") else {
            break;
        };
        let comment_start = pos + open_offset;
        let after_open = &line[comment_start + 5..]; // skip "<!-- "

        if let Some((directive, consumed)) = try_parse_directive(after_open) {
            results.push(directive);
            pos = comment_start + 5 + consumed;
        } else {
            pos = comment_start + 5;
        }
    }

    results
}

// ── Backward-compatible wrapper functions ────────────────────────────────────
//
// These delegate to parse_inline_directives and filter by DirectiveKind.
// External callers (e.g., MD040) use these; internal code uses the unified parser.

fn find_directive_rules<'a>(line: &'a str, kind: DirectiveKind) -> Option<Vec<&'a str>> {
    parse_inline_directives(line)
        .into_iter()
        .find(|d| d.kind == kind)
        .map(|d| d.rules)
}

/// Parse a disable comment and return the list of rules (empty vec means all rules)
pub fn parse_disable_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::Disable)
}

/// Parse an enable comment and return the list of rules (empty vec means all rules)
pub fn parse_enable_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::Enable)
}

/// Parse a disable-line comment
pub fn parse_disable_line_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::DisableLine)
}

/// Parse a disable-next-line comment
pub fn parse_disable_next_line_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::DisableNextLine)
}

/// Parse a disable-file comment and return the list of rules (empty vec means all rules)
pub fn parse_disable_file_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::DisableFile)
}

/// Parse an enable-file comment and return the list of rules (empty vec means all rules)
pub fn parse_enable_file_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::EnableFile)
}

/// Check if line contains a capture comment
pub fn is_capture_comment(line: &str) -> bool {
    parse_inline_directives(line)
        .iter()
        .any(|d| d.kind == DirectiveKind::Capture)
}

/// Check if line contains a restore comment
pub fn is_restore_comment(line: &str) -> bool {
    parse_inline_directives(line)
        .iter()
        .any(|d| d.kind == DirectiveKind::Restore)
}

/// Parse a configure-file comment and return the JSON configuration.
///
/// Uses the unified parser for directive detection/disambiguation, then
/// extracts the raw JSON payload directly from the line (since JSON
/// cannot be reliably reconstructed from whitespace-split tokens).
pub fn parse_configure_file_comment(line: &str) -> Option<JsonValue> {
    // First check if the unified parser even found a configure-file directive
    if !parse_inline_directives(line)
        .iter()
        .any(|d| d.kind == DirectiveKind::ConfigureFile)
    {
        return None;
    }

    // Extract the raw JSON content between the keyword and -->
    for tool in TOOL_PREFIXES {
        let prefix = format!("<!-- {tool}configure-file");
        if let Some(start) = line.find(&prefix) {
            let after_prefix = &line[start + prefix.len()..];
            if let Some(end) = after_prefix.find("-->") {
                let json_str = after_prefix[..end].trim();
                if !json_str.is_empty() {
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

        // Parse all directives on this line once
        let directives = parse_inline_directives(line);
        let mut rule_entries: Vec<(&str, &str)> = Vec::new();

        for directive in &directives {
            let comment_type = match directive.kind {
                DirectiveKind::Disable => "disable",
                DirectiveKind::Enable => "enable",
                DirectiveKind::DisableLine => "disable-line",
                DirectiveKind::DisableNextLine => "disable-next-line",
                DirectiveKind::DisableFile => "disable-file",
                DirectiveKind::EnableFile => "enable-file",
                DirectiveKind::ConfigureFile => {
                    // configure-file: rule names are JSON keys, handle separately
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
                    continue;
                }
                DirectiveKind::Capture | DirectiveKind::Restore => continue,
            };
            for rule in &directive.rules {
                rule_entries.push((rule, comment_type));
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

    // ── Unified parser tests ─────────────────────────────────────────────

    #[test]
    fn test_parse_inline_directives_all_kinds() {
        // Every directive kind is correctly identified
        let cases: &[(&str, DirectiveKind)] = &[
            ("<!-- rumdl-disable -->", DirectiveKind::Disable),
            ("<!-- rumdl-disable-line -->", DirectiveKind::DisableLine),
            ("<!-- rumdl-disable-next-line -->", DirectiveKind::DisableNextLine),
            ("<!-- rumdl-disable-file -->", DirectiveKind::DisableFile),
            ("<!-- rumdl-enable -->", DirectiveKind::Enable),
            ("<!-- rumdl-enable-file -->", DirectiveKind::EnableFile),
            ("<!-- rumdl-capture -->", DirectiveKind::Capture),
            ("<!-- rumdl-restore -->", DirectiveKind::Restore),
            ("<!-- rumdl-configure-file {} -->", DirectiveKind::ConfigureFile),
            // markdownlint variants
            ("<!-- markdownlint-disable -->", DirectiveKind::Disable),
            ("<!-- markdownlint-disable-line -->", DirectiveKind::DisableLine),
            (
                "<!-- markdownlint-disable-next-line -->",
                DirectiveKind::DisableNextLine,
            ),
            ("<!-- markdownlint-enable -->", DirectiveKind::Enable),
            ("<!-- markdownlint-capture -->", DirectiveKind::Capture),
            ("<!-- markdownlint-restore -->", DirectiveKind::Restore),
        ];
        for (input, expected_kind) in cases {
            let directives = parse_inline_directives(input);
            assert_eq!(
                directives.len(),
                1,
                "Expected 1 directive for {input:?}, got {directives:?}"
            );
            assert_eq!(directives[0].kind, *expected_kind, "Wrong kind for {input:?}");
        }
    }

    #[test]
    fn test_parse_inline_directives_disambiguation() {
        // The core property: "disable" must NOT match "disable-line" etc.
        let line = "<!-- rumdl-disable-line MD001 -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::DisableLine);

        let line = "<!-- rumdl-disable-next-line -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::DisableNextLine);

        let line = "<!-- rumdl-disable-file MD001 -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::DisableFile);

        let line = "<!-- rumdl-enable-file -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::EnableFile);
    }

    #[test]
    fn test_parse_inline_directives_no_space_before_close() {
        // <!-- rumdl-disable--> must parse as Disable (the bug that started this refactor)
        let directives = parse_inline_directives("<!-- rumdl-disable-->");
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::Disable);
        assert!(directives[0].rules.is_empty());

        let directives = parse_inline_directives("<!-- rumdl-enable-->");
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::Enable);
    }

    #[test]
    fn test_parse_inline_directives_multiple_on_one_line() {
        let line = "<!-- rumdl-disable MD001 --> text <!-- rumdl-enable MD001 -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 2);
        assert_eq!(directives[0].kind, DirectiveKind::Disable);
        assert_eq!(directives[0].rules, vec!["MD001"]);
        assert_eq!(directives[1].kind, DirectiveKind::Enable);
        assert_eq!(directives[1].rules, vec!["MD001"]);
    }

    #[test]
    fn test_parse_inline_directives_global_disable_then_specific_enable() {
        let line = "<!-- rumdl-disable --> <!-- rumdl-enable MD001 -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 2);
        assert_eq!(directives[0].kind, DirectiveKind::Disable);
        assert!(directives[0].rules.is_empty());
        assert_eq!(directives[1].kind, DirectiveKind::Enable);
        assert_eq!(directives[1].rules, vec!["MD001"]);
    }

    #[test]
    fn test_parse_inline_directives_word_boundary() {
        // "disablefoo" should NOT match "disable"
        assert!(parse_inline_directives("<!-- rumdl-disablefoo -->").is_empty());
        // "enablebar" should NOT match "enable"
        assert!(parse_inline_directives("<!-- rumdl-enablebar -->").is_empty());
        // "captures" should NOT match "capture"
        assert!(parse_inline_directives("<!-- rumdl-captures -->").is_empty());
    }

    #[test]
    fn test_parse_inline_directives_no_closing_tag() {
        // Missing --> means no directive
        assert!(parse_inline_directives("<!-- rumdl-disable MD001").is_empty());
        assert!(parse_inline_directives("<!-- rumdl-enable").is_empty());
    }

    #[test]
    fn test_parse_inline_directives_not_a_comment() {
        assert!(parse_inline_directives("rumdl-disable MD001 -->").is_empty());
        assert!(parse_inline_directives("Some regular text").is_empty());
        assert!(parse_inline_directives("").is_empty());
    }

    #[test]
    fn test_parse_inline_directives_case_sensitive() {
        assert!(parse_inline_directives("<!-- RUMDL-DISABLE -->").is_empty());
        assert!(parse_inline_directives("<!-- Markdownlint-Disable -->").is_empty());
    }

    #[test]
    fn test_parse_inline_directives_rules_extraction() {
        let directives = parse_inline_directives("<!-- rumdl-disable MD001 MD002 MD013 -->");
        assert_eq!(directives[0].rules, vec!["MD001", "MD002", "MD013"]);

        // Tabs between rules
        let directives = parse_inline_directives("<!-- rumdl-disable\tMD001\tMD002 -->");
        assert_eq!(directives[0].rules, vec!["MD001", "MD002"]);

        // Extra whitespace
        let directives = parse_inline_directives("<!-- rumdl-disable   MD001   -->");
        assert_eq!(directives[0].rules, vec!["MD001"]);
    }

    #[test]
    fn test_parse_inline_directives_embedded_in_text() {
        let line = "Some text <!-- rumdl-disable MD001 --> more text";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].rules, vec!["MD001"]);

        let line = "🚀 <!-- rumdl-disable MD001 --> 🎉";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].rules, vec!["MD001"]);
    }

    #[test]
    fn test_parse_inline_directives_mixed_tools_same_line() {
        let line = "<!-- rumdl-disable MD001 --> <!-- markdownlint-enable MD002 -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 2);
        assert_eq!(directives[0].kind, DirectiveKind::Disable);
        assert_eq!(directives[0].rules, vec!["MD001"]);
        assert_eq!(directives[1].kind, DirectiveKind::Enable);
        assert_eq!(directives[1].rules, vec!["MD002"]);
    }

    // ── Backward-compatible wrapper tests ────────────────────────────────

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

    // ── parse_disable_comment / parse_enable_comment edge cases ──────────

    #[test]
    fn test_parse_disable_does_not_match_disable_line() {
        // parse_disable_comment must NOT match disable-line or disable-next-line
        assert_eq!(parse_disable_comment("<!-- rumdl-disable-line MD001 -->"), None);
        assert_eq!(parse_disable_comment("<!-- markdownlint-disable-line MD001 -->"), None);
        assert_eq!(parse_disable_comment("<!-- rumdl-disable-next-line MD001 -->"), None);
        assert_eq!(parse_disable_comment("<!-- markdownlint-disable-next-line -->"), None);
        assert_eq!(parse_disable_comment("<!-- rumdl-disable-file MD001 -->"), None);
        assert_eq!(parse_disable_comment("<!-- markdownlint-disable-file -->"), None);
    }

    #[test]
    fn test_parse_enable_does_not_match_enable_file() {
        assert_eq!(parse_enable_comment("<!-- rumdl-enable-file MD001 -->"), None);
        assert_eq!(parse_enable_comment("<!-- markdownlint-enable-file -->"), None);
    }

    #[test]
    fn test_parse_disable_comment_edge_cases() {
        // No space before closing
        assert_eq!(parse_disable_comment("<!-- rumdl-disable-->"), Some(vec![]));

        // Tabs between rules
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable\tMD001\tMD002 -->"),
            Some(vec!["MD001", "MD002"])
        );

        // Comment not at start of line
        assert_eq!(
            parse_disable_comment("Some text <!-- rumdl-disable MD001 --> more text"),
            Some(vec!["MD001"])
        );

        // Malformed: no closing
        assert_eq!(parse_disable_comment("<!-- rumdl-disable MD001"), None);

        // Malformed: no opening
        assert_eq!(parse_disable_comment("rumdl-disable MD001 -->"), None);

        // Case sensitive: uppercase should not match
        assert_eq!(parse_disable_comment("<!-- RUMDL-DISABLE -->"), None);

        // Empty rule list with whitespace
        assert_eq!(parse_disable_comment("<!-- rumdl-disable   -->"), Some(vec![]));

        // Duplicate rules preserved (caller may deduplicate)
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD001 MD001 MD002 -->"),
            Some(vec!["MD001", "MD001", "MD002"])
        );

        // Unicode around the comment
        assert_eq!(
            parse_disable_comment("🚀 <!-- rumdl-disable MD001 --> 🎉"),
            Some(vec!["MD001"])
        );

        // 100 rules
        let many_rules = (1..=100).map(|i| format!("MD{i:03}")).collect::<Vec<_>>().join(" ");
        let comment = format!("<!-- rumdl-disable {many_rules} -->");
        let parsed = parse_disable_comment(&comment);
        assert!(parsed.is_some());
        assert_eq!(parsed.unwrap().len(), 100);

        // Special characters in rule names (forward compat)
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD001-test -->"),
            Some(vec!["MD001-test"])
        );
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable custom_rule -->"),
            Some(vec!["custom_rule"])
        );
    }

    #[test]
    fn test_parse_enable_comment_edge_cases() {
        assert_eq!(parse_enable_comment("<!-- rumdl-enable-->"), Some(vec![]));
        assert_eq!(parse_enable_comment("<!-- RUMDL-ENABLE -->"), None);
        assert_eq!(parse_enable_comment("<!-- rumdl-enable MD001"), None);
        assert_eq!(parse_enable_comment("<!-- rumdl-enable   -->"), Some(vec![]));
    }

    // ── InlineConfig: code blocks must be transparent ────────────────────

    #[test]
    fn test_disable_inside_fenced_code_block_ignored() {
        let content = "# Document\n```markdown\n<!-- rumdl-disable MD001 -->\nContent\n```\nAfter code block\n";
        let config = InlineConfig::from_content(content);
        // The disable comment is inside a code block — must have no effect
        assert!(!config.is_rule_disabled("MD001", 6));
    }

    #[test]
    fn test_disable_inside_tilde_fence_ignored() {
        let content = "# Document\n~~~\n<!-- rumdl-disable -->\nContent\n~~~\nAfter code block\n";
        let config = InlineConfig::from_content(content);
        assert!(!config.is_rule_disabled("MD001", 6));
    }

    #[test]
    fn test_disable_before_code_block_persists_after() {
        // Disable before code block should persist through and after it
        let content = "<!-- rumdl-disable MD001 -->\n```\ncode\n```\nStill disabled\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD001", 5));
    }

    #[test]
    fn test_enable_inside_code_block_ignored() {
        // Disable before, enable inside code block (should be ignored), still disabled after
        let content = "<!-- rumdl-disable MD001 -->\n```\n<!-- rumdl-enable MD001 -->\n```\nShould still be disabled\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD001", 5));
    }

    // ── InlineConfig: mixed comment styles ───────────────────────────────

    #[test]
    fn test_markdownlint_disable_rumdl_enable_interop() {
        let content = "<!-- markdownlint-disable MD001 -->\nDisabled\n<!-- rumdl-enable MD001 -->\nEnabled\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD001", 2));
        assert!(!config.is_rule_disabled("MD001", 4));
    }

    #[test]
    fn test_rumdl_disable_markdownlint_enable_interop() {
        let content = "<!-- rumdl-disable MD013 -->\nDisabled\n<!-- markdownlint-enable MD013 -->\nEnabled\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD013", 2));
        assert!(!config.is_rule_disabled("MD013", 4));
    }

    // ── InlineConfig: nested/overlapping disable/enable ──────────────────

    #[test]
    fn test_global_disable_then_specific_enable() {
        let content = "<!-- rumdl-disable -->\nAll off\n<!-- rumdl-enable MD001 -->\nMD001 on, rest off\n";
        let config = InlineConfig::from_content(content);
        assert!(!config.is_rule_disabled("MD001", 4));
        assert!(config.is_rule_disabled("MD002", 4));
        assert!(config.is_rule_disabled("MD013", 4));
    }

    #[test]
    fn test_specific_disable_then_global_enable() {
        let content = "<!-- rumdl-disable MD001 MD002 -->\nBoth off\n<!-- rumdl-enable -->\nAll on\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD001", 2));
        assert!(config.is_rule_disabled("MD002", 2));
        assert!(!config.is_rule_disabled("MD001", 4));
        assert!(!config.is_rule_disabled("MD002", 4));
    }

    #[test]
    fn test_multiple_rules_disable_enable_independently() {
        let content = "\
Line 1\n\
<!-- rumdl-disable MD001 MD002 -->\n\
Line 3\n\
<!-- rumdl-enable MD001 -->\n\
Line 5\n\
<!-- rumdl-disable -->\n\
Line 7\n\
<!-- rumdl-enable MD002 -->\n\
Line 9\n";
        let config = InlineConfig::from_content(content);

        // Line 1: nothing disabled
        assert!(!config.is_rule_disabled("MD001", 1));
        assert!(!config.is_rule_disabled("MD002", 1));

        // Line 3: both disabled
        assert!(config.is_rule_disabled("MD001", 3));
        assert!(config.is_rule_disabled("MD002", 3));

        // Line 5: MD001 enabled, MD002 still disabled
        assert!(!config.is_rule_disabled("MD001", 5));
        assert!(config.is_rule_disabled("MD002", 5));

        // Line 7: all disabled
        assert!(config.is_rule_disabled("MD001", 7));
        assert!(config.is_rule_disabled("MD002", 7));

        // Line 9: MD002 enabled, MD001 still disabled
        assert!(config.is_rule_disabled("MD001", 9));
        assert!(!config.is_rule_disabled("MD002", 9));
    }

    // ── InlineConfig: empty/minimal content ──────────────────────────────

    #[test]
    fn test_empty_content() {
        let config = InlineConfig::from_content("");
        assert!(!config.is_rule_disabled("MD001", 1));
    }

    #[test]
    fn test_single_disable_comment_only() {
        // Persistent disable takes effect from the NEXT line, not the current line.
        // For a single-line document, the disable on line 1 takes effect at line 2+.
        let config = InlineConfig::from_content("<!-- rumdl-disable -->");
        assert!(!config.is_rule_disabled("MD001", 1));
        assert!(config.is_rule_disabled("MD001", 2));
        assert!(config.is_rule_disabled("MD999", 2));

        // With content after the disable, rules are disabled from line 2 onward
        let config = InlineConfig::from_content("<!-- rumdl-disable -->\n# Heading\nSome text");
        assert!(!config.is_rule_disabled("MD001", 1));
        assert!(config.is_rule_disabled("MD001", 2));
        assert!(config.is_rule_disabled("MD001", 3));
    }

    #[test]
    fn test_no_inline_markers() {
        let config = InlineConfig::from_content("# Heading\n\nSome text\n\n- list item\n");
        assert!(!config.is_rule_disabled("MD001", 1));
        assert!(!config.is_rule_disabled("MD001", 5));
    }

    // ── InlineConfig: export_for_file_index correctness ──────────────────

    #[test]
    fn test_export_for_file_index_persistent_transitions() {
        let content = "Line 1\n<!-- rumdl-disable MD001 -->\nLine 3\n<!-- rumdl-enable MD001 -->\nLine 5\n";
        let config = InlineConfig::from_content(content);
        let (file_disabled, persistent, _line_disabled) = config.export_for_file_index();

        assert!(file_disabled.is_empty());
        // Should have transitions for the disable and enable
        assert!(
            persistent.len() >= 2,
            "Expected at least 2 transitions, got {}",
            persistent.len()
        );
    }

    #[test]
    fn test_export_for_file_index_disable_file() {
        let content = "<!-- rumdl-disable-file MD001 -->\n# Heading\n";
        let config = InlineConfig::from_content(content);
        let (file_disabled, _persistent, _line_disabled) = config.export_for_file_index();

        assert!(file_disabled.contains("MD001"));
    }

    #[test]
    fn test_export_for_file_index_disable_line() {
        let content = "Line 1\nLine 2 <!-- rumdl-disable-line MD001 -->\nLine 3\n";
        let config = InlineConfig::from_content(content);
        let (_file_disabled, _persistent, line_disabled) = config.export_for_file_index();

        assert!(line_disabled.contains_key(&2), "Line 2 should have disabled rules");
        assert!(line_disabled[&2].contains("MD001"));
        assert!(!line_disabled.contains_key(&3), "Line 3 should not be affected");
    }
}
