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
            let norm_name = if let Some((name, toml::Value::Table(mut table))) = rule.config_schema() {
                let norm_name = normalize_key(&name); // Normalize the name from config_schema
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

    /// Resolve a key as the user wrote it to the schema key it names, trying the rule's
    /// aliases and the separator/case variants.
    ///
    /// Returns `None` when the rule does not accept the key at all. A key that resolves
    /// may still carry a sentinel value, so this answers "is this key known?" where
    /// [`RuleRegistry::expected_value_for`] answers "what type must it be?".
    pub fn canonical_config_key(&self, rule: &str, key: &str) -> Option<&str> {
        let schema = self.rule_schemas.get(rule)?;

        // Check if this key is an alias
        if let Some(aliases) = self.rule_aliases.get(rule)
            && let Some(canonical_key) = aliases.get(key)
            && let Some((schema_key, _)) = schema.get_key_value(canonical_key)
        {
            return Some(schema_key);
        }

        // Try the original key
        if let Some((schema_key, _)) = schema.get_key_value(key) {
            return Some(schema_key);
        }

        // Try key variants
        let key_variants = [
            key.replace('-', "_"), // Convert kebab-case to snake_case
            key.replace('_', "-"), // Convert snake_case to kebab-case
            normalize_key(key),    // Normalized key (lowercase, kebab-case)
        ];

        for variant in &key_variants {
            if let Some((schema_key, _)) = schema.get_key_value(variant) {
                return Some(schema_key);
            }
        }

        None
    }

    /// Get the expected value type for a rule's configuration key, trying variants.
    /// Returns `None` both for an unknown key and for sentinel values (nullable Option
    /// fields, polymorphic fields that accept multiple TOML types), which signals the
    /// caller to skip type checking while still recognizing the key as valid. Use
    /// [`RuleRegistry::canonical_config_key`] to tell those two cases apart.
    pub fn expected_value_for(&self, rule: &str, key: &str) -> Option<&toml::Value> {
        let schema = self.rule_schemas.get(rule)?;
        let canonical = self.canonical_config_key(rule, key)?;
        filter_type_check_sentinels(schema.get(canonical)?)
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

        // Try static alias resolution (built-in markdownlint aliases)
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

/// A read-only string map looked up by binary search.
///
/// The rule-name tables below are fixed at compile time and read on every config
/// load, so they want a lookup with no build step and no hashing: at these sizes
/// a binary search is a handful of comparisons over data the linker places in
/// `.rodata`. Entries may be a literal slice or a projection of the rule
/// catalog; keeping either source sorted is the caller's job, which tests pin.
pub struct StaticMap {
    source: StaticMapSource,
}

#[derive(Clone, Copy)]
enum StaticMapSource {
    Entries(&'static [(&'static str, &'static str)]),
    RulePrimaryAliases,
}

struct StaticMapEntries {
    source: StaticMapSource,
    index: usize,
}

impl Iterator for StaticMapEntries {
    type Item = (&'static str, &'static str);

    fn next(&mut self) -> Option<Self::Item> {
        let entry = match self.source {
            StaticMapSource::Entries(entries) => entries.get(self.index).copied(),
            StaticMapSource::RulePrimaryAliases => crate::rules::rule_identity(self.index),
        };
        self.index += usize::from(entry.is_some());
        entry
    }
}

impl StaticMap {
    const fn new(entries: &'static [(&'static str, &'static str)]) -> Self {
        Self {
            source: StaticMapSource::Entries(entries),
        }
    }

    const fn rule_primary_aliases() -> Self {
        Self {
            source: StaticMapSource::RulePrimaryAliases,
        }
    }

    /// The value for `key`, or `None` if the map has no such key.
    pub fn get(&self, key: &str) -> Option<&'static str> {
        match self.source {
            StaticMapSource::Entries(entries) => entries
                .binary_search_by_key(&key, |(entry_key, _)| entry_key)
                .ok()
                .map(|index| entries[index].1),
            StaticMapSource::RulePrimaryAliases => crate::rules::primary_alias(key),
        }
    }

    /// Every key, in sorted order.
    pub fn keys(&self) -> impl Iterator<Item = &'static str> {
        self.entries().map(|(key, _)| key)
    }

    /// Every key/value pair, in sorted order.
    pub fn entries(&self) -> impl Iterator<Item = (&'static str, &'static str)> {
        StaticMapEntries {
            source: self.source,
            index: 0,
        }
    }

    /// Whether the backing array is sorted by key, the invariant [`get`](Self::get) needs.
    ///
    /// Nothing at runtime can act on the answer, so this exists for the tests that
    /// hold the tables to the order they are written in.
    #[cfg(test)]
    fn is_sorted_by_key(&self) -> bool {
        self.entries()
            .map(|(key, _)| key)
            .try_fold(None, |previous, key| match previous {
                Some(previous) if previous >= key => Err(()),
                _ => Ok(Some(key)),
            })
            .is_ok()
    }
}

/// Every spelling rumdl accepts for a rule, mapped to its canonical ID.
///
/// Keys are the normalized form [`resolve_rule_name_alias`] produces: uppercase
/// with hyphens. Sorted, because [`StaticMap`] binary-searches it; the
/// `rule_alias_map_is_sorted` test holds new entries to that.
pub static RULE_ALIAS_MAP: StaticMap = StaticMap::new(&[
    ("BLANK-LINE-AFTER-FRONTMATTER", "MD071"),
    ("BLANKS-AROUND-FENCES", "MD031"),
    ("BLANKS-AROUND-HEADINGS", "MD022"),
    ("BLANKS-AROUND-HORIZONTAL-RULES", "MD065"),
    ("BLANKS-AROUND-LISTS", "MD032"),
    ("BLANKS-AROUND-TABLES", "MD058"),
    ("CHUNK-LABEL-SPACES", "MD079"),
    ("CODE-BLOCK-STYLE", "MD046"),
    ("CODE-FENCE-STYLE", "MD048"),
    ("COMMANDS-SHOW-OUTPUT", "MD014"),
    ("DESCRIPTIVE-LINK-TEXT", "MD059"),
    ("EMPHASIS-STYLE", "MD049"),
    ("EMPTY-FOOTNOTE-DEFINITION", "MD068"),
    ("EXISTING-RELATIVE-LINKS", "MD057"),
    ("FENCED-CODE-LANGUAGE", "MD040"),
    ("FIRST-LINE-H1", "MD041"),
    ("FIRST-LINE-HEADING", "MD041"),
    ("FOOTNOTE-DEFINITION-ORDER", "MD067"),
    ("FOOTNOTE-VALIDATION", "MD066"),
    ("FORBIDDEN-TERMS", "MD061"),
    ("FRONTMATTER-KEY-SORT", "MD072"),
    ("HEADING-ANCHOR-COLLISION", "MD080"),
    ("HEADING-CAPITALIZATION", "MD063"),
    ("HEADING-INCREMENT", "MD001"),
    ("HEADING-START-LEFT", "MD023"),
    ("HEADING-STYLE", "MD003"),
    ("HR-STYLE", "MD035"),
    ("INVISIBLE-CHARACTERS", "MD084"),
    ("LINE-LENGTH", "MD013"),
    ("LINK-DESTINATION-WHITESPACE", "MD062"),
    ("LINK-FRAGMENTS", "MD051"),
    ("LINK-IMAGE-REFERENCE-DEFINITIONS", "MD053"),
    ("LINK-IMAGE-STYLE", "MD054"),
    ("LIST-CONTINUATION-INDENT", "MD077"),
    ("LIST-INDENT", "MD005"),
    ("LIST-ITEM-SPACING", "MD076"),
    ("LIST-MARKER-SPACE", "MD030"),
    ("MD001", "MD001"),
    ("MD003", "MD003"),
    ("MD004", "MD004"),
    ("MD005", "MD005"),
    ("MD007", "MD007"),
    ("MD009", "MD009"),
    ("MD010", "MD010"),
    ("MD011", "MD011"),
    ("MD012", "MD012"),
    ("MD013", "MD013"),
    ("MD014", "MD014"),
    ("MD018", "MD018"),
    ("MD019", "MD019"),
    ("MD020", "MD020"),
    ("MD021", "MD021"),
    ("MD022", "MD022"),
    ("MD023", "MD023"),
    ("MD024", "MD024"),
    ("MD025", "MD025"),
    ("MD026", "MD026"),
    ("MD027", "MD027"),
    ("MD028", "MD028"),
    ("MD029", "MD029"),
    ("MD030", "MD030"),
    ("MD031", "MD031"),
    ("MD032", "MD032"),
    ("MD033", "MD033"),
    ("MD034", "MD034"),
    ("MD035", "MD035"),
    ("MD036", "MD036"),
    ("MD037", "MD037"),
    ("MD038", "MD038"),
    ("MD039", "MD039"),
    ("MD040", "MD040"),
    ("MD041", "MD041"),
    ("MD042", "MD042"),
    ("MD043", "MD043"),
    ("MD044", "MD044"),
    ("MD045", "MD045"),
    ("MD046", "MD046"),
    ("MD047", "MD047"),
    ("MD048", "MD048"),
    ("MD049", "MD049"),
    ("MD050", "MD050"),
    ("MD051", "MD051"),
    ("MD052", "MD052"),
    ("MD053", "MD053"),
    ("MD054", "MD054"),
    ("MD055", "MD055"),
    ("MD056", "MD056"),
    ("MD057", "MD057"),
    ("MD058", "MD058"),
    ("MD059", "MD059"),
    ("MD060", "MD060"),
    ("MD061", "MD061"),
    ("MD062", "MD062"),
    ("MD063", "MD063"),
    ("MD064", "MD064"),
    ("MD065", "MD065"),
    ("MD066", "MD066"),
    ("MD067", "MD067"),
    ("MD068", "MD068"),
    ("MD069", "MD069"),
    ("MD070", "MD070"),
    ("MD071", "MD071"),
    ("MD072", "MD072"),
    ("MD073", "MD073"),
    ("MD074", "MD074"),
    ("MD075", "MD075"),
    ("MD076", "MD076"),
    ("MD077", "MD077"),
    ("MD078", "MD078"),
    ("MD079", "MD079"),
    ("MD080", "MD080"),
    ("MD081", "MD081"),
    ("MD082", "MD082"),
    ("MD083", "MD083"),
    ("MD084", "MD084"),
    ("MD085", "MD085"),
    ("MD086", "MD086"),
    ("MD087", "MD087"),
    ("MD088", "MD088"),
    ("MISSING-CHUNK-LABELS", "MD078"),
    ("MKDOCS-NAV", "MD074"),
    ("MOJIBAKE", "MD083"),
    ("NESTED-CODE-FENCE", "MD070"),
    ("NO-ALT-TEXT", "MD045"),
    ("NO-BARE-URLS", "MD034"),
    ("NO-BLANKS-BLOCKQUOTE", "MD028"),
    ("NO-DUPLICATE-HEADING", "MD024"),
    ("NO-DUPLICATE-LIST-MARKERS", "MD069"),
    ("NO-EMPHASIS-AS-HEADING", "MD036"),
    ("NO-EMPTY-LINKS", "MD042"),
    ("NO-EMPTY-SECTIONS", "MD082"),
    ("NO-EXCESSIVE-EMPHASIS", "MD081"),
    ("NO-HARD-TABS", "MD010"),
    ("NO-INLINE-HTML", "MD033"),
    ("NO-MISSING-SPACE-ATX", "MD018"),
    ("NO-MISSING-SPACE-CLOSED-ATX", "MD020"),
    ("NO-MULTIPLE-BLANKS", "MD012"),
    ("NO-MULTIPLE-CONSECUTIVE-SPACES", "MD064"),
    ("NO-MULTIPLE-SPACE-ATX", "MD019"),
    ("NO-MULTIPLE-SPACE-BLOCKQUOTE", "MD027"),
    ("NO-MULTIPLE-SPACE-CLOSED-ATX", "MD021"),
    ("NO-REVERSED-LINKS", "MD011"),
    ("NO-SPACE-IN-CODE", "MD038"),
    ("NO-SPACE-IN-EMPHASIS", "MD037"),
    ("NO-SPACE-IN-LINK-DESTINATION", "MD062"),
    ("NO-SPACE-IN-LINKS", "MD039"),
    ("NO-TRAILING-PUNCTUATION", "MD026"),
    ("NO-TRAILING-SPACES", "MD009"),
    ("NO-UNCLOSED-COMMENTS", "MD086"),
    ("OL-PREFIX", "MD029"),
    ("ORPHANED-TABLE-ROWS", "MD075"),
    ("PARAGRAPH-CONTINUATION-INDENT", "MD085"),
    ("PROPER-NAMES", "MD044"),
    ("QUOTES-DASHES", "MD088"),
    ("REFERENCE-LINKS-IMAGES", "MD052"),
    ("REQUIRED-HEADINGS", "MD043"),
    ("SINGLE-H1", "MD025"),
    ("SINGLE-TITLE", "MD025"),
    ("SINGLE-TRAILING-NEWLINE", "MD047"),
    ("STRONG-STYLE", "MD050"),
    ("TABLE-CELL-ALIGNMENT", "MD060"),
    ("TABLE-COLUMN-COUNT", "MD056"),
    ("TABLE-FORMAT", "MD060"),
    ("TABLE-PIPE-STYLE", "MD055"),
    ("TOC-VALIDATION", "MD073"),
    ("UL-INDENT", "MD007"),
    ("UL-STYLE", "MD004"),
    ("UNUSED-DISABLE-COMMENT", "MD087"),
]);

/// The name rumdl uses when it writes a rule name itself, one per rule.
///
/// A rule can answer to several aliases, so the readable name it is given in
/// generated output (a disable comment written by the language server, the name
/// `rumdl rule` reports) has to be chosen rather than derived. The choice is the
/// alias each rule's documentation lists first.
///
/// Read-only compatibility view over primary aliases owned by the rule catalog.
/// The primary readable alias for each canonical rule ID. The public type stays
/// `StaticMap`; its entries are projected directly from the sorted rule catalog.
pub static RULE_PRIMARY_ALIAS: StaticMap = StaticMap::rule_primary_aliases();
pub fn primary_alias(rule_id: &str) -> Option<&'static str> {
    RULE_PRIMARY_ALIAS.get(rule_id)
}

