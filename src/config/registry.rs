use std::sync::LazyLock;

use crate::rule::Rule;

use super::flavor::normalize_key;

/// Lazily-initialized default `RuleRegistry` built from rules with default config.
///
/// Rule config schemas (valid keys, types, aliases) are intrinsic to each rule type
/// and do not change based on runtime configuration. This static registry avoids
/// repeatedly constructing 67+ rule instances just to extract their schemas.
static DEFAULT_REGISTRY: LazyLock<RuleRegistry> = LazyLock::new(|| {
    let default_config = super::types::Config::default();
    let rules = crate::rules::all_rules(&default_config);
    RuleRegistry::from_rules(&rules)
});

/// Returns a reference to the lazily-initialized default `RuleRegistry`.
///
/// Use this instead of `all_rules(&Config::default())` + `RuleRegistry::from_rules()`
/// when you only need rule metadata (names, config schemas, aliases) rather than
/// configured rule instances for linting.
pub fn default_registry() -> &'static RuleRegistry {
    &DEFAULT_REGISTRY
}

/// Registry of all known rules and their config schemas
pub struct RuleRegistry {
    /// Map of rule name (e.g. "MD013") to set of valid config keys and their TOML value types
    pub rule_schemas: std::collections::BTreeMap<String, toml::map::Map<String, toml::Value>>,
    /// Map of rule name to config key aliases
    pub rule_aliases: std::collections::BTreeMap<String, std::collections::HashMap<String, String>>,
}

impl RuleRegistry {
    /// Build a registry from a list of rules
    pub fn from_rules(rules: &[Box<dyn Rule>]) -> Self {
        let mut rule_schemas = std::collections::BTreeMap::new();
        let mut rule_aliases = std::collections::BTreeMap::new();

        for rule in rules {
            let norm_name = if let Some((name, toml::Value::Table(mut table))) = rule.default_config_section() {
                let norm_name = normalize_key(&name); // Normalize the name from default_config_section
                // Overwrite polymorphic keys with the sentinel so the validator skips
                // type checking for fields whose deserializer accepts multiple TOML
                // types. The clean default is preserved for `rumdl config --defaults`
                // because that path calls `default_config_section()` directly.
                for key in rule.polymorphic_config_keys() {
                    table.insert(
                        (*key).to_string(),
                        crate::rule_config_serde::polymorphic_sentinel_value(),
                    );
                }
                rule_schemas.insert(norm_name.clone(), table);
                norm_name
            } else {
                let norm_name = normalize_key(rule.name()); // Normalize the name from rule.name()
                rule_schemas.insert(norm_name.clone(), toml::map::Map::new());
                norm_name
            };

            // Store aliases if the rule provides them
            if let Some(aliases) = rule.config_aliases() {
                rule_aliases.insert(norm_name, aliases);
            }
        }

        RuleRegistry {
            rule_schemas,
            rule_aliases,
        }
    }

    /// Get all known rule names
    pub fn rule_names(&self) -> std::collections::BTreeSet<String> {
        self.rule_schemas.keys().cloned().collect()
    }

    /// Get the valid configuration keys for a rule, including both original and normalized variants
    pub fn config_keys_for(&self, rule: &str) -> Option<std::collections::BTreeSet<String>> {
        self.rule_schemas.get(rule).map(|schema| {
            let mut all_keys = std::collections::BTreeSet::new();

            // Always allow 'severity' and 'enabled' for any rule
            all_keys.insert("severity".to_string());
            all_keys.insert("enabled".to_string());

            // Add original keys from schema
            for key in schema.keys() {
                all_keys.insert(key.clone());
            }

            // Add normalized variants for markdownlint compatibility
            for key in schema.keys() {
                // Add kebab-case variant
                all_keys.insert(key.replace('_', "-"));
                // Add snake_case variant
                all_keys.insert(key.replace('-', "_"));
                // Add normalized variant
                all_keys.insert(normalize_key(key));
            }

            // Add any aliases defined by the rule
            if let Some(aliases) = self.rule_aliases.get(rule) {
                for alias_key in aliases.keys() {
                    all_keys.insert(alias_key.clone());
                    // Also add normalized variants of the alias
                    all_keys.insert(alias_key.replace('_', "-"));
                    all_keys.insert(alias_key.replace('-', "_"));
                    all_keys.insert(normalize_key(alias_key));
                }
            }

            all_keys
        })
    }

