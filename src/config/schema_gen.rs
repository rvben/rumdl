//! The JSON Schema published for `rumdl.toml`.
//!
//! [`config_schema_json`] is the single entry point used by `rumdl schema
//! generate|print|check` and by the test that keeps the committed
//! `rumdl.schema.json` in step with the code, so the two paths cannot drift.
//!
//! Rule sections are invisible to `schemars`: they live in a
//! `#[serde(flatten)]` map, which erases them from `properties`. Everything
//! below puts them back, using the same [`RuleRegistry`](super::registry::RuleRegistry) the config loader
//! validates a file with. That sharing is the invariant worth keeping: the
//! schema must never reject a key rumdl accepts, so every "is this key valid?"
//! answer comes from the registry rather than from a list maintained here.
//!
//! What this covers, and what it deliberately does not:
//!
//! * Covered: the value of every documented option is typed, so
//!   `[MD013] line-length = "eighty"` is caught in the editor instead of at
//!   load time.
//! * Covered as well: every option carries the doc comment of its field and the
//!   default the loader falls back to, because the rule config structs derive
//!   `schemars::JsonSchema`. The struct that deserializes a section is the one
//!   that describes it ([`Rule::config_json_schema`](crate::rule::Rule::config_json_schema)),
//!   so the keys an editor offers and the keys the loader takes cannot drift
//!   apart. Options are still typed from the rule's own registry entry when a
//!   struct does not derive the trait.
//! * Option values that are enums are described, not enumerated
//!   (`relax_value_enums`): `#[serde(alias)]` on a variant widens the spellings
//!   the loader accepts and `schemars` does not report aliases, so a published
//!   list would reject input rumdl takes. The spellings move into the
//!   description, where an editor still shows them.
//! * Options the registry deliberately leaves untyped (its nullable and
//!   polymorphic sentinels) keep an empty schema but keep their description: a
//!   description constrains nothing, and these are the options — `preferred-style`,
//!   `lines-above` — that need explaining most.
//! * MD073 is the one rule without a config struct: it builds its section by
//!   hand, so its options stay typed from the rule's defaults and stay
//!   undescribed. `test_schema_describes_the_options_it_derives` names it as the
//!   only allowed gap, so a newly added rule cannot join it silently.
//! * Not covered: a misspelled *key*. rumdl matches option keys case- and
//!   separator-insensitively (`RuleRegistry::canonical_config_key`,
//!   [`super::flavor::normalize_key`]), so the spellings it takes for one key
//!   are an open set (`LINE_LENGTH`, `line_length`, `Line-Length`, …) and no
//!   finite `properties` list can be closed with `additionalProperties: false`
//!   without rejecting input the loader accepts. The schema names what it knows
//!   and leaves the set open. Flagging unknown keys is a loader question rather
//!   than a schema one: rumdl warns instead of failing (`validation.rs`), so an
//!   editor error would overstate it.
//! * One place where the schema *is* stricter than the loader, on purpose: a
//!   rule section has to be a table. `MD013 = false`, at the root or under
//!   `[rules]`, is warned about and then dropped (`parsers.rs`, "Expected table
//!   for rule …; ignoring"), so the rule keeps running with its defaults while
//!   the file reads as if it had switched the rule off. The schema rejects it
//!   (`$defs/MD013` is an object) because nothing here is a guess: the name is
//!   one the schema models, the shape it accepts is total, and the intent has
//!   one spelling — `[MD013] enabled = false`, or `disable = ["MD013"]` in
//!   `[global]`. The open-set argument above is about *names* the schema cannot
//!   rule in or out; this is the *shape* of a name it does model.
//! * `global.enable`/`disable`/`fixable`/`unfixable` and `per-file-ignores` keep
//!   `{"type": "string"}` for rule names. A case-insensitive `pattern` naming
//!   the 90 IDs and their 95 name aliases is buildable — `relax_severity`
//!   shows how — so the reason is parity rather than expressiveness: rumdl only
//!   warns about a rule name it does not know (`validation.rs`), and an editor
//!   error would then be stricter than the loader. Same argument as the
//!   misspelled key above.
//! * The markdownlint-style wrapper the parser also accepts, `[rules.MD013]`,
//!   gets the same properties as the root (see `rules_wrapper_schema`).
//!   Without it the wrapper would still validate — it lands on the root
//!   `additionalProperties` — but nothing inside it would be typed or completed.
//! * `$defs/GlobalConfig` stays open as well. Closing it would mean listing the
//!   snake_case aliases there, and `test_schema_globalconfig_uses_kebab_case`
//!   requires the definition to stay kebab-only. The `[global]` spellings rumdl
//!   takes are accepted, untyped, inside the section and typed at the root
//!   (see `global_shorthand_properties`).

