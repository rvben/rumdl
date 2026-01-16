/// MkDocs emoji and icon extension support
///
/// This module provides support for the MkDocs Material emoji/icons extension,
/// which allows using shortcodes for various icon sets:
/// - Material Design Icons: `:material-check:`
/// - GitHub Octicons: `:octicons-mark-github-16:`
/// - FontAwesome: `:fontawesome-brands-github:`
/// - Simple Icons: `:simple-github:`
/// - Custom icons: `:custom-icon-name:`
///
/// ## Syntax
///
/// ```markdown
/// :material-check:           # Material Design icon
/// :octicons-mark-github-16:  # GitHub Octicon with size
/// :fontawesome-brands-github: # FontAwesome brand icon
/// :fontawesome-solid-star:   # FontAwesome solid icon
/// :simple-github:            # Simple Icons
/// ```
///
/// ## References
///
/// - [MkDocs Material Icons](https://squidfunk.github.io/mkdocs-material/reference/icons-emojis/)
/// - [Python-Markdown Emoji](https://facelessuser.github.io/pymdown-extensions/extensions/emoji/)
use regex::Regex;
use std::sync::LazyLock;

/// Pattern to match MkDocs icon shortcodes
/// Format: `:prefix-name:` or `:prefix-name-modifier:`
/// Examples: :material-check:, :octicons-mark-github-16:, :fontawesome-brands-github:
///
/// Pattern breakdown:
/// - Starts and ends with `:`
/// - First part is the icon set prefix (material, octicons, fontawesome, simple, custom, etc.)
/// - Followed by hyphen-separated parts (name, modifiers, sizes)
/// - Each part is lowercase alphanumeric with optional underscores
static ICON_SHORTCODE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":([a-z][a-z0-9_]*(?:-[a-z0-9_]+)+):").unwrap());

/// Pattern to match standard emoji shortcodes (GitHub style)
/// Format: `:emoji_name:` or `:emoji-name:`
/// Examples: :smile:, :thumbsup:, :+1:, :heart:
static EMOJI_SHORTCODE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r":([a-zA-Z0-9_+-]+):").unwrap());

/// Known MkDocs icon set prefixes
pub const ICON_SET_PREFIXES: &[&str] = &["material", "octicons", "fontawesome", "simple", "custom"];

/// Parsed icon shortcode information
#[derive(Debug, Clone, PartialEq)]
pub struct IconShortcode {
    /// The full shortcode text including colons (e.g., `:material-check:`)
    pub full_text: String,
    /// The icon set prefix (e.g., `material`, `octicons`)
    pub prefix: String,
    /// The icon name parts after the prefix (e.g., `["check"]` or `["mark", "github", "16"]`)
    pub name_parts: Vec<String>,
    /// Start position in the line (0-indexed)
    pub start: usize,
    /// End position in the line (0-indexed, exclusive)
    pub end: usize,
}

impl IconShortcode {
    /// Get the full icon name (prefix + name parts joined with hyphens)
    pub fn full_name(&self) -> String {
        if self.name_parts.is_empty() {
            self.prefix.clone()
        } else {
            format!("{}-{}", self.prefix, self.name_parts.join("-"))
        }
    }

    /// Check if this is a known MkDocs icon set
    pub fn is_known_icon_set(&self) -> bool {
        ICON_SET_PREFIXES.iter().any(|&p| self.prefix.starts_with(p))
    }
}

/// Check if a line contains icon shortcodes
#[inline]
pub fn contains_icon_shortcode(line: &str) -> bool {
    // Fast path: check for colon first
    if !line.contains(':') {
        return false;
    }
    ICON_SHORTCODE_PATTERN.is_match(line)
}

/// Check if a line contains any emoji/icon shortcode (both MkDocs icons and standard emoji)
#[inline]
pub fn contains_any_shortcode(line: &str) -> bool {
    if !line.contains(':') {
        return false;
    }
    ICON_SHORTCODE_PATTERN.is_match(line) || EMOJI_SHORTCODE_PATTERN.is_match(line)
}

