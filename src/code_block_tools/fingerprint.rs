//! What the lint tools a configuration names currently resolve to.
//!
//! A code-block tool's verdict depends on which binary runs, so a cached lint
//! result is only valid for the binaries that produced it. The fingerprint
//! records, for every binary a lint run could call, where it resolves on `PATH`
//! and the identity of the file there (its target through any symlinks, its size
//! and its modification time), or that it is missing. Installing, removing,
//! upgrading or re-ordering a tool on `PATH` changes the fingerprint.
//!
//! It is a proxy, not a version: a wrapper that picks the real binary at run
//! time (a mise or asdf shim, `npx`, `uvx`) keeps its identity when the version
//! behind it changes.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::path::Path;
use std::time::UNIX_EPOCH;

use super::config::CodeBlockToolsConfig;
use super::lookup;
use super::registry::{ToolRegistry, ToolSlot};

/// The fingerprint of every binary the `lint` slots of `config` could run.
///
/// Empty when code-block tools are disabled. Costs one `PATH` lookup and one
/// `stat` per distinct binary, so it is computed once per configuration rather
/// than once per file.
pub fn lint_tools_fingerprint(config: &CodeBlockToolsConfig) -> String {
    if !config.enabled {
        return String::new();
    }

    let registry = ToolRegistry::new(config.tools.clone());
    let binaries: BTreeSet<&str> = config
        .languages
        .values()
        .filter(|language| language.enabled)
        .flat_map(|language| &language.lint)
        // A `rumdl` id is rumdl itself only in a markdown block and a user tool
        // anywhere else, so it is fingerprinted like any other id: an extra
        // lookup costs a `stat`, a missing one replays stale results.
        .filter_map(|tool_id| registry.resolve(tool_id, ToolSlot::Lint))
        .filter_map(|tool_def| tool_def.command.first().map(String::as_str))
        .collect();

    let search_path = std::env::var_os("PATH");
    let mut fingerprint = String::new();
    for binary in binaries {
        let identity = lookup::resolve_program(OsStr::new(binary), search_path.as_deref())
            .map_or_else(|| "missing".to_string(), |path| file_identity(&path));
        let _ = writeln!(fingerprint, "{binary}\0{identity}");
    }
    fingerprint
}

fn file_identity(path: &Path) -> String {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let Ok(metadata) = std::fs::metadata(&target) else {
        return format!("{}\0unreadable", path.display());
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or_else(|| "unknown".to_string(), |age| age.as_nanos().to_string());
    format!(
        "{}\0{}\0{}\0{modified}",
        path.display(),
        target.display(),
        metadata.len()
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::code_block_tools::config::{LanguageToolConfig, ToolDefinition};
    use std::os::unix::fs::PermissionsExt;

    fn config_with_lint_tool(command: &str) -> CodeBlockToolsConfig {
        let mut config = CodeBlockToolsConfig {
            enabled: true,
            ..Default::default()
        };
        config.tools.insert(
            "t".to_string(),
            ToolDefinition {
                command: vec![command.to_string()],
                stdin: true,
                stdout: true,
                lint_args: vec![],
                format_args: vec![],
            },
        );
        config.languages.insert(
            "json".to_string(),
            LanguageToolConfig {
                lint: vec!["t".to_string()],
                ..Default::default()
            },
        );
        config
    }

    fn write_executable(path: &Path, body: &str) {
        std::fs::write(path, body).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn disabled_tools_have_no_fingerprint() {
        let mut config = config_with_lint_tool("/bin/sh");
        config.enabled = false;
        assert_eq!(lint_tools_fingerprint(&config), "");
    }

    #[test]
    fn a_missing_binary_is_recorded_as_missing() {
        let config = config_with_lint_tool("/nonexistent/rumdl-fingerprint-tool");
        assert_eq!(
            lint_tools_fingerprint(&config),
            "/nonexistent/rumdl-fingerprint-tool\0missing\n"
        );
    }

    #[test]
    fn the_fingerprint_follows_the_binary() {
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("tool");
        let config = config_with_lint_tool(tool.to_str().unwrap());
        let missing = lint_tools_fingerprint(&config);

        write_executable(&tool, "#!/bin/sh\n");
        let installed = lint_tools_fingerprint(&config);
        assert_ne!(missing, installed);
        assert_eq!(installed, lint_tools_fingerprint(&config), "not stable");

        write_executable(&tool, "#!/bin/sh\nexit 0\n");
        assert_ne!(installed, lint_tools_fingerprint(&config), "an upgrade went unnoticed");
    }

    #[test]
    fn a_symlink_is_fingerprinted_by_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, link) = (dir.path().join("a"), dir.path().join("b"), dir.path().join("tool"));
        write_executable(&a, "#!/bin/sh\n");
        write_executable(&b, "#!/bin/sh\n");
        let config = config_with_lint_tool(link.to_str().unwrap());

        std::os::unix::fs::symlink(&a, &link).unwrap();
        let to_a = lint_tools_fingerprint(&config);
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&b, &link).unwrap();

        assert_ne!(to_a, lint_tools_fingerprint(&config));
    }

    /// Only a markdown block short-circuits a `rumdl` id to rumdl itself. For any
    /// other language a user tool of that name is resolved and run like any other.
    #[test]
    fn a_user_tool_named_like_the_builtin_is_fingerprinted() {
        let mut config = config_with_lint_tool("/nonexistent/custom-rumdl");
        let tool = config.tools.remove("t").unwrap();
        config.tools.insert("rumdl".to_string(), tool);
        config.languages.get_mut("json").unwrap().lint = vec!["rumdl".to_string()];

        assert!(
            lint_tools_fingerprint(&config).contains("custom-rumdl"),
            "{:?}",
            lint_tools_fingerprint(&config)
        );
    }

    #[test]
    fn format_only_tools_are_not_fingerprinted() {
        let mut config = config_with_lint_tool("/nonexistent/lint-tool");
        config.languages.get_mut("json").unwrap().format = vec!["other".to_string()];
        config.tools.insert(
            "other".to_string(),
            ToolDefinition {
                command: vec!["/nonexistent/format-tool".to_string()],
                stdin: true,
                stdout: true,
                lint_args: vec![],
                format_args: vec![],
            },
        );
        assert!(!lint_tools_fingerprint(&config).contains("format-tool"));
    }
}