use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value, json};

use super::registry::{RULE_ALIAS_MAP, default_registry};
use super::types::Config;
use crate::rule_config_serde::is_sentinel;

/// Build the JSON Schema document for `rumdl.toml` / `.rumdl.toml`.
pub fn config_schema_json() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(Config)).expect("the Config schema serializes");

    let root = schema.as_object_mut().expect("schemars emits an object for Config");

    // Rule sections arrive through the flatten map, so any root key that is not
    // one of the known sections is a rule section.
    root.insert(
        "additionalProperties".to_string(),
        json!({ "$ref": "#/$defs/RuleConfig" }),
    );

    let global_properties = root
        .get("$defs")
        .and_then(|defs| defs.get("GlobalConfig"))
        .and_then(|global| global.get("properties"))
        .and_then(Value::as_object)
        .expect("schemars emits $defs/GlobalConfig.properties")
        .clone();

    let defs = root
        .get_mut("$defs")
        .and_then(Value::as_object_mut)
        .expect("schemars emits $defs");
    add_rule_config_keys(defs);
    relax_severity(defs);
    defs.extend(rule_section_definitions());

    if let Some(properties) = root.get_mut("properties").and_then(Value::as_object_mut) {
        let sections = rule_section_properties();
        let mut merged = global_shorthand_properties(&global_properties);

        // Global shorthand includes snake_case spellings too. Resolve each
        // spelling through the registry so a name such as `line_length`
        // retains its meaning as a rule section as well as a global value.
        for (key, value) in &mut merged {
            if let Some(canonical) = default_registry().resolve_rule_name(key) {
                *value = json!({ "anyOf": [value.clone(), rule_ref(&canonical)] });
            }
        }

        for (key, section) in &sections {
            // Preserve any global-or-rule union assembled above.
            merged.entry(key.clone()).or_insert_with(|| section.clone());
        }

        properties.extend(merged);
        properties.insert("rules".to_string(), rules_wrapper_schema(sections));
    }

    schema
}

/// Root-level shorthand: rumdl reads the global keys from the document root as
/// well as from `[global]`, through one dispatch table
/// ([`is_global_value_key`](super::global_keys::is_global_value_key) and
/// `parse_global_key`). Without these entries the root properties would miss
/// `line-length = 100`, and `additionalProperties` — which holds the rule
/// section schema — would reject it.
fn global_shorthand_properties(global: &Map<String, Value>) -> Map<String, Value> {
    let mut properties = Map::new();

    for key in super::global_keys::GLOBAL_VALUE_KEYS {
        let snake_case = key.replace('-', "_");
        // A key listed as a global value key but missing from `GlobalConfig` would
        // otherwise land here as an untyped `{}` — the silent downgrade this module
        // exists to remove — so the invariant fails loudly instead.
        let schema = global
            .get(&snake_case)
            .or_else(|| global.get(*key))
            .cloned()
            .expect("`GLOBAL_VALUE_KEYS` and `GlobalConfig` agree on the global keys");
        // Both separators are accepted at the root too (`normalize_key`).
        properties.insert((*key).to_string(), schema.clone());
        properties.insert(snake_case, schema);
    }

    properties
}

