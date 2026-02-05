//! Built-in tool registry with definitions for common formatters and linters.
//!
//! This module provides default configurations for popular tools like ruff, prettier,
//! shellcheck, etc. Users can override these in their configuration.

use super::config::ToolDefinition;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Registry of built-in tool definitions.
pub struct ToolRegistry {
    /// User-defined tools (override built-ins)
    user_tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    /// Create a new registry with user-defined tools.
    pub fn new(user_tools: HashMap<String, ToolDefinition>) -> Self {
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
        let mut tools: Vec<&str> = self.user_tools.keys().map(|s| s.as_str()).collect();
        for key in BUILTIN_TOOLS.keys() {
            if !self.user_tools.contains_key(*key) {
                tools.push(key);
            }
        }
        tools.sort();
        tools
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new(HashMap::new())
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

    // JavaScript/TypeScript - eslint (lint only)
    m.insert(
        "eslint",
        ToolDefinition {
            command: vec![
                "eslint".to_string(),
                "--stdin".to_string(),
                "--stdin-filename=_.js".to_string(),
            ],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec!["--fix-dry-run".to_string()],
        },
    );

    // Shell - shellcheck (lint only)
    m.insert(
        "shellcheck",
        ToolDefinition {
            command: vec!["shellcheck".to_string(), "-".to_string()],
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

    // JSON - jq (format)
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
            command: vec!["yamlfmt".to_string(), "-".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec!["-dry".to_string()],
            format_args: vec![],
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

    // TOML - tombi
    m.insert(
        "tombi",
        ToolDefinition {
            command: vec!["tombi".to_string(), "format".to_string(), "-".to_string()],
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

    m
});

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
    fn test_get_user_tool_overrides_builtin() {
        let mut user_tools = HashMap::new();
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
        let mut user_tools = HashMap::new();
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

        // oxfmt
        let tool = registry.get("oxfmt").expect("Should find oxfmt");
        assert!(tool.command.contains(&"oxfmt".to_string()));
        assert!(tool.stdin);

        let tool = registry.get("oxfmt:ts").expect("Should find oxfmt:ts");
        assert!(tool.command.iter().any(|s| s.contains("_.ts")));
    }
}