    /// Get the expected value type for a rule's configuration key, trying variants.
    /// Returns `None` for sentinel values (nullable Option fields, polymorphic fields
    /// that accept multiple TOML types), which signals the caller to skip type checking
    /// for that key while still recognizing the key as valid.
    pub fn expected_value_for(&self, rule: &str, key: &str) -> Option<&toml::Value> {
        let schema = self.rule_schemas.get(rule)?;

        // Check if this key is an alias
        if let Some(aliases) = self.rule_aliases.get(rule)
            && let Some(canonical_key) = aliases.get(key)
            && let Some(value) = schema.get(canonical_key)
        {
            return filter_type_check_sentinels(value);
        }

        // Try the original key
        if let Some(value) = schema.get(key) {
            return filter_type_check_sentinels(value);
        }

        // Try key variants
        let key_variants = [
            key.replace('-', "_"), // Convert kebab-case to snake_case
            key.replace('_', "-"), // Convert snake_case to kebab-case
            normalize_key(key),    // Normalized key (lowercase, kebab-case)
        ];

        for variant in &key_variants {
            if let Some(value) = schema.get(variant) {
                return filter_type_check_sentinels(value);
            }
        }

        None
    }

    /// Resolve any rule name (canonical or alias) to its canonical form
    /// Returns None if the rule name is not recognized
    ///
    /// Resolution order:
    /// 1. Direct canonical name match
    /// 2. Static aliases (built-in markdownlint aliases)
    pub fn resolve_rule_name(&self, name: &str) -> Option<String> {
        // Try normalized canonical name first
        let normalized = normalize_key(name);
        if self.rule_schemas.contains_key(&normalized) {
            return Some(normalized);
        }

        // Try static alias resolution (O(1) perfect hash lookup)
        resolve_rule_name_alias(name).map(std::string::ToString::to_string)
    }
}

/// Returns `None` if the value is a sentinel that signals "skip type check"
/// (nullable Option fields, polymorphic fields that accept multiple types).
/// Otherwise returns `Some(value)` so the validator can compare types.
fn filter_type_check_sentinels(value: &toml::Value) -> Option<&toml::Value> {
    if crate::rule_config_serde::is_nullable_sentinel(value) || crate::rule_config_serde::is_polymorphic_sentinel(value)
    {
        None
    } else {
        Some(value)
    }
}

