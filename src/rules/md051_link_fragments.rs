use crate::rule::{CrossFileScope, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::anchor_styles::AnchorStyle;
use crate::utils::frontmatter_values;
use crate::utils::range_utils::byte_to_char_count;
use crate::workspace_index::{CrossFileLinkIndex, FileIndex, HeadingIndex};
use pulldown_cmark::LinkType;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

/// Configuration for MD051 (Link fragments)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD051Config {
    /// Anchor generation style to match the target platform
    #[serde(default, alias = "anchor_style")]
    pub anchor_style: AnchorStyle,

    /// Match link fragments against headings case-insensitively.
    ///
    /// rumdl defaults to `true` (permissive matching), which deviates from
    /// markdownlint's default of `false`. Set this to `false` for strict
    /// markdownlint parity.
    #[serde(default = "default_ignore_case", alias = "ignore_case")]
    pub ignore_case: bool,

    /// Optional regex applied to the fragment text (without the leading `#`).
    /// Fragments that match are skipped — useful for runtime-generated anchors
    /// (e.g., footnote IDs) that aren't visible to the linter.
    #[serde(default, alias = "ignored_pattern")]
    pub ignored_pattern: Option<String>,

    /// Also check fragments in path-shaped frontmatter values.
    ///
    /// Off by default, because frontmatter has no syntax marking a value as a
    /// link: a path-shaped value is only a guess at one. Static-site generators
    /// also resolve frontmatter paths from the site root rather than the
    /// document's own directory, so checking them like body links reports
    /// working paths as broken.
    ///
    /// Enable it for projects whose frontmatter links really are relative to
    /// the document, and use `ignore-frontmatter-fields` for the keys that are
    /// not.
    ///
    /// Example:
    /// ```toml
    /// [MD051]
    /// check-frontmatter = true
    /// ignore-frontmatter-fields = ["image", "cover"]
    /// ```
    #[serde(default)]
    pub check_frontmatter: bool,

    /// Top-level frontmatter keys whose values are not checked. Matched
    /// case-insensitively. A parent key excludes its whole subtree. Applies
    /// only when `check-frontmatter` is enabled.
    #[serde(default)]
    pub ignore_frontmatter_fields: Vec<String>,
}

fn default_ignore_case() -> bool {
    true
}

impl Default for MD051Config {
    fn default() -> Self {
        Self {
            anchor_style: AnchorStyle::default(),
            ignore_case: true,
            ignored_pattern: None,
            check_frontmatter: false,
            ignore_frontmatter_fields: Vec::new(),
        }
    }
}

impl RuleConfig for MD051Config {
    const RULE_NAME: &'static str = "MD051";
}
// HTML tags with id or name attributes (supports any HTML element, not just <a>)
// This pattern only captures the first id/name attribute in a tag
static HTML_ANCHOR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b(?:id|name)\s*=\s*["']([^"']+)["']"#).unwrap());

// Attribute anchor pattern for kramdown/MkDocs { #id } syntax
// Matches {#id} or { #id } with optional spaces, supports multiple anchors
// Also supports classes and attributes: { #id .class key=value }
static ATTR_ANCHOR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\{\s*#([a-zA-Z0-9_][a-zA-Z0-9_-]*)[^}]*\}"#).unwrap());

// Material for MkDocs setting anchor pattern: <!-- md:setting NAME -->
// Used in headings to generate anchors for configuration option references
static MD_SETTING_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!--\s*md:setting\s+([^\s]+)\s*-->").unwrap());

/// Normalize a path by resolving . and .. components
fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {} // Skip .
            Component::ParentDir => {
                result.pop(); // Go up one level for ..
            }
            c => result.push(c.as_os_str()),
        }
    }
    result
}

/// Rule MD051: Link fragments
///
/// See [docs/md051.md](../../docs/md051.md) for full documentation, configuration, and examples.
///
/// This rule validates that link anchors (the part after #) point to existing headings.
/// Supports both same-document anchors and cross-file fragment links when linting a workspace.
#[derive(Clone)]
pub struct MD051LinkFragments {
    config: MD051Config,
    /// Pre-compiled `ignored_pattern` regex. `None` if the user did not set the
    /// option, or if the pattern failed to compile (a `log::warn!` is emitted
    /// once at construction time so the user can fix the config).
    ignored_pattern_regex: Option<Regex>,
    /// `ignore_frontmatter_fields` lowercased for case-insensitive matching.
    ignored_front_matter_fields: HashSet<String>,
    /// Whether `config.anchor_style` was chosen rather than derived from a
    /// flavor. Unpinned, the style follows the flavor of the file being
    /// checked; see [`MD051LinkFragments::anchor_style`].
    anchor_style_pinned: bool,
}

/// Anchor sets extracted from a single document, with parallel lowercase and
/// case-preserving storage. The `*_exact` sets are empty unless
/// `ignore_case = false` so the default permissive path costs no extra
/// allocations.
struct AnchorSets {
    markdown_headings: HashSet<String>,
    markdown_headings_exact: HashSet<String>,
    html_anchors: HashSet<String>,
    html_anchors_exact: HashSet<String>,
}

impl Default for MD051LinkFragments {
    fn default() -> Self {
        Self::new()
    }
}

impl MD051LinkFragments {
    pub fn new() -> Self {
        Self::from_config_struct(MD051Config::default())
    }

    /// Create with specific anchor style (other options use defaults)
    pub fn with_anchor_style(style: AnchorStyle) -> Self {
        Self::from_config_struct(MD051Config {
            anchor_style: style,
            ..MD051Config::default()
        })
    }

    /// Create from a fully-populated config struct.
    ///
    /// Compiles `ignored_pattern` once. An invalid regex is logged via
    /// `log::warn!` and the rule falls back to "no filter" so linting still
    /// works rather than silently swallowing every fragment.
    pub fn from_config_struct(config: MD051Config) -> Self {
        Self::from_config_struct_from(config, false)
    }

    /// [`Self::from_config_struct`], told whether a message about `ignored_pattern`
    /// may quote it. See [`crate::rule_config_serde::compile_config_regex`].
    fn from_config_struct_from(config: MD051Config, values_withheld: bool) -> Self {
        Self::build(config, values_withheld, true)
    }

    /// The shared constructor. `anchor_style_pinned` is false only when the
    /// style in `config` was derived from a flavor rather than chosen by the
    /// user, which is what lets [`Self::anchor_style`] re-derive it per file.
    fn build(config: MD051Config, values_withheld: bool, anchor_style_pinned: bool) -> Self {
        let ignored_pattern_regex = config.ignored_pattern.as_deref().and_then(|pattern| {
            crate::rule_config_serde::compile_config_regex(pattern, "MD051", "ignored-pattern", values_withheld)
        });
        let ignored_front_matter_fields = config
            .ignore_frontmatter_fields
            .iter()
            .map(|field| field.to_lowercase())
            .collect();
        Self {
            config,
            ignored_pattern_regex,
            ignored_front_matter_fields,
            anchor_style_pinned,
        }
    }

    /// The anchor style to generate fragments with for the file in `ctx`.
    ///
    /// A style the user pinned applies to every file. Otherwise it follows the
    /// flavor the file is parsed with, which `per-file-flavor` can make
    /// different from the global flavor the rule was constructed with, so it
    /// cannot be settled until a file is in hand.
    fn anchor_style(&self, ctx: &crate::lint_context::LintContext) -> AnchorStyle {
        if self.anchor_style_pinned {
            self.config.anchor_style
        } else {
            AnchorStyle::for_flavor(ctx.flavor)
        }
    }

    /// Parse ATX heading content from blockquote inner text.
    /// Strips the leading `# ` marker, optional closing hash sequence, and extracts custom IDs.
    /// Returns `(clean_text, custom_id)` or None if not a heading.
    fn parse_blockquote_heading(bq_content: &str) -> Option<(String, Option<String>)> {
        crate::utils::header_id_utils::parse_blockquote_atx_heading(bq_content)
    }