/// One `$defs` entry per rule, named by its canonical ID (`MD013`).
fn rule_section_definitions() -> BTreeMap<String, Value> {
    let registry = default_registry();

    registry
        .rule_schemas
        .iter()
        .map(|(name, options)| {
            let definition = rule_section_definition(
                options,
                registry.rule_json_schemas.get(name),
                registry.rule_descriptions.get(name).map(String::as_str),
                registry.rule_aliases.get(name),
            );
            (name.clone(), definition)
        })
        .collect()
}

/// The definition of one rule section: `severity`, `enabled`, and every option
/// the rule reports, `description` taken from the rule itself.
///
/// `derived` is the JSON Schema the rule's config struct produces through
/// `schemars` (see [`Rule::config_json_schema`](crate::rule::Rule::config_json_schema)).
/// Where it has an entry for a key, that entry is used: it carries the doc comment
/// of the field and the variants of any enum behind it, which a value read off a
/// serialized default cannot express. It never overrides a sentinel key, so the
/// loader's "recognized but untyped" keys stay untyped in the schema too.
fn rule_section_definition(
    options: &toml::map::Map<String, toml::Value>,
    derived: Option<&Value>,
    description: Option<&str>,
    aliases: Option<&HashMap<String, String>>,
) -> Value {
    let mut properties = Map::new();
    let derived_properties = derived
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object);

    // One schema per option key, reused for the key itself, its other separator and
    // every alias that names it.
    let typed_option = |key: &str, value: &toml::Value| -> Value {
        let derived = derived_properties.and_then(|properties| properties.get(key));

        if is_sentinel(value) {
            // The registry skips its own type check for this key because no single
            // TOML type describes it. A derived schema would describe the one variant
            // the field's `Deserialize` happens to name first and then reject the
            // other, so the key keeps an empty schema — but a description constrains
            // nothing, and these are the options that most need explaining.
            return match derived.and_then(|derived| derived.get("description")) {
                Some(description) => json!({ "description": description.clone() }),
                None => json!({}),
            };
        }

        match derived {
            Some(derived) => {
                let mut schema = derived.clone();
                relax_value_enums(&mut schema);
                schema
            }
            None => option_schema(value),
        }
    };

    for (key, value) in options {
        let schema = typed_option(key, value);
        properties.insert(key.clone(), schema.clone());
        // The loader accepts either separator for every option key
        // ([`super::registry::RuleRegistry::canonical_config_key`]), so both
        // spellings get a type. Case variants stay valid through the open
        // property set; they just carry no type here.
        let snake_case = key.replace('-', "_");
        if snake_case != *key {
            properties.insert(snake_case, schema);
        }
    }

    // An alias names the same option, so it carries the same schema. Only the
    // spelling the rule declares is listed; the case and separator variants the
    // loader also accepts stay valid through the open property set.
    //
    // Sorted, because the rule hands them over in a `HashMap` and the schema is
    // compared against the committed file byte for byte.
    let mut alias_pairs: Vec<(&String, &String)> = aliases.into_iter().flatten().collect();
    alias_pairs.sort();
    for (alias, canonical) in alias_pairs {
        if let Some(value) = options.get(canonical) {
            properties.insert(alias.clone(), typed_option(canonical, value));
        }
    }

    // Inserted last: a few rules publish `severity`/`enabled` as part of their
    // own defaults, and the documented form of both belongs here, not the
    // inferred one.
    properties.insert("severity".to_string(), json!({ "$ref": "#/$defs/Severity" }));
    properties.insert("enabled".to_string(), enabled_schema());

    let mut definition = Map::new();
    if let Some(description) = description {
        definition.insert("description".to_string(), Value::String(description.to_string()));
    }
    definition.insert("type".to_string(), Value::String("object".to_string()));
    definition.insert("properties".to_string(), Value::Object(properties));
    Value::Object(definition)
}

