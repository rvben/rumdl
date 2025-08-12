use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::regex_cache::get_cached_regex;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

lazy_static! {
    // Pre-compiled optimized patterns for quick checks
    static ref QUICK_MARKDOWN_CHECK: Regex = Regex::new(r"[*_`\[\]]").unwrap();
    // Match emphasis only when not part of snake_case (requires space or start/end)
    static ref EMPHASIS_PATTERN: Regex = Regex::new(r"\*+([^*]+)\*+|\b_([^_]+)_\b").unwrap();
    static ref CODE_PATTERN: Regex = Regex::new(r"`([^`]+)`").unwrap();
    static ref LINK_PATTERN: Regex = Regex::new(r"\[([^\]]+)\]\([^)]+\)|\[([^\]]+)\]\[[^\]]*\]").unwrap();
    static ref TOC_SECTION_START: Regex = Regex::new(r"(?i)^#+\s*(table\s+of\s+contents?|contents?|toc)\s*$").unwrap();
}

/// Rule MD051: Link fragments
///
/// See [docs/md051.md](../../docs/md051.md) for full documentation, configuration, and examples.
///
/// This rule validates that link anchors (the part after #) exist in the current document.
/// Only applies to internal document links (like #heading), not to external URLs or cross-file links.
#[derive(Clone)]
pub struct MD051LinkFragments {
    /// Anchor style to use for validation
    anchor_style: AnchorStyle,
}

/// Anchor generation style for heading fragments
#[derive(Clone, Debug, PartialEq)]
pub enum AnchorStyle {
    /// GitHub/GFM style (default): preserves underscores, removes punctuation
    GitHub,
    /// kramdown style: removes underscores and punctuation
    Kramdown,
    /// Bitbucket style: adds 'markdown-header-' prefix
    Bitbucket,
}

impl Default for MD051LinkFragments {
    fn default() -> Self {
        Self::new()
    }
}

impl MD051LinkFragments {
    pub fn new() -> Self {
        Self {
            anchor_style: AnchorStyle::GitHub,
        }
    }

    /// Create with specific anchor style
    pub fn with_anchor_style(style: AnchorStyle) -> Self {
        Self { anchor_style: style }
    }

    /// Extract headings from cached LintContext information
    fn extract_headings_from_context(&self, ctx: &crate::lint_context::LintContext) -> HashSet<String> {
        let mut headings = HashSet::with_capacity(32);
        let mut fragment_counts = std::collections::HashMap::new();
        let mut in_toc = false;

        // Single pass through lines, only processing lines with headings
        for line_info in &ctx.lines {
            // Skip front matter
            if line_info.in_front_matter {
                continue;
            }

            if let Some(heading) = &line_info.heading {
                let line = &line_info.content;

                // Check if we're entering a TOC section
                let is_toc_heading = TOC_SECTION_START.is_match(line);

                // If we were in TOC and hit another heading, we're out of TOC
                if in_toc && !is_toc_heading {
                    in_toc = false;
                }

                // Skip if we're inside a TOC section (but not the TOC heading itself)
                if in_toc && !is_toc_heading {
                    continue;
                }

                // Generate fragment based on configured style
                let fragment = match self.anchor_style {
                    AnchorStyle::GitHub => self.heading_to_fragment_github(&heading.text),
                    AnchorStyle::Kramdown => self.heading_to_fragment_kramdown(&heading.text),
                    AnchorStyle::Bitbucket => self.heading_to_fragment_bitbucket(&heading.text),
                };

                if !fragment.is_empty() {
                    // Handle duplicate fragments by appending numbers
                    let final_fragment = if let Some(count) = fragment_counts.get_mut(&fragment) {
                        let suffix = *count;
                        *count += 1;
                        format!("{fragment}-{suffix}")
                    } else {
                        fragment_counts.insert(fragment.clone(), 1);
                        fragment
                    };
                    headings.insert(final_fragment);
                }

                // After processing the TOC heading, mark that we're in a TOC section
                if is_toc_heading {
                    in_toc = true;
                }
            }
        }

        headings
    }