    /// Insert a heading fragment with deduplication.
    /// When `use_underscore_dedup` is true (Python-Markdown/MkDocs), the primary suffix
    /// uses `_N` and `-N` is registered as a fallback. Otherwise, only `-N` is used.
    ///
    /// Empty fragments (from CJK-only headings) are handled specially for Python-Markdown:
    /// the first empty slug gets `_1`, the second `_2`, etc. (matching Python-Markdown's
    /// `unique()` function which always enters the dedup loop for falsy IDs).
    fn insert_deduplicated_fragment(
        fragment: String,
        fragment_counts: &mut HashMap<String, usize>,
        markdown_headings: &mut HashSet<String>,
        mut markdown_headings_exact: Option<&mut HashSet<String>>,
        use_underscore_dedup: bool,
    ) {
        // Slugs from generate_fragment are already lowercase, so the exact set
        // ends up identical to the lowercased set for slugs. The exact set is
        // only meaningfully different for case-preserving custom IDs (handled
        // by the caller). Skipping the parallel inserts when the caller passes
        // None avoids unnecessary allocations on the default ignore_case=true path.
        let mut also_insert_exact = |form: &str| {
            if let Some(set) = markdown_headings_exact.as_deref_mut() {
                set.insert(form.to_string());
            }
        };

        if fragment.is_empty() {
            if !use_underscore_dedup {
                return;
            }
            // Python-Markdown: empty slug → _1, _2, _3, ...
            let count = fragment_counts.entry(fragment).or_insert(0);
            *count += 1;
            let formed = format!("_{count}");
            also_insert_exact(&formed);
            markdown_headings.insert(formed);
            return;
        }
        if let Some(count) = fragment_counts.get_mut(&fragment) {
            let suffix = *count;
            *count += 1;
            if use_underscore_dedup {
                // Python-Markdown primary: heading_1, heading_2
                let underscore_form = format!("{fragment}_{suffix}");
                also_insert_exact(&underscore_form);
                markdown_headings.insert(underscore_form);
                // Also accept GitHub-style for compatibility
                let dash_form = format!("{fragment}-{suffix}");
                also_insert_exact(&dash_form);
                markdown_headings.insert(dash_form);
            } else {
                // GitHub-style: heading-1, heading-2
                let form = format!("{fragment}-{suffix}");
                also_insert_exact(&form);
                markdown_headings.insert(form);
            }
        } else {
            fragment_counts.insert(fragment.clone(), 1);
            also_insert_exact(&fragment);
            markdown_headings.insert(fragment);
        }
    }

    /// Add a heading to the cross-file index with proper deduplication.
    /// When `use_underscore_dedup` is true (Python-Markdown/MkDocs), the primary anchor
    /// uses `_N` and `-N` is registered as a fallback alias.
    ///
    /// Empty fragments (from CJK-only headings) get `_1`, `_2`, etc. in Python-Markdown mode.
    fn add_heading_to_index(
        fragment: &str,
        text: &str,
        custom_anchor: Option<String>,
        line: usize,
        fragment_counts: &mut HashMap<String, usize>,
        file_index: &mut FileIndex,
        use_underscore_dedup: bool,
    ) {
        if fragment.is_empty() {
            if !use_underscore_dedup {
                return;
            }
            // Python-Markdown: empty slug → _1, _2, _3, ...
            let count = fragment_counts.entry(fragment.to_string()).or_insert(0);
            *count += 1;
            file_index.add_heading(HeadingIndex {
                text: text.to_string(),
                auto_anchor: format!("_{count}"),
                custom_anchor,
                line,
                is_setext: false,
            });
            return;
        }
        if let Some(count) = fragment_counts.get_mut(fragment) {
            let suffix = *count;
            *count += 1;
            let (primary, alias) = if use_underscore_dedup {
                // Python-Markdown primary: heading_1; GitHub fallback: heading-1
                (format!("{fragment}_{suffix}"), Some(format!("{fragment}-{suffix}")))
            } else {
                // GitHub-style primary: heading-1
                (format!("{fragment}-{suffix}"), None)
            };
            file_index.add_heading(HeadingIndex {
                text: text.to_string(),
                auto_anchor: primary,
                custom_anchor,
                line,
                is_setext: false,
            });
            if let Some(alias_anchor) = alias {
                let heading_idx = file_index.headings.len() - 1;
                file_index.add_anchor_alias(&alias_anchor, heading_idx);
            }
        } else {
            fragment_counts.insert(fragment.to_string(), 1);
            file_index.add_heading(HeadingIndex {
                text: text.to_string(),
                auto_anchor: fragment.to_string(),
                custom_anchor,
                line,
                is_setext: false,
            });
        }
    }

