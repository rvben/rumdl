//! Language alias resolution using GitHub Linguist data.
//!
//! This module provides mapping from language aliases (e.g., "py", "bash")
//! to canonical language names (e.g., "python", "shell") for consistent
//! tool configuration lookup.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Resolver for language aliases to canonical names.
pub struct LinguistResolver {
    /// Map from alias -> canonical name
    alias_map: &'static HashMap<&'static str, &'static str>,
}

impl LinguistResolver {
    /// Create a new resolver using embedded Linguist data.
    pub fn new() -> Self {
        Self {
            alias_map: &LANGUAGE_ALIASES,
        }
    }

    /// Resolve a language tag to its canonical name.
    ///
    /// Returns the canonical name if the input is a known alias,
    /// otherwise returns the input lowercased.
    pub fn resolve(&self, language: &str) -> String {
        let lower = language.to_lowercase();
        self.alias_map
            .get(lower.as_str())
            .map(|&s| s.to_string())
            .unwrap_or(lower)
    }

    /// Check if a language (or alias) is known.
    pub fn is_known(&self, language: &str) -> bool {
        let lower = language.to_lowercase();
        self.alias_map.contains_key(lower.as_str())
    }
}

impl Default for LinguistResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Embedded language alias map.
///
/// Maps aliases and canonical names to canonical names.
/// Curated subset inspired by GitHub Linguist languages.yml.
///
/// The map includes:
/// - Canonical name -> canonical name (identity)
/// - Alias -> canonical name
/// - Extension (without dot) -> canonical name (for common extensions)
static LANGUAGE_ALIASES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Python
    m.insert("python", "python");
    m.insert("py", "python");
    m.insert("python3", "python");
    m.insert("py3", "python");
    m.insert("pyw", "python");

    // JavaScript
    m.insert("javascript", "javascript");
    m.insert("js", "javascript");
    m.insert("node", "javascript");
    m.insert("nodejs", "javascript");
    m.insert("mjs", "javascript");
    m.insert("cjs", "javascript");

    // TypeScript
    m.insert("typescript", "typescript");
    m.insert("ts", "typescript");
    m.insert("mts", "typescript");
    m.insert("cts", "typescript");

    // Shell/Bash
    m.insert("shell", "shell");
    m.insert("bash", "shell");
    m.insert("sh", "shell");
    m.insert("zsh", "shell");
    m.insert("ksh", "shell");
    m.insert("fish", "shell");
    m.insert("shellscript", "shell");
    m.insert("shell-script", "shell");

    // Rust
    m.insert("rust", "rust");
    m.insert("rs", "rust");

    // Go
    m.insert("go", "go");
    m.insert("golang", "go");

    // Ruby
    m.insert("ruby", "ruby");
    m.insert("rb", "ruby");
    m.insert("jruby", "ruby");

    // Java
    m.insert("java", "java");

    // Kotlin
    m.insert("kotlin", "kotlin");
    m.insert("kt", "kotlin");
    m.insert("kts", "kotlin");

    // Scala
    m.insert("scala", "scala");

    // C
    m.insert("c", "c");
    m.insert("h", "c");

    // C++
    m.insert("c++", "cpp");
    m.insert("cpp", "cpp");
    m.insert("cxx", "cpp");
    m.insert("cc", "cpp");
    m.insert("hpp", "cpp");
    m.insert("hxx", "cpp");

    // C#
    m.insert("c#", "csharp");
    m.insert("csharp", "csharp");
    m.insert("cs", "csharp");

    // F#
    m.insert("f#", "fsharp");
    m.insert("fsharp", "fsharp");
    m.insert("fs", "fsharp");

    // Swift
    m.insert("swift", "swift");

    // Objective-C
    m.insert("objective-c", "objective-c");
    m.insert("objc", "objective-c");
    m.insert("obj-c", "objective-c");

    // PHP
    m.insert("php", "php");

    // Perl
    m.insert("perl", "perl");
    m.insert("pl", "perl");

    // R
    m.insert("r", "r");

    // Lua
    m.insert("lua", "lua");

    // Haskell
    m.insert("haskell", "haskell");
    m.insert("hs", "haskell");

    // Elixir
    m.insert("elixir", "elixir");
    m.insert("ex", "elixir");
    m.insert("exs", "elixir");

    // Erlang
    m.insert("erlang", "erlang");
    m.insert("erl", "erlang");

    // Clojure
    m.insert("clojure", "clojure");
    m.insert("clj", "clojure");
    m.insert("cljs", "clojure");
    m.insert("cljc", "clojure");

    // HTML
    m.insert("html", "html");
    m.insert("htm", "html");
    m.insert("xhtml", "html");

    // CSS
    m.insert("css", "css");

    // SCSS/Sass
    m.insert("scss", "scss");
    m.insert("sass", "sass");

    // Less
    m.insert("less", "less");

    // JSON
    m.insert("json", "json");
    m.insert("jsonc", "json");
    m.insert("json5", "json");

    // YAML
    m.insert("yaml", "yaml");
    m.insert("yml", "yaml");

    // TOML
    m.insert("toml", "toml");

    // XML
    m.insert("xml", "xml");
    m.insert("xsd", "xml");
    m.insert("xsl", "xml");
    m.insert("xslt", "xml");

    // Markdown
    m.insert("markdown", "markdown");
    m.insert("md", "markdown");
    m.insert("mkd", "markdown");
    m.insert("mdx", "markdown");

    // SQL
    m.insert("sql", "sql");
    m.insert("mysql", "sql");
    m.insert("postgresql", "sql");
    m.insert("postgres", "sql");
    m.insert("sqlite", "sql");
    m.insert("plsql", "sql");
    m.insert("tsql", "sql");

    // GraphQL
    m.insert("graphql", "graphql");
    m.insert("gql", "graphql");

    // Protocol Buffers
    m.insert("protobuf", "protobuf");
    m.insert("proto", "protobuf");

    // Terraform/HCL
    m.insert("terraform", "terraform");
    m.insert("tf", "terraform");
    m.insert("hcl", "hcl");

    // Dockerfile
    m.insert("dockerfile", "dockerfile");
    m.insert("docker", "dockerfile");

    // Makefile
    m.insert("makefile", "makefile");
    m.insert("make", "makefile");

    // Nix
    m.insert("nix", "nix");

    // Vim script
    m.insert("vim", "vim");
    m.insert("viml", "vim");
    m.insert("vimscript", "vim");

    // Zig
    m.insert("zig", "zig");

    // Nim
    m.insert("nim", "nim");

    // Julia
    m.insert("julia", "julia");
    m.insert("jl", "julia");

    // OCaml
    m.insert("ocaml", "ocaml");
    m.insert("ml", "ocaml");

    // ReasonML
    m.insert("reason", "reason");
    m.insert("re", "reason");

    // Dart
    m.insert("dart", "dart");

    // V
    m.insert("v", "v");
    m.insert("vlang", "v");

    // Awk
    m.insert("awk", "awk");
    m.insert("gawk", "awk");

    // Sed
    m.insert("sed", "sed");

    // PowerShell
    m.insert("powershell", "powershell");
    m.insert("pwsh", "powershell");
    m.insert("ps1", "powershell");

    // Batch
    m.insert("batch", "batch");
    m.insert("bat", "batch");
    m.insert("cmd", "batch");

    // Diff
    m.insert("diff", "diff");
    m.insert("patch", "diff");

    // INI
    m.insert("ini", "ini");
    m.insert("cfg", "ini");
    m.insert("conf", "ini");

    // AppleScript
    m.insert("applescript", "applescript");

    // Groovy
    m.insert("groovy", "groovy");

    // LaTeX
    m.insert("latex", "latex");
    m.insert("tex", "latex");

    // Plain text
    m.insert("text", "text");
    m.insert("txt", "text");
    m.insert("plaintext", "text");
    m.insert("plain", "text");

    m
});