    /// Fragment generation following GitHub's official algorithm
    /// GitHub preserves most Unicode characters, underscores, and consecutive hyphens
    #[inline]
    fn heading_to_fragment_github(&self, heading: &str) -> String {
        if heading.is_empty() {
            return String::new();
        }

        // Strip markdown formatting first
        let text = if QUICK_MARKDOWN_CHECK.is_match(heading) {
            self.strip_markdown_formatting_fast(heading)
        } else {
            heading.to_string()
        };

        // Trim whitespace first
        let text = text.trim();

        // Follow GitHub's actual algorithm (verified with github-slugger)
        // GitHub PRESERVES most Unicode characters including:
        // - Accented Latin (café, résumé, über)
        // - Chinese, Japanese, Korean
        // - Arabic, Hebrew
        // - Greek, Cyrillic
        // - Hindi and other scripts
        // It only removes emoji and certain punctuation
        let mut fragment = String::with_capacity(text.len());
        let mut last_was_space = false;

        for c in text.to_lowercase().chars() {
            // Check if character should be kept
            if Self::is_valid_github_char(c) {
                fragment.push(c);
                last_was_space = false;
            } else if c == '-' {
                // Always preserve hyphens
                fragment.push('-');
                last_was_space = false;
            } else if c.is_whitespace() {
                // Convert whitespace to hyphens, but avoid consecutive spaces becoming multiple hyphens
                if !last_was_space {
                    fragment.push('-');
                    last_was_space = true;
                }
            } else {
                // Skip emoji, symbols, etc.
                // If the last character we added was not a hyphen, and this character
                // is skipped (like emoji), we might need to add a hyphen if the next
                // character is not whitespace. For now, just skip it.
                // This handles cases like "A🎉B" → "a-b" but we don't implement that complexity yet.
            }
        }

        // Remove leading and trailing hyphens
        fragment.trim_matches('-').to_string()
    }

    /// Check if a character should be preserved in GitHub-style anchors
    #[inline]
    fn is_valid_github_char(c: char) -> bool {
        // First, exclude emoji and symbols even if they're "alphanumeric"
        if matches!(c,
            '\u{1F300}'..='\u{1F9FF}' | // Emoji & Symbols (includes 🎉 at U+1F389)
            '\u{2600}'..='\u{26FF}' |   // Miscellaneous Symbols
            '\u{2700}'..='\u{27BF}' |   // Dingbats
            '\u{2000}'..='\u{206F}' |   // General punctuation
            '\u{2E00}'..='\u{2E7F}' |   // Supplemental punctuation
            '\u{3000}'..='\u{303F}' |   // CJK symbols and punctuation (but keep CJK letters)
            '\u{FE00}'..='\u{FE0F}'     // Variation selectors
        ) {
            return false;
        }

        // Keep ASCII alphanumeric and underscores
        c.is_ascii_alphanumeric() || c == '_' ||
        // Keep Unicode letters and digits (but not emoji, which we filtered above)
        c.is_alphabetic() || c.is_numeric()
    }

    /// Fragment generation following kramdown's algorithm
    /// kramdown strips diacritics but keeps base letters, removes underscores
    #[inline]
    fn heading_to_fragment_kramdown(&self, heading: &str) -> String {
        if heading.is_empty() {
            return String::new();
        }

        // Strip markdown formatting first
        let text = if QUICK_MARKDOWN_CHECK.is_match(heading) {
            self.strip_markdown_formatting_fast(heading)
        } else {
            heading.to_string()
        };

        // Trim whitespace first
        let text = text.trim();

        // Check if this would result in empty/non-Latin content
        let has_latin = text
            .chars()
            .any(|c| c.is_ascii_alphabetic() || (c as u32 >= 0x00C0 && c as u32 <= 0x024F)); // Latin extended

        if !has_latin {
            // kramdown generates generic "section" for non-Latin scripts
            return "section".to_string();
        }

        // Follow kramdown's algorithm
        let mut result = String::with_capacity(text.len());
        let mut found_letter = false;

        for c in text.chars() {
            // Skip leading non-letters
            if !found_letter && !Self::is_kramdown_letter(c) {
                continue;
            }
            if Self::is_kramdown_letter(c) {
                found_letter = true;
            }

            // Process character
            if c.is_ascii_alphabetic() {
                result.push(c.to_ascii_lowercase());
            } else if c.is_ascii_digit() && found_letter {
                result.push(c);
            } else if c == ' ' || c == '-' {
                result.push('-');
            } else if let Some(replacement) = Self::kramdown_normalize_char(c) {
                // kramdown normalizes accented characters
                result.push_str(replacement);
            }
            // Skip underscores and other characters
        }

        // Remove leading and trailing hyphens
        result.trim_matches('-').to_string()
    }

