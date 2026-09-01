use crate::config::MarkdownFlavor;
use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// GFM security tags that are filtered/disallowed by default in GitHub Flavored Markdown.
/// These tags can execute scripts, load external content, or otherwise pose security risks.
///
/// Reference: <https://github.github.com/gfm/#disallowed-raw-html-extension->
pub(super) const GFM_DISALLOWED_TAGS: &[&str] = &[
    "title",
    "textarea",
    "style",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "script",
    "plaintext",
];

/// HTML tags that have unambiguous Markdown equivalents and can be safely auto-fixed.
/// These conversions are lossless for simple cases (no attributes, no nesting).
pub(super) const SAFE_FIXABLE_TAGS: &[&str] = &[
    "em", "i", // italic: *text*
    "strong", "b",    // bold: **text**
    "code", // inline code: `text`
    "br",   // line break
    "hr",   // horizontal rule: ---
    "a",    // link: [text](url) - requires href attribute
    "img",  // image: ![alt](src) - requires src attribute
];

/// Tags that require attribute extraction for conversion (unlike simple tags like em/strong).
/// These tags are fixable only when they have the required attributes.
pub(super) const ATTRIBUTE_FIXABLE_TAGS: &[&str] = &["a", "img"];

/// Value accepted inside `allowed-elements` and `table-allowed-elements` that permits
/// every element Markdown has no syntax for.
pub(super) const NO_MARKDOWN_EQUIVALENT: &str = "no-markdown-equivalent";

/// Elements CommonMark and GFM express with their own syntax, so writing them as HTML
/// is the very thing this rule is about.
///
/// Sorted for binary search - must remain sorted when adding elements.
const MARKDOWN_EQUIVALENT_TAGS: &[&str] = &[
    "a",          // link: [text](url)
    "b",          // bold: **text**
    "blockquote", // > quote
    "br",         // hard break: two trailing spaces
    "code",       // inline code: `text`
    "del",        // strikethrough: ~~text~~
    "em",         // italic: *text*
    "h1",         // heading: # text
    "h2",         // heading: ## text
    "h3",         // heading: ### text
    "h4",         // heading: #### text
    "h5",         // heading: ##### text
    "h6",         // heading: ###### text
    "hr",         // thematic break: ---
    "i",          // italic: *text*
    "img",        // image: ![alt](src)
    "li",         // list item: - text
    "ol",         // ordered list: 1. text
    "p",          // paragraph: a blank line between blocks
    "pre",        // code block: a fence or an indent
    "s",          // strikethrough: ~~text~~
    "strike",     // strikethrough: ~~text~~
    "strong",     // bold: **text**
    "table",      // GFM table
    "tbody",      // GFM table
    "td",         // GFM table cell
    "th",         // GFM table header cell
    "thead",      // GFM table header row
    "tr",         // GFM table row
    "ul",         // unordered list: - text
];

/// Elements a flavor's own syntax expresses beyond `MARKDOWN_EQUIVALENT_TAGS`.
///
/// Only syntax the flavor enables by itself counts. Flavors whose sub/sup/highlight
/// syntax comes from an optional extension the reader has to switch on are absent.
fn flavor_equivalent_tags(flavor: MarkdownFlavor) -> &'static [&'static str] {
    match flavor {
        // Pandoc writes subscript as `~text~`, superscript as `^text^`, and has
        // definition lists. Quarto is Pandoc-based and shares all four.
        MarkdownFlavor::Pandoc | MarkdownFlavor::Quarto => &["dd", "dl", "dt", "sub", "sup"],
        // Obsidian writes a highlight as `==text==`.
        MarkdownFlavor::Obsidian => &["mark"],
        _ => &[],
    }
}

/// Whether Markdown can express this element, so it keeps being reported under
/// `"no-markdown-equivalent"`.
///
/// A hard break is the one element whose answer depends on where it sits: two
/// trailing spaces do not survive inside a GFM table cell, so `<br>` has no
/// equivalent there.
fn has_markdown_equivalent(tag_name: &str, flavor: MarkdownFlavor, in_table: bool) -> bool {
    if in_table && tag_name == "br" {
        return false;
    }
    MARKDOWN_EQUIVALENT_TAGS.binary_search(&tag_name).is_ok() || flavor_equivalent_tags(flavor).contains(&tag_name)
}