/// Resolve a rule name alias to its canonical form
/// Converts rule aliases (like "ul-style", "line-length") to canonical IDs (like "MD004", "MD013")
/// Returns None if the rule name is not recognized
pub fn resolve_rule_name_alias(key: &str) -> Option<&'static str> {
    // Normalize: uppercase and replace underscores with hyphens
    let normalized_key = key.to_ascii_uppercase().replace('_', "-");

    RULE_ALIAS_MAP.get(normalized_key.as_str())
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
mod primary_alias_tests {
    use super::{RULE_ALIAS_MAP, RULE_PRIMARY_ALIAS, default_registry, primary_alias, resolve_rule_name_alias};

    /// Every rule ID the alias map knows, paired with the aliases it answers to.
    fn aliases_by_rule() -> std::collections::BTreeMap<&'static str, Vec<&'static str>> {
        let mut by_rule: std::collections::BTreeMap<&'static str, Vec<&'static str>> =
            std::collections::BTreeMap::new();
        for (alias, rule_id) in RULE_ALIAS_MAP.entries() {
            let entry = by_rule.entry(rule_id).or_default();
            if alias != rule_id {
                entry.push(alias);
            }
        }
        by_rule
    }

    /// A lookup answers correctly only while the array it binary-searches is sorted,
    /// and an out-of-order entry silently becomes unreachable rather than failing to
    /// compile, so the order is asserted rather than assumed.
    #[test]
    fn the_rule_name_tables_are_sorted_by_key() {
        assert!(RULE_ALIAS_MAP.is_sorted_by_key(), "RULE_ALIAS_MAP is out of order");
        assert!(
            RULE_PRIMARY_ALIAS.is_sorted_by_key(),
            "RULE_PRIMARY_ALIAS is out of order"
        );
    }

