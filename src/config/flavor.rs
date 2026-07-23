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
    /// Pandoc Markdown — fenced divs, attribute lists, citations, definition
    /// lists, math, and other Pandoc-specific syntax.
    #[serde(rename = "pandoc")]
    Pandoc,
    /// Quarto/RMarkdown flavor for scientific publishing (.qmd, .Rmd files)
    #[serde(rename = "quarto")]
    Quarto,
    /// Obsidian flavor with tag syntax support (#tagname as tags, not headings)
    #[serde(rename = "obsidian")]
    Obsidian,
    /// Kramdown flavor for Jekyll sites with IAL, ALD, and extension block support
    #[serde(rename = "kramdown")]
    Kramdown,
    /// Azure DevOps flavor — treats `:::lang` blocks as opaque code fences
    #[serde(rename = "azure_devops", alias = "azure", alias = "ado")]
    AzureDevOps,
    /// MyST (Markedly Structured Text) flavor — directives, roles, dollar math, % comments
    #[serde(rename = "myst", alias = "mystmd")]
    MyST,
    /// Hugo flavor — GFM plus Goldmark block attribute lists (`{class="a" id="b"}`)
    #[serde(rename = "hugo", alias = "goldmark")]
    Hugo,
}

/// Custom JSON schema for MarkdownFlavor that includes all accepted values and aliases
fn markdown_flavor_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "description": "Markdown flavor/dialect. Accepts: standard, gfm, mkdocs, mdx, pandoc, quarto, obsidian, kramdown, azure_devops, myst, hugo. Aliases: commonmark/github map to standard, qmd/rmd/rmarkdown map to quarto, jekyll maps to kramdown, azure/ado map to azure_devops, mystmd maps to myst, goldmark maps to hugo.",
        "type": "string",
        "enum": ["standard", "gfm", "github", "commonmark", "mkdocs", "mdx", "pandoc", "quarto", "qmd", "rmd", "rmarkdown", "obsidian", "kramdown", "jekyll", "azure_devops", "azure", "ado", "myst", "mystmd", "hugo", "goldmark"]
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
            MarkdownFlavor::Pandoc => write!(f, "pandoc"),
            MarkdownFlavor::Quarto => write!(f, "quarto"),
            MarkdownFlavor::Obsidian => write!(f, "obsidian"),
            MarkdownFlavor::Kramdown => write!(f, "kramdown"),
            MarkdownFlavor::AzureDevOps => write!(f, "azure_devops"),
            MarkdownFlavor::MyST => write!(f, "myst"),
            MarkdownFlavor::Hugo => write!(f, "hugo"),
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
            "pandoc" => Ok(MarkdownFlavor::Pandoc),
            "quarto" | "qmd" | "rmd" | "rmarkdown" => Ok(MarkdownFlavor::Quarto),
            "obsidian" => Ok(MarkdownFlavor::Obsidian),
            "kramdown" | "jekyll" => Ok(MarkdownFlavor::Kramdown),
            "azure_devops" | "azure" | "ado" => Ok(MarkdownFlavor::AzureDevOps),
            "myst" | "mystmd" => Ok(MarkdownFlavor::MyST),
            "hugo" | "goldmark" => Ok(MarkdownFlavor::Hugo),
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
            "kramdown" => Self::Kramdown,
            _ => Self::Standard,
        }
    }

    /// Detect flavor from file path
    pub fn from_path(path: &std::path::Path) -> Self {
        path.extension()
            .and_then(|e| e.to_str())
            .map_or(Self::Standard, Self::from_extension)
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

    /// Check if this flavor supports kramdown syntax (IALs, ALDs, extension blocks)
    pub fn supports_kramdown_syntax(self) -> bool {
        matches!(self, Self::Kramdown)
    }

    /// Check if this flavor supports attribute lists ({#id .class key="value"})
    pub fn supports_attr_lists(self) -> bool {
        matches!(self, Self::MkDocs | Self::Kramdown | Self::Hugo)
    }

    /// Check if this flavor requires strict (≥4-space) list continuation indent.
    ///
    /// Python-Markdown (used by MkDocs) requires 4-space indentation for ordered
    /// list continuation content, regardless of marker width.
    pub fn requires_strict_list_indent(self) -> bool {
        matches!(self, Self::MkDocs)
    }

    /// True for any flavor that includes Pandoc-style syntax — fenced divs,
    /// attribute lists, citations, definition lists, math, raw blocks.
    /// Use this to gate behavior shared by both Pandoc and Quarto users.
    pub fn is_pandoc_compatible(self) -> bool {
        matches!(self, Self::Pandoc | Self::Quarto)
    }

    /// Get a human-readable name for this flavor
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::MkDocs => "MkDocs",
            Self::MDX => "MDX",
            Self::Pandoc => "Pandoc",
            Self::Quarto => "Quarto",
            Self::Obsidian => "Obsidian",
            Self::Kramdown => "Kramdown",
            Self::AzureDevOps => "AzureDevOps",
            Self::MyST => "MyST",
            Self::Hugo => "Hugo",
        }
    }

    /// True only for Azure DevOps flavor, which uses `:::lang` as a code fence.
    pub fn supports_colon_code_fences(self) -> bool {
        matches!(self, Self::AzureDevOps)
    }

    /// True for MyST flavor — supports directive syntax (backtick and colon fences with `{name}`)
    pub fn supports_myst_directives(self) -> bool {
        matches!(self, Self::MyST)
    }

    /// True for MyST flavor — supports role syntax (`{role}`content``)
    pub fn supports_myst_roles(self) -> bool {
        matches!(self, Self::MyST)
    }

    /// True for MyST flavor — supports `%` line comments
    pub fn supports_myst_comments(self) -> bool {
        matches!(self, Self::MyST)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every MarkdownFlavor variant must produce a lowercase, unquoted string via Display.
    /// This guards against new variants being added without a matching Display arm,
    /// and against the Display impl regressing to Debug-style output (e.g. "Standard").
    #[test]
    fn test_display_all_variants_are_lowercase() {
        let cases = [
            (MarkdownFlavor::Standard, "standard"),
            (MarkdownFlavor::MkDocs, "mkdocs"),
            (MarkdownFlavor::MDX, "mdx"),
            (MarkdownFlavor::Pandoc, "pandoc"),
            (MarkdownFlavor::Quarto, "quarto"),
            (MarkdownFlavor::Obsidian, "obsidian"),
            (MarkdownFlavor::Kramdown, "kramdown"),
            (MarkdownFlavor::AzureDevOps, "azure_devops"),
            (MarkdownFlavor::MyST, "myst"),
            (MarkdownFlavor::Hugo, "hugo"),
        ];
        for (variant, expected) in cases {
            let displayed = variant.to_string();
            assert_eq!(
                displayed, expected,
                "MarkdownFlavor::{variant:?} Display should produce \"{expected}\", got \"{displayed}\""
            );
            // Must be lowercase — no uppercase letters anywhere
            assert!(
                displayed.chars().all(|c| !c.is_ascii_uppercase()),
                "MarkdownFlavor::{variant:?} Display must be entirely lowercase, got \"{displayed}\""
            );
        }
    }

    /// Display output must round-trip through FromStr — every variant's Display string
    /// must parse back to the same variant.
    #[test]
    fn test_display_round_trips_through_from_str() {
        let variants = [
            MarkdownFlavor::Standard,
            MarkdownFlavor::MkDocs,
            MarkdownFlavor::MDX,
            MarkdownFlavor::Pandoc,
            MarkdownFlavor::Quarto,
            MarkdownFlavor::Obsidian,
            MarkdownFlavor::Kramdown,
            MarkdownFlavor::AzureDevOps,
            MarkdownFlavor::MyST,
            MarkdownFlavor::Hugo,
        ];
        for variant in variants {
            let displayed = variant.to_string();
            let parsed: MarkdownFlavor = displayed
                .parse()
                .unwrap_or_else(|e| panic!("Display string \"{displayed}\" for {variant:?} failed to parse back: {e}"));
            assert_eq!(
                parsed, variant,
                "Display(\"{displayed}\") for {variant:?} round-trips to a different variant: {parsed:?}"
            );
        }
    }

    #[test]
    fn test_pandoc_from_str() {
        assert_eq!("pandoc".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::Pandoc);
        assert_eq!("PANDOC".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::Pandoc);
    }

    #[test]
    fn test_pandoc_name_and_display() {
        assert_eq!(MarkdownFlavor::Pandoc.name(), "Pandoc");
        assert_eq!(MarkdownFlavor::Pandoc.to_string(), "pandoc");
    }

    #[test]
    fn test_from_extension_does_not_auto_detect_pandoc() {
        // Pandoc files use .md — must NOT auto-detect to Pandoc.
        assert_eq!(MarkdownFlavor::from_extension("md"), MarkdownFlavor::Standard);
        assert_eq!(MarkdownFlavor::from_extension("markdown"), MarkdownFlavor::Standard);
    }

    #[test]
    fn test_is_pandoc_compatible() {
        assert!(MarkdownFlavor::Pandoc.is_pandoc_compatible());
        assert!(MarkdownFlavor::Quarto.is_pandoc_compatible());

        assert!(!MarkdownFlavor::Standard.is_pandoc_compatible());
        assert!(!MarkdownFlavor::MkDocs.is_pandoc_compatible());
        assert!(!MarkdownFlavor::MDX.is_pandoc_compatible());
        assert!(!MarkdownFlavor::Obsidian.is_pandoc_compatible());
        assert!(!MarkdownFlavor::Kramdown.is_pandoc_compatible());
    }

    #[test]
    fn test_azure_devops_from_str() {
        assert_eq!(
            "azure_devops".parse::<MarkdownFlavor>().unwrap(),
            MarkdownFlavor::AzureDevOps
        );
        assert_eq!("azure".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::AzureDevOps);
        assert_eq!("ado".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::AzureDevOps);
        assert_eq!(
            "AZURE_DEVOPS".parse::<MarkdownFlavor>().unwrap(),
            MarkdownFlavor::AzureDevOps
        );
    }

    #[test]
    fn test_azure_devops_display_and_round_trip() {
        assert_eq!(MarkdownFlavor::AzureDevOps.to_string(), "azure_devops");
        let parsed: MarkdownFlavor = "azure_devops".parse().unwrap();
        assert_eq!(parsed, MarkdownFlavor::AzureDevOps);
    }

    #[test]
    fn test_supports_colon_code_fences() {
        assert!(MarkdownFlavor::AzureDevOps.supports_colon_code_fences());
        assert!(!MarkdownFlavor::Standard.supports_colon_code_fences());
        assert!(!MarkdownFlavor::MkDocs.supports_colon_code_fences());
        assert!(!MarkdownFlavor::MDX.supports_colon_code_fences());
        assert!(!MarkdownFlavor::Pandoc.supports_colon_code_fences());
        assert!(!MarkdownFlavor::Quarto.supports_colon_code_fences());
        assert!(!MarkdownFlavor::Obsidian.supports_colon_code_fences());
        assert!(!MarkdownFlavor::Kramdown.supports_colon_code_fences());
        assert!(!MarkdownFlavor::MyST.supports_colon_code_fences());
    }

    #[test]
    fn test_azure_devops_not_pandoc_compatible() {
        assert!(!MarkdownFlavor::AzureDevOps.is_pandoc_compatible());
    }

    #[test]
    fn test_display_all_variants_covers_azure_devops() {
        let displayed = MarkdownFlavor::AzureDevOps.to_string();
        assert!(displayed.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn test_myst_from_str() {
        assert_eq!("myst".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::MyST);
        assert_eq!("MYST".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::MyST);
        assert_eq!("mystmd".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::MyST);
    }

    #[test]
    fn test_myst_display_and_round_trip() {
        assert_eq!(MarkdownFlavor::MyST.to_string(), "myst");
        let parsed: MarkdownFlavor = "myst".parse().unwrap();
        assert_eq!(parsed, MarkdownFlavor::MyST);
    }

    #[test]
    fn test_myst_capabilities() {
        assert!(MarkdownFlavor::MyST.supports_myst_directives());
        assert!(MarkdownFlavor::MyST.supports_myst_roles());
        assert!(MarkdownFlavor::MyST.supports_myst_comments());
        assert!(!MarkdownFlavor::MyST.is_pandoc_compatible());
        assert!(!MarkdownFlavor::MyST.supports_colon_code_fences());
        assert!(!MarkdownFlavor::MyST.supports_jsx());

        assert!(!MarkdownFlavor::Standard.supports_myst_directives());
        assert!(!MarkdownFlavor::Standard.supports_myst_roles());
        assert!(!MarkdownFlavor::Standard.supports_myst_comments());
    }

    #[test]
    fn test_myst_name() {
        assert_eq!(MarkdownFlavor::MyST.name(), "MyST");
    }

    #[test]
    fn test_hugo_from_str() {
        assert_eq!("hugo".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::Hugo);
        assert_eq!("HUGO".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::Hugo);
        assert_eq!("goldmark".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::Hugo);
    }

    #[test]
    fn test_hugo_display_name_and_round_trip() {
        assert_eq!(MarkdownFlavor::Hugo.to_string(), "hugo");
        assert_eq!(MarkdownFlavor::Hugo.name(), "Hugo");
        let parsed: MarkdownFlavor = "hugo".parse().unwrap();
        assert_eq!(parsed, MarkdownFlavor::Hugo);
    }

    #[test]
    fn test_hugo_supports_attr_lists() {
        // Hugo/Goldmark supports block attribute lists like MkDocs and Kramdown.
        assert!(MarkdownFlavor::Hugo.supports_attr_lists());
        assert!(MarkdownFlavor::MkDocs.supports_attr_lists());
        assert!(MarkdownFlavor::Kramdown.supports_attr_lists());

        // Standard Markdown does not: there, `{class="a"}` is literal text.
        assert!(!MarkdownFlavor::Standard.supports_attr_lists());
        assert!(!MarkdownFlavor::Pandoc.supports_attr_lists());

        // Hugo is otherwise plain GFM.
        assert!(!MarkdownFlavor::Hugo.is_pandoc_compatible());
        assert!(!MarkdownFlavor::Hugo.supports_myst_directives());
        assert!(!MarkdownFlavor::Hugo.supports_colon_code_fences());
    }
}
