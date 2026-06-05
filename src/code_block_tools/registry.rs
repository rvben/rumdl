//! Built-in tool registry with definitions for common formatters and linters.
//!
//! This module provides default configurations for popular tools like ruff, prettier,
//! shellcheck, etc. Users can override these in their configuration.

use super::config::ToolDefinition;
use super::processor::RUMDL_BUILTIN_TOOL;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Registry of built-in tool definitions.
pub struct ToolRegistry {
    /// User-defined tools (override built-ins)
    user_tools: BTreeMap<String, ToolDefinition>,
}

impl ToolRegistry {
    /// Create a new registry with user-defined tools.
    pub fn new(user_tools: BTreeMap<String, ToolDefinition>) -> Self {
        Self { user_tools }
    }

    /// Get a tool definition by ID.
    ///
    /// Checks user tools first, then falls back to built-in tools.
    pub fn get(&self, tool_id: &str) -> Option<&ToolDefinition> {
        self.user_tools.get(tool_id).or_else(|| BUILTIN_TOOLS.get(tool_id))
    }

    /// Check if a tool ID is valid (either user-defined or built-in).
    pub fn contains(&self, tool_id: &str) -> bool {
        self.user_tools.contains_key(tool_id) || BUILTIN_TOOLS.contains_key(tool_id)
    }

    /// List all available tool IDs.
    pub fn list_tools(&self) -> Vec<&str> {
        let mut tools: Vec<&str> = self.user_tools.keys().map(std::string::String::as_str).collect();
        for key in BUILTIN_TOOLS.keys() {
            if !self.user_tools.contains_key(*key) {
                tools.push(key);
            }
        }
        tools.sort_unstable();
        tools
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}

/// Built-in tool definitions.
///
/// These are common formatters and linters that work well with stdin/stdout.
static BUILTIN_TOOLS: LazyLock<HashMap<&'static str, ToolDefinition>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Python - ruff
    m.insert(
        "ruff:check",
        ToolDefinition {
            command: vec![
                "ruff".to_string(),
                "check".to_string(),
                "--output-format=concise".to_string(),
                "--stdin-filename=_.py".to_string(),
                "-".to_string(),
            ],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        },
    );

    m.insert(
        "ruff:format",
        ToolDefinition {
            command: vec![
                "ruff".to_string(),
                "format".to_string(),
                "--stdin-filename=_.py".to_string(),
                "-".to_string(),
            ],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        },
    );