/// Whether `"no-markdown-equivalent"` permits this element.
///
/// The tags GFM strips from rendered Markdown are never permitted: no reader shows
/// them, so writing them is not the expressiveness this option is about.
pub(super) fn is_permitted_without_markdown_equivalent(tag_name: &str, flavor: MarkdownFlavor, in_table: bool) -> bool {
    let lower = tag_name.to_ascii_lowercase();
    !GFM_DISALLOWED_TAGS.contains(&lower.as_str()) && !has_markdown_equivalent(&lower, flavor, in_table)
}

/// Whether a configured value is the `no-markdown-equivalent` sentinel.
///
/// Config *values* are not normalized on the way in, so the snake_case spelling a
/// user may reach for is recognized here.
fn is_no_markdown_equivalent(value: &str) -> bool {
    value.to_lowercase().replace('_', "-") == NO_MARKDOWN_EQUIVALENT
}

/// URL schemes that are safe to convert to Markdown links.
/// Dangerous schemes like javascript: or data: are rejected.
pub(super) const SAFE_URL_SCHEMES: &[&str] = &["http://", "https://", "mailto:", "tel:", "ftp://", "ftps://"];

/// URL schemes that are explicitly dangerous and must not be converted.
pub(super) const DANGEROUS_URL_SCHEMES: &[&str] = &["javascript:", "vbscript:", "data:", "about:", "blob:", "file:"];

/// Style for converting `<br>` tags to Markdown line breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrStyle {
    /// Use two trailing spaces followed by newline (CommonMark standard)
    #[default]
    TrailingSpaces,
    /// Use backslash followed by newline (Pandoc/extended markdown)
    Backslash,
}

/// Auto-fix conversion strictness for MD033.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MD033FixMode {
    /// Preserve existing behavior: skip conversions when significant extra
    /// attributes are present.
    #[default]
    Conservative,
    /// Allow conversion by dropping configured extra attributes.
    Relaxed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD033Config {
    #[serde(default, rename = "allowed-elements", alias = "allowed_elements", alias = "allowed")]
    pub allowed: Vec<String>,

    /// Elements whose contents are permitted whatever tags appear inside them.
    ///
    /// Everything from an element's opening tag through its matching closing tag is
    /// left alone, the tags of the element itself included. An element that cannot
    /// hold content (`<br>`, `<img>`, and the other void elements) holds nothing, so
    /// naming one here permits nothing; name it in `allowed` instead.
    #[serde(default, rename = "allowed-inside", alias = "allowed_inside")]
    pub allowed_inside: Vec<String>,

    /// List of HTML tags that are explicitly disallowed.
    /// When set, only these tags will trigger warnings (allowlist mode is disabled).
    /// Use `"gfm"` as a special value to use GFM's security-filtered tags.
    #[serde(
        default,
        rename = "disallowed-elements",
        alias = "disallowed_elements",
        alias = "disallowed"
    )]
    pub disallowed: Vec<String>,

    /// Enable auto-fix to convert simple HTML tags to Markdown equivalents.
    /// When enabled, tags like `<em>`, `<strong>`, `<code>`, `<br>`, `<hr>` are converted.
    /// Tags with attributes or complex nesting are not auto-fixed.
    /// Default: false (opt-in like MD036)
    #[serde(default)]
    pub fix: bool,

    /// Attribute handling mode for auto-fix.
    /// - conservative: current safe behavior (default)
    /// - relaxed: allow dropping configured attributes during conversion
    #[serde(default, rename = "fix-mode", alias = "fix_mode")]
    pub fix_mode: MD033FixMode,

    /// Extra attributes that may be dropped when `fix-mode = "relaxed"`.
    /// These attributes are not representable in Markdown link/image syntax.
    #[serde(
        default = "default_drop_attributes",
        rename = "drop-attributes",
        alias = "drop_attributes"
    )]
    pub drop_attributes: Vec<String>,

    /// Wrapper elements that may be stripped when `fix-mode = "relaxed"`.
    /// Wrapper stripping is applied only when the wrapper's inner content no
    /// longer contains HTML tags.
    #[serde(
        default = "default_strip_wrapper_elements",
        rename = "strip-wrapper-elements",
        alias = "strip_wrapper_elements"
    )]
    pub strip_wrapper_elements: Vec<String>,

    /// Style for converting `<br>` tags to Markdown line breaks.
    /// - "trailing-spaces": Two spaces + newline (CommonMark standard, default)
    /// - "backslash": Backslash + newline (Pandoc/extended markdown)
    #[serde(default, rename = "br-style", alias = "br_style")]
    pub br_style: BrStyle,

    /// HTML elements explicitly permitted inside GFM table cells.
    ///
    /// Mirrors markdownlint's `table_allowed_elements`. The semantics
    /// distinguish three states:
    /// - `None` (unset): in-table tags fall back to the `allowed` list.
    /// - `Some(vec![])`: no tags are permitted inside table cells, even
    ///   ones present in `allowed`.
    /// - `Some([...])`: only the listed tags are permitted inside table
    ///   cells; `allowed` is ignored for in-table contexts.
    ///
    /// Tags outside GFM tables are never affected by this option.
    // Config keys reach serde already normalized to lowercase kebab-case, so the
    // kebab spelling of every accepted key has to be declared; a snake_case-only
    // alias never matches and silently drops the value.
    #[serde(
        default,
        rename = "table-allowed-elements",
        alias = "table_allowed_elements",
        alias = "table-allowed",
        alias = "table_allowed"
    )]
    pub table_allowed_elements: Option<Vec<String>>,
}

