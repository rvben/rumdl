//! A rule answers to its ID and to the readable names its documentation lists,
//! and rumdl writes the first of those names whenever it names a rule itself (an
//! ignore comment from the language server, the name `rumdl rule` reports). Three
//! places therefore have to agree: `docs/mdXXX.md`, `RULE_ALIAS_MAP` and
//! `RULE_PRIMARY_ALIAS`. A name present in only one of them is either never
//! mentioned to users or rejected when they type it, so these tests hold the
//! three together.

use std::collections::BTreeSet;
use std::fs;

use rumdl_lib::config::{Config, RULE_ALIAS_MAP, primary_alias, resolve_rule_name_alias};
use rumdl_lib::rules::all_rules;

/// The names `docs/mdXXX.md` lists, in the order it writes them.
fn documented_names(rule_id: &str) -> Vec<String> {
    let path = format!("{}/docs/{}.md", env!("CARGO_MANIFEST_DIR"), rule_id.to_lowercase());
    let content = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rule_id} has no documentation at {path}: {e}"));
    let line = content
        .lines()
        .find(|line| line.trim_start_matches('*').starts_with("Aliases:"))
        .unwrap_or_else(|| panic!("{rule_id}'s documentation has no `Aliases:` line"));
    // Every other backtick-delimited part is a name.
    line.split('`').skip(1).step_by(2).map(str::to_string).collect()
}

/// The names the registry resolves to `rule_id`, excluding the ID itself.
fn registered_names(rule_id: &str) -> BTreeSet<String> {
    RULE_ALIAS_MAP
        .entries()
        .filter(|(alias, canonical)| *canonical == rule_id && alias != canonical)
        .map(|(alias, _)| alias.to_lowercase())
        .collect()
}

fn rule_ids() -> Vec<String> {
    let rules = all_rules(&Config::default());
    assert!(rules.len() > 50, "control: the rule set is populated ({})", rules.len());
    rules.iter().map(|rule| rule.name().to_string()).collect()
}

#[test]
fn every_rule_documents_the_names_the_registry_accepts() {
    assert!(
        documented_names("MD013").contains(&"line-length".to_string()),
        "control: the probe reads names out of a rule's documentation"
    );

    let mismatched: Vec<String> = rule_ids()
        .iter()
        .filter_map(|id| {
            let documented: BTreeSet<String> = documented_names(id).into_iter().collect();
            let registered = registered_names(id);
            (documented != registered).then(|| format!("{id}: documented {documented:?}, registry {registered:?}"))
        })
        .collect();

    assert!(
        mismatched.is_empty(),
        "a name users can type must be documented, and a documented name must work:\n{}",
        mismatched.join("\n")
    );
}

#[test]
fn the_first_documented_name_is_the_one_rumdl_writes() {
    for id in rule_ids() {
        let documented = documented_names(&id);
        assert_eq!(
            primary_alias(&id),
            documented.first().map(String::as_str),
            "{id} is written as {:?} but its documentation leads with {:?}",
            primary_alias(&id),
            documented.first()
        );
    }
}

#[test]
fn every_documented_name_resolves_to_its_rule() {
    for id in rule_ids() {
        for name in documented_names(&id) {
            assert_eq!(
                resolve_rule_name_alias(&name),
                Some(id.as_str()),
                "{id} documents `{name}`, which must name it wherever a rule is named"
            );
        }
    }
}

#[test]
fn a_name_no_rule_documents_is_not_accepted() {
    // Control: the resolver rejects a misspelling rather than answering with a rule.
    assert_eq!(resolve_rule_name_alias("line-lenght"), None);
    assert!(
        !rule_ids()
            .iter()
            .any(|id| documented_names(id).iter().any(|name| name == "line-lenght")),
        "control: the misspelling is genuinely undocumented"
    );
}