    // Python - black
    m.insert(
        "black",
        ToolDefinition {
            command: vec!["black".to_string(), "--quiet".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    // JavaScript/TypeScript - prettier
    m.insert(
        "prettier",
        ToolDefinition {
            command: vec!["prettier".to_string(), "--stdin-filepath=_.js".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    m.insert(
        "prettier:json",
        ToolDefinition {
            command: vec!["prettier".to_string(), "--stdin-filepath=_.json".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    m.insert(
        "prettier:yaml",
        ToolDefinition {
            command: vec!["prettier".to_string(), "--stdin-filepath=_.yaml".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    m.insert(
        "prettier:html",
        ToolDefinition {
            command: vec!["prettier".to_string(), "--stdin-filepath=_.html".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    m.insert(
        "prettier:css",
        ToolDefinition {
            command: vec!["prettier".to_string(), "--stdin-filepath=_.css".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    m.insert(
        "prettier:markdown",
        ToolDefinition {
            command: vec!["prettier".to_string(), "--stdin-filepath=_.md".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    // Shell - shellcheck (lint only). `--shell=bash` because code blocks rarely carry
    // a shebang; without it shellcheck emits a "target shell unknown" tip instead of
    // real diagnostics. bash is the common, permissive default; override with a custom
    // tool for sh/ksh/dash.
    m.insert(
        "shellcheck",
        ToolDefinition {
            command: vec!["shellcheck".to_string(), "--shell=bash".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        },
    );

    // Shell - shfmt
    m.insert(
        "shfmt",
        ToolDefinition {
            command: vec!["shfmt".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["-d".to_string()], // diff mode for lint
            format_args: vec![],
        },
    );

    // Rust - rustfmt
    m.insert(
        "rustfmt",
        ToolDefinition {
            command: vec!["rustfmt".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    // Go - gofmt
    m.insert(
        "gofmt",
        ToolDefinition {
            command: vec!["gofmt".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["-d".to_string()], // diff mode for lint
            format_args: vec![],
        },
    );

    // Go - goimports
    m.insert(
        "goimports",
        ToolDefinition {
            command: vec!["goimports".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["-d".to_string()],
            format_args: vec![],
        },
    );

    // C/C++ - clang-format
    m.insert(
        "clang-format",
        ToolDefinition {
            command: vec!["clang-format".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--dry-run".to_string(), "--Werror".to_string()],
            format_args: vec![],
        },
    );

    // SQL - sqlfluff
    m.insert(
        "sqlfluff:lint",
        ToolDefinition {
            command: vec!["sqlfluff".to_string(), "lint".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        },
    );

    m.insert(
        "sqlfluff:fix",
        ToolDefinition {
            command: vec!["sqlfluff".to_string(), "fix".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        },
    );

    // JSON - jq (format/lint)
    m.insert(
        "jq",
        ToolDefinition {
            command: vec!["jq".to_string(), ".".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        },
    );

    // YAML - yamlfmt
    m.insert(
        "yamlfmt",
        ToolDefinition {
            command: vec!["yamlfmt".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["-lint".to_string(), "-".to_string()],
            format_args: vec!["-".to_string()],
        },
    );

    // TOML - taplo
    m.insert(
        "taplo",
        ToolDefinition {
            command: vec!["taplo".to_string(), "fmt".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    // Terraform - terraform fmt
    m.insert(
        "terraform-fmt",
        ToolDefinition {
            command: vec!["terraform".to_string(), "fmt".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["-check".to_string()],
            format_args: vec![],
        },
    );

    // Nix - nixfmt
    m.insert(
        "nixfmt",
        ToolDefinition {
            command: vec!["nixfmt".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    // Lua - stylua
    m.insert(
        "stylua",
        ToolDefinition {
            command: vec!["stylua".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    // Ruby - rubocop
    m.insert(
        "rubocop",
        ToolDefinition {
            command: vec!["rubocop".to_string(), "--stdin".to_string(), "_.rb".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec!["--autocorrect".to_string()],
        },
    );

    // Haskell - ormolu
    m.insert(
        "ormolu",
        ToolDefinition {
            command: vec!["ormolu".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check-idempotence".to_string()],
            format_args: vec![],
        },
    );

    // Elm - elm-format
    m.insert(
        "elm-format",
        ToolDefinition {
            command: vec!["elm-format".to_string(), "--stdin".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--validate".to_string()],
            format_args: vec![],
        },
    );

    // Zig - zig fmt
    m.insert(
        "zig-fmt",
        ToolDefinition {
            command: vec!["zig".to_string(), "fmt".to_string(), "--stdin".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    // Dart - dart format
    m.insert(
        "dart-format",
        ToolDefinition {
            command: vec!["dart".to_string(), "format".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--output=none".to_string(), "--set-exit-if-changed".to_string()],
            format_args: vec![],
        },
    );

    // Swift - swift-format
    m.insert(
        "swift-format",
        ToolDefinition {
            command: vec!["swift-format".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["lint".to_string()],
            format_args: vec![],
        },
    );

    // Kotlin - ktfmt
    m.insert(
        "ktfmt",
        ToolDefinition {
            command: vec!["ktfmt".to_string(), "--stdin".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--dry-run".to_string()],
            format_args: vec![],
        },
    );

    // Jinja/HTML - djlint
    m.insert(
        "djlint",
        ToolDefinition {
            command: vec!["djlint".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec!["--reformat".to_string()],
        },
    );

    m.insert(
        "djlint:lint",
        ToolDefinition {
            command: vec!["djlint".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        },
    );

    m.insert(
        "djlint:reformat",
        ToolDefinition {
            command: vec!["djlint".to_string(), "-".to_string(), "--reformat".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        },
    );

    // Shell - beautysh
    m.insert(
        "beautysh",
        ToolDefinition {
            command: vec!["beautysh".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    // TOML - tombi (default runs `tombi lint` since users typically configure it in the lint slot)
    m.insert(
        "tombi",
        ToolDefinition {
            command: vec!["tombi".to_string(), "lint".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        },
    );

    m.insert(
        "tombi:format",
        ToolDefinition {
            command: vec!["tombi".to_string(), "format".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        },
    );

    m.insert(
        "tombi:lint",
        ToolDefinition {
            command: vec!["tombi".to_string(), "lint".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        },
    );

    // JavaScript/CSS/HTML/JSON - oxfmt (OXC formatter)
    m.insert(
        "oxfmt",
        ToolDefinition {
            command: vec!["oxfmt".to_string(), "--stdin-filepath=_.js".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    m.insert(
        "oxfmt:js",
        ToolDefinition {
            command: vec!["oxfmt".to_string(), "--stdin-filepath=_.js".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    m.insert(
        "oxfmt:ts",
        ToolDefinition {
            command: vec!["oxfmt".to_string(), "--stdin-filepath=_.ts".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    m.insert(
        "oxfmt:jsx",
        ToolDefinition {
            command: vec!["oxfmt".to_string(), "--stdin-filepath=_.jsx".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    m.insert(
        "oxfmt:tsx",
        ToolDefinition {
            command: vec!["oxfmt".to_string(), "--stdin-filepath=_.tsx".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    m.insert(
        "oxfmt:json",
        ToolDefinition {
            command: vec!["oxfmt".to_string(), "--stdin-filepath=_.json".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    m.insert(
        "oxfmt:css",
        ToolDefinition {
            command: vec!["oxfmt".to_string(), "--stdin-filepath=_.css".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["--check".to_string()],
            format_args: vec![],
        },
    );

    // Multi-language - deno fmt. stdin needs --ext to pick the parser, so one entry
    // per supported extension. Bare "deno-fmt" defaults to TypeScript.
    let deno_fmt = |ext: &str| ToolDefinition {
        command: vec![
            "deno".to_string(),
            "fmt".to_string(),
            format!("--ext={ext}"),
            "-".to_string(),
        ],
        stdin: true,
        stdout: true,
        lint_args: vec!["--check".to_string()],
        format_args: vec![],
    };
    m.insert("deno-fmt", deno_fmt("ts"));
    m.insert("deno-fmt:ts", deno_fmt("ts"));
    m.insert("deno-fmt:js", deno_fmt("js"));
    m.insert("deno-fmt:json", deno_fmt("json"));
    m.insert("deno-fmt:jsonc", deno_fmt("jsonc"));
    m.insert("deno-fmt:md", deno_fmt("md"));

    m
});

/// Whether a built-in tool lints, formats, or both, for the docs table "Type" column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    Lint,
    Format,
    Both,
}

impl ToolKind {
    const fn label(self) -> &'static str {
        match self {
            ToolKind::Lint => "Lint",
            ToolKind::Format => "Format",
            ToolKind::Both => "Both",
        }
    }
}

/// Documentation metadata for a built-in tool, paired with [`BUILTIN_TOOLS`] by `id`.
///
/// This is the source of the generated table in `docs/code-block-tools.md`. It carries
/// the display-only columns (`language`, `kind`) that must not live on the user-facing
/// `ToolDefinition`. Command text is NOT duplicated here: the table pulls it from the
/// runtime definition unless `display_command` provides a curated override.
///
/// Invariants (enforced by tests, so CI fails on any drift):
/// - every `runtime` id has a matching `BUILTIN_TOOLS` entry, and vice versa;
/// - `DocsOnly` ids (`runtime == false`, e.g. `rumdl`, handled specially in the
///   processor) are absent from `BUILTIN_TOOLS` and must set `display_command`;
/// - a `doc_group` with more than one runtime entry must set `display_command` on each;
/// - all entries in a `doc_group` share `language` and `kind`.
struct ToolDocMeta {
    id: &'static str,
    language: &'static str,
    kind: ToolKind,
    /// Rows sharing a `doc_group` collapse into one table row (extension variants).
    doc_group: &'static str,
    /// Curated command for the table; falls back to the runtime command join.
    display_command: Option<&'static str>,
    /// True for real external tools (in `BUILTIN_TOOLS`); false for docs-only entries.
    runtime: bool,
}

/// Display metadata for the built-in tools table. Order here is the table row order.
const BUILTIN_TOOLS_DOCS: &[ToolDocMeta] = &[
    ToolDocMeta {
        id: "ruff:check",
        language: "Python",
        kind: ToolKind::Lint,
        doc_group: "ruff:check",
        display_command: Some("ruff check --output-format=concise -"),
        runtime: true,
    },
    ToolDocMeta {
        id: "ruff:format",
        language: "Python",
        kind: ToolKind::Format,
        doc_group: "ruff:format",
        display_command: Some("ruff format -"),
        runtime: true,
    },
    ToolDocMeta {
        id: "black",
        language: "Python",
        kind: ToolKind::Format,
        doc_group: "black",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "prettier",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "prettier",
        display_command: Some("prettier --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "prettier:json",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "prettier",
        display_command: Some("prettier --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "prettier:yaml",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "prettier",
        display_command: Some("prettier --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "prettier:html",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "prettier",
        display_command: Some("prettier --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "prettier:css",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "prettier",
        display_command: Some("prettier --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "prettier:markdown",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "prettier",
        display_command: Some("prettier --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "shellcheck",
        language: "Shell",
        kind: ToolKind::Lint,
        doc_group: "shellcheck",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "shfmt",
        language: "Shell",
        kind: ToolKind::Format,
        doc_group: "shfmt",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "rustfmt",
        language: "Rust",
        kind: ToolKind::Format,
        doc_group: "rustfmt",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "gofmt",
        language: "Go",
        kind: ToolKind::Format,
        doc_group: "gofmt",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "goimports",
        language: "Go",
        kind: ToolKind::Format,
        doc_group: "goimports",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "clang-format",
        language: "C/C++",
        kind: ToolKind::Format,
        doc_group: "clang-format",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "sqlfluff:lint",
        language: "SQL",
        kind: ToolKind::Lint,
        doc_group: "sqlfluff:lint",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "sqlfluff:fix",
        language: "SQL",
        kind: ToolKind::Format,
        doc_group: "sqlfluff:fix",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "jq",
        language: "JSON",
        kind: ToolKind::Both,
        doc_group: "jq",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "yamlfmt",
        language: "YAML",
        kind: ToolKind::Format,
        doc_group: "yamlfmt",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "taplo",
        language: "TOML",
        kind: ToolKind::Format,
        doc_group: "taplo",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "terraform-fmt",
        language: "Terraform",
        kind: ToolKind::Format,
        doc_group: "terraform-fmt",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "nixfmt",
        language: "Nix",
        kind: ToolKind::Format,
        doc_group: "nixfmt",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "stylua",
        language: "Lua",
        kind: ToolKind::Format,
        doc_group: "stylua",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "rubocop",
        language: "Ruby",
        kind: ToolKind::Both,
        doc_group: "rubocop",
        display_command: Some("rubocop --stdin / rubocop --stdin --autocorrect"),
        runtime: true,
    },
    ToolDocMeta {
        id: "ormolu",
        language: "Haskell",
        kind: ToolKind::Format,
        doc_group: "ormolu",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "elm-format",
        language: "Elm",
        kind: ToolKind::Format,
        doc_group: "elm-format",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "zig-fmt",
        language: "Zig",
        kind: ToolKind::Format,
        doc_group: "zig-fmt",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "dart-format",
        language: "Dart",
        kind: ToolKind::Format,
        doc_group: "dart-format",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "swift-format",
        language: "Swift",
        kind: ToolKind::Format,
        doc_group: "swift-format",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "ktfmt",
        language: "Kotlin",
        kind: ToolKind::Format,
        doc_group: "ktfmt",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "djlint",
        language: "Jinja/HTML",
        kind: ToolKind::Both,
        doc_group: "djlint",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "djlint:lint",
        language: "Jinja/HTML",
        kind: ToolKind::Lint,
        doc_group: "djlint:lint",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "djlint:reformat",
        language: "Jinja/HTML",
        kind: ToolKind::Format,
        doc_group: "djlint:reformat",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "beautysh",
        language: "Shell",
        kind: ToolKind::Both,
        doc_group: "beautysh",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "tombi",
        language: "TOML",
        kind: ToolKind::Lint,
        doc_group: "tombi",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "tombi:format",
        language: "TOML",
        kind: ToolKind::Format,
        doc_group: "tombi:format",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "tombi:lint",
        language: "TOML",
        kind: ToolKind::Lint,
        doc_group: "tombi:lint",
        display_command: None,
        runtime: true,
    },
    ToolDocMeta {
        id: "oxfmt",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "oxfmt",
        display_command: Some("oxfmt --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "oxfmt:js",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "oxfmt",
        display_command: Some("oxfmt --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "oxfmt:ts",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "oxfmt",
        display_command: Some("oxfmt --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "oxfmt:jsx",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "oxfmt",
        display_command: Some("oxfmt --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "oxfmt:tsx",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "oxfmt",
        display_command: Some("oxfmt --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "oxfmt:json",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "oxfmt",
        display_command: Some("oxfmt --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "oxfmt:css",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "oxfmt",
        display_command: Some("oxfmt --stdin-filepath=_.EXT"),
        runtime: true,
    },
    ToolDocMeta {
        id: "deno-fmt",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "deno-fmt",
        display_command: Some("deno fmt --ext=EXT -"),
        runtime: true,
    },
    ToolDocMeta {
        id: "deno-fmt:ts",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "deno-fmt",
        display_command: Some("deno fmt --ext=EXT -"),
        runtime: true,
    },
    ToolDocMeta {
        id: "deno-fmt:js",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "deno-fmt",
        display_command: Some("deno fmt --ext=EXT -"),
        runtime: true,
    },
    ToolDocMeta {
        id: "deno-fmt:json",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "deno-fmt",
        display_command: Some("deno fmt --ext=EXT -"),
        runtime: true,
    },
    ToolDocMeta {
        id: "deno-fmt:jsonc",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "deno-fmt",
        display_command: Some("deno fmt --ext=EXT -"),
        runtime: true,
    },
    ToolDocMeta {
        id: "deno-fmt:md",
        language: "Multi",
        kind: ToolKind::Format,
        doc_group: "deno-fmt",
        display_command: Some("deno fmt --ext=EXT -"),
        runtime: true,
    },
    // Docs-only: rumdl's own markdown linting, short-circuited in the processor before
    // tool resolution (never a registry entry).
    ToolDocMeta {
        id: RUMDL_BUILTIN_TOOL,
        language: "Markdown",
        kind: ToolKind::Lint,
        doc_group: RUMDL_BUILTIN_TOOL,
        display_command: Some("built-in markdown linting"),
        runtime: false,
    },
];

/// Markers fencing the generated table in `docs/code-block-tools.md`.
const TABLE_BEGIN: &str = "<!-- BEGIN builtin-tools (generated) -->";
const TABLE_END: &str = "<!-- END builtin-tools (generated) -->";

/// Error from splicing the generated docs into `docs/code-block-tools.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocsError {
    /// A begin/end marker pair is missing.
    MissingMarker,
    /// A begin or end marker appears more than once.
    DuplicateMarker,
    /// The end marker precedes the begin marker.
    MarkerOrder,
    /// The "Built-in tools" count row was not found in the comparison table.
    CountRowMissing,
    /// More than one "Built-in tools" count row was found.
    CountRowAmbiguous,
    /// The "Built-in tools" count row does not have the expected cell layout.
    CountRowMalformed,
}

impl std::fmt::Display for DocsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            DocsError::MissingMarker => "missing `<!-- BEGIN/END builtin-tools (generated) -->` marker pair",
            DocsError::DuplicateMarker => "duplicate builtin-tools marker",
            DocsError::MarkerOrder => "END builtin-tools marker precedes BEGIN",
            DocsError::CountRowMissing => "`| Built-in tools` count row not found",
            DocsError::CountRowAmbiguous => "multiple `| Built-in tools` count rows found",
            DocsError::CountRowMalformed => "`| Built-in tools` count row has an unexpected layout",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for DocsError {}

/// Number of rows the generated table renders (distinct `doc_group`s).
fn builtin_tools_group_count() -> usize {
    let mut seen: Vec<&str> = Vec::new();
    for m in BUILTIN_TOOLS_DOCS {
        if !seen.contains(&m.doc_group) {
            seen.push(m.doc_group);
        }
    }
    seen.len()
}

/// Render the built-in tools table (the markdown fenced by the doc markers).
///
/// One row per `doc_group`, in list order. Columns are padded to their widest cell so
/// the output matches rumdl's own table formatting, keeping the generated docs
/// `rumdl fmt --check`-clean (the project config normalizes table column widths).
pub fn render_builtin_tools_table() -> String {
    let headers = ["Tool ID", "Language", "Type", "Command"];
    let mut rows: Vec<[String; 4]> = Vec::new();

    let mut seen: Vec<&str> = Vec::new();
    for m in BUILTIN_TOOLS_DOCS {
        if seen.contains(&m.doc_group) {
            continue;
        }
        seen.push(m.doc_group);

        // Docs-only entries have no runtime command to fall back to; runtime entries
        // render the exact mode-specific invocation the executor runs.
        let command = match m.display_command {
            Some(cmd) => cmd.to_string(),
            None if m.runtime => runtime_command_for_kind(m.id, m.kind),
            None => String::new(),
        };

        rows.push([
            format!("`{}`", m.doc_group),
            m.language.to_string(),
            m.kind.label().to_string(),
            format!("`{command}`"),
        ]);
    }

    // Column widths = widest cell (header included), counted in chars (all ASCII here).
    let mut widths = headers.map(str::chars).map(Iterator::count);
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }

    let mut out = String::new();
    push_table_row(&mut out, &headers.map(String::from), &widths);
    let separators = std::array::from_fn(|i| "-".repeat(widths[i]));
    push_table_row(&mut out, &separators, &widths);
    for row in &rows {
        push_table_row(&mut out, row, &widths);
    }
    out
}

/// The exact command the executor runs for a built-in tool in its documented mode:
/// the base command plus the mode-specific args it appends (`lint_args` for Lint,
/// `format_args` for Format). `Both` tools show both invocations when they differ.
/// This keeps the table honest about what actually runs, rather than the bare command.
fn runtime_command_for_kind(id: &str, kind: ToolKind) -> String {
    let Some(def) = BUILTIN_TOOLS.get(id) else {
        return String::new();
    };
    let invocation =
        |extra: &[String]| -> String { def.command.iter().chain(extra).cloned().collect::<Vec<_>>().join(" ") };
    match kind {
        ToolKind::Lint => invocation(&def.lint_args),
        ToolKind::Format => invocation(&def.format_args),
        ToolKind::Both => {
            let lint = invocation(&def.lint_args);
            let format = invocation(&def.format_args);
            if lint == format {
                lint
            } else {
                format!("{lint} / {format}")
            }
        }
    }
}

/// Append one `| cell | cell | ... |` row, left-padding each cell to its column width.
fn push_table_row(out: &mut String, cells: &[String; 4], widths: &[usize; 4]) {
    out.push('|');
    for (i, cell) in cells.iter().enumerate() {
        out.push(' ');
        out.push_str(cell);
        for _ in cell.chars().count()..widths[i] {
            out.push(' ');
        }
        out.push_str(" |");
    }
    out.push('\n');
}

/// Splice the generated table and the "Built-in tools" count into the docs file text.
///
/// Replaces only the content between the marker pair and the single count cell in the
/// mdsf comparison table; all other prose is preserved. Fails loudly on malformed or
/// missing markers rather than silently corrupting the file.
pub fn splice_builtin_tools_docs(existing: &str) -> Result<String, DocsError> {
    if existing.matches(TABLE_BEGIN).count() == 0 || existing.matches(TABLE_END).count() == 0 {
        return Err(DocsError::MissingMarker);
    }
    if existing.matches(TABLE_BEGIN).count() > 1 || existing.matches(TABLE_END).count() > 1 {
        return Err(DocsError::DuplicateMarker);
    }
    let begin_pos = existing.find(TABLE_BEGIN).unwrap();
    let end_pos = existing.find(TABLE_END).unwrap();
    if end_pos < begin_pos {
        return Err(DocsError::MarkerOrder);
    }

    let table = render_builtin_tools_table();
    let replacement = format!("{TABLE_BEGIN}\n\n{table}\n{TABLE_END}");

    let mut result = String::with_capacity(existing.len() + replacement.len());
    result.push_str(&existing[..begin_pos]);
    result.push_str(&replacement);
    result.push_str(&existing[end_pos + TABLE_END.len()..]);

    update_builtin_tools_count(&result, builtin_tools_group_count())
}

/// Rewrite the count cell in the unique `| Built-in tools | N | ... |` comparison row,
/// preserving the cell's width so the hand-aligned comparison table stays tidy.
fn update_builtin_tools_count(text: &str, count: usize) -> Result<String, DocsError> {
    let lines: Vec<&str> = text.lines().collect();
    let mut target: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("| Built-in tools") {
            if target.is_some() {
                return Err(DocsError::CountRowAmbiguous);
            }
            target = Some(i);
        }
    }
    let i = target.ok_or(DocsError::CountRowMissing)?;
    let new_line = replace_count_cell(lines[i], count).ok_or(DocsError::CountRowMalformed)?;

    let mut out = String::with_capacity(text.len());
    for (j, line) in lines.iter().enumerate() {
        out.push_str(if j == i { &new_line } else { line });
        out.push('\n');
    }
    if !text.ends_with('\n') {
        out.pop();
    }
    Ok(out)
}

/// Replace the second cell (the count) of a markdown table row, keeping its width.
fn replace_count_cell(line: &str, count: usize) -> Option<String> {
    let pipes: Vec<usize> = line.match_indices('|').map(|(i, _)| i).collect();
    if pipes.len() < 3 {
        return None;
    }
    let start = pipes[1] + 1;
    let end = pipes[2];
    let cell = line.get(start..end)?;
    let width = cell.len();
    let num = count.to_string();

    let mut new_cell = String::with_capacity(width.max(num.len() + 2));
    new_cell.push(' ');
    new_cell.push_str(&num);
    while new_cell.len() < width {
        new_cell.push(' ');
    }
    if new_cell.len() > width {
        // Number grew past the original cell width; keep a single padding space.
        new_cell = format!(" {num} ");
    }

    Some(format!("{}{}{}", &line[..start], new_cell, &line[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_tool() {
        let registry = ToolRegistry::default();

        let tool = registry.get("ruff:check").expect("Should find ruff:check");
        assert!(tool.command.contains(&"ruff".to_string()));
        assert!(tool.stdin);
        assert!(tool.stdout);

        let tool = registry.get("shellcheck").expect("Should find shellcheck");
        assert!(tool.command.contains(&"shellcheck".to_string()));
    }

    #[test]
    fn test_builtin_yamlfmt_lint_command_validates_stdin() {
        let registry = ToolRegistry::default();

        let tool = registry.get("yamlfmt").expect("Should find yamlfmt");
        let mut argv = tool.command.clone();
        argv.extend(tool.lint_args.clone());

        assert_eq!(argv, vec!["yamlfmt", "-lint", "-"]);
    }

    #[test]
    fn test_get_user_tool_overrides_builtin() {
        let mut user_tools = BTreeMap::new();
        user_tools.insert(
            "ruff:check".to_string(),
            ToolDefinition {
                command: vec!["custom-ruff".to_string()],
                stdin: false,
                stdout: false,
                lint_args: vec![],
                format_args: vec![],
            },
        );

        let registry = ToolRegistry::new(user_tools);

        let tool = registry.get("ruff:check").expect("Should find ruff:check");
        assert_eq!(tool.command, vec!["custom-ruff"]);
        assert!(!tool.stdin); // User override
    }

    #[test]
    fn test_contains() {
        let registry = ToolRegistry::default();

        assert!(registry.contains("ruff:check"));
        assert!(registry.contains("prettier"));
        assert!(registry.contains("shellcheck"));
        assert!(!registry.contains("nonexistent-tool"));
    }

    #[test]
    fn test_list_tools() {
        let registry = ToolRegistry::default();
        let tools = registry.list_tools();

        assert!(tools.contains(&"ruff:check"));
        assert!(tools.contains(&"ruff:format"));
        assert!(tools.contains(&"prettier"));
        assert!(tools.contains(&"shellcheck"));
        assert!(tools.contains(&"shfmt"));
        assert!(tools.contains(&"rustfmt"));
        assert!(tools.contains(&"gofmt"));
    }

    #[test]
    fn test_user_tools_in_list() {
        let mut user_tools = BTreeMap::new();
        user_tools.insert("my-custom-tool".to_string(), ToolDefinition::default());

        let registry = ToolRegistry::new(user_tools);
        let tools = registry.list_tools();

        assert!(tools.contains(&"my-custom-tool"));
        assert!(tools.contains(&"ruff:check")); // Built-in still available
    }

    #[test]
    fn test_new_builtin_tools() {
        let registry = ToolRegistry::default();

        // djlint
        let tool = registry.get("djlint").expect("Should find djlint");
        assert!(tool.command.contains(&"djlint".to_string()));
        assert!(tool.stdin);

        // beautysh
        let tool = registry.get("beautysh").expect("Should find beautysh");
        assert!(tool.command.contains(&"beautysh".to_string()));
        assert!(tool.stdin);

        // tombi
        let tool = registry.get("tombi").expect("Should find tombi");
        assert!(tool.command.contains(&"tombi".to_string()));
        assert!(tool.stdin);

        let tool = registry.get("tombi:lint").expect("Should find tombi:lint");
        assert!(tool.command.contains(&"lint".to_string()));

        let tool = registry.get("tombi:format").expect("Should find tombi:format");
        assert!(
            tool.command.contains(&"format".to_string()),
            "tombi:format should use 'format' subcommand, got: {:?}",
            tool.command
        );

        // oxfmt
        let tool = registry.get("oxfmt").expect("Should find oxfmt");
        assert!(tool.command.contains(&"oxfmt".to_string()));
        assert!(tool.stdin);

        let tool = registry.get("oxfmt:ts").expect("Should find oxfmt:ts");
        assert!(tool.command.iter().any(|s| s.contains("_.ts")));
    }

    // =========================================================================
    // Issue #527: bare "tombi" in format slot resolves to lint command
    // =========================================================================

    /// The bare "tombi" registry entry defaults to `tombi lint -`.
    /// The processor's `resolve_tool` method handles context-aware resolution:
    /// in format context, it resolves "tombi" to "tombi:format" automatically.
    #[test]
    fn test_bare_tombi_resolves_to_lint_not_format() {
        let registry = ToolRegistry::default();

        let bare = registry.get("tombi").expect("Should find bare tombi");
        let format = registry.get("tombi:format").expect("Should find tombi:format");

        // The bare entry uses `lint` subcommand
        assert!(
            bare.command.contains(&"lint".to_string()),
            "Bare 'tombi' uses lint subcommand: {:?}",
            bare.command
        );

        // The format entry uses `format` subcommand
        assert!(
            format.command.contains(&"format".to_string()),
            "tombi:format uses format subcommand: {:?}",
            format.command
        );

        // These are different commands — using bare "tombi" in format = [...] is a bug
        assert_ne!(
            bare.command, format.command,
            "Bare 'tombi' and 'tombi:format' should have different commands (this is the root cause of #527)"
        );
    }

    /// Tools that have both lint and format variants should have distinct entries.
    /// The processor resolves bare names to context-specific variants automatically.
    #[test]
    fn test_tools_with_lint_format_variants_are_distinct() {
        let registry = ToolRegistry::default();

        // ruff has both check and format
        let ruff_check = registry.get("ruff:check").expect("ruff:check");
        let ruff_format = registry.get("ruff:format").expect("ruff:format");
        assert_ne!(
            ruff_check.command, ruff_format.command,
            "ruff:check and ruff:format should be distinct"
        );

        // tombi has both lint and format
        let tombi_lint = registry.get("tombi:lint").expect("tombi:lint");
        let tombi_format = registry.get("tombi:format").expect("tombi:format");
        assert_ne!(
            tombi_lint.command, tombi_format.command,
            "tombi:lint and tombi:format should be distinct"
        );
    }

    #[test]
    fn test_deno_fmt_has_per_extension_variants() {
        // deno fmt bakes the extension into each variant (the registry does no runtime
        // substitution), so a per-language slot picks the right parser.
        let registry = ToolRegistry::default();

        let deno_json = registry.get("deno-fmt:json").expect("deno-fmt:json");
        assert!(deno_json.command.iter().any(|a| a == "--ext=json"));
        let deno_md = registry.get("deno-fmt:md").expect("deno-fmt:md");
        assert!(deno_md.command.iter().any(|a| a == "--ext=md"));

        // Bare entry defaults to TypeScript.
        let deno = registry.get("deno-fmt").expect("deno-fmt");
        assert!(deno.command.iter().any(|a| a == "--ext=ts"));
    }

    // =========================================================================
    // Docs metadata <-> registry invariants (lock the table to the registry)
    // =========================================================================

    #[test]
    fn test_docs_metadata_ids_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for m in BUILTIN_TOOLS_DOCS {
            assert!(seen.insert(m.id), "duplicate metadata id: {}", m.id);
        }
    }

    #[test]
    fn test_runtime_metadata_matches_registry_keys() {
        let runtime_meta: std::collections::BTreeSet<&str> =
            BUILTIN_TOOLS_DOCS.iter().filter(|m| m.runtime).map(|m| m.id).collect();
        let map_keys: std::collections::BTreeSet<&str> = BUILTIN_TOOLS.keys().copied().collect();
        assert_eq!(
            runtime_meta, map_keys,
            "BUILTIN_TOOLS_DOCS runtime ids must exactly match BUILTIN_TOOLS keys (add/remove the doc entry alongside the registry entry)"
        );
    }

    #[test]
    fn test_docs_only_ids_absent_from_registry() {
        for m in BUILTIN_TOOLS_DOCS.iter().filter(|m| !m.runtime) {
            assert!(
                !BUILTIN_TOOLS.contains_key(m.id),
                "docs-only id {} must not be a runtime registry tool",
                m.id
            );
        }
        // rumdl is the docs-only entry and tracks the processor constant.
        assert!(
            BUILTIN_TOOLS_DOCS
                .iter()
                .any(|m| !m.runtime && m.id == RUMDL_BUILTIN_TOOL),
            "rumdl must be present as a docs-only entry"
        );
    }

    #[test]
    fn test_docs_only_requires_display_command() {
        for m in BUILTIN_TOOLS_DOCS.iter().filter(|m| !m.runtime) {
            assert!(
                m.display_command.is_some(),
                "docs-only id {} needs a display_command (no runtime command to derive)",
                m.id
            );
        }
    }

    #[test]
    fn test_multi_runtime_group_requires_display_command() {
        let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for m in BUILTIN_TOOLS_DOCS.iter().filter(|m| m.runtime) {
            *counts.entry(m.doc_group).or_default() += 1;
        }
        for m in BUILTIN_TOOLS_DOCS {
            if counts.get(m.doc_group).copied().unwrap_or(0) > 1 {
                assert!(
                    m.display_command.is_some(),
                    "doc_group `{}` has multiple runtime entries; `{}` needs an explicit display_command",
                    m.doc_group,
                    m.id
                );
            }
        }
    }

    #[test]
    fn test_doc_group_language_and_kind_consistent() {
        let mut groups: std::collections::BTreeMap<&str, (&str, ToolKind)> = std::collections::BTreeMap::new();
        for m in BUILTIN_TOOLS_DOCS {
            match groups.get(m.doc_group) {
                None => {
                    groups.insert(m.doc_group, (m.language, m.kind));
                }
                Some((lang, kind)) => {
                    assert_eq!(
                        *lang, m.language,
                        "doc_group `{}` has mismatched languages",
                        m.doc_group
                    );
                    assert_eq!(*kind, m.kind, "doc_group `{}` has mismatched kinds", m.doc_group);
                }
            }
        }
    }

    // =========================================================================
    // Generator: golden rows (hand-authored, independent of `generate`)
    // =========================================================================

    /// Parse the rendered table into trimmed `(id, language, type, command)` tuples,
    /// independent of column-padding width.
    fn rendered_rows(table: &str) -> Vec<(String, String, String, String)> {
        table
            .lines()
            .skip(2) // header + separator
            .filter(|l| l.starts_with('|'))
            .map(|l| {
                let cells: Vec<String> = l.trim().trim_matches('|').split('|').map(|c| c.trim().to_string()).collect();
                (cells[0].clone(), cells[1].clone(), cells[2].clone(), cells[3].clone())
            })
            .collect()
    }

    #[test]
    fn test_render_table_golden_rows() {
        let table = render_builtin_tools_table();
        let rows = rendered_rows(&table);
        let has = |id: &str, lang: &str, kind: &str, cmd: &str| {
            rows.iter()
                .any(|(i, l, k, c)| i == id && l == lang && k == kind && c == cmd)
        };
        // One exact row per kind, plus a collapsed group and the docs-only row.
        assert!(has("`shellcheck`", "Shell", "Lint", "`shellcheck --shell=bash -`"));
        assert!(has("`rustfmt`", "Rust", "Format", "`rustfmt`"));
        assert!(has("`jq`", "JSON", "Both", "`jq .`"));
        assert!(has(
            "`prettier`",
            "Multi",
            "Format",
            "`prettier --stdin-filepath=_.EXT`"
        ));
        assert!(has("`rumdl`", "Markdown", "Lint", "`built-in markdown linting`"));
        // Extension variants collapse into their group row.
        assert!(
            !table.contains("prettier:json"),
            "prettier variants must collapse into one row"
        );
        assert!(!table.contains("oxfmt:ts"), "oxfmt variants must collapse into one row");
    }

    #[test]
    fn test_render_table_one_row_per_group() {
        let table = render_builtin_tools_table();
        assert_eq!(rendered_rows(&table).len(), builtin_tools_group_count());
    }

    #[test]
    fn test_render_table_command_fallback_from_registry() {
        // A row with display_command: None pulls its command from BUILTIN_TOOLS.
        let table = render_builtin_tools_table();
        assert!(
            rendered_rows(&table)
                .iter()
                .any(|(i, _, _, c)| i == "`tombi`" && c == "`tombi lint -`")
        );
    }

    #[test]
    fn test_render_command_reflects_mode_args() {
        let table = render_builtin_tools_table();
        let rows = rendered_rows(&table);
        let cmd = |id: &str| {
            let row = rows
                .iter()
                .find(|(i, _, _, _)| i == id)
                .unwrap_or_else(|| panic!("row {id} not found"));
            row.3.clone()
        };
        // Format-typed tools include format_args, not just the bare command.
        assert_eq!(cmd("`yamlfmt`"), "`yamlfmt -`");
        // Both-typed tools show the lint and format invocations when they differ.
        assert_eq!(cmd("`djlint`"), "`djlint - / djlint - --reformat`");
        // Lint-typed tools include their subcommand args.
        assert_eq!(cmd("`sqlfluff:lint`"), "`sqlfluff lint -`");
    }

    #[test]
    fn test_runtime_command_for_kind_both_collapses_when_equal() {
        // jq lints and formats with the same invocation, so it renders once.
        assert_eq!(runtime_command_for_kind("jq", ToolKind::Both), "jq .");
    }

    // =========================================================================
    // Splice / marker handling (fail-loud)
    // =========================================================================

    #[test]
    fn test_splice_replaces_region_and_preserves_prose() {
        let doc = format!(
            "# Title\n\nIntro.\n\n{TABLE_BEGIN}\n\nstale\n\n{TABLE_END}\n\nAfter.\n\n| Built-in tools | 31 | 339 |\n"
        );
        let out = splice_builtin_tools_docs(&doc).expect("splice");
        assert!(out.starts_with("# Title\n\nIntro.\n\n"));
        assert!(out.contains("After.\n"));
        assert!(!out.contains("stale"));
        assert!(
            out.contains(&render_builtin_tools_table()),
            "generated table is spliced in verbatim"
        );
        // Count updated to the group count, width preserved (2-digit -> 2-digit here).
        assert!(out.contains(&format!("| Built-in tools | {} | 339 |", builtin_tools_group_count())));
    }

    #[test]
    fn test_splice_missing_marker_errors() {
        let doc = "# Title\n\nno markers\n\n| Built-in tools | 31 | 339 |\n";
        assert_eq!(splice_builtin_tools_docs(doc), Err(DocsError::MissingMarker));
    }

    #[test]
    fn test_splice_duplicate_marker_errors() {
        let doc = format!("{TABLE_BEGIN}\n\nx\n\n{TABLE_END}\n{TABLE_BEGIN}\n\ny\n\n{TABLE_END}\n");
        assert_eq!(splice_builtin_tools_docs(&doc), Err(DocsError::DuplicateMarker));
    }

    #[test]
    fn test_splice_marker_order_errors() {
        let doc = format!("{TABLE_END}\n\nx\n\n{TABLE_BEGIN}\n\n| Built-in tools | 31 | 339 |\n");
        assert_eq!(splice_builtin_tools_docs(&doc), Err(DocsError::MarkerOrder));
    }

    #[test]
    fn test_splice_missing_count_row_errors() {
        let doc = format!("{TABLE_BEGIN}\n\nx\n\n{TABLE_END}\n\nno count row here\n");
        assert_eq!(splice_builtin_tools_docs(&doc), Err(DocsError::CountRowMissing));
    }

    #[test]
    fn test_splice_is_idempotent() {
        let doc = format!("{TABLE_BEGIN}\n\nstale\n\n{TABLE_END}\n\n| Built-in tools | 31 | 339 |\n");
        let once = splice_builtin_tools_docs(&doc).expect("first");
        let twice = splice_builtin_tools_docs(&once).expect("second");
        assert_eq!(once, twice, "splice must be idempotent");
    }
}