impl Default for MD033Config {
    fn default() -> Self {
        Self {
            allowed: Vec::new(),
            allowed_inside: Vec::new(),
            disallowed: Vec::new(),
            fix: false,
            fix_mode: MD033FixMode::default(),
            drop_attributes: default_drop_attributes(),
            strip_wrapper_elements: default_strip_wrapper_elements(),
            br_style: BrStyle::default(),
            table_allowed_elements: None,
        }
    }
}

fn default_drop_attributes() -> Vec<String> {
    vec!["target", "rel", "width", "height", "align", "class", "id", "style"]
        .into_iter()
        .map(ToString::to_string)
        .collect()
}

fn default_strip_wrapper_elements() -> Vec<String> {
    vec!["p".to_string()]
}

impl MD033Config {
    /// Convert allowed elements to HashSet for efficient lookup.
    /// The `no-markdown-equivalent` sentinel names no element, so it is not a member.
    pub fn allowed_set(&self) -> HashSet<String> {
        Self::element_set(&self.allowed)
    }

    /// Convert the elements whose contents are permitted to a HashSet.
    pub fn allowed_inside_set(&self) -> HashSet<String> {
        Self::element_set(&self.allowed_inside)
    }

    /// Resolve the effective allowlist for tags inside GFM table cells.
    ///
    /// When `table_allowed_elements` is unset, falls back to `allowed_set`
    /// (matching markdownlint's `table_allowed_elements || allowed_elements`
    /// precedence). When set (even to an empty vec), takes precedence inside tables.
    pub fn table_allowed_set(&self) -> HashSet<String> {
        match &self.table_allowed_elements {
            Some(list) => Self::element_set(list),
            None => self.allowed_set(),
        }
    }

    /// Whether `allowed_elements` permits every element Markdown has no syntax for.
    pub fn allows_no_markdown_equivalent(&self) -> bool {
        self.allowed.iter().any(|value| is_no_markdown_equivalent(value))
    }

    /// Whether the allowlist that governs GFM table cells permits every element
    /// Markdown has no syntax for, following the same fallback as `table_allowed_set`.
    pub fn table_allows_no_markdown_equivalent(&self) -> bool {
        match &self.table_allowed_elements {
            Some(list) => list.iter().any(|value| is_no_markdown_equivalent(value)),
            None => self.allows_no_markdown_equivalent(),
        }
    }

    /// Lowercase the element names in a configured list, leaving out the sentinel
    /// values that stand for a set of elements rather than one.
    fn element_set(values: &[String]) -> HashSet<String> {
        values
            .iter()
            .filter(|value| !is_no_markdown_equivalent(value))
            .map(|value| value.to_lowercase())
            .collect()
    }