/// Compile-time perfect hash map for O(1) rule alias lookups
/// Uses phf for zero-cost abstraction - compiles to direct jumps
pub static RULE_ALIAS_MAP: phf::Map<&'static str, &'static str> = phf::phf_map! {
    // Canonical names (identity mapping for consistency)
    "MD001" => "MD001",
    "MD003" => "MD003",
    "MD004" => "MD004",
    "MD005" => "MD005",
    "MD007" => "MD007",
    "MD009" => "MD009",
    "MD010" => "MD010",
    "MD011" => "MD011",
    "MD012" => "MD012",
    "MD013" => "MD013",
    "MD014" => "MD014",
    "MD018" => "MD018",
    "MD019" => "MD019",
    "MD020" => "MD020",
    "MD021" => "MD021",
    "MD022" => "MD022",
    "MD023" => "MD023",
    "MD024" => "MD024",
    "MD025" => "MD025",
    "MD026" => "MD026",
    "MD027" => "MD027",
    "MD028" => "MD028",
    "MD029" => "MD029",
    "MD030" => "MD030",
    "MD031" => "MD031",
    "MD032" => "MD032",
    "MD033" => "MD033",
    "MD034" => "MD034",
    "MD035" => "MD035",
    "MD036" => "MD036",
    "MD037" => "MD037",
    "MD038" => "MD038",
    "MD039" => "MD039",
    "MD040" => "MD040",
    "MD041" => "MD041",
    "MD042" => "MD042",
    "MD043" => "MD043",
    "MD044" => "MD044",
    "MD045" => "MD045",
    "MD046" => "MD046",
    "MD047" => "MD047",
    "MD048" => "MD048",
    "MD049" => "MD049",
    "MD050" => "MD050",
    "MD051" => "MD051",
    "MD052" => "MD052",
    "MD053" => "MD053",
    "MD054" => "MD054",
    "MD055" => "MD055",
    "MD056" => "MD056",
    "MD057" => "MD057",
    "MD058" => "MD058",
    "MD059" => "MD059",
    "MD060" => "MD060",
    "MD061" => "MD061",
    "MD062" => "MD062",
    "MD063" => "MD063",
    "MD064" => "MD064",
    "MD065" => "MD065",
    "MD066" => "MD066",
    "MD067" => "MD067",
    "MD068" => "MD068",
    "MD069" => "MD069",
    "MD070" => "MD070",
    "MD071" => "MD071",
    "MD072" => "MD072",
    "MD073" => "MD073",
    "MD074" => "MD074",
    "MD075" => "MD075",
    "MD076" => "MD076",
    "MD077" => "MD077",

    // Aliases (hyphen format)
    "HEADING-INCREMENT" => "MD001",
    "HEADING-STYLE" => "MD003",
    "UL-STYLE" => "MD004",
    "LIST-INDENT" => "MD005",
    "UL-INDENT" => "MD007",
    "NO-TRAILING-SPACES" => "MD009",
    "NO-HARD-TABS" => "MD010",
    "NO-REVERSED-LINKS" => "MD011",
    "NO-MULTIPLE-BLANKS" => "MD012",
    "LINE-LENGTH" => "MD013",
    "COMMANDS-SHOW-OUTPUT" => "MD014",
    "NO-MISSING-SPACE-ATX" => "MD018",
    "NO-MULTIPLE-SPACE-ATX" => "MD019",
    "NO-MISSING-SPACE-CLOSED-ATX" => "MD020",
    "NO-MULTIPLE-SPACE-CLOSED-ATX" => "MD021",
    "BLANKS-AROUND-HEADINGS" => "MD022",
    "HEADING-START-LEFT" => "MD023",
    "NO-DUPLICATE-HEADING" => "MD024",
    "SINGLE-TITLE" => "MD025",
    "SINGLE-H1" => "MD025",
    "NO-TRAILING-PUNCTUATION" => "MD026",
    "NO-MULTIPLE-SPACE-BLOCKQUOTE" => "MD027",
    "NO-BLANKS-BLOCKQUOTE" => "MD028",
    "OL-PREFIX" => "MD029",
    "LIST-MARKER-SPACE" => "MD030",
    "BLANKS-AROUND-FENCES" => "MD031",
    "BLANKS-AROUND-LISTS" => "MD032",
    "NO-INLINE-HTML" => "MD033",
    "NO-BARE-URLS" => "MD034",
    "HR-STYLE" => "MD035",
    "NO-EMPHASIS-AS-HEADING" => "MD036",
    "NO-SPACE-IN-EMPHASIS" => "MD037",
    "NO-SPACE-IN-CODE" => "MD038",
    "NO-SPACE-IN-LINKS" => "MD039",
    "FENCED-CODE-LANGUAGE" => "MD040",
    "FIRST-LINE-HEADING" => "MD041",
    "FIRST-LINE-H1" => "MD041",
    "NO-EMPTY-LINKS" => "MD042",
    "REQUIRED-HEADINGS" => "MD043",
    "PROPER-NAMES" => "MD044",
    "NO-ALT-TEXT" => "MD045",
    "CODE-BLOCK-STYLE" => "MD046",
    "SINGLE-TRAILING-NEWLINE" => "MD047",
    "CODE-FENCE-STYLE" => "MD048",
    "EMPHASIS-STYLE" => "MD049",
    "STRONG-STYLE" => "MD050",
    "LINK-FRAGMENTS" => "MD051",
    "REFERENCE-LINKS-IMAGES" => "MD052",
    "LINK-IMAGE-REFERENCE-DEFINITIONS" => "MD053",
    "LINK-IMAGE-STYLE" => "MD054",
    "TABLE-PIPE-STYLE" => "MD055",
    "TABLE-COLUMN-COUNT" => "MD056",
    "EXISTING-RELATIVE-LINKS" => "MD057",
    "BLANKS-AROUND-TABLES" => "MD058",
    "DESCRIPTIVE-LINK-TEXT" => "MD059",
    "TABLE-CELL-ALIGNMENT" => "MD060",
    "TABLE-FORMAT" => "MD060",
    "FORBIDDEN-TERMS" => "MD061",
    "LINK-DESTINATION-WHITESPACE" => "MD062",
    "HEADING-CAPITALIZATION" => "MD063",
    "NO-MULTIPLE-CONSECUTIVE-SPACES" => "MD064",
    "BLANKS-AROUND-HORIZONTAL-RULES" => "MD065",
    "FOOTNOTE-VALIDATION" => "MD066",
    "FOOTNOTE-DEFINITION-ORDER" => "MD067",
    "EMPTY-FOOTNOTE-DEFINITION" => "MD068",
    "NO-DUPLICATE-LIST-MARKERS" => "MD069",
    "NESTED-CODE-FENCE" => "MD070",
    "BLANK-LINE-AFTER-FRONTMATTER" => "MD071",
    "FRONTMATTER-KEY-SORT" => "MD072",
    "TOC-VALIDATION" => "MD073",
    "MKDOCS-NAV" => "MD074",
    "ORPHANED-TABLE-ROWS" => "MD075",
    "LIST-ITEM-SPACING" => "MD076",
    "LIST-CONTINUATION-INDENT" => "MD077",
};