/// Root properties: every canonical rule ID plus every name alias, each
/// pointing at that rule's definition, so editors complete and document both
/// `[MD013]` and `[line-length]`.
fn rule_section_properties() -> Map<String, Value> {
    let mut properties = Map::new();

    for name in default_registry().rule_schemas.keys() {
        properties.insert(name.clone(), rule_ref(name));
    }

    for (alias, canonical) in RULE_ALIAS_MAP.entries() {
        // The table also lists each ID as its own alias. `md013` needs no entry
        // of its own — `normalize_key` uppercases it to `MD013`, which is
        // already a property — so an entry here would only duplicate the name.
        if alias.eq_ignore_ascii_case(canonical) {
            continue;
        }
        // The table is keyed by the normalized form (uppercase, hyphens); users
        // write kebab-case, which is what completion should offer.
        properties
            .entry(alias.to_ascii_lowercase())
            .or_insert_with(|| rule_ref(canonical));
    }

    properties
}

fn rule_ref(name: &str) -> Value {
    json!({ "$ref": format!("#/$defs/{name}") })
}

/// Replaces every enumerated set of spellings with the plain JSON type it
/// constrains, moving the spellings into the description.
///
/// `schemars` lists what a variant is *renamed* to, but a `#[serde(alias)]` on the
/// variant — which the loader honours — widens that set without telling the
/// schema, so publishing the list would reject configs rumdl accepts. The values
/// stay readable in the description (an editor shows them on hover) while the type
/// check keeps working. `$defs/Severity` is widened by [`relax_severity`] for the
/// same reason: a list of spellings is only as complete as the list.
///
/// Recurses into nested objects and arrays, because an option can be an object of
/// its own (`[MD022]`) and hold enums one level down.
fn relax_value_enums(value: &mut Value) {
    let Some(map) = value.as_object_mut() else {
        if let Some(items) = value.as_array_mut() {
            for item in items {
                relax_value_enums(item);
            }
        }
        return;
    };

    if let Some((kind, spellings)) = enumerated_spellings(map) {
        map.remove("enum");
        map.remove("oneOf");
        map.remove("anyOf");
        map.remove("const");
        map.insert("type".to_string(), Value::String(kind));
        let sentence = format!("One of {spellings}.");
        let existing = map.get("description").and_then(Value::as_str).map(str::to_string);
        let description = match existing {
            Some(description) => format!("{description} {sentence}"),
            None => sentence,
        };
        map.insert("description".to_string(), Value::String(description));
        return;
    }

    for key in [
        "properties",
        "patternProperties",
        "$defs",
        "definitions",
        "dependentSchemas",
    ] {
        if let Some(Value::Object(map)) = map.get_mut(key) {
            for nested in map.values_mut() {
                relax_value_enums(nested);
            }
        }
    }
    for key in ["items", "additionalProperties", "propertyNames", "contains"] {
        if let Some(nested) = map.get_mut(key) {
            relax_value_enums(nested);
        }
    }
    // A list of subschemas, whether alternatives or tuples.
    for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        if let Some(Value::Array(subschemas)) = map.get_mut(key) {
            for subschema in subschemas {
                relax_value_enums(subschema);
            }
        }
    }
}

/// The JSON type an enumerated subschema constrains, plus its spellings rendered
/// as a comma-separated list. `None` when the subschema is not one.
fn enumerated_spellings(map: &Map<String, Value>) -> Option<(String, String)> {
    let values: Vec<&Value> = if let Some(values) = map.get("enum").and_then(Value::as_array) {
        values.iter().collect()
    } else {
        let variants = map.get("oneOf").or_else(|| map.get("anyOf"))?.as_array()?;
        let mut constants = Vec::with_capacity(variants.len());
        for variant in variants {
            constants.push(variant.get("const")?);
        }
        constants
    };

    if values.is_empty() {
        return None;
    }

    let kind = match values[0] {
        Value::String(_) => "string",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        _ => return None,
    };

    let mut rendered = Vec::with_capacity(values.len());
    for value in values {
        let spelling = match value {
            Value::String(text) => format!("`{text}`"),
            other => format!("`{other}`"),
        };
        rendered.push(spelling);
    }

    Some((kind.to_string(), rendered.join(", ")))
}