    /// Check if character is considered a letter by kramdown
    #[inline]
    fn is_kramdown_letter(c: char) -> bool {
        c.is_ascii_alphabetic() ||
        // Latin-1 Supplement and Extended Latin
        (c as u32 >= 0x00C0 && c as u32 <= 0x024F)
    }

    /// Normalize accented characters for kramdown (simplified)
    #[inline]
    fn kramdown_normalize_char(c: char) -> Option<&'static str> {
        match c {
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => Some("a"),
            'È' | 'É' | 'Ê' | 'Ë' | 'è' | 'é' | 'ê' | 'ë' => Some("e"),
            'Ì' | 'Í' | 'Î' | 'Ï' | 'ì' | 'í' | 'î' | 'ï' => Some("i"),
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => Some("o"),
            'Ù' | 'Ú' | 'Û' | 'Ü' | 'ù' | 'ú' | 'û' | 'ü' => Some("u"),
            'Ý' | 'ý' | 'ÿ' => Some("y"),
            'Ñ' | 'ñ' => Some("n"),
            'Ç' | 'ç' => Some("c"),
            'ß' => Some("ss"),
            'Æ' | 'æ' => Some("ae"),
            'Œ' | 'œ' => Some("oe"),
            _ => None,
        }
    }

    /// Fragment generation for Bitbucket style
    /// Bitbucket adds 'markdown-header-' prefix to all anchors
    #[inline]
    fn heading_to_fragment_bitbucket(&self, heading: &str) -> String {
        if heading.is_empty() {
            return String::new();
        }

        // Official Bitbucket algorithm (from bitbucket-slug npm package):
        // 1. Remove markdown formatting
        // 2. Apply deburr (accent removal)
        // 3. Remove link URLs: ](...)
        // 4. Replace space-hyphen-space patterns with spaces
        // 5. Remove non-word/digit/space/hyphen characters
        // 6. Collapse whitespace
        // 7. Lowercase and trim
        // 8. Convert spaces to hyphens

        // Step 1: Strip markdown formatting
        let mut text = if QUICK_MARKDOWN_CHECK.is_match(heading) {
            self.strip_markdown_formatting_fast(heading)
        } else {
            heading.to_string()
        };

        // Step 2: Apply deburr (accent removal) - use kramdown normalization
        let mut result = String::with_capacity(text.len());
        for c in text.chars() {
            if let Some(replacement) = Self::kramdown_normalize_char(c) {
                result.push_str(replacement);
            } else {
                result.push(c);
            }
        }
        text = result;

        // Step 3: Remove link URLs (]: ](...)
        // For simplicity, we can skip this as markdown formatting is already stripped

        // Step 4: Replace space-hyphen-space patterns with single spaces
        // This handles cases like "A - B - C" -> "A B C"
        text = text.replace(" - ", " ");
        // Also handle multiple hyphens between spaces
        while text.contains(" -- ") || text.contains(" --- ") {
            text = text.replace(" -- ", " ");
            text = text.replace(" --- ", " ");
        }

        // Step 5: Remove non-word/digit/space/hyphen characters
        // JavaScript \w is ASCII only: [a-zA-Z0-9_]
        let mut cleaned = String::with_capacity(text.len());
        for c in text.chars() {
            if c.is_ascii_alphanumeric() || c == '_' || c.is_whitespace() || c == '-' {
                cleaned.push(c);
            }
        }
        text = cleaned;

        // Step 6: Collapse multiple whitespace to single spaces
        while text.contains("  ") {
            text = text.replace("  ", " ");
        }

        // Step 7: Lowercase and trim
        text = text.to_lowercase().trim().to_string();

        // Step 8: Convert spaces to hyphens and add prefix
        let fragment = text.replace(' ', "-");
        if fragment.is_empty() {
            "markdown-header-".to_string()
        } else {
            format!("markdown-header-{fragment}")
        }
    }

    /// Strip markdown formatting from heading text (optimized for common patterns)
    fn strip_markdown_formatting_fast(&self, text: &str) -> String {
        let mut result = text.to_string();

        // Strip emphasis (bold/italic)
        if result.contains('*') || result.contains('_') {
            result = EMPHASIS_PATTERN.replace_all(&result, "$1$2").to_string();
        }

        // Strip inline code
        if result.contains('`') {
            result = CODE_PATTERN.replace_all(&result, "$1").to_string();
        }

        // Strip links
        if result.contains('[') {
            result = LINK_PATTERN.replace_all(&result, "$1$2").to_string();
        }

        result
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

            // Check if it looks like a file path:
            // - Contains a file extension (dot followed by letters)
            // - Contains path separators
            // - Contains relative path indicators
            path_part.contains('.')
                && (
                    // Has file extension pattern (handle query parameters by splitting on them first)
                    {
                    let clean_path = path_part.split('?').next().unwrap_or(path_part);
                    // Handle files starting with dot
                    if let Some(after_dot) = clean_path.strip_prefix('.') {
                        let dots_count = clean_path.matches('.').count();
                        if dots_count == 1 {
                            // Could be ".ext" (just extension) or ".hidden" (hidden file)
                            // If it's a known file extension, treat as cross-file link
                            !after_dot.is_empty() && after_dot.len() <= 10 &&
                            after_dot.chars().all(|c| c.is_ascii_alphanumeric()) &&
                            // Additional check: common file extensions are likely cross-file
                            (after_dot.len() <= 4 || matches!(after_dot, "html" | "json" | "yaml" | "toml"))
                        } else {
                            // Hidden file with extension like ".hidden.txt"
                            clean_path.split('.').next_back().is_some_and(|ext| {
                                !ext.is_empty() && ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric())
                            })
                        }
                    } else {
                        // Regular file path
                        clean_path.split('.').next_back().is_some_and(|ext| {
                            !ext.is_empty() && ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric())
                        })
                    }
                } ||
                // Or contains path separators
                path_part.contains('/') || path_part.contains('\\') ||
                // Or starts with relative path indicators
                path_part.starts_with("./") || path_part.starts_with("../")
                )
        } else {
            false
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

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no link fragments present
        !ctx.content.contains("#")
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let content = ctx.content;

        // Skip empty content
        if content.is_empty() {
            return Ok(warnings);
        }

        // Extract all valid heading anchors
        let valid_headings = self.extract_headings_from_context(ctx);

        // Find all links with fragments
        let link_regex = get_cached_regex(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            // Skip front matter
            if ctx.lines[line_num].in_front_matter {
                continue;
            }

            // Skip code blocks
            if ctx.lines[line_num].in_code_block {
                continue;
            }

            for cap in link_regex.captures_iter(line) {
                if let Some(url_match) = cap.get(2) {
                    let url = url_match.as_str();
                    let full_match = cap.get(0).unwrap(); // Get the entire link match

                    // Calculate byte position for this match within the entire content
                    let line_byte_offset = if line_num == 0 {
                        0
                    } else {
                        content.lines().take(line_num).map(|l| l.len() + 1).sum::<usize>() // +1 for newline
                    };
                    let match_byte_pos = line_byte_offset + full_match.start();

                    // Skip links in code blocks or inline code spans
                    if ctx.is_in_code_block_or_span(match_byte_pos) {
                        continue;
                    }

                    // Check if this URL contains a fragment
                    if url.contains('#') && !Self::is_external_url_fast(url) {
                        // If it's a cross-file link, skip validation as the target file may not be in the current context
                        if Self::is_cross_file_link(url) {
                            continue;
                        }

                        // Extract fragment (everything after #)
                        if let Some(fragment_pos) = url.find('#') {
                            let fragment = &url[fragment_pos + 1..];

                            // Skip empty fragments
                            if fragment.is_empty() {
                                continue;
                            }

                            // Check if fragment exists in document (case-insensitive)
                            let fragment_lower = fragment.to_lowercase();
                            let found = valid_headings.iter().any(|h| h.to_lowercase() == fragment_lower);
                            if !found {
                                let column = full_match.start() + 1; // Point to start of entire link

                                warnings.push(LintWarning {
                                    rule_name: Some(self.name()),
                                    message: format!("Link anchor '#{fragment}' does not exist in document headings"),
                                    line: line_num + 1,
                                    column,
                                    end_line: line_num + 1,
                                    end_column: full_match.end() + 1, // End of entire link
                                    severity: Severity::Warning,
                                    fix: None, // No auto-fix per industry standard
                                });
                            }
                        }
                    }
                }
            }
        }

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
        // Config keys are normalized to kebab-case by the config system
        let anchor_style = if let Some(rule_config) = config.rules.get("MD051") {
            if let Some(style_str) = rule_config.values.get("anchor-style").and_then(|v| v.as_str()) {
                match style_str.to_lowercase().as_str() {
                    "kramdown" | "jekyll" => AnchorStyle::Kramdown,
                    "bitbucket" => AnchorStyle::Bitbucket,
                    _ => AnchorStyle::GitHub,
                }
            } else {
                AnchorStyle::GitHub
            }
        } else {
            AnchorStyle::GitHub
        };

        Box::new(MD051LinkFragments::with_anchor_style(anchor_style))
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let value: toml::Value = toml::from_str(r#"anchor-style = "github""#).ok()?;
        Some(("MD051".to_string(), value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_heading_to_fragment_github() {
        let rule = MD051LinkFragments::new();

        // Simple text
        assert_eq!(rule.heading_to_fragment_github("Hello World"), "hello-world");

        // Underscores preserved
        assert_eq!(
            rule.heading_to_fragment_github("test_with_underscores"),
            "test_with_underscores"
        );

        // Consecutive hyphens preserved
        assert_eq!(rule.heading_to_fragment_github("Double--Hyphen"), "double--hyphen");

        // Numbers preserved
        assert_eq!(
            rule.heading_to_fragment_github("Step 1: Getting Started"),
            "step-1-getting-started"
        );

        // Special characters removed
        assert_eq!(rule.heading_to_fragment_github("FAQ: What's New?"), "faq-whats-new");

        // Accented characters preserved
        assert_eq!(rule.heading_to_fragment_github("Café"), "café");
        assert_eq!(rule.heading_to_fragment_github("Über uns"), "über-uns");

        // Emojis should be stripped
        assert_eq!(rule.heading_to_fragment_github("Emoji 🎉 Party"), "emoji-party");
    }

    #[test]
    fn test_heading_to_fragment_kramdown() {
        let rule = MD051LinkFragments::with_anchor_style(AnchorStyle::Kramdown);

        // Simple text
        assert_eq!(rule.heading_to_fragment_kramdown("Hello World"), "hello-world");

        // Underscores REMOVED for kramdown
        assert_eq!(
            rule.heading_to_fragment_kramdown("test_with_underscores"),
            "testwithunderscores"
        );

        // Numbers preserved
        assert_eq!(
            rule.heading_to_fragment_kramdown("Step 1: Getting Started"),
            "step-1-getting-started"
        );

        // Accented characters normalized
        assert_eq!(rule.heading_to_fragment_kramdown("Café"), "cafe");
        assert_eq!(rule.heading_to_fragment_kramdown("Über uns"), "uber-uns");

        // Leading/trailing hyphens removed
        assert_eq!(rule.heading_to_fragment_kramdown("---test---"), "test");
    }

    #[test]
    fn test_heading_to_fragment_bitbucket_comprehensive() {
        let rule = MD051LinkFragments::with_anchor_style(AnchorStyle::Bitbucket);

        // Test cases verified against official bitbucket-slug npm package
        assert_eq!(
            rule.heading_to_fragment_bitbucket("test_with_underscores"),
            "markdown-header-test_with_underscores"
        );
        assert_eq!(
            rule.heading_to_fragment_bitbucket("Hello World"),
            "markdown-header-hello-world"
        );
        assert_eq!(
            rule.heading_to_fragment_bitbucket("Double--Hyphen"),
            "markdown-header-double--hyphen"
        );
        assert_eq!(
            rule.heading_to_fragment_bitbucket("Triple---Dash"),
            "markdown-header-triple---dash"
        );
        assert_eq!(rule.heading_to_fragment_bitbucket("A - B - C"), "markdown-header-a-b-c");
        assert_eq!(
            rule.heading_to_fragment_bitbucket("Café au Lait"),
            "markdown-header-cafe-au-lait"
        );
        assert_eq!(
            rule.heading_to_fragment_bitbucket("123 Numbers"),
            "markdown-header-123-numbers"
        );
        assert_eq!(
            rule.heading_to_fragment_bitbucket("Version 2.1.0"),
            "markdown-header-version-210"
        );
        assert_eq!(
            rule.heading_to_fragment_bitbucket("__dunder__"),
            "markdown-header-__dunder__"
        );
        assert_eq!(
            rule.heading_to_fragment_bitbucket("_private_method"),
            "markdown-header-_private_method"
        );
        assert_eq!(
            rule.heading_to_fragment_bitbucket("Pre-existing-hyphens"),
            "markdown-header-pre-existing-hyphens"
        );
        assert_eq!(
            rule.heading_to_fragment_bitbucket("Simple-Hyphen"),
            "markdown-header-simple-hyphen"
        );
        assert_eq!(rule.heading_to_fragment_bitbucket("你好世界"), "markdown-header-");
        assert_eq!(rule.heading_to_fragment_bitbucket("こんにちは"), "markdown-header-");
        assert_eq!(rule.heading_to_fragment_bitbucket("!!!"), "markdown-header-");
        assert_eq!(rule.heading_to_fragment_bitbucket("---"), "markdown-header----");
        assert_eq!(rule.heading_to_fragment_bitbucket("..."), "markdown-header-");
    }

    #[test]
    fn test_bitbucket_style_validation() {
        let rule = MD051LinkFragments::with_anchor_style(AnchorStyle::Bitbucket);
        let content = "# My Section\n\n[correct](#markdown-header-my-section)\n[wrong](#my-section)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only the second link should trigger a warning
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("#my-section"));
    }

    #[test]
    fn test_issue_39_heading_with_hyphens() {
        let github_rule = MD051LinkFragments::new();
        let kramdown_rule = MD051LinkFragments::with_anchor_style(AnchorStyle::Kramdown);

        // Test the specific case from issue 39
        let heading = "respect_gitignore";
        assert_eq!(github_rule.heading_to_fragment_github(heading), "respect_gitignore");
        assert_eq!(kramdown_rule.heading_to_fragment_kramdown(heading), "respectgitignore");
    }

    #[test]
    fn test_kramdown_style_validation() {
        let rule = MD051LinkFragments::with_anchor_style(AnchorStyle::Kramdown);
        // For kramdown, underscores are removed
        let content = "# respect_gitignore\n\n[correct](#respectgitignore)\n[wrong](#respect_gitignore)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only the second link should trigger a warning
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("#respect_gitignore"));
    }

    #[test]
    fn test_github_style_validation() {
        let rule = MD051LinkFragments::new(); // Default is GitHub style
        let content = "# test_with_underscores\n\n[correct](#test_with_underscores)\n[wrong](#testwithunderscores)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only the second link should trigger a warning
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("#testwithunderscores"));
    }

    #[test]
    fn test_liquid_tags_ignored() {
        let rule = MD051LinkFragments::new();

        // Test various Liquid tag patterns with fragments (commonly used by Jekyll)
        let content = r#"# Test Liquid Tag Links

## CVE-2022-0811

This is a heading that exists.

## Some Anchor

Another heading.

## Technical Details

More content here.

### Testing Liquid cross-file links

[Liquid post_url link]({% post_url 2023-03-25-htb-vessel %}#cve-2022-0811)
[Another Liquid link]({% post_url 2023-09-09-htb-pikatwoo %}#some-anchor)
[Third Liquid link]({% post_url 2024-01-15-some-post %}#technical-details)

### Testing Liquid include with fragment

[Liquid include link]({% include file.html %}#section)

### Testing other liquid tags

[Liquid link tag]({% link _posts/2023-01-01-post.md %}#heading)
[Liquid variable]({{ site.url }}/page#fragment)

### Regular links that should still be validated

[Valid internal link](#some-anchor)
[Invalid internal link](#non-existent-anchor)
"#;

        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only the invalid internal link should trigger a warning
        // All Liquid tag links should be ignored
        assert_eq!(
            result.len(),
            1,
            "Should only have one warning for the invalid internal link"
        );
        assert!(
            result[0].message.contains("#non-existent-anchor"),
            "Warning should be for the non-existent anchor, not Liquid tag links"
        );
    }

    #[test]
    fn test_liquid_variables_ignored() {
        let rule = MD051LinkFragments::new();

        // Test Liquid variable patterns ({{ }}) with fragments
        let content = r#"# Test Liquid Variables

## Valid Section

This section exists.

## Links with Liquid Variables

These should NOT be flagged as invalid:

- [Site URL]({{ site.url }}/page#anchor)
- [Page URL]({{ page.url }}#fragment)
- [Base URL]({{ site.baseurl }}/docs#section)
- [Variable Path]({{ post.url }}#heading)
"#;

        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // No errors should be found for Liquid variable links
        assert_eq!(result.len(), 0, "Liquid variable links should not be flagged");
    }

    #[test]
    fn test_liquid_post_url_regression() {
        // Specific test for the regression reported in issue #39 comments
        let rule = MD051LinkFragments::new();
        let content = r#"# Post Title

This is very similar to what I did on [Vessel]({% post_url 2023-03-25-htb-vessel %}#cve-2022-0811), though through Kubernetes this time.

## Some Section

Content here.
"#;

        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should have no warnings - Liquid tag link should be ignored
        assert_eq!(
            result.len(),
            0,
            "Liquid post_url tags should not trigger MD051 warnings"
        );
    }

    #[test]
    fn test_mixed_liquid_and_regular_links() {
        let rule = MD051LinkFragments::new();
        let content = r#"# Mixed Links Test

## Valid Section

Some content.

## Another Section

More content.

### Links

[Liquid tag link]({% post_url 2023-01-01-post %}#section) - should be ignored
[Valid link](#valid-section) - should pass
[Invalid link](#invalid-section) - should fail
[Another Liquid tag]({% include file.md %}#part) - should be ignored
[Cross-file](other.md#heading) - should be ignored (cross-file)
"#;

        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only the invalid internal link should fail
        assert_eq!(result.len(), 1, "Should only warn about the invalid internal link");
        assert!(result[0].message.contains("#invalid-section"));
    }

    #[test]
    fn test_liquid_syntax_detection() {
        // Test Liquid tags ({% %})
        assert!(MD051LinkFragments::is_cross_file_link(
            "{% post_url 2023-03-25-htb-vessel %}#cve-2022-0811"
        ));
        assert!(MD051LinkFragments::is_cross_file_link(
            "{% link _posts/2023-03-25-post.md %}#section"
        ));
        assert!(MD051LinkFragments::is_cross_file_link(
            "{% include anchor.html %}#fragment"
        ));

        // Test Liquid variables ({{ }})
        assert!(MD051LinkFragments::is_cross_file_link("{{ site.url }}/page#anchor"));
        assert!(MD051LinkFragments::is_cross_file_link("{{ page.url }}#fragment"));
        assert!(MD051LinkFragments::is_cross_file_link(
            "{{ site.baseurl }}/docs#section"
        ));
        assert!(MD051LinkFragments::is_cross_file_link("{{ post.url }}#heading"));

        // Regular fragments should not be detected as Liquid
        assert!(!MD051LinkFragments::is_cross_file_link("#regular-fragment"));

        // Malformed or reversed brackets should not be detected as Liquid
        assert!(!MD051LinkFragments::is_cross_file_link("%}{%#fragment"));
        assert!(!MD051LinkFragments::is_cross_file_link("}}{{#fragment"));
        assert!(!MD051LinkFragments::is_cross_file_link("%}some{%#fragment"));
        assert!(!MD051LinkFragments::is_cross_file_link("}}text{{#fragment"));
    }
}