    /// Convert disallowed elements to HashSet for efficient lookup.
    /// If the list contains "gfm", expands to the GFM security tags.
    pub fn disallowed_set(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        for tag in &self.disallowed {
            let lower = tag.to_lowercase();
            if lower == "gfm" {
                // Expand "gfm" to all GFM security tags
                for gfm_tag in GFM_DISALLOWED_TAGS {
                    set.insert((*gfm_tag).to_string());
                }
            } else {
                set.insert(lower);
            }
        }
        set
    }

    /// Check if the rule is operating in disallowed-only mode
    pub fn is_disallowed_mode(&self) -> bool {
        !self.disallowed.is_empty()
    }

    /// Check if a tag is safe to auto-fix (has a simple Markdown equivalent)
    pub fn is_safe_fixable_tag(tag_name: &str) -> bool {
        SAFE_FIXABLE_TAGS.contains(&tag_name.to_ascii_lowercase().as_str())
    }

    /// Check if a tag requires attribute extraction for conversion
    pub fn requires_attribute_extraction(tag_name: &str) -> bool {
        ATTRIBUTE_FIXABLE_TAGS.contains(&tag_name.to_ascii_lowercase().as_str())
    }

    /// Convert drop attributes to lowercase `HashSet` for efficient lookup.
    pub fn drop_attributes_set(&self) -> HashSet<String> {
        self.drop_attributes.iter().map(|s| s.to_lowercase()).collect()
    }

    /// Convert wrapper elements to lowercase `HashSet` for efficient lookup.
    pub fn strip_wrapper_elements_set(&self) -> HashSet<String> {
        self.strip_wrapper_elements.iter().map(|s| s.to_lowercase()).collect()
    }

    /// Decode percent-encoded characters in a URL for safety checking.
    /// This prevents bypass attempts like `java%73cript:` for `javascript:`.
    fn decode_percent_encoding(url: &str) -> String {
        let mut result = String::with_capacity(url.len());
        let mut chars = url.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '%' {
                // Try to read two hex digits
                let hex: String = chars.by_ref().take(2).collect();
                if hex.len() == 2
                    && let Ok(byte) = u8::from_str_radix(&hex, 16)
                {
                    result.push(byte as char);
                    continue;
                }
                // Invalid encoding, keep as-is
                result.push('%');
                result.push_str(&hex);
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Decode common HTML entities in URLs.
    /// This prevents bypass attempts like `javascript&#58;` for `javascript:`.
    fn decode_html_entities(url: &str) -> String {
        url.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&#58;", ":")
            .replace("&#x3a;", ":")
            .replace("&#x3A;", ":")
            .replace("&#47;", "/")
            .replace("&#x2f;", "/")
            .replace("&#x2F;", "/")
    }

    /// Check if a URL scheme is safe to convert to Markdown.
    /// Safe URLs include: absolute URLs with safe schemes, relative URLs, fragments, empty.
    /// Dangerous schemes (javascript:, data:, etc.) are rejected.
    /// This function decodes percent-encoding and HTML entities to prevent bypass attacks.
    pub fn is_safe_url(url: &str) -> bool {
        // Decode URL to catch encoding bypass attempts
        let decoded = Self::decode_percent_encoding(url);
        let decoded = Self::decode_html_entities(&decoded);
        let url_lower = decoded.to_ascii_lowercase();
        let trimmed = url_lower.trim();

        // Empty URLs are safe (though the link will be useless)
        if trimmed.is_empty() {
            return true;
        }

        // Check for dangerous schemes first (after decoding)
        for scheme in DANGEROUS_URL_SCHEMES {
            if trimmed.starts_with(scheme) {
                return false;
            }
        }

        // Also check without the colon in case of partial encoding
        let dangerous_prefixes: &[&str] = &["javascript", "vbscript", "data", "about", "blob", "file"];
        for prefix in dangerous_prefixes {
            // Check for scheme with any variation of colon encoding
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                // After the prefix, should be followed by : or encoded :
                if rest.starts_with(':') || rest.starts_with("%3a") || rest.starts_with("&#") {
                    return false;
                }
            }
        }

        // Relative URLs and fragments are safe
        // These include: /path, ./path, ../path, #anchor, ?query, path/to/file
        if trimmed.starts_with('/') || trimmed.starts_with('.') || trimmed.starts_with('#') || trimmed.starts_with('?')
        {
            return true;
        }

        // Check for safe absolute schemes
        for scheme in SAFE_URL_SCHEMES {
            if trimmed.starts_with(scheme) {
                return true;
            }
        }

        // Protocol-relative URLs (//example.com) are safe
        if trimmed.starts_with("//") {
            return true;
        }

        // URLs without a scheme (relative paths like "path/to/file.html") are safe
        // They don't contain ":" before any "/" which would indicate a scheme
        if let Some(colon_pos) = trimmed.find(':') {
            if let Some(slash_pos) = trimmed.find('/') {
                // If colon comes after slash, it's a relative path with a port or something else
                if colon_pos > slash_pos {
                    return true;
                }
            }
            // Has a colon before any slash - likely an unknown scheme, reject for safety
            false
        } else {
            // No colon at all - relative path, safe
            true
        }
    }
}

impl RuleConfig for MD033Config {
    const RULE_NAME: &'static str = "MD033";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_lists_searched_by_binary_search_are_sorted() {
        assert!(MARKDOWN_EQUIVALENT_TAGS.is_sorted(), "{MARKDOWN_EQUIVALENT_TAGS:?}");
    }