/// Find all icon shortcodes in a line
pub fn find_icon_shortcodes(line: &str) -> Vec<IconShortcode> {
    if !line.contains(':') {
        return Vec::new();
    }

    let mut results = Vec::new();

    for m in ICON_SHORTCODE_PATTERN.find_iter(line) {
        let full_text = m.as_str().to_string();
        // Remove the surrounding colons and split by hyphen
        let inner = &full_text[1..full_text.len() - 1];
        let parts: Vec<&str> = inner.split('-').collect();

        if parts.is_empty() {
            continue;
        }

        let prefix = parts[0].to_string();
        let name_parts: Vec<String> = parts[1..].iter().map(|&s| s.to_string()).collect();

        results.push(IconShortcode {
            full_text,
            prefix,
            name_parts,
            start: m.start(),
            end: m.end(),
        });
    }

    results
}

/// Check if a position in a line is within an icon shortcode
pub fn is_in_icon_shortcode(line: &str, position: usize) -> bool {
    for shortcode in find_icon_shortcodes(line) {
        if shortcode.start <= position && position < shortcode.end {
            return true;
        }
    }
    false
}

/// Check if a position in a line is within any emoji/icon shortcode
pub fn is_in_any_shortcode(line: &str, position: usize) -> bool {
    if !line.contains(':') {
        return false;
    }

    // Check MkDocs icon shortcodes
    for m in ICON_SHORTCODE_PATTERN.find_iter(line) {
        if m.start() <= position && position < m.end() {
            return true;
        }
    }

    // Check standard emoji shortcodes
    for m in EMOJI_SHORTCODE_PATTERN.find_iter(line) {
        if m.start() <= position && position < m.end() {
            return true;
        }
    }

    false
}