/// `[rules.MD013]` is accepted as an alias for `[MD013]` (`parsers.rs`), which is
/// how configs ported from markdownlint keep their shape. The wrapper carries the
/// same properties as the root, so it gets the same completion and the same option
/// types; `additionalProperties` stays open for the same parity reason as `[global]`.
fn rules_wrapper_schema(sections: Map<String, Value>) -> Value {
    json!({
        "description": "Rule sections under a `[rules]` heading: `[rules.MD013]` is the same as `[MD013]`.",
        "type": "object",
        "properties": Value::Object(sections),
        "additionalProperties": { "$ref": "#/$defs/RuleConfig" },
    })
}

/// `severity` and `enabled` are valid in every rule section. `schemars` sees
/// only `severity`, because `enabled` is read off the raw config map instead of
/// being deserialized into [`RuleConfig`](super::types::RuleConfig).
fn add_rule_config_keys(defs: &mut Map<String, Value>) {
    let properties = defs
        .get_mut("RuleConfig")
        .and_then(Value::as_object_mut)
        .and_then(|definition| definition.get_mut("properties"))
        .and_then(Value::as_object_mut)
        .expect("schemars emits $defs/RuleConfig.properties");

    properties.insert("enabled".to_string(), enabled_schema());
}

/// The `enabled` entry, shared by every rule definition and by `RuleConfig`, so
/// the two cannot describe the same key differently.
fn enabled_schema() -> Value {
    json!({
        "type": "boolean",
        "description": "Enables or disables this rule."
    })
}

/// Replace the lowercase `Severity` enum with a case-insensitive pattern.
///
/// rumdl lowercases the value before matching
/// ([`Severity`](crate::rule::Severity)'s hand-written `Deserialize`), so
/// `severity = "WARNING"` loads. A plain enum would reject it, and JSON Schema
/// has no `(?i)` flag — but character classes express the same thing, so the
/// three values stay constrained without the schema being stricter than rumdl.
/// The cost is value completion: an editor can no longer list the three
/// options, only reject a fourth.
fn relax_severity(defs: &mut Map<String, Value>) {
    defs.insert(
        "Severity".to_string(),
        json!({
            "description": "Severity of the findings a rule reports: `error`, `warning` or `info`.\nMatched case-insensitively.",
            "type": "string",
            "pattern": "^(?:[Ee][Rr][Rr][Oo][Rr]|[Ww][Aa][Rr][Nn][Ii][Nn][Gg]|[Ii][Nn][Ff][Oo])$"
        }),
    );
}

/// Schema for one option, inferred from the value the rule reports for it.
///
/// Those values are the rule's default config serialized to TOML, which is the
/// same table the loader type-checks user values against, so the inferred type
/// matches what rumdl enforces. Keys whose default is a placeholder leave the
/// value unconstrained: their deserializer accepts more than a serialized
/// default can express, which is exactly when the loader skips the type check
/// too.
fn option_schema(value: &toml::Value) -> Value {
    if is_sentinel(value) {
        return json!({});
    }

    match value {
        toml::Value::String(_) => json!({ "type": "string" }),
        toml::Value::Integer(_) => json!({ "type": "integer" }),
        toml::Value::Float(_) => json!({ "type": "number" }),
        toml::Value::Boolean(_) => json!({ "type": "boolean" }),
        toml::Value::Array(items) => array_schema(items),
        toml::Value::Table(_) => json!({ "type": "object" }),
        // No rule has a datetime option today. Should one appear, the value
        // stays unconstrained rather than guessed: the same rule as the
        // sentinels above, and consistent with the loader, which has no JSON
        // form for a datetime to type-check against.
        toml::Value::Datetime(_) => json!({}),
    }
}

/// Arrays hold strings in every rule today; an empty default says nothing about
/// the element type, so in that case only the container is constrained.
fn array_schema(items: &[toml::Value]) -> Value {
    let Some(first) = items.first() else {
        return json!({ "type": "array" });
    };

    let first_schema = option_schema(first);
    if items.iter().all(|item| option_schema(item) == first_schema) {
        json!({ "type": "array", "items": first_schema })
    } else {
        json!({ "type": "array" })
    }
}