    /// Extract all valid heading anchors from the document.
    ///
    /// Returns parallel lowercase + case-preserving sets so the same-document
    /// check can honor `ignore_case` consistently with cross-file lookups.
    /// The `*_exact` sets are only populated when `ignore_case = false` to
    /// avoid unnecessary allocations on the default permissive path.
    fn extract_headings_from_context(&self, ctx: &crate::lint_context::LintContext) -> AnchorSets {
        let track_exact = !self.config.ignore_case;
        let mut markdown_headings = HashSet::with_capacity(32);
        let mut markdown_headings_exact = if track_exact {
            HashSet::with_capacity(32)
        } else {
            HashSet::new()
        };
        let mut html_anchors = HashSet::with_capacity(16);
        let mut html_anchors_exact = if track_exact {
            HashSet::with_capacity(16)
        } else {
            HashSet::new()
        };
        let mut fragment_counts = std::collections::HashMap::new();
        let anchor_style = self.anchor_style(ctx);
        let use_underscore_dedup = anchor_style == AnchorStyle::PythonMarkdown;

        for line_info in &ctx.lines {
            if line_info.in_front_matter {
                continue;
            }

            // Skip code blocks for anchor extraction
            if line_info.in_code_block {
                continue;
            }

            let content = line_info.content(ctx.content);
            let bytes = content.as_bytes();

            // Extract HTML anchor tags with id/name attributes
            if bytes.contains(&b'<') && (content.contains("id=") || content.contains("name=")) {
                // HTML spec: only the first id attribute per element is valid
                // Process element by element to handle multiple id attributes correctly
                let mut pos = 0;
                while pos < content.len() {
                    if let Some(start) = content[pos..].find('<') {
                        let tag_start = pos + start;
                        if let Some(end) = content[tag_start..].find('>') {
                            let tag_end = tag_start + end + 1;
                            let tag = &content[tag_start..tag_end];

                            // Extract first id or name attribute from this tag
                            if let Some(caps) = HTML_ANCHOR_PATTERN.find(tag) {
                                let matched_text = caps.as_str();
                                if let Some(caps) = HTML_ANCHOR_PATTERN.captures(matched_text)
                                    && let Some(id_match) = caps.get(1)
                                {
                                    let id = id_match.as_str();
                                    if !id.is_empty() {
                                        html_anchors.insert(id.to_lowercase());
                                        if track_exact {
                                            html_anchors_exact.insert(id.to_string());
                                        }
                                    }
                                }
                            }
                            pos = tag_end;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }

            // Extract attribute anchors { #id } from non-heading lines
            // Headings already have custom_id extracted below
            if line_info.heading.is_none() && content.contains('{') && content.contains('#') {
                for caps in ATTR_ANCHOR_PATTERN.captures_iter(content) {
                    if let Some(id_match) = caps.get(1) {
                        let id = id_match.as_str();
                        markdown_headings.insert(id.to_lowercase());
                        if track_exact {
                            markdown_headings_exact.insert(id.to_string());
                        }
                    }
                }
            }

            // Extract heading anchors from blockquote content
            // Blockquote headings (e.g., "> ## Heading") are not detected by the main heading parser
            // because the regex operates on the full line, but they still generate valid anchors
            if line_info.heading.is_none()
                && let Some(bq) = &line_info.blockquote
                && let Some((clean_text, custom_id)) = Self::parse_blockquote_heading(&bq.content)
            {
                if let Some(id) = custom_id {
                    markdown_headings.insert(id.to_lowercase());
                    if track_exact {
                        markdown_headings_exact.insert(id);
                    }
                }
                let fragment = anchor_style.generate_fragment(&clean_text);
                Self::insert_deduplicated_fragment(
                    fragment,
                    &mut fragment_counts,
                    &mut markdown_headings,
                    track_exact.then_some(&mut markdown_headings_exact),
                    use_underscore_dedup,
                );
            }

            // Extract markdown heading anchors
            if let Some(heading) = &line_info.heading {
                // Custom ID from {#custom-id} syntax
                if let Some(custom_id) = &heading.custom_id {
                    markdown_headings.insert(custom_id.to_lowercase());
                    if track_exact {
                        markdown_headings_exact.insert(custom_id.clone());
                    }
                }

                // Generate fragment directly from heading text
                // Note: HTML stripping was removed because it interfered with arrow patterns
                // like <-> and placeholders like <FILE>. The anchor styles handle these correctly.
                let fragment = anchor_style.generate_fragment(&heading.text);

                Self::insert_deduplicated_fragment(
                    fragment,
                    &mut fragment_counts,
                    &mut markdown_headings,
                    track_exact.then_some(&mut markdown_headings_exact),
                    use_underscore_dedup,
                );
            }
        }

        AnchorSets {
            markdown_headings,
            markdown_headings_exact,
            html_anchors,
            html_anchors_exact,
        }
    }

    /// Fast check if URL is external (doesn't need to be validated)
    #[inline]
    fn is_external_url_fast(url: &str) -> bool {
        // Quick prefix checks for common protocols
        url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("ftp://")
            || url.starts_with("mailto:")
            || url.starts_with("tel:")
            || url.starts_with("//")
    }

    /// Resolve a path by trying markdown extensions if it has no extension
    ///
    /// For extension-less paths (e.g., `page`), returns a list of paths to try:
    /// 1. The original path (in case it's already in the index)
    /// 2. The path with each markdown extension (e.g., `page.md`, `page.markdown`, etc.)
    ///
    /// For paths with extensions, returns just the original path.
    #[inline]
    fn resolve_path_with_extensions(path: &Path, extensions: &[&str]) -> Vec<PathBuf> {
        if path.extension().is_none() {
            // Extension-less path - try with markdown extensions
            let mut paths = Vec::with_capacity(extensions.len() + 1);
            // First try the exact path (in case it's already in the index)
            paths.push(path.to_path_buf());
            // Then try with each markdown extension
            for ext in extensions {
                let path_with_ext = path.with_extension(&ext[1..]); // Remove leading dot
                paths.push(path_with_ext);
            }
            paths
        } else {
            // Path has extension - use as-is
            vec![path.to_path_buf()]
        }
    }

    /// Check if a path part (without fragment or query) is an extension-less path
    ///
    /// Extension-less paths are potential cross-file links that need resolution
    /// with markdown extensions (e.g., `page#section` -> `page.md#section`).
    ///
    /// We recognize them as extension-less if:
    /// 1. Path has no extension (no dot)
    /// 2. Path is not empty
    /// 3. Path doesn't look like query syntax
    /// 4. Path contains at least one alphanumeric character (valid filename)
    /// 5. Path contains only valid path characters (alphanumeric, slashes, hyphens, underscores)
    ///
    /// Optimized: single pass through characters to check both conditions.
    #[inline]
    fn is_extensionless_path(path_part: &str) -> bool {
        // Quick rejections for common non-extension-less cases
        if path_part.is_empty() || path_part.contains('.') || path_part.contains('&') || path_part.contains('=') {
            return false;
        }

        // Single pass: check for alphanumeric and validate all characters
        let mut has_alphanumeric = false;
        for c in path_part.chars() {
            if c.is_alphanumeric() {
                has_alphanumeric = true;
            } else if !matches!(c, '/' | '\\' | '-' | '_') {
                // Invalid character found - early exit
                return false;
            }
        }

        // Must have at least one alphanumeric character to be a valid filename
        has_alphanumeric
    }

    /// Check if URL is a cross-file link (contains a file path before #)
    #[inline]
    fn is_cross_file_link(url: &str) -> bool {
        if let Some(fragment_pos) = url.find('#') {
            let path_part = &url[..fragment_pos];

            // If there's no path part, it's just a fragment (#heading)
            if path_part.is_empty() {
                return false;
            }

            // Check for Liquid syntax used by Jekyll and other static site generators
            // Liquid tags: {% ... %} for control flow and includes
            // Liquid variables: {{ ... }} for outputting values
            // These are template directives that reference external content and should be skipped
            // We check for proper bracket order to avoid false positives
            if let Some(tag_start) = path_part.find("{%")
                && path_part[tag_start + 2..].contains("%}")
            {
                return true;
            }
            if let Some(var_start) = path_part.find("{{")
                && path_part[var_start + 2..].contains("}}")
            {
                return true;
            }

            // Check if it's an absolute path (starts with /)
            // These are links to other pages on the same site
            if path_part.starts_with('/') {
                return true;
            }

            // A query string belongs to the destination, not to the path it names,
            // so `page.md?raw=true` and `page?raw=true` name `page.md` and `page`.
            let path_part = path_part.split('?').next().unwrap_or(path_part);

            // A destination that is only a query and a fragment stays on this page
            if path_part.is_empty() {
                return false;
            }

            // Check if it looks like a file path:
            // - Contains a file extension (dot followed by letters)
            // - Contains path separators
            // - Contains relative path indicators
            // - OR is an extension-less path with a fragment (GitHub-style: page#section)
            let has_extension = path_part.contains('.')
                && (
                    // Has file extension pattern
                    {
                    // Handle files starting with dot
                    if let Some(after_dot) = path_part.strip_prefix('.') {
                        let dots_count = path_part.matches('.').count();
                        if dots_count == 1 {
                            // Could be ".ext" (file extension) or ".hidden" (hidden file)
                            // Treat short alphanumeric suffixes as file extensions
                            !after_dot.is_empty() && after_dot.len() <= 10 &&
                            after_dot.chars().all(|c| c.is_ascii_alphanumeric())
                        } else {
                            // Hidden file with extension like ".hidden.txt"
                            path_part.split('.').next_back().is_some_and(|ext| {
                                !ext.is_empty() && ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric())
                            })
                        }
                    } else {
                        // Regular file path
                        path_part.split('.').next_back().is_some_and(|ext| {
                            !ext.is_empty() && ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric())
                        })
                    }
                } ||
                // Or contains path separators
                path_part.contains('/') || path_part.contains('\\') ||
                // Or starts with relative path indicators
                path_part.starts_with("./") || path_part.starts_with("../")
                );

            // Extension-less paths with fragments are potential cross-file links
            // This supports GitHub-style links like [link](page#section) that resolve to page.md#section
            let is_extensionless = Self::is_extensionless_path(path_part);

            has_extension || is_extensionless
        } else {
            false
        }
    }

    /// Whether this document has frontmatter the rule is configured to check.
    ///
    /// Gates the body-link early exits, which would otherwise skip a document
    /// whose only links sit in its frontmatter.
    fn checks_front_matter_of(&self, ctx: &crate::lint_context::LintContext) -> bool {
        self.config.check_frontmatter && ctx.front_matter_end_line() > 0
    }

    /// Frontmatter values that read as link destinations, or nothing when the
    /// rule is not configured to check frontmatter.
    fn front_matter_links(&self, ctx: &crate::lint_context::LintContext) -> Vec<frontmatter_values::FrontMatterLink> {
        if !self.checks_front_matter_of(ctx) {
            return Vec::new();
        }
        frontmatter_values::link_destinations(ctx, &self.ignored_front_matter_fields)
    }

    /// Whether a fragment is one the flavor or the configuration resolves
    /// outside the document's own anchors.
    fn fragment_is_exempt(&self, ctx: &crate::lint_context::LintContext, fragment: &str) -> bool {
        // MkDocs runtime-generated anchors:
        // - #fn:NAME, #fnref:NAME from the footnotes extension
        // - #+key.path or #+key:value from Material for MkDocs option references
        //   (e.g., #+type:abstract, #+toc.slugify, #+pymdownx.highlight.anchor_linenums)
        if ctx.flavor == crate::config::MarkdownFlavor::MkDocs
            && (fragment.starts_with("fn:")
                || fragment.starts_with("fnref:")
                || (fragment.starts_with('+') && (fragment.contains('.') || fragment.contains(':'))))
        {
            return true;
        }

        // Fragments matching the user-configured ignored_pattern
        self.ignored_pattern_regex
            .as_ref()
            .is_some_and(|re| re.is_match(fragment))
    }

    /// Whether a fragment names an anchor this document defines. Both HTML and
    /// markdown anchors honor the `ignore_case` option, mirroring markdownlint
    /// and the cross-file path.
    fn fragment_resolves(&self, fragment: &str, anchors: &AnchorSets) -> bool {
        if self.config.ignore_case {
            let lower = fragment.to_lowercase();
            anchors.html_anchors.contains(&lower) || anchors.markdown_headings.contains(&lower)
        } else {
            anchors.html_anchors_exact.contains(fragment) || anchors.markdown_headings_exact.contains(fragment)
        }
    }

    /// Report frontmatter fragments that name no anchor in this document.
    ///
    /// Only a fragment-only value is resolved here. A value that carries a path
    /// (`page.md#section`) is a cross-file link: `contribute_to_index` hands it
    /// to the workspace index and `cross_file_check` resolves it against the
    /// target file's anchors.
    fn check_front_matter(
        &self,
        ctx: &crate::lint_context::LintContext,
        links: &[frontmatter_values::FrontMatterLink],
        anchors: &AnchorSets,
        warnings: &mut Vec<LintWarning>,
    ) {
        for link in links {
            let line = ctx.lines[link.line - 1].content(ctx.content);
            let Some(fragment) = line[link.range.clone()].strip_prefix('#') else {
                continue;
            };
            if fragment.is_empty() {
                continue;
            }

            // Pandoc and Quarto slugs diverge from GitHub style for headings
            // that contain punctuation.
            if ctx.flavor.is_pandoc_compatible() && ctx.has_pandoc_slug(fragment) {
                continue;
            }

            if self.fragment_is_exempt(ctx, fragment) || self.fragment_resolves(fragment, anchors) {
                continue;
            }

            let column = byte_to_char_count(line, link.range.start);
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message: format!("Link anchor '#{fragment}' does not exist in document headings"),
                line: link.line,
                column,
                end_line: link.line,
                end_column: column + 1 + fragment.chars().count(),
                severity: Severity::Error,
                fix: None,
            });
        }
    }
}

