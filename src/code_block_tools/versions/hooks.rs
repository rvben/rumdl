//! Offline version metadata from explicitly recognized hook repositories.

use pep440_rs::Version;
use serde::Deserialize;
use std::collections::BTreeMap;

use super::{ExpectedVersion, ExpectedVersions, ToolVersionDefinition};

#[derive(Deserialize)]
struct Config {
    repos: Vec<Repository>,
    #[serde(flatten)]
    options: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct Repository {
    repo: String,
    rev: Option<serde_json::Value>,
    hooks: Vec<Hook>,
}

#[derive(Deserialize)]
struct Hook {
    id: String,
    #[serde(flatten)]
    options: BTreeMap<String, serde_json::Value>,
}

pub(super) fn parse(text: &str, source: ToolVersionDefinition) -> Result<ExpectedVersions, String> {
    let error = || format!("Cannot parse {} for tool version verification", source.filename());
    // YAML aliases and merge keys have the same meaning as in hook configuration.
    // Parser errors can contain arbitrary values and must never be quoted.
    let config: Config = match source {
        ToolVersionDefinition::PreCommit | ToolVersionDefinition::PreCommitYml => {
            let mut options = crate::utils::yaml::options();
            options.reject_unsupported_tags = true;
            // Hook metadata is shallow. Reject excessive nesting before recursive
            // Value deserialization can exhaust Windows' smaller main-thread stack.
            options.budget.get_or_insert_with(Default::default).max_depth = 16;
            let mut value: serde_json::Value =
                serde_saphyr::from_str_with_options(text, options).map_err(|_| error())?;
            merge_yaml(&mut value, 0).map_err(|()| error())?;
            serde_json::from_value(value).map_err(|_| error())?
        }
        ToolVersionDefinition::Prek => toml::from_str(text).map_err(|_| error())?,
        _ => return Err(error()),
    };
    let mut versions = ExpectedVersions::new();
    for repo in config.repos {
        if !matches!(repo.repo.as_str(), "local" | "meta" | "builtin")
            && repo.rev.as_ref().and_then(serde_json::Value::as_str).is_none()
        {
            return Err(error());
        }
        let identity = repository_identity(&repo.repo);
        let name = match identity.as_deref() {
            Some("astral-sh/ruff-pre-commit" | "charliermarsh/ruff-pre-commit") => "ruff",
            Some("psf/black" | "psf/black-pre-commit-mirror") => "black",
            Some("sqlfluff/sqlfluff") => "sqlfluff",
            Some("djlint/djlint") => "djlint",
            _ => continue,
        };
        for hook in repo.hooks {
            let recognized = match name {
                "ruff" => matches!(hook.id.as_str(), "ruff" | "ruff-check" | "ruff-format"),
                "black" => matches!(hook.id.as_str(), "black" | "black-jupyter"),
                "sqlfluff" => matches!(hook.id.as_str(), "sqlfluff-lint" | "sqlfluff-fix"),
                "djlint" => djlint_hook(&hook.id),
                _ => false,
            };
            let version = if !recognized
                || config.options.contains_key("default_env")
                || !hook.options.iter().all(|(key, value)| safe_option(key, value))
            {
                None
            } else {
                repo.rev
                    .as_ref()
                    .and_then(serde_json::Value::as_str)
                    .and_then(|text| revision(text, name))
            }
            .map(ExpectedVersion::Exact);
            versions
                .entry(name.into())
                .and_modify(|previous| {
                    if previous != &version {
                        *previous = None;
                    }
                })
                .or_insert(version);
        }
    }
    Ok(versions)
}

/// Resolve merge sources before their consumers. A single `apply_merge` pass
/// leaves nested `<<` keys behind; repeating it also loses precedence when an
/// earlier merge-list member inherits a key supplied directly by a later one.
fn merge_yaml(value: &mut serde_json::Value, depth: usize) -> Result<(), ()> {
    if depth >= 128 {
        return Err(());
    }
    match value {
        serde_json::Value::Object(mapping) => {
            for child in mapping.values_mut() {
                merge_yaml(child, depth + 1)?;
            }
            let merge = match mapping.remove("<<") {
                None => return Ok(()),
                Some(source @ serde_json::Value::Object(_)) => vec![source],
                Some(serde_json::Value::Array(merges)) => merges,
                Some(_) => return Err(()),
            };
            for source in merge {
                let serde_json::Value::Object(source) = source else {
                    return Err(());
                };
                for (key, value) in source {
                    mapping.entry(key).or_insert(value);
                }
            }
        }
        serde_json::Value::Array(sequence) => {
            for child in sequence {
                merge_yaml(child, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn safe_option(key: &str, value: &serde_json::Value) -> bool {
    match key {
        "language" => value.as_str() == Some("python"),
        "additional_dependencies" => value.as_array().is_some_and(Vec::is_empty),
        "name"
        | "alias"
        | "args"
        | "files"
        | "exclude"
        | "types"
        | "types_or"
        | "exclude_types"
        | "stages"
        | "language_version"
        | "minimum_pre_commit_version"
        | "require_serial"
        | "always_run"
        | "pass_filenames"
        | "verbose"
        | "log_file"
        | "priority"
        | "fail_fast"
        | "description" => true,
        _ => key.starts_with("x-"),
    }
}

fn repository_identity(repo: &str) -> Option<String> {
    let path = repo
        .strip_prefix("https://github.com/")
        .or_else(|| repo.strip_prefix("git@github.com:"))
        .or_else(|| repo.strip_prefix("ssh://git@github.com/"))?;
    let path = path.strip_suffix('/').unwrap_or(path).to_ascii_lowercase();
    Some(path.strip_suffix(".git").unwrap_or(&path).to_string())
}

fn revision(text: &str, name: &str) -> Option<Version> {
    let text = if matches!(name, "ruff" | "djlint") {
        text.strip_prefix('v')?
    } else {
        text
    };
    if !text.bytes().next()?.is_ascii_digit() || text.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return None;
    }
    let version: Version = text.parse().ok()?;
    (version.release().len() >= 2
        && version.local().is_empty()
        && version.epoch() == 0
        && version.dev().is_none()
        && version.to_string() == text)
        .then_some(version)
}

fn djlint_hook(id: &str) -> bool {
    let Some(suffix) = id.strip_prefix("djlint") else {
        return false;
    };
    let suffix = suffix.strip_prefix("-reformat").unwrap_or(suffix);
    matches!(
        suffix,
        "" | "-django" | "-jinja" | "-nunjucks" | "-handlebars" | "-liquid" | "-golang" | "-tera" | "-askama"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deeply_nested_yaml_is_rejected_on_a_small_stack() {
        // Approximate the default Windows main-thread stack on every platform.
        std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(|| {
                let text = format!("repos: []\nx-deep: {}0{}\n", "[".repeat(140), "]".repeat(140));
                let error = parse(&text, ToolVersionDefinition::PreCommit).unwrap_err();
                assert!(error.starts_with("Cannot parse"), "{error}");
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