    /// The control for the test above: every key the tables hold is reachable, which
    /// is what sortedness buys and what an unsorted table would quietly break.
    #[test]
    fn every_key_in_the_rule_name_tables_is_reachable() {
        for (key, value) in RULE_ALIAS_MAP.entries() {
            assert_eq!(RULE_ALIAS_MAP.get(key), Some(value), "RULE_ALIAS_MAP lost '{key}'");
        }
        for (key, value) in RULE_PRIMARY_ALIAS.entries() {
            assert_eq!(
                RULE_PRIMARY_ALIAS.get(key),
                Some(value),
                "RULE_PRIMARY_ALIAS lost '{key}'"
            );
        }
        assert_eq!(
            RULE_ALIAS_MAP.get("NOT-A-RULE"),
            None,
            "control: a name the table does not hold answers None"
        );
    }

    #[test]
    fn every_rule_has_a_readable_name() {
        let rule_ids = default_registry().rule_names();
        assert!(
            rule_ids.contains("MD013"),
            "control: the registry lists rules by canonical ID, got {rule_ids:?}"
        );
        let missing: Vec<_> = rule_ids
            .into_iter()
            .filter(|rule_id| primary_alias(rule_id).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "these rules have no entry in RULE_PRIMARY_ALIAS: {missing:?}"
        );
    }