/// Resolve a rule name alias to its canonical form with O(1) perfect hash lookup
/// Converts rule aliases (like "ul-style", "line-length") to canonical IDs (like "MD004", "MD013")
/// Returns None if the rule name is not recognized
pub fn resolve_rule_name_alias(key: &str) -> Option<&'static str> {
    // Normalize: uppercase and replace underscores with hyphens
    let normalized_key = key.to_ascii_uppercase().replace('_', "-");

    // O(1) perfect hash lookup
    RULE_ALIAS_MAP.get(normalized_key.as_str()).copied()
}

/// Resolves a rule name to its canonical ID, supporting both rule IDs and aliases.
/// Returns the canonical ID (e.g., "MD001") for any valid input:
/// - "MD001" → "MD001" (canonical)
/// - "heading-increment" → "MD001" (alias)
/// - "HEADING_INCREMENT" → "MD001" (case-insensitive, underscore variant)
///
/// For unknown names, falls back to normalization (uppercase for MDxxx pattern, otherwise kebab-case).
pub fn resolve_rule_name(name: &str) -> String {
    resolve_rule_name_alias(name).map_or_else(|| normalize_key(name), std::string::ToString::to_string)
}

/// Resolves a comma-separated list of rule names to canonical IDs.
/// Handles CLI input like "MD001,line-length,heading-increment".
/// Empty entries and whitespace are filtered out.
pub fn resolve_rule_names(input: &str) -> std::collections::HashSet<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(resolve_rule_name)
        .collect()
}

/// Checks if a rule name (or alias) is valid.
/// Returns true if the name resolves to a known rule.
/// Handles the special "all" value and all aliases.
pub fn is_valid_rule_name(name: &str) -> bool {
    // Check for special "all" value (case-insensitive)
    if name.eq_ignore_ascii_case("all") {
        return true;
    }
    resolve_rule_name_alias(name).is_some()
}

/// Canonicalizes a rule-name list in place: every entry is rewritten to its canonical
/// rule ID via [`resolve_rule_name`], duplicates are removed (keeping first occurrence),
/// and the special `"all"` keyword is preserved as-is (case-insensitive).
///
/// This enforces the runtime invariant that rule lists in `Config` (`enable`, `disable`,
/// `extend_enable`, `extend_disable`, `fixable`, `unfixable`, and per-file ignore values)
/// always contain canonical rule IDs. Consumers can therefore compare against
/// `rule.name()` with simple string equality without needing alias resolution at every
/// call site.
///
/// The operation is idempotent: running it twice produces the same result as once.
pub fn canonicalize_rule_list_in_place(list: &mut Vec<String>) {
    if list.is_empty() {
        return;
    }
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::with_capacity(list.len());
    let mut out: Vec<String> = Vec::with_capacity(list.len());
    for entry in list.drain(..) {
        let canonical = if entry.eq_ignore_ascii_case("all") {
            "all".to_string()
        } else {
            resolve_rule_name(&entry)
        };
        if seen.insert(canonical.clone()) {
            out.push(canonical);
        }
    }
    *list = out;
}

#[cfg(test)]
mod canonicalize_tests {
    use super::canonicalize_rule_list_in_place;

    #[test]
    fn rewrites_aliases_to_canonical_ids() {
        let mut list = vec!["no-inline-html".to_string(), "line-length".to_string()];
        canonicalize_rule_list_in_place(&mut list);
        assert_eq!(list, vec!["MD033".to_string(), "MD013".to_string()]);
    }

    #[test]
    fn dedupes_alias_and_canonical_preserving_order() {
        let mut list = vec!["MD033".to_string(), "no-inline-html".to_string(), "MD013".to_string()];
        canonicalize_rule_list_in_place(&mut list);
        assert_eq!(list, vec!["MD033".to_string(), "MD013".to_string()]);
    }

    #[test]
    fn preserves_all_keyword_normalized() {
        let mut list = vec!["ALL".to_string(), "MD013".to_string()];
        canonicalize_rule_list_in_place(&mut list);
        assert_eq!(list, vec!["all".to_string(), "MD013".to_string()]);
    }

    #[test]
    fn is_idempotent() {
        let mut list = vec!["no-inline-html".to_string(), "MD013".to_string()];
        canonicalize_rule_list_in_place(&mut list);
        let once = list.clone();
        canonicalize_rule_list_in_place(&mut list);
        assert_eq!(list, once);
    }

    #[test]
    fn handles_empty_and_unknown_inputs() {
        let mut empty: Vec<String> = Vec::new();
        canonicalize_rule_list_in_place(&mut empty);
        assert!(empty.is_empty());

        let mut unknown = vec!["custom-rule".to_string(), "Custom-Rule".to_string()];
        canonicalize_rule_list_in_place(&mut unknown);
        // Both normalize to the same kebab-case form, so they dedupe.
        assert_eq!(unknown, vec!["custom-rule".to_string()]);
    }
}
