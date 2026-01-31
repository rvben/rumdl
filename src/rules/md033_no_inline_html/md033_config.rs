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
];

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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

    /// Style for converting `<br>` tags to Markdown line breaks.
    /// - "trailing-spaces": Two spaces + newline (CommonMark standard, default)
    /// - "backslash": Backslash + newline (Pandoc/extended markdown)
    #[serde(default, rename = "br-style", alias = "br_style")]
    pub br_style: BrStyle,
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
}

impl RuleConfig for MD033Config {
    const RULE_NAME: &'static str = "MD033";
}