/// Replace icon shortcodes with placeholder text to avoid false positives in other rules
///
/// This is useful for rules like MD037 that might incorrectly flag
/// characters inside icon shortcodes.
pub fn mask_icon_shortcodes(line: &str) -> String {
    if !line.contains(':') {
        return line.to_string();
    }

    let mut result = line.to_string();
    let shortcodes = find_icon_shortcodes(line);

    // Process in reverse order to maintain correct positions
    for shortcode in shortcodes.into_iter().rev() {
        let replacement = " ".repeat(shortcode.end - shortcode.start);
        result.replace_range(shortcode.start..shortcode.end, &replacement);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_icon_shortcode() {
        // Valid MkDocs icon shortcodes
        assert!(contains_icon_shortcode("Check :material-check: this"));
        assert!(contains_icon_shortcode(":octicons-mark-github-16:"));
        assert!(contains_icon_shortcode(":fontawesome-brands-github:"));
        assert!(contains_icon_shortcode(":fontawesome-solid-star:"));
        assert!(contains_icon_shortcode(":simple-github:"));

        // Not icon shortcodes (no hyphen in name)
        assert!(!contains_icon_shortcode(":smile:"));
        assert!(!contains_icon_shortcode(":thumbsup:"));
        assert!(!contains_icon_shortcode("No icons here"));
        assert!(!contains_icon_shortcode("Just text"));
    }

    #[test]
    fn test_find_icon_shortcodes_material() {
        let shortcodes = find_icon_shortcodes("Click :material-check: to confirm");
        assert_eq!(shortcodes.len(), 1);
        assert_eq!(shortcodes[0].full_text, ":material-check:");
        assert_eq!(shortcodes[0].prefix, "material");
        assert_eq!(shortcodes[0].name_parts, vec!["check"]);
        assert!(shortcodes[0].is_known_icon_set());
    }

    #[test]
    fn test_find_icon_shortcodes_octicons() {
        let shortcodes = find_icon_shortcodes(":octicons-mark-github-16:");
        assert_eq!(shortcodes.len(), 1);
        assert_eq!(shortcodes[0].prefix, "octicons");
        assert_eq!(shortcodes[0].name_parts, vec!["mark", "github", "16"]);
        assert!(shortcodes[0].is_known_icon_set());
    }

    #[test]
    fn test_find_icon_shortcodes_fontawesome() {
        let shortcodes = find_icon_shortcodes(":fontawesome-brands-github:");
        assert_eq!(shortcodes.len(), 1);
        assert_eq!(shortcodes[0].prefix, "fontawesome");
        assert_eq!(shortcodes[0].name_parts, vec!["brands", "github"]);

        let shortcodes = find_icon_shortcodes(":fontawesome-solid-star:");
        assert_eq!(shortcodes.len(), 1);
        assert_eq!(shortcodes[0].name_parts, vec!["solid", "star"]);
    }

    #[test]
    fn test_find_icon_shortcodes_multiple() {
        let shortcodes = find_icon_shortcodes(":material-check: and :material-close:");
        assert_eq!(shortcodes.len(), 2);
        assert_eq!(shortcodes[0].full_text, ":material-check:");
        assert_eq!(shortcodes[1].full_text, ":material-close:");
    }

    #[test]
    fn test_icon_shortcode_full_name() {
        let shortcodes = find_icon_shortcodes(":octicons-mark-github-16:");
        assert_eq!(shortcodes[0].full_name(), "octicons-mark-github-16");
    }

    #[test]
    fn test_is_in_icon_shortcode() {
        let line = "Text :material-check: more text";
        assert!(!is_in_icon_shortcode(line, 0)); // "T"
        assert!(!is_in_icon_shortcode(line, 4)); // " "
        assert!(is_in_icon_shortcode(line, 5)); // ":"
        assert!(is_in_icon_shortcode(line, 10)); // "a"
        assert!(is_in_icon_shortcode(line, 20)); // ":"
        assert!(!is_in_icon_shortcode(line, 21)); // " "
    }

    #[test]
    fn test_mask_icon_shortcodes() {
        let line = "Text :material-check: more";
        let masked = mask_icon_shortcodes(line);
        assert_eq!(masked, "Text                  more");
        assert_eq!(masked.len(), line.len());

        let line2 = ":material-a: and :material-b:";
        let masked2 = mask_icon_shortcodes(line2);
        assert!(!masked2.contains(":material"));
        assert_eq!(masked2.len(), line2.len());
    }

    #[test]
    fn test_shortcode_positions() {
        let line = "Pre :material-check: post";
        let shortcodes = find_icon_shortcodes(line);
        assert_eq!(shortcodes.len(), 1);
        assert_eq!(shortcodes[0].start, 4);
        assert_eq!(shortcodes[0].end, 20);
        assert_eq!(&line[shortcodes[0].start..shortcodes[0].end], ":material-check:");
    }

    #[test]
    fn test_unknown_icon_set() {
        let shortcodes = find_icon_shortcodes(":custom-my-icon:");
        assert_eq!(shortcodes.len(), 1);
        assert_eq!(shortcodes[0].prefix, "custom");
        assert!(shortcodes[0].is_known_icon_set());

        let shortcodes = find_icon_shortcodes(":unknown-prefix-icon:");
        assert_eq!(shortcodes.len(), 1);
        assert!(!shortcodes[0].is_known_icon_set());
    }

    #[test]
    fn test_emoji_vs_icon() {
        // Standard emoji (single word) - not matched by icon pattern
        assert!(!contains_icon_shortcode(":smile:"));
        assert!(!contains_icon_shortcode(":+1:"));

        // MkDocs icons (hyphenated) - matched
        assert!(contains_icon_shortcode(":material-check:"));

        // But both are "any shortcode"
        assert!(contains_any_shortcode(":smile:"));
        assert!(contains_any_shortcode(":material-check:"));
    }

    #[test]
    fn test_is_in_any_shortcode() {
        let line = ":smile: and :material-check:";

        // In emoji
        assert!(is_in_any_shortcode(line, 0)); // ":"
        assert!(is_in_any_shortcode(line, 3)); // "l"
        assert!(is_in_any_shortcode(line, 6)); // ":"

        // Between shortcodes
        assert!(!is_in_any_shortcode(line, 7)); // " "
        assert!(!is_in_any_shortcode(line, 10)); // "d"

        // In icon
        assert!(is_in_any_shortcode(line, 12)); // ":"
        assert!(is_in_any_shortcode(line, 20)); // "c"
    }

    #[test]
    fn test_underscore_in_icon_names() {
        let shortcodes = find_icon_shortcodes(":material-file_download:");
        assert_eq!(shortcodes.len(), 1);
        assert_eq!(shortcodes[0].name_parts, vec!["file_download"]);
    }
}