#[cfg(test)]
mod bridge_to_main_linguist_table {
    use super::LANGUAGE_ALIASES;
    use std::collections::BTreeMap;

    /// Fence tags this table deliberately answers differently from rumdl's main Linguist
    /// table (`crate::linguist_data`), as `alias -> (this table, the main table)`.
    ///
    /// Three reasons, and no fourth: a canonical name here is a TOML key under
    /// `[code-block-tools.languages.*]`, so it avoids punctuation and spaces; and a
    /// language whose blocks are handled by another language's tools is named after that
    /// language, since the name exists to find a tool rather than to identify a grammar.
    ///
    /// Anything not listed here is drift between the two tables, which surfaces as a
    /// language whose configured tools quietly stop being found for a fence tag.
    const DELIBERATE_DIVERGENCES: &[(&str, &str, &str)] = &[
        // Punctuation a bare TOML key cannot hold.
        ("c#", "csharp", "c#"),
        ("csharp", "csharp", "c#"),
        ("c++", "cpp", "c++"),
        ("cpp", "cpp", "c++"),
        ("f#", "fsharp", "f#"),
        ("fsharp", "fsharp", "f#"),
        // Spaces a bare TOML key cannot hold.
        ("bat", "batch", "batchfile"),
        ("batch", "batch", "batchfile"),
        ("proto", "protobuf", "protocol buffer"),
        ("protobuf", "protobuf", "protocol buffer"),
        ("vim", "vim", "vim script"),
        ("viml", "vim", "vim script"),
        ("vimscript", "vim", "vim script"),
        // One language's tools serve another's blocks.
        ("json5", "json", "json5"),
        ("plsql", "sql", "plsql"),
        ("tsql", "sql", "tsql"),
        ("mdx", "markdown", "mdx"),
        ("xsl", "xml", "xslt"),
        ("xslt", "xml", "xslt"),
        ("latex", "latex", "tex"),
        ("tex", "latex", "tex"),
        // `terraform fmt` is the tool users reach for, and it is what the built-in runs.
        ("terraform", "terraform", "hcl"),
    ];

