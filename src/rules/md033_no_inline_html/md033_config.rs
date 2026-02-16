use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// GFM security tags that are filtered/disallowed by default in GitHub Flavored Markdown.
/// These tags can execute scripts, load external content, or otherwise pose security risks.
///
/// Reference: <https://github.github.com/gfm/#disallowed-raw-html-extension->
pub const GFM_DISALLOWED_TAGS: &[&str] = &[
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
pub const SAFE_FIXABLE_TAGS: &[&str] = &[
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
pub const ATTRIBUTE_FIXABLE_TAGS: &[&str] = &["a", "img"];

/// URL schemes that are safe to convert to Markdown links.
/// Dangerous schemes like javascript: or data: are rejected.
pub const SAFE_URL_SCHEMES: &[&str] = &["http://", "https://", "mailto:", "tel:", "ftp://", "ftps://"];

/// URL schemes that are explicitly dangerous and must not be converted.
pub const DANGEROUS_URL_SCHEMES: &[&str] = &["javascript:", "vbscript:", "data:", "about:", "blob:", "file:"];

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
}

impl Default for MD033Config {
    fn default() -> Self {
        Self {
            allowed: Vec::new(),
            disallowed: Vec::new(),
            fix: false,
            fix_mode: MD033FixMode::default(),
            drop_attributes: default_drop_attributes(),
            strip_wrapper_elements: default_strip_wrapper_elements(),
            br_style: BrStyle::default(),
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
    /// Convert allowed elements to HashSet for efficient lookup
    pub fn allowed_set(&self) -> HashSet<String> {
        self.allowed.iter().map(|s| s.to_lowercase()).collect()
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