impl Rule for MD051LinkFragments {
    fn name(&self) -> &'static str {
        "MD051"
    }

    fn description(&self) -> &'static str {
        "Link fragments should reference valid headings"
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no link fragments present. A document whose only links sit in
        // its frontmatter has no link syntax to find, so that shortcut only
        // applies when frontmatter is not being checked.
        if !ctx.likely_has_links_or_images() && !self.checks_front_matter_of(ctx) {
            return true;
        }
        // Check for # character (fragments)
        !ctx.has_char('#')
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        if ctx.content.is_empty() || self.should_skip(ctx) {
            return Ok(warnings);
        }

        let front_matter_links = self.front_matter_links(ctx);
        if ctx.links.is_empty() && front_matter_links.is_empty() {
            return Ok(warnings);
        }

        let anchors = self.extract_headings_from_context(ctx);

        for link in &ctx.links {
            if link.is_reference {
                continue;
            }

            // Skip links inside PyMdown blocks (MkDocs flavor)
            if ctx.line_info(link.line).is_some_and(|info| info.in_pymdown_block) {
                continue;
            }

            // Skip wiki-links - they reference other files and may have their own fragment validation
            if matches!(link.link_type, LinkType::WikiLink { .. }) {
                continue;
            }

            // Skip links inside Jinja templates
            if ctx.is_in_jinja_range(link.byte_offset) {
                continue;
            }

            // Skip Pandoc/Quarto citations ([@citation], @citation)
            // Citations are bibliography references, not link fragments
            if ctx.flavor.is_pandoc_compatible() && ctx.is_in_citation(link.byte_offset) {
                continue;
            }

            // Skip links inside shortcodes ({{< ... >}} or {{% ... %}})
            // Shortcodes may contain template syntax that looks like fragment links
            if ctx.is_in_shortcode(link.byte_offset) {
                continue;
            }

            let url = &link.url;

            // Skip links without fragments or external URLs
            if !url.contains('#') || Self::is_external_url_fast(url) {
                continue;
            }

            // Skip mdbook template placeholders ({{#VARIABLE}})
            // mdbook uses {{#VARIABLE}} syntax where # is part of the template, not a fragment
            if url.contains("{{#") && url.contains("}}") {
                continue;
            }

            // Resolve link fragments against Pandoc heading slugs. Pandoc/Quarto
            // auto-generate slugs that diverge from GitHub style for headings that
            // contain punctuation (e.g. `# 5. Five Things` becomes `5.-five-things`
            // under Pandoc but `5-five-things` under GitHub). Treat such fragments
            // as resolved when running under a Pandoc-compatible flavor.
            if ctx.flavor.is_pandoc_compatible()
                && let Some(frag) = url.strip_prefix('#')
                && ctx.has_pandoc_slug(frag)
            {
                continue;
            }

            // Skip Quarto/RMarkdown cross-references (@fig-, @tbl-, @sec-, @eq-, etc.)
            // These are special cross-reference syntax, not HTML anchors
            // Format: @prefix-identifier or just @identifier
            if url.starts_with('@') {
                continue;
            }

            // Cross-file links are valid if the file exists (not checked here)
            if Self::is_cross_file_link(url) {
                continue;
            }

            let Some(fragment_pos) = url.find('#') else {
                continue;
            };

            let fragment = &url[fragment_pos + 1..];

            // Skip Liquid template variables and filters
            if (url.contains("{{") && fragment.contains('|')) || fragment.ends_with("}}") || fragment.ends_with("%}") {
                continue;
            }

            if fragment.is_empty() {
                continue;
            }

            if self.fragment_is_exempt(ctx, fragment) {
                continue;
            }

            if !self.fragment_resolves(fragment, &anchors) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: format!("Link anchor '#{fragment}' does not exist in document headings"),
                    line: link.line,
                    column: link.start_col + 1,
                    end_line: link.line,
                    end_column: link.end_col + 1,
                    severity: Severity::Error,
                    fix: None,
                });
            }
        }

        self.check_front_matter(ctx, &front_matter_links, &anchors, &mut warnings);

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // MD051 does not provide auto-fix
        // Link fragment corrections require human judgment to avoid incorrect fixes
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let mut rule_config = crate::rule_config_serde::load_rule_config::<MD051Config>(config);

        // When no explicit anchor style is configured (the user didn't override the default),
        // the style follows the flavor's native anchor generation. The global flavor settles
        // it here for `rumdl config`; a file `per-file-flavor` gives another flavor re-derives
        // it in `anchor_style()`.
        let explicit_style_present = config
            .rules
            .get("MD051")
            .is_some_and(|rc| rc.values.contains_key("anchor-style") || rc.values.contains_key("anchor_style"));
        if !explicit_style_present {
            rule_config.anchor_style = AnchorStyle::for_flavor(config.global.flavor);
        }

        Box::new(MD051LinkFragments::build(
            rule_config,
            config.withheld_rule_values.contains("MD051"),
            explicit_style_present,
        ))
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn skippable_by_category(&self) -> bool {
        // A frontmatter fragment is a link this rule resolves, and the document
        // holding it needs no link syntax anywhere else.
        !self.config.check_frontmatter
    }

    fn cross_file_scope(&self) -> CrossFileScope {
        CrossFileScope::Workspace
    }

    fn contribute_to_index(&self, ctx: &crate::lint_context::LintContext, file_index: &mut FileIndex) {
        let mut fragment_counts = HashMap::new();
        let anchor_style = self.anchor_style(ctx);
        let use_underscore_dedup = anchor_style == AnchorStyle::PythonMarkdown;

        // Extract headings, HTML anchors, and attribute anchors (for other files to reference)
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            if line_info.in_front_matter {
                continue;
            }

            // Skip code blocks for anchor extraction
            if line_info.in_code_block {
                continue;
            }

            let content = line_info.content(ctx.content);

            // Extract HTML anchors (id or name attributes on any element)
            if content.contains('<') && (content.contains("id=") || content.contains("name=")) {
                let mut pos = 0;
                while pos < content.len() {
                    if let Some(start) = content[pos..].find('<') {
                        let tag_start = pos + start;
                        if let Some(end) = content[tag_start..].find('>') {
                            let tag_end = tag_start + end + 1;
                            let tag = &content[tag_start..tag_end];

                            if let Some(caps) = HTML_ANCHOR_PATTERN.captures(tag)
                                && let Some(id_match) = caps.get(1)
                            {
                                file_index.add_html_anchor(id_match.as_str());
                            }
                            pos = tag_end;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }

            // Extract attribute anchors { #id } on non-heading lines
            // Headings already have custom_id extracted via heading.custom_id
            if line_info.heading.is_none() && content.contains('{') && content.contains('#') {
                for caps in ATTR_ANCHOR_PATTERN.captures_iter(content) {
                    if let Some(id_match) = caps.get(1) {
                        file_index.add_attribute_anchor(id_match.as_str());
                    }
                }
            }

            // Extract heading anchors from blockquote content
            if line_info.heading.is_none()
                && let Some(bq) = &line_info.blockquote
                && let Some((clean_text, custom_id)) = Self::parse_blockquote_heading(&bq.content)
            {
                let fragment = anchor_style.generate_fragment(&clean_text);
                Self::add_heading_to_index(
                    &fragment,
                    &clean_text,
                    custom_id,
                    line_idx + 1,
                    &mut fragment_counts,
                    file_index,
                    use_underscore_dedup,
                );
            }

            // Extract heading anchors
            if let Some(heading) = &line_info.heading {
                let fragment = anchor_style.generate_fragment(&heading.text);

                Self::add_heading_to_index(
                    &fragment,
                    &heading.text,
                    heading.custom_id.clone(),
                    line_idx + 1,
                    &mut fragment_counts,
                    file_index,
                    use_underscore_dedup,
                );

                // Extract Material for MkDocs setting anchors from headings.
                // These are rendered as anchors at build time by Material's JS.
                // Most references use #+key.path format (handled by the skip logic in check()),
                // but this extraction enables cross-file validation for direct #key.path references.
                if ctx.flavor == crate::config::MarkdownFlavor::MkDocs
                    && let Some(caps) = MD_SETTING_PATTERN.captures(content)
                    && let Some(name) = caps.get(1)
                {
                    file_index.add_html_anchor(name.as_str());
                }
            }
        }

        // Extract cross-file links (for validation against other files)
        for link in &ctx.links {
            if link.is_reference {
                continue;
            }

            // Skip links inside PyMdown blocks (MkDocs flavor)
            if ctx.line_info(link.line).is_some_and(|info| info.in_pymdown_block) {
                continue;
            }

            // Skip wiki-links - they use a different linking system and are not validated
            // as relative file paths
            if matches!(link.link_type, LinkType::WikiLink { .. }) {
                continue;
            }

            let url = &link.url;

            // Skip external URLs
            if Self::is_external_url_fast(url) {
                continue;
            }

            // Only process cross-file links with fragments
            if Self::is_cross_file_link(url)
                && let Some(fragment_pos) = url.find('#')
            {
                let path_part = &url[..fragment_pos];
                let fragment = &url[fragment_pos + 1..];

                // Skip empty fragments or template syntax
                if fragment.is_empty() || fragment.contains("{{") || fragment.contains("{%") {
                    continue;
                }

                file_index.add_cross_file_link(CrossFileLinkIndex {
                    target_path: path_part.to_string(),
                    fragment: fragment.to_string(),
                    line: link.line,
                    column: link.start_col + 1,
                });
            }
        }

        // Extract cross-file links from frontmatter values that carry a fragment
        for link in self.front_matter_links(ctx) {
            let line = ctx.lines[link.line - 1].content(ctx.content);
            let value = &line[link.range.clone()];

            if Self::is_external_url_fast(value) || !Self::is_cross_file_link(value) {
                continue;
            }

            let Some(fragment_pos) = value.find('#') else {
                continue;
            };
            let path_part = &value[..fragment_pos];
            let fragment = &value[fragment_pos + 1..];

            // Skip empty fragments or template syntax
            if fragment.is_empty() || fragment.contains("{{") || fragment.contains("{%") {
                continue;
            }

            file_index.add_cross_file_link(CrossFileLinkIndex {
                target_path: path_part.to_string(),
                fragment: fragment.to_string(),
                line: link.line,
                column: byte_to_char_count(line, link.range.start),
            });
        }
    }

    fn cross_file_check(
        &self,
        file_path: &Path,
        file_index: &FileIndex,
        workspace_index: &crate::workspace_index::WorkspaceIndex,
    ) -> LintResult {
        let mut warnings = Vec::new();

        // Supported markdown file extensions (with leading dot, matching MD057)
        const MARKDOWN_EXTENSIONS: &[&str] = &[
            ".md",
            ".markdown",
            ".mdx",
            ".mkd",
            ".mkdn",
            ".mdown",
            ".mdwn",
            ".qmd",
            ".rmd",
        ];

        let ignored_pattern = self.ignored_pattern_regex.as_ref();
        let ignore_case = self.config.ignore_case;

        // Check each cross-file link in this file
        for cross_link in &file_index.cross_file_links {
            // Skip cross-file links without fragments - nothing to validate
            if cross_link.fragment.is_empty() {
                continue;
            }

            // Honor `ignored-pattern`: skip fragments matching the configured regex.
            if ignored_pattern.is_some_and(|re| re.is_match(&cross_link.fragment)) {
                continue;
            }

            // A query string is not part of the file name: `other.md?raw=true`
            // names `other.md`. The message keeps the destination as written.
            let target_path = cross_link
                .target_path
                .split('?')
                .next()
                .unwrap_or(&cross_link.target_path);

            // Resolve the target file path relative to the current file
            let base_target_path = if let Some(parent) = file_path.parent() {
                parent.join(target_path)
            } else {
                Path::new(target_path).to_path_buf()
            };

            // Normalize the path (remove . and ..)
            let base_target_path = normalize_path(&base_target_path);

            // For extension-less paths, try resolving with markdown extensions
            // This handles GitHub-style links like [link](page#section) -> page.md#section
            let target_paths_to_try = Self::resolve_path_with_extensions(&base_target_path, MARKDOWN_EXTENSIONS);

            // Try to find the target file in the workspace index
            let mut target_file_index = None;

            for target_path in &target_paths_to_try {
                if let Some(index) = workspace_index.get_file(target_path) {
                    target_file_index = Some(index);
                    break;
                }
            }

            if let Some(target_file_index) = target_file_index {
                // Check if the fragment matches any heading in the target file (O(1) lookup)
                if !target_file_index.has_anchor_with_case(&cross_link.fragment, ignore_case) {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: cross_link.line,
                        column: cross_link.column,
                        end_line: cross_link.line,
                        end_column: cross_link.column
                            + cross_link.target_path.chars().count()
                            + 1
                            + cross_link.fragment.chars().count(),
                        message: format!(
                            "Link fragment '{}' not found in '{}'",
                            cross_link.fragment, cross_link.target_path
                        ),
                        severity: Severity::Error,
                        fix: None,
                    });
                }
            }
            // If target file not in index, skip (could be external file or not in workspace)
        }

        Ok(warnings)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let table = crate::rule_config_serde::config_schema_table(&MD051Config::default())?;
        if table.is_empty() {
            None
        } else {
            Some((MD051Config::RULE_NAME.to_string(), toml::Value::Table(table)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    /// An em dash collapses to one hyphen under Python-Markdown and to nothing
    /// (leaving both surrounding spaces as hyphens) under GitHub, so exactly one
    /// of these two links is invalid and which one names the style in force.
    const ANCHOR_STYLE_PROBE: &str = "### Getting Started — Advanced\n\n\
        [python-markdown slug](#getting-started-advanced)\n\
        [github slug](#getting-started--advanced)\n";

    fn flagged_fragment(rule: &dyn Rule, flavor: crate::config::MarkdownFlavor) -> String {
        let ctx = LintContext::new(ANCHOR_STYLE_PROBE, flavor, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            1,
            "exactly one of the two links must be invalid under any style: {warnings:?}"
        );
        warnings[0].message.clone()
    }

    /// An unpinned anchor style follows the flavor of the file being checked,
    /// not the global flavor the rule was constructed with. `per-file-flavor`
    /// makes those differ, and the style is decided per file.
    #[test]
    fn test_unpinned_anchor_style_follows_the_file_flavor() {
        let rule_from_global = |flavor| {
            let mut config = crate::config::Config::default();
            config.global.flavor = flavor;
            MD051LinkFragments::from_config(&config)
        };

        // Global standard: construction settles on GitHub anchors.
        let standard_global = rule_from_global(crate::config::MarkdownFlavor::Standard);
        // A file the global flavor applies to keeps them, so the Python-Markdown
        // slug is the one that does not exist.
        assert!(
            flagged_fragment(standard_global.as_ref(), crate::config::MarkdownFlavor::Standard)
                .contains("#getting-started-advanced'"),
            "a standard file must be checked against GitHub anchors"
        );
        // A file `per-file-flavor` parses as MkDocs is checked against
        // Python-Markdown anchors, so the GitHub slug is the missing one.
        assert!(
            flagged_fragment(standard_global.as_ref(), crate::config::MarkdownFlavor::MkDocs)
                .contains("#getting-started--advanced'"),
            "a mkdocs file must be checked against Python-Markdown anchors even under a standard global flavor"
        );

        // The same in reverse: a standard file under a MkDocs global flavor.
        let mkdocs_global = rule_from_global(crate::config::MarkdownFlavor::MkDocs);
        assert!(
            flagged_fragment(mkdocs_global.as_ref(), crate::config::MarkdownFlavor::MkDocs)
                .contains("#getting-started--advanced'"),
            "a mkdocs file must be checked against Python-Markdown anchors"
        );
        assert!(
            flagged_fragment(mkdocs_global.as_ref(), crate::config::MarkdownFlavor::Standard)
                .contains("#getting-started-advanced'"),
            "a standard file must be checked against GitHub anchors even under a mkdocs global flavor"
        );
    }

    /// Control for the above: a style the user pinned is theirs, and applies to
    /// every file whatever flavor it is parsed with.
    #[test]
    fn test_pinned_anchor_style_ignores_the_file_flavor() {
        let mut config = crate::config::Config::default();
        config.global.flavor = crate::config::MarkdownFlavor::Standard;
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config
            .values
            .insert("anchor-style".to_string(), toml::Value::String("github".to_string()));
        config.rules.insert("MD051".to_string(), rule_config);
        let rule = MD051LinkFragments::from_config(&config);

        for flavor in [
            crate::config::MarkdownFlavor::Standard,
            crate::config::MarkdownFlavor::MkDocs,
            crate::config::MarkdownFlavor::Kramdown,
        ] {
            assert!(
                flagged_fragment(rule.as_ref(), flavor).contains("#getting-started-advanced'"),
                "pinned github anchors must survive a {flavor:?} file"
            );
        }
    }

    /// Directly constructed rules are pinned: nothing derived their style from a
    /// flavor, so there is nothing to re-derive.
    #[test]
    fn test_directly_constructed_rule_keeps_its_anchor_style() {
        let rule = MD051LinkFragments::from_config_struct(MD051Config {
            anchor_style: AnchorStyle::PythonMarkdown,
            ..Default::default()
        });
        assert!(
            flagged_fragment(&rule, crate::config::MarkdownFlavor::Standard).contains("#getting-started--advanced'"),
            "an explicitly constructed Python-Markdown rule must not follow the file flavor"
        );
    }

    #[test]
    fn test_quarto_cross_references() {
        let rule = MD051LinkFragments::new();

        // Test that Quarto cross-references are skipped
        let content = r#"# Test Document

## Figures

See [@fig-plot] for the visualization.

More details in [@tbl-results] and [@sec-methods].

The equation [@eq-regression] shows the relationship.

Reference to [@lst-code] for implementation."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Quarto cross-references (@fig-, @tbl-, @sec-, @eq-) should not trigger MD051 warnings. Got {} warnings",
            result.len()
        );

        // Test that normal anchors still work
        let content_with_anchor = r#"# Test

See [link](#test) for details."#;
        let ctx_anchor = LintContext::new(content_with_anchor, crate::config::MarkdownFlavor::Quarto, None);
        let result_anchor = rule.check(&ctx_anchor).unwrap();
        assert!(result_anchor.is_empty(), "Valid anchor should not trigger warning");

        // Test that invalid anchors are still flagged
        let content_invalid = r#"# Test

See [link](#nonexistent) for details."#;
        let ctx_invalid = LintContext::new(content_invalid, crate::config::MarkdownFlavor::Quarto, None);
        let result_invalid = rule.check(&ctx_invalid).unwrap();
        assert_eq!(result_invalid.len(), 1, "Invalid anchor should still trigger warning");
    }

    #[test]
    fn test_jsx_in_heading_anchor() {
        // Issue #510: JSX/HTML tags in headings should be stripped for anchor generation
        let rule = MD051LinkFragments::new();

        // Self-closing JSX tag
        let content = "# Test\n\n### `retentionPolicy`<Component />\n\n[link](#retentionpolicy)\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "JSX self-closing tag should be stripped from anchor: got {result:?}"
        );

        // JSX with attributes
        let content2 =
            "### retentionPolicy<HeaderTag type=\"danger\" text=\"required\" />\n\n[link](#retentionpolicy)\n";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert!(
            result2.is_empty(),
            "JSX tag with attributes should be stripped from anchor: got {result2:?}"
        );

        // HTML tags with inner text preserved
        let content3 = "### Test <span>extra</span>\n\n[link](#test-extra)\n";
        let ctx3 = LintContext::new(content3, crate::config::MarkdownFlavor::Standard, None);
        let result3 = rule.check(&ctx3).unwrap();
        assert!(
            result3.is_empty(),
            "HTML tag content should be preserved in anchor: got {result3:?}"
        );
    }

    // Cross-file validation tests
    #[test]
    fn test_cross_file_scope() {
        let rule = MD051LinkFragments::new();
        assert_eq!(rule.cross_file_scope(), CrossFileScope::Workspace);
    }

    #[test]
    fn test_contribute_to_index_extracts_headings() {
        let rule = MD051LinkFragments::new();
        let content = "# First Heading\n\n# Second { #custom }\n\n## Third";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let mut file_index = FileIndex::new();
        rule.contribute_to_index(&ctx, &mut file_index);

        assert_eq!(file_index.headings.len(), 3);
        assert_eq!(file_index.headings[0].text, "First Heading");
        assert_eq!(file_index.headings[0].auto_anchor, "first-heading");
        assert!(file_index.headings[0].custom_anchor.is_none());

        assert_eq!(file_index.headings[1].text, "Second");
        assert_eq!(file_index.headings[1].custom_anchor, Some("custom".to_string()));

        assert_eq!(file_index.headings[2].text, "Third");
    }

    #[test]
    fn test_contribute_to_index_extracts_cross_file_links() {
        let rule = MD051LinkFragments::new();
        let content = "See [docs](other.md#installation) and [more](../guide.md#getting-started)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let mut file_index = FileIndex::new();
        rule.contribute_to_index(&ctx, &mut file_index);

        assert_eq!(file_index.cross_file_links.len(), 2);
        assert_eq!(file_index.cross_file_links[0].target_path, "other.md");
        assert_eq!(file_index.cross_file_links[0].fragment, "installation");
        assert_eq!(file_index.cross_file_links[1].target_path, "../guide.md");
        assert_eq!(file_index.cross_file_links[1].fragment, "getting-started");
    }

    #[test]
    fn test_cross_file_check_valid_fragment() {
        use crate::workspace_index::WorkspaceIndex;

        let rule = MD051LinkFragments::new();

        // Build workspace index with target file
        let mut workspace_index = WorkspaceIndex::new();
        let mut target_file_index = FileIndex::new();
        target_file_index.add_heading(HeadingIndex {
            text: "Installation Guide".to_string(),
            auto_anchor: "installation-guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        workspace_index.insert_file(PathBuf::from("docs/install.md"), target_file_index);

        // Create a FileIndex for the file being checked
        let mut current_file_index = FileIndex::new();
        current_file_index.add_cross_file_link(CrossFileLinkIndex {
            target_path: "install.md".to_string(),
            fragment: "installation-guide".to_string(),
            line: 3,
            column: 5,
        });

        let warnings = rule
            .cross_file_check(Path::new("docs/readme.md"), &current_file_index, &workspace_index)
            .unwrap();

        // Should find no warnings since fragment exists
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_cross_file_check_invalid_fragment() {
        use crate::workspace_index::WorkspaceIndex;

        let rule = MD051LinkFragments::new();

        // Build workspace index with target file
        let mut workspace_index = WorkspaceIndex::new();
        let mut target_file_index = FileIndex::new();
        target_file_index.add_heading(HeadingIndex {
            text: "Installation Guide".to_string(),
            auto_anchor: "installation-guide".to_string(),
            custom_anchor: None,
            line: 1,
            is_setext: false,
        });
        workspace_index.insert_file(PathBuf::from("docs/install.md"), target_file_index);

        // Create a FileIndex with a cross-file link pointing to non-existent fragment
        let mut current_file_index = FileIndex::new();
        current_file_index.add_cross_file_link(CrossFileLinkIndex {
            target_path: "install.md".to_string(),
            fragment: "nonexistent".to_string(),
            line: 3,
            column: 5,
        });

        let warnings = rule
            .cross_file_check(Path::new("docs/readme.md"), &current_file_index, &workspace_index)
            .unwrap();

        // Should find one warning since fragment doesn't exist
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("nonexistent"));
        assert!(warnings[0].message.contains("install.md"));
    }

    #[test]
    fn test_cross_file_check_custom_anchor_match() {
        use crate::workspace_index::WorkspaceIndex;

        let rule = MD051LinkFragments::new();

        // Build workspace index with target file that has custom anchor
        let mut workspace_index = WorkspaceIndex::new();
        let mut target_file_index = FileIndex::new();
        target_file_index.add_heading(HeadingIndex {
            text: "Installation Guide".to_string(),
            auto_anchor: "installation-guide".to_string(),
            custom_anchor: Some("install".to_string()),
            line: 1,
            is_setext: false,
        });
        workspace_index.insert_file(PathBuf::from("docs/install.md"), target_file_index);

        // Link uses custom anchor
        let mut current_file_index = FileIndex::new();
        current_file_index.add_cross_file_link(CrossFileLinkIndex {
            target_path: "install.md".to_string(),
            fragment: "install".to_string(),
            line: 3,
            column: 5,
        });

        let warnings = rule
            .cross_file_check(Path::new("docs/readme.md"), &current_file_index, &workspace_index)
            .unwrap();

        // Should find no warnings since custom anchor matches
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_cross_file_check_target_not_in_workspace() {
        use crate::workspace_index::WorkspaceIndex;

        let rule = MD051LinkFragments::new();

        // Empty workspace index
        let workspace_index = WorkspaceIndex::new();

        // Link to file not in workspace
        let mut current_file_index = FileIndex::new();
        current_file_index.add_cross_file_link(CrossFileLinkIndex {
            target_path: "external.md".to_string(),
            fragment: "heading".to_string(),
            line: 3,
            column: 5,
        });

        let warnings = rule
            .cross_file_check(Path::new("docs/readme.md"), &current_file_index, &workspace_index)
            .unwrap();

        // Should not warn about files not in workspace
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_wikilinks_skipped_in_check() {
        // Wikilinks should not trigger MD051 warnings for missing fragments
        let rule = MD051LinkFragments::new();

        let content = r#"# Test Document

## Valid Heading

[[Microsoft#Windows OS]]
[[SomePage#section]]
[[page|Display Text]]
[[path/to/page#section]]
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Wikilinks should not trigger MD051 warnings. Got: {result:?}"
        );
    }

    #[test]
    fn test_wikilinks_not_added_to_cross_file_index() {
        // Wikilinks should not be added to the cross-file link index
        let rule = MD051LinkFragments::new();

        let content = r#"# Test Document

[[Microsoft#Windows OS]]
[[SomePage#section]]
[Regular Link](other.md#section)
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let mut file_index = FileIndex::new();
        rule.contribute_to_index(&ctx, &mut file_index);

        // Should only have one cross-file link (the regular markdown link)
        // Wikilinks should not be added
        let cross_file_links = &file_index.cross_file_links;
        assert_eq!(
            cross_file_links.len(),
            1,
            "Only regular markdown links should be indexed, not wikilinks. Got: {cross_file_links:?}"
        );
        assert_eq!(file_index.cross_file_links[0].target_path, "other.md");
        assert_eq!(file_index.cross_file_links[0].fragment, "section");
    }

    #[test]
    fn test_pandoc_flavor_skips_citations() {
        // Pandoc citations ([@key]) are bibliography references, not link fragments.
        // MD051 should skip them under Pandoc flavor, mirroring the Quarto skip behavior
        // tested in test_quarto_cross_references.
        let rule = MD051LinkFragments::new();
        let content = "# Test Document\n\nSee [@smith2020] for details.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD051 should skip Pandoc citations under Pandoc flavor: {result:?}"
        );
    }

    #[test]
    fn md051_pandoc_resolves_pandoc_slug_diverging_from_github() {
        // The Pandoc heading slug for `# 5. Five Things` is `5.-five-things` (the
        // dot is preserved per Pandoc's rule of keeping `.`/`_`/`-`), whereas the
        // GitHub anchor for the same heading is `5-five-things` (the dot is
        // stripped). A link to `#5.-five-things` would be flagged under the
        // GitHub default but must be accepted under Pandoc-compatible flavors via
        // the `has_pandoc_slug` short-circuit.
        use crate::config::MarkdownFlavor;
        let rule = MD051LinkFragments::new();
        let content = "# 5. Five Things\n\nSee [details](#5.-five-things).\n";

        // Sanity check: under Standard flavor (GitHub anchor style), the
        // divergent fragment is reported as an unknown anchor.
        let ctx_std = LintContext::new(content, MarkdownFlavor::Standard, None);
        let std_result = rule.check(&ctx_std).unwrap();
        assert_eq!(
            std_result.len(),
            1,
            "Standard flavor should flag the Pandoc-style fragment: {std_result:?}"
        );

        // Under Pandoc flavor, the Pandoc slug guard should resolve it.
        let ctx_pandoc = LintContext::new(content, MarkdownFlavor::Pandoc, None);
        let pandoc_result = rule.check(&ctx_pandoc).unwrap();
        assert!(
            pandoc_result.is_empty(),
            "Pandoc flavor should resolve `#5.-five-things` against the heading slug: {pandoc_result:?}"
        );
    }

    /// A link whose text contains an email address must still be checked under
    /// Pandoc — the `@` embedded in a word is not a citation marker, so the
    /// citation guard must not silence MD051 on a missing fragment.
    #[test]
    fn md051_pandoc_flags_missing_fragment_with_email_in_link_text() {
        use crate::config::MarkdownFlavor;
        let rule = MD051LinkFragments::new();
        let content = "# Title\n\n[contact user@example.com](#missing)\n";

        let ctx_std = LintContext::new(content, MarkdownFlavor::Standard, None);
        let std_result = rule.check(&ctx_std).unwrap();
        assert_eq!(
            std_result.len(),
            1,
            "Standard flavor must flag the missing fragment: {std_result:?}"
        );

        let ctx_pandoc = LintContext::new(content, MarkdownFlavor::Pandoc, None);
        let pandoc_result = rule.check(&ctx_pandoc).unwrap();
        assert_eq!(
            pandoc_result.len(),
            1,
            "Pandoc flavor must also flag the missing fragment — link text with embedded email is not a citation: {pandoc_result:?}"
        );
    }

    /// `[see @smith2020](#missing)` is a Markdown link, not a citation —
    /// Pandoc prefers the link interpretation when `[...]` is immediately
    /// followed by `(...)`. MD051 must still flag the missing fragment.
    #[test]
    fn md051_pandoc_flags_missing_fragment_with_citation_in_link_text() {
        use crate::config::MarkdownFlavor;
        let rule = MD051LinkFragments::new();
        let content = "# Title\n\n[see @smith2020](#missing)\n";

        let ctx_std = LintContext::new(content, MarkdownFlavor::Standard, None);
        let std_result = rule.check(&ctx_std).unwrap();
        assert_eq!(
            std_result.len(),
            1,
            "Standard flavor must flag the missing fragment: {std_result:?}"
        );

        let ctx_pandoc = LintContext::new(content, MarkdownFlavor::Pandoc, None);
        let pandoc_result = rule.check(&ctx_pandoc).unwrap();
        assert_eq!(
            pandoc_result.len(),
            1,
            "Pandoc flavor must flag the missing fragment — `[label](url)` is a link, not a citation: {pandoc_result:?}"
        );
    }

    /// Pandoc's auto_identifiers extension disambiguates duplicate headings by
    /// appending `-1`, `-2`, etc. A link to `#a.-1` must resolve against the
    /// second `# A.` heading.
    #[test]
    fn md051_pandoc_resolves_duplicate_heading_suffix_slug() {
        use crate::config::MarkdownFlavor;
        let rule = MD051LinkFragments::new();
        let content = "# A.\n\nfirst\n\n# A.\n\nsecond\n\n[first](#a.) and [second](#a.-1).\n";

        let ctx_pandoc = LintContext::new(content, MarkdownFlavor::Pandoc, None);
        let pandoc_result = rule.check(&ctx_pandoc).unwrap();
        assert!(
            pandoc_result.is_empty(),
            "Pandoc flavor should resolve `#a.` and `#a.-1` against duplicate headings: {pandoc_result:?}"
        );

        let ctx_quarto = LintContext::new(content, MarkdownFlavor::Quarto, None);
        let quarto_result = rule.check(&ctx_quarto).unwrap();
        assert!(
            quarto_result.is_empty(),
            "Quarto flavor should also resolve duplicate-heading suffix slugs: {quarto_result:?}"
        );
    }

    /// A link to `#a.-2` with only two `# A.` headings must still be flagged —
    /// only `-1` exists when there are two duplicates.
    #[test]
    fn md051_pandoc_flags_overshoot_duplicate_suffix() {
        use crate::config::MarkdownFlavor;
        let rule = MD051LinkFragments::new();
        let content = "# A.\n\n# A.\n\n[overshoot](#a.-2)\n";

        let ctx_pandoc = LintContext::new(content, MarkdownFlavor::Pandoc, None);
        let pandoc_result = rule.check(&ctx_pandoc).unwrap();
        assert_eq!(
            pandoc_result.len(),
            1,
            "Pandoc must flag `#a.-2` when only `-1` exists (two duplicates): {pandoc_result:?}"
        );
    }

    fn front_matter_checked() -> MD051Config {
        MD051Config {
            check_frontmatter: true,
            ..MD051Config::default()
        }
    }

    fn check_front_matter(content: &str, config: MD051Config) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        MD051LinkFragments::from_config_struct(config).check(&ctx).unwrap()
    }

    #[test]
    fn a_broken_frontmatter_fragment_is_reported_when_enabled() {
        let content = "---\nanchor: '#missing'\nvalid: '#title'\n---\n\n# Title\n";
        let result = check_front_matter(content, front_matter_checked());

        assert_eq!(
            result.len(),
            1,
            "Only the unresolved fragment is reported. Got: {result:?}"
        );
        assert_eq!(
            result[0].message,
            "Link anchor '#missing' does not exist in document headings"
        );
        assert_eq!(result[0].line, 2);
        assert_eq!(result[0].column, 10, "The warning points at the value, not the key");
        assert_eq!(result[0].end_column, 18);
    }

    #[test]
    fn frontmatter_fragments_are_not_checked_by_default() {
        let content = "---\nanchor: '#missing'\n---\n\n# Title\n";
        let result = check_front_matter(content, MD051Config::default());

        assert!(
            result.is_empty(),
            "Frontmatter is only checked on request. Got: {result:?}"
        );
    }

    #[test]
    fn an_ignored_frontmatter_field_is_not_checked() {
        let content = "---\nhero: '#missing'\nanchor: '#other'\n---\n\n# Title\n";
        let config = MD051Config {
            check_frontmatter: true,
            ignore_frontmatter_fields: vec!["Hero".to_string()],
            ..MD051Config::default()
        };
        let result = check_front_matter(content, config);

        assert_eq!(
            result.len(),
            1,
            "The ignored field is skipped and the other is not. Got: {result:?}"
        );
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn the_ignored_pattern_applies_to_frontmatter_fragments() {
        let content = "---\nnote: '#fn:1'\nanchor: '#missing'\n---\n\n# Title\n";
        let config = MD051Config {
            check_frontmatter: true,
            ignored_pattern: Some("^fn:".to_string()),
            ..MD051Config::default()
        };
        let result = check_front_matter(content, config);

        assert_eq!(
            result.len(),
            1,
            "The matching fragment is skipped and the other is not. Got: {result:?}"
        );
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn a_frontmatter_fragment_honors_ignore_case() {
        let content = "---\nanchor: '#Title'\n---\n\n# Title\n";

        let permissive = check_front_matter(content, front_matter_checked());
        assert!(
            permissive.is_empty(),
            "The default resolves a case mismatch. Got: {permissive:?}"
        );

        let strict = check_front_matter(
            content,
            MD051Config {
                check_frontmatter: true,
                ignore_case: false,
                ..MD051Config::default()
            },
        );
        assert_eq!(strict.len(), 1, "Strict matching reports it. Got: {strict:?}");
    }

    #[test]
    fn prose_in_frontmatter_is_not_read_as_a_fragment() {
        let content = "---\ntitle: Node.js\ntags: ci/cd\n---\n\n# Title\n";
        let result = check_front_matter(content, front_matter_checked());

        assert!(
            result.is_empty(),
            "Only path-shaped values are destinations. Got: {result:?}"
        );
    }

    #[test]
    fn a_frontmatter_path_with_a_fragment_is_validated_across_files() {
        let rule = MD051LinkFragments::from_config_struct(front_matter_checked());
        let source = "---\ntemplate: other.md#missing\nvalid: other.md#target\n---\n\n# Source\n";

        let source_ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        let mut source_index = FileIndex::default();
        rule.contribute_to_index(&source_ctx, &mut source_index);

        let target_ctx = LintContext::new("# Target\n", crate::config::MarkdownFlavor::Standard, None);
        let mut target_index = FileIndex::default();
        rule.contribute_to_index(&target_ctx, &mut target_index);

        let source_path = PathBuf::from("docs/source.md");
        let mut workspace = crate::workspace_index::WorkspaceIndex::new();
        workspace.insert_file(source_path.clone(), source_index.clone());
        workspace.insert_file(PathBuf::from("docs/other.md"), target_index);

        let warnings = rule.cross_file_check(&source_path, &source_index, &workspace).unwrap();

        assert_eq!(
            warnings.len(),
            1,
            "Only the unresolved fragment is reported. Got: {warnings:?}"
        );
        assert_eq!(warnings[0].message, "Link fragment 'missing' not found in 'other.md'");
        assert_eq!(warnings[0].line, 2);
        assert_eq!(warnings[0].column, 11);
    }

    #[test]
    fn a_query_string_does_not_hide_the_target_file() {
        let rule = MD051LinkFragments::new();
        let source = "# Source\n\n- [a](other.md?raw=true#missing)\n- [b](other.md?raw=true#target)\n";

        let source_ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        let mut source_index = FileIndex::default();
        rule.contribute_to_index(&source_ctx, &mut source_index);

        let target_ctx = LintContext::new("# Target\n", crate::config::MarkdownFlavor::Standard, None);
        let mut target_index = FileIndex::default();
        rule.contribute_to_index(&target_ctx, &mut target_index);

        let source_path = PathBuf::from("docs/source.md");
        let mut workspace = crate::workspace_index::WorkspaceIndex::new();
        workspace.insert_file(source_path.clone(), source_index.clone());
        workspace.insert_file(PathBuf::from("docs/other.md"), target_index);

        let warnings = rule.cross_file_check(&source_path, &source_index, &workspace).unwrap();

        assert_eq!(
            warnings.len(),
            1,
            "The query is stripped to find the file, so both fragments resolve against it. Got: {warnings:?}"
        );
        assert_eq!(
            warnings[0].message,
            "Link fragment 'missing' not found in 'other.md?raw=true'"
        );
        assert_eq!(warnings[0].line, 3);
    }

    #[test]
    fn a_query_string_does_not_hide_an_extensionless_target_file() {
        let rule = MD051LinkFragments::new();
        let source = "# Source\n\n- [a](other?raw=true#target)\n- [b](other#target)\n- [c](other?raw=true#absent)\n";

        let source_ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        let same_document = rule.check(&source_ctx).unwrap();
        assert!(
            same_document.is_empty(),
            "Every fragment here belongs to another file, so none is a missing anchor of this one. Got: {same_document:?}"
        );

        let mut source_index = FileIndex::default();
        rule.contribute_to_index(&source_ctx, &mut source_index);

        let target_ctx = LintContext::new("# Other\n\n## Target\n", crate::config::MarkdownFlavor::Standard, None);
        let mut target_index = FileIndex::default();
        rule.contribute_to_index(&target_ctx, &mut target_index);

        let source_path = PathBuf::from("docs/source.md");
        let mut workspace = crate::workspace_index::WorkspaceIndex::new();
        workspace.insert_file(source_path.clone(), source_index.clone());
        workspace.insert_file(PathBuf::from("docs/other.md"), target_index);

        let warnings = rule.cross_file_check(&source_path, &source_index, &workspace).unwrap();

        assert_eq!(
            warnings.len(),
            1,
            "The query is stripped before the markdown extension is added. Got: {warnings:?}"
        );
        assert_eq!(
            warnings[0].message,
            "Link fragment 'absent' not found in 'other?raw=true'"
        );
        assert_eq!(warnings[0].line, 5);
    }

    #[test]
    fn a_destination_that_is_only_a_query_stays_on_this_page() {
        let rule = MD051LinkFragments::new();
        let source = "# Source\n\n## Here\n\n- [a](?raw=true#here)\n- [b](?raw=true#nowhere)\n";

        let source_ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&source_ctx).unwrap();

        assert_eq!(
            warnings.len(),
            1,
            "Only the absent anchor is reported. Got: {warnings:?}"
        );
        assert_eq!(
            warnings[0].message,
            "Link anchor '#nowhere' does not exist in document headings"
        );
        assert_eq!(warnings[0].line, 6);

        let mut source_index = FileIndex::default();
        rule.contribute_to_index(&source_ctx, &mut source_index);
        assert!(
            source_index.cross_file_links.is_empty(),
            "A query with no path names no other file. Got: {:?}",
            source_index.cross_file_links
        );
    }

    #[test]
    fn a_frontmatter_path_carrying_a_query_is_indexed() {
        let rule = MD051LinkFragments::from_config_struct(front_matter_checked());
        let source = "---\ntemplate: docs/other.md?raw=true#missing\n---\n\n# Source\n";

        let source_ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        let mut source_index = FileIndex::default();
        rule.contribute_to_index(&source_ctx, &mut source_index);

        assert_eq!(source_index.cross_file_links.len(), 1);
        assert_eq!(source_index.cross_file_links[0].target_path, "docs/other.md?raw=true");
        assert_eq!(source_index.cross_file_links[0].fragment, "missing");
    }

    #[test]
    fn frontmatter_cross_file_paths_are_not_indexed_by_default() {
        let rule = MD051LinkFragments::new();
        let source = "---\ntemplate: other.md#missing\n---\n\n# Source\n";

        let source_ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        let mut source_index = FileIndex::default();
        rule.contribute_to_index(&source_ctx, &mut source_index);

        assert!(
            source_index.cross_file_links.is_empty(),
            "Frontmatter is only checked on request. Got: {:?}",
            source_index.cross_file_links
        );
    }
}