    fn deliberate() -> BTreeMap<&'static str, (&'static str, &'static str)> {
        DELIBERATE_DIVERGENCES
            .iter()
            .map(|(alias, ours, theirs)| (*alias, (*ours, *theirs)))
            .collect()
    }

    /// Every alias both tables know resolves to the same language, or is listed above.
    #[test]
    fn a_divergence_from_the_main_linguist_table_is_deliberate() {
        let deliberate = deliberate();
        let mut unlisted = Vec::new();

        for (alias, ours) in LANGUAGE_ALIASES.iter() {
            let Some(theirs) = crate::linguist_data::resolve_canonical(alias) else {
                continue;
            };
            let theirs = theirs.to_lowercase();
            if *ours == theirs {
                continue;
            }
            match deliberate.get(alias) {
                Some((listed_ours, listed_theirs)) if *listed_ours == *ours && *listed_theirs == theirs => {}
                _ => unlisted.push(format!("(\"{alias}\", \"{ours}\", \"{theirs}\")")),
            }
        }

        unlisted.sort();
        assert!(
            unlisted.is_empty(),
            "these fence tags resolve to different languages in the two tables; add them to \
             DELIBERATE_DIVERGENCES with a reason, or make the tables agree:\n{}",
            unlisted.join("\n")
        );
    }

    /// Every listed divergence is still one. A tie-break the main table has since adopted
    /// is a line to delete, and an alias one of the tables has dropped is a real change
    /// hiding behind an entry that reads as still current.
    #[test]
    fn a_listed_divergence_has_not_gone_stale() {
        let mut stale = Vec::new();

        for (alias, ours, theirs) in DELIBERATE_DIVERGENCES {
            let Some(actual_ours) = LANGUAGE_ALIASES.get(alias) else {
                stale.push(format!("{alias}: this table no longer knows it"));
                continue;
            };
            let Some(actual_theirs) = crate::linguist_data::resolve_canonical(alias) else {
                stale.push(format!("{alias}: the main table no longer knows it"));
                continue;
            };
            let actual_theirs = actual_theirs.to_lowercase();
            if *actual_ours != *ours || actual_theirs != *theirs {
                stale.push(format!(
                    "{alias}: listed as {ours}/{theirs}, actually {actual_ours}/{actual_theirs}"
                ));
            }
        }

        assert!(
            stale.is_empty(),
            "DELIBERATE_DIVERGENCES is out of date:\n{}",
            stale.join("\n")
        );
    }

    /// Every language this table resolves TO is one rumdl's main table knows.
    ///
    /// The key side is deliberately wider than the main table's alias list: it also holds
    /// bare extensions (`kt`, `mjs`, `hpp`) and editor language ids (`shellscript`) that
    /// people write in fences. The value side has no such licence - it is the name a user
    /// puts in `[code-block-tools.languages.*]`, and inventing one there gives a language
    /// rumdl recognizes nowhere else.
    #[test]
    fn every_language_this_table_resolves_to_is_one_rumdl_knows() {
        let mut unknown: Vec<&str> = LANGUAGE_ALIASES
            .values()
            .filter(|canonical| crate::linguist_data::resolve_canonical(canonical).is_none())
            .copied()
            .collect();
        unknown.sort_unstable();
        unknown.dedup();
        assert!(
            unknown.is_empty(),
            "languages known only to code-block-tools: {}",
            unknown.join(", ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_known_alias() {
        let resolver = LinguistResolver::new();

        // Python aliases
        assert_eq!(resolver.resolve("py"), "python");
        assert_eq!(resolver.resolve("python3"), "python");
        assert_eq!(resolver.resolve("Python"), "python");
        assert_eq!(resolver.resolve("PY"), "python");

        // Shell aliases
        assert_eq!(resolver.resolve("bash"), "shell");
        assert_eq!(resolver.resolve("sh"), "shell");
        assert_eq!(resolver.resolve("zsh"), "shell");

        // JavaScript aliases
        assert_eq!(resolver.resolve("js"), "javascript");
        assert_eq!(resolver.resolve("node"), "javascript");

        // Rust
        assert_eq!(resolver.resolve("rs"), "rust");
        assert_eq!(resolver.resolve("Rust"), "rust");
    }

    #[test]
    fn test_resolve_unknown_language() {
        let resolver = LinguistResolver::new();

        // Unknown languages are returned lowercased
        assert_eq!(resolver.resolve("UnknownLang"), "unknownlang");
        assert_eq!(resolver.resolve("CUSTOM"), "custom");
    }

    #[test]
    fn test_resolve_canonical_name() {
        let resolver = LinguistResolver::new();

        // Canonical names resolve to themselves
        assert_eq!(resolver.resolve("python"), "python");
        assert_eq!(resolver.resolve("javascript"), "javascript");
        assert_eq!(resolver.resolve("rust"), "rust");
    }

    #[test]
    fn test_is_known() {
        let resolver = LinguistResolver::new();

        assert!(resolver.is_known("python"));
        assert!(resolver.is_known("py"));
        assert!(resolver.is_known("bash"));
        assert!(resolver.is_known("JavaScript"));

        assert!(!resolver.is_known("unknownlang"));
        assert!(!resolver.is_known("customformat"));
    }

    #[test]
    fn test_case_insensitivity() {
        let resolver = LinguistResolver::new();

        assert_eq!(resolver.resolve("PYTHON"), "python");
        assert_eq!(resolver.resolve("Python"), "python");
        assert_eq!(resolver.resolve("pYtHoN"), "python");
        assert_eq!(resolver.resolve("JAVASCRIPT"), "javascript");
        assert_eq!(resolver.resolve("JavaScript"), "javascript");
    }
}