    #[test]
    fn no_markdown_equivalent_accepts_the_spellings_a_user_reaches_for() {
        // Config values are not normalized on the way in, unlike config keys.
        for spelling in [
            "no-markdown-equivalent",
            "no_markdown_equivalent",
            "No-Markdown-Equivalent",
        ] {
            assert!(is_no_markdown_equivalent(spelling), "{spelling}");
        }
        assert!(!is_no_markdown_equivalent("nomarkdownequivalent"));
        assert!(!is_no_markdown_equivalent("kbd"));
    }

    #[test]
    fn the_sentinel_names_no_element_of_its_own() {
        let config = MD033Config {
            allowed: vec!["no-markdown-equivalent".to_string(), "kbd".to_string()],
            ..MD033Config::default()
        };
        assert!(config.allows_no_markdown_equivalent());
        assert_eq!(config.allowed_set(), HashSet::from(["kbd".to_string()]));
    }

    #[test]
    fn a_table_allowlist_decides_the_sentinel_for_table_cells() {
        let sentinel_everywhere = MD033Config {
            allowed: vec!["no-markdown-equivalent".to_string()],
            ..MD033Config::default()
        };
        assert!(
            sentinel_everywhere.table_allows_no_markdown_equivalent(),
            "an unset table allowlist falls back to allowed-elements"
        );

        let table_overrides = MD033Config {
            allowed: vec!["no-markdown-equivalent".to_string()],
            table_allowed_elements: Some(vec!["br".to_string()]),
            ..MD033Config::default()
        };
        assert!(
            !table_overrides.table_allows_no_markdown_equivalent(),
            "an explicit table allowlist is the whole answer for a table cell"
        );
    }

    #[test]
    fn a_hard_break_has_no_equivalent_inside_a_table_cell() {
        let flavor = MarkdownFlavor::Standard;
        assert!(!is_permitted_without_markdown_equivalent("br", flavor, false));
        assert!(is_permitted_without_markdown_equivalent("br", flavor, true));
        // Every other element answers the same in both places.
        for tag in ["b", "em", "table", "kbd", "details"] {
            assert_eq!(
                is_permitted_without_markdown_equivalent(tag, flavor, false),
                is_permitted_without_markdown_equivalent(tag, flavor, true),
                "{tag}"
            );
        }
    }

    #[test]
    fn tags_no_reader_renders_are_never_permitted() {
        for flavor in [
            MarkdownFlavor::Standard,
            MarkdownFlavor::Pandoc,
            MarkdownFlavor::Obsidian,
        ] {
            for tag in GFM_DISALLOWED_TAGS {
                assert!(
                    !is_permitted_without_markdown_equivalent(tag, flavor, false),
                    "{tag} under {flavor:?}"
                );
                assert!(
                    !is_permitted_without_markdown_equivalent(tag, flavor, true),
                    "{tag} under {flavor:?} in a table"
                );
            }
        }
    }
}
