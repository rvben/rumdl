//! Declared constraints, without resolving or modifying a Python environment.

use pep440_rs::{Operator, Version, VersionSpecifier, VersionSpecifiers};
use serde::Deserialize;
use std::collections::BTreeMap;

use super::{ExpectedVersion, ExpectedVersions, ProbedVersion};

#[derive(Debug, PartialEq)]
pub(super) struct Constraints {
    specifiers: Vec<VersionSpecifier>,
    black: Option<String>,
}

impl Constraints {
    fn empty() -> Self {
        Self {
            specifiers: Vec::new(),
            black: None,
        }
    }

    pub(super) fn is_resolved(&self) -> bool {
        !self.specifiers.is_empty() || self.black.is_some()
    }

    pub(super) fn mismatch(&self, actual: &ProbedVersion) -> Option<String> {
        if let Some(specifier) = self
            .specifiers
            .iter()
            .find(|specifier| !contains(specifier, &actual.version))
        {
            let specifier: String = specifier.to_string().chars().take(192).collect();
            return Some(format!("does not satisfy declared constraint {specifier}"));
        }
        if self
            .black
            .as_ref()
            .is_some_and(|required| required != &actual.raw && Some(required.as_str()) != actual.raw.split('.').next())
        {
            return Some("does not satisfy Black required-version".into());
        }
        None
    }
}

/// Match installed versions, including padded release prefixes and exclusive
/// pre/post-release boundaries that pep440_rs 0.7.3 handles inconsistently.
fn contains(specifier: &VersionSpecifier, actual: &Version) -> bool {
    let bound = specifier.version();
    match specifier.operator() {
        Operator::EqualStar | Operator::NotEqualStar => {
            let matches = actual.epoch() == bound.epoch()
                && bound
                    .release()
                    .iter()
                    .enumerate()
                    .all(|(index, part)| *part == actual.release().get(index).copied().unwrap_or(0));
            matches == (*specifier.operator() == Operator::EqualStar)
        }
        Operator::LessThan => {
            if bound.any_prerelease() {
                actual < bound
            } else {
                actual < &bound.clone().with_dev(Some(0))
            }
        }
        Operator::GreaterThan => {
            let public = actual.clone().without_local();
            public > *bound
                && !(bound.dev().is_none()
                    && bound.post().is_none()
                    && actual.post().is_some()
                    && actual.pre() == bound.pre()
                    && actual.epoch() == bound.epoch()
                    && actual.only_release() == bound.only_release())
        }
        _ => specifier.contains(actual),
    }
}

#[derive(Default, Deserialize)]
struct Project {
    #[serde(default)]
    dynamic: Vec<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default, rename = "optional-dependencies")]
    optional_dependencies: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct Pyproject {
    #[serde(default)]
    project: Project,
    #[serde(default, rename = "dependency-groups")]
    groups: BTreeMap<String, Vec<GroupItem>>,
    #[serde(default)]
    tool: BTreeMap<String, toml::Value>,
}