    #[test]
    fn a_readable_name_is_one_of_the_rules_own_aliases() {
        let by_rule = aliases_by_rule();
        for (rule_id, primary) in RULE_PRIMARY_ALIAS.entries() {
            let aliases = by_rule
                .get(rule_id)
                .unwrap_or_else(|| panic!("{rule_id} has a readable name but is not in RULE_ALIAS_MAP"));
            assert!(
                aliases.iter().any(|alias| alias.eq_ignore_ascii_case(primary)),
                "{rule_id}'s readable name '{primary}' is not one of its aliases {aliases:?}"
            );
        }
    }

    #[test]
    fn a_readable_name_resolves_back_to_its_rule() {
        for (rule_id, primary) in RULE_PRIMARY_ALIAS.entries() {
            assert_eq!(
                resolve_rule_name_alias(primary),
                Some(rule_id),
                "'{primary}' must be usable anywhere a rule name is accepted"
            );
        }
    }

    #[test]
    fn a_name_that_is_not_a_rule_id_has_no_readable_name() {
        // Control: the lookup takes canonical IDs, so an alias or a typo answers None
        // rather than something plausible.
        assert_eq!(primary_alias("MD013"), Some("line-length"));
        assert_eq!(primary_alias("line-length"), None);
        assert_eq!(primary_alias("MD999"), None);
    }
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
