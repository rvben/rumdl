use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// ============================================================================
// Typestate markers for configuration pipeline
// ============================================================================

/// Marker type for configuration that has been loaded but not yet validated.
/// This is the initial state after `load_with_discovery()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigLoaded;

/// Marker type for configuration that has been validated.
/// Only validated configs can be converted to `Config`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigValidated;

/// Markdown flavor/dialect enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MarkdownFlavor {
    /// Standard Markdown without flavor-specific adjustments
    #[serde(rename = "standard", alias = "none", alias = "")]
    #[default]
    Standard,
    /// MkDocs flavor with auto-reference support
    #[serde(rename = "mkdocs")]
    MkDocs,
    /// MDX flavor with JSX and ESM support (.mdx files)
    #[serde(rename = "mdx")]
    MDX,
    /// Quarto/RMarkdown flavor for scientific publishing (.qmd, .Rmd files)
    #[serde(rename = "quarto")]
    Quarto,
    /// Obsidian flavor with tag syntax support (#tagname as tags, not headings)
    #[serde(rename = "obsidian")]
    Obsidian,
}

/// Custom JSON schema for MarkdownFlavor that includes all accepted values and aliases
fn markdown_flavor_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "description": "Markdown flavor/dialect. Accepts: standard, gfm, mkdocs, mdx, quarto, obsidian. Aliases: commonmark/github map to standard, qmd/rmd/rmarkdown map to quarto.",
        "type": "string",
        "enum": ["standard", "gfm", "github", "commonmark", "mkdocs", "mdx", "quarto", "qmd", "rmd", "rmarkdown", "obsidian"]
    })
}

impl schemars::JsonSchema for MarkdownFlavor {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("MarkdownFlavor")
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        markdown_flavor_schema(generator)
    }
}

impl fmt::Display for MarkdownFlavor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarkdownFlavor::Standard => write!(f, "standard"),
            MarkdownFlavor::MkDocs => write!(f, "mkdocs"),
            MarkdownFlavor::MDX => write!(f, "mdx"),
            MarkdownFlavor::Quarto => write!(f, "quarto"),
            MarkdownFlavor::Obsidian => write!(f, "obsidian"),
        }
    }
}

impl FromStr for MarkdownFlavor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standard" | "" | "none" => Ok(MarkdownFlavor::Standard),
            "mkdocs" => Ok(MarkdownFlavor::MkDocs),
            "mdx" => Ok(MarkdownFlavor::MDX),
            "quarto" | "qmd" | "rmd" | "rmarkdown" => Ok(MarkdownFlavor::Quarto),
            "obsidian" => Ok(MarkdownFlavor::Obsidian),
            // GFM and CommonMark are aliases for Standard since the base parser
            // (pulldown-cmark) already supports GFM extensions (tables, task lists,
            // strikethrough, autolinks, etc.) which are a superset of CommonMark
            "gfm" | "github" | "commonmark" => Ok(MarkdownFlavor::Standard),
            _ => Err(format!("Unknown markdown flavor: {s}")),
        }
    }
}

impl MarkdownFlavor {
    /// Detect flavor from file extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "mdx" => Self::MDX,
            "qmd" => Self::Quarto,
            "rmd" => Self::Quarto,
            _ => Self::Standard,
        }
    }

    /// Detect flavor from file path
    pub fn from_path(path: &std::path::Path) -> Self {
        path.extension()
            .and_then(|e| e.to_str())
            .map(Self::from_extension)
            .unwrap_or(Self::Standard)
    }

    /// Check if this flavor supports ESM imports/exports (MDX-specific)
    pub fn supports_esm_blocks(self) -> bool {
        matches!(self, Self::MDX)
    }

    /// Check if this flavor supports JSX components (MDX-specific)
    pub fn supports_jsx(self) -> bool {
        matches!(self, Self::MDX)
    }

    /// Check if this flavor supports auto-references (MkDocs-specific)
    pub fn supports_auto_references(self) -> bool {
        matches!(self, Self::MkDocs)
    }

    /// Get a human-readable name for this flavor
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::MkDocs => "MkDocs",
            Self::MDX => "MDX",
            Self::Quarto => "Quarto",
            Self::Obsidian => "Obsidian",
        }
    }
}

/// Normalizes configuration keys (rule names, option names) to lowercase kebab-case.
pub fn normalize_key(key: &str) -> String {
    // If the key looks like a rule name (e.g., MD013), uppercase it
    if key.len() == 5 && key.to_ascii_lowercase().starts_with("md") && key[2..].chars().all(|c| c.is_ascii_digit()) {
        key.to_ascii_uppercase()
    } else {
        key.replace('_', "-").to_ascii_lowercase()
    }
}

/// Warns if a per-file-ignores pattern contains a comma but no braces.
/// This is a common mistake where users expect "A.md,B.md" to match both files,
/// but glob syntax requires "{A.md,B.md}" for brace expansion.
pub(super) fn warn_comma_without_brace_in_pattern(pattern: &str, config_file: &str) {
    if pattern.contains(',') && !pattern.contains('{') {
        eprintln!("Warning: Pattern \"{pattern}\" in {config_file} contains a comma but no braces.");
        eprintln!("  To match multiple files, use brace expansion: \"{{{pattern}}}\"");
        eprintln!("  Or use separate entries for each file.");
    }
}