#[derive(Default, Deserialize)]
struct Uv {
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: Vec<String>,
    #[serde(default, rename = "constraint-dependencies")]
    constraint_dependencies: Vec<String>,
    #[serde(default, rename = "override-dependencies")]
    override_dependencies: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum GroupItem {
    Requirement(String),
    Include(IncludeGroup),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IncludeGroup {
    #[serde(rename = "include-group")]
    _group: String,
}

pub(super) fn parse(text: &str) -> Result<ExpectedVersions, String> {
    let mut project: Pyproject =
        toml::from_str(text).map_err(|_| "Cannot parse pyproject.toml for tool version verification")?;
    let uv: Uv = project
        .tool
        .remove("uv")
        .map(toml::Value::try_into)
        .transpose()
        .map_err(|_| "Cannot parse tool.uv dependencies in pyproject.toml for tool version verification")?
        .unwrap_or_default();
    let mut versions = ExpectedVersions::new();
    // All group declarations are inspected; includes need no expansion or
    // environment selection and cannot multiply work or introduce recursion.
    let groups = project.groups.into_values().flatten().filter_map(|item| match item {
        GroupItem::Requirement(declaration) => Some(declaration),
        GroupItem::Include(include) => {
            let _ = include;
            None
        }
    });
    for declaration in project
        .project
        .dependencies
        .into_iter()
        .chain(project.project.optional_dependencies.into_values().flatten())
        .chain(groups)
        .chain(uv.dev_dependencies)
        .chain(uv.constraint_dependencies)
    {
        let Some(name) = mapped_name(&declaration) else {
            continue;
        };
        add_constraint(&mut versions, name.into(), requirement_specifiers(&declaration, name));
    }
    if let Some(ruff) = project.tool.get("ruff") {
        match ruff.as_table() {
            Some(table) => {
                if let Some(required) = table.get("required-version") {
                    let constraint = required.as_str().and_then(ruff_specifiers);
                    add_constraint(&mut versions, "ruff".into(), constraint);
                }
            }
            None => {
                versions.insert("ruff".into(), None);
            }
        }
    }
    if let Some(black) = project.tool.get("black") {
        match black.as_table() {
            Some(table) => {
                let hyphen = table.get("required-version");
                let underscore = table.get("required_version");
                if let Some(required) = hyphen.or(underscore) {
                    let required = if hyphen.is_some() && underscore.is_some() {
                        None
                    } else {
                        black_required(required)
                    };
                    let entry = versions
                        .entry("black".into())
                        .or_insert_with(|| Some(ExpectedVersion::Constraints(Constraints::empty())));
                    match (entry.as_mut(), required) {
                        (Some(ExpectedVersion::Constraints(constraints)), Some(required)) => {
                            constraints.black = Some(required)
                        }
                        _ => *entry = None,
                    }
                }
            }
            None => {
                versions.insert("black".into(), None);
            }
        }
    }
    for declaration in uv.override_dependencies {
        if let Some(name) = mapped_name(&declaration) {
            versions.insert(name.into(), None);
        }
    }
    if project
        .project
        .dynamic
        .iter()
        .any(|field| matches!(field.as_str(), "dependencies" | "optional-dependencies"))
    {
        for name in ["ruff", "black", "djlint", "sqlfluff"] {
            versions.insert(name.into(), None);
        }
    }
    Ok(versions)
}

fn black_required(value: &toml::Value) -> Option<String> {
    let text = match value {
        toml::Value::String(text) => text.clone(),
        toml::Value::Integer(value) if *value >= 0 => value.to_string(),
        _ => return None,
    };
    text.parse::<Version>().ok()?;
    Some(text)
}

fn ruff_specifiers(text: &str) -> Option<VersionSpecifiers> {
    if text.contains("===") {
        return None;
    }
    let specifiers = text
        .parse::<VersionSpecifiers>()
        .ok()
        .filter(|specifiers| !specifiers.is_empty());
    specifiers.or_else(|| {
        let version: Version = text.parse().ok()?;
        Some(VersionSpecifier::equals_version(version).into())
    })
}

/// Attribute even malformed declarations without confusing ruff-lsp with ruff.
fn mapped_name(declaration: &str) -> Option<&'static str> {
    let declaration = declaration.trim_start();
    let length = identifier_length(declaration);
    // These four package names have no internal separators. Trailing separators
    // are attributed conservatively, then rejected by the strict reader below.
    let name = declaration[..length].trim_end_matches(['.', '-', '_']);
    ["ruff", "black", "djlint", "sqlfluff"]
        .into_iter()
        .find(|known| name.eq_ignore_ascii_case(known))
}

fn identifier_length(text: &str) -> usize {
    text.bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(byte))
        .count()
}

fn valid_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.as_bytes()[0].is_ascii_alphanumeric()
        && text.as_bytes()[text.len() - 1].is_ascii_alphanumeric()
        && identifier_length(text) == text.len()
}

/// Strict PEP508 subset: a known name, optional extras, and PEP440 specifiers.
/// URLs, every marker, and arbitrary equality are deliberately unresolved.
fn requirement_specifiers(declaration: &str, name: &str) -> Option<VersionSpecifiers> {
    if !declaration.is_ascii() || declaration.bytes().any(|byte| byte.is_ascii_control() && byte != b'\t') {
        return None;
    }
    let declaration = declaration.trim_matches([' ', '\t']);
    let length = identifier_length(declaration);
    if !declaration[..length].eq_ignore_ascii_case(name) {
        return None;
    }
    let mut rest = declaration[length..].trim_matches([' ', '\t']);
    if let Some(extras) = rest.strip_prefix('[') {
        let (extras, tail) = extras.split_once(']')?;
        if !extras.trim_matches([' ', '\t']).is_empty()
            && !extras
                .split(',')
                .all(|extra| valid_identifier(extra.trim_matches([' ', '\t'])))
        {
            return None;
        }
        rest = tail.trim_matches([' ', '\t']);
    }
    if rest.contains(['@', ';']) || rest.contains("===") {
        return None;
    }
    if rest.is_empty() {
        return Some(VersionSpecifiers::empty());
    }
    if let Some(parenthesized) = rest.strip_prefix('(') {
        rest = parenthesized.strip_suffix(')')?.trim_matches([' ', '\t']);
        if rest.is_empty() {
            return None;
        }
    }
    // PEP 508 permits one trailing comma, but not empty interior clauses.
    let clauses = rest.strip_suffix(',').unwrap_or(rest).trim_matches([' ', '\t']);
    if clauses
        .split(',')
        .any(|clause| clause.trim_matches([' ', '\t']).is_empty())
    {
        return None;
    }
    let specifiers: VersionSpecifiers = clauses.parse().ok()?;
    (!specifiers.is_empty()).then_some(specifiers)
}

fn add_constraint(versions: &mut ExpectedVersions, name: String, constraint: Option<VersionSpecifiers>) {
    let entry = versions
        .entry(name)
        .or_insert_with(|| Some(ExpectedVersion::Constraints(Constraints::empty())));
    match (entry.as_mut(), constraint) {
        (Some(ExpectedVersion::Constraints(previous)), Some(incoming)) => {
            previous.specifiers.extend(incoming);
        }
        _ => *entry = None,
    }
}
