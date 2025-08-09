use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::regex_cache::get_cached_regex;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

lazy_static! {
    // Pre-compiled optimized patterns for quick checks
    static ref QUICK_MARKDOWN_CHECK: Regex = Regex::new(r"[*_`\[\]]").unwrap();
    static ref EMPHASIS_PATTERN: Regex = Regex::new(r"\*+([^*]+)\*+|_+([^_]+)_+").unwrap();
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

                // Check for TOC section
                if TOC_SECTION_START.is_match(line) {
                    in_toc = true;
                    continue;
                }

                // If we were in TOC and hit another heading, we're out of TOC
                if in_toc {
                    in_toc = false;
                }

                // Skip if in TOC
                if in_toc {
                    continue;
                }

                // Generate fragment based on configured style
                let fragment = match self.anchor_style {
                    AnchorStyle::GitHub => self.heading_to_fragment_github(&heading.text),
                    AnchorStyle::Kramdown => self.heading_to_fragment_kramdown(&heading.text),
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
            }
        }

        headings
    }

    /// Fragment generation following GitHub's official algorithm
    /// GitHub preserves underscores and consecutive hyphens
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

        // Follow GitHub's algorithm
        let mut fragment = String::with_capacity(text.len());

        for c in text.to_lowercase().chars() {
            match c {
                // Keep ASCII letters and numbers only (GitHub removes non-ASCII)
                c if c.is_ascii_alphabetic() || c.is_ascii_digit() => fragment.push(c),
                // Keep underscores (GitHub preserves these)
                '_' => fragment.push('_'),
                // Keep hyphens
                '-' => fragment.push('-'),
                // Convert spaces to hyphens
                ' ' => fragment.push('-'),
                // Remove all other punctuation and non-ASCII
                _ => {}
            }
        }

        // Remove leading and trailing hyphens
        fragment.trim_matches('-').to_string()
    }

    /// Fragment generation following kramdown's algorithm
    /// kramdown removes underscores but preserves consecutive hyphens
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

        // Follow kramdown's algorithm
        let mut result = String::with_capacity(text.len());
        let mut found_letter = false;

        for c in text.chars() {
            // Step 1: Skip leading non-letters (kramdown removes leading numbers)
            if !found_letter && !c.is_ascii_alphabetic() {
                continue;
            }
            if c.is_ascii_alphabetic() {
                found_letter = true;
            }

            // Step 2: Keep only ASCII letters, numbers, spaces, and hyphens
            match c {
                c if c.is_ascii_alphabetic() => result.push(c.to_ascii_lowercase()),
                c if c.is_ascii_digit() => result.push(c),
                ' ' => result.push('-'), // Step 3: spaces to hyphens
                '-' => result.push('-'),
                '_' => {} // kramdown REMOVES underscores (key difference!)
                _ => {}   // Remove everything else including non-ASCII
            }
        }

        // Remove leading and trailing hyphens
        result.trim_matches('-').to_string()
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

        // If no headings, skip checking
        if valid_headings.is_empty() {
            return Ok(warnings);
        }

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

                    // Only check internal fragments (starting with #)
                    if let Some(fragment) = url.strip_prefix('#') {
                        // Skip empty fragments
                        if fragment.is_empty() {
                            continue;
                        }

                        // Check if fragment exists in document
                        if !valid_headings.contains(fragment) {
                            let column = url_match.start() + 1;
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                message: format!("Link anchor '#{fragment}' does not exist in document headings"),
                                line: line_num + 1,
                                column,
                                end_line: line_num + 1,
                                end_column: column + url.len(),
                                severity: Severity::Warning,
                                fix: None,
                            });
                        }
                    }
                    // For cross-file links (like file.md#heading), we skip validation
                    // as the target file may not be in the current context
                    else if url.contains('#') && !Self::is_external_url_fast(url) {
                        // This is a cross-file link, skip validation
                        continue;
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // MD051 cannot automatically fix invalid link fragments
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

        // With punctuation
        assert_eq!(rule.heading_to_fragment_github("Hello, World!"), "hello-world");

        // With markdown formatting
        assert_eq!(
            rule.heading_to_fragment_github("**Bold** and *italic*"),
            "bold-and-italic"
        );

        // With code
        assert_eq!(rule.heading_to_fragment_github("Using `code` here"), "using-code-here");

        // With ampersand (punctuation removed, spaces preserved as hyphens)
        assert_eq!(rule.heading_to_fragment_github("This & That"), "this--that");

        // Leading/trailing spaces and hyphens
        assert_eq!(rule.heading_to_fragment_github("  Spaces  "), "spaces");

        // Multiple spaces (GitHub does NOT collapse consecutive hyphens)
        assert_eq!(
            rule.heading_to_fragment_github("Multiple   Spaces"),
            "multiple---spaces"
        );

        // Test underscores - GitHub PRESERVES these
        assert_eq!(
            rule.heading_to_fragment_github("respect_gitignore"),
            "respect_gitignore"
        );
        assert_eq!(
            rule.heading_to_fragment_github("`respect_gitignore`"),
            "respect_gitignore"
        );

        // Test slash conversion (punctuation removed)
        assert_eq!(rule.heading_to_fragment_github("CI/CD Migration"), "cicd-migration");
    }

    #[test]
    fn test_heading_to_fragment_kramdown() {
        let rule = MD051LinkFragments::with_anchor_style(AnchorStyle::Kramdown);

        // Simple text
        assert_eq!(rule.heading_to_fragment_kramdown("Hello World"), "hello-world");

        // With punctuation
        assert_eq!(rule.heading_to_fragment_kramdown("Hello, World!"), "hello-world");

        // With markdown formatting
        assert_eq!(
            rule.heading_to_fragment_kramdown("**Bold** and *italic*"),
            "bold-and-italic"
        );

        // With code
        assert_eq!(
            rule.heading_to_fragment_kramdown("Using `code` here"),
            "using-code-here"
        );

        // With ampersand (punctuation removed, spaces preserved as hyphens)
        assert_eq!(rule.heading_to_fragment_kramdown("This & That"), "this--that");

        // Leading/trailing spaces and hyphens
        assert_eq!(rule.heading_to_fragment_kramdown("  Spaces  "), "spaces");

        // Multiple spaces (kramdown does NOT collapse consecutive hyphens)
        assert_eq!(
            rule.heading_to_fragment_kramdown("Multiple   Spaces"),
            "multiple---spaces"
        );

        // Test underscores - kramdown REMOVES these (key difference!)
        assert_eq!(
            rule.heading_to_fragment_kramdown("respect_gitignore"),
            "respectgitignore"
        );
        assert_eq!(
            rule.heading_to_fragment_kramdown("`respect_gitignore`"),
            "respectgitignore"
        );
        assert_eq!(
            rule.heading_to_fragment_kramdown("snake_case_example"),
            "snakecaseexample"
        );

        // Test slash conversion (punctuation removed)
        assert_eq!(rule.heading_to_fragment_kramdown("CI/CD Migration"), "cicd-migration");
    }

    #[test]
    fn test_issue_39_heading_with_hyphens() {
        let github_rule = MD051LinkFragments::new();
        let kramdown_rule = MD051LinkFragments::with_anchor_style(AnchorStyle::Kramdown);

        // Test the specific case from issue 39
        // Both GitHub and kramdown preserve consecutive hyphens
        assert_eq!(github_rule.heading_to_fragment_github("The End - yay"), "the-end---yay");
        assert_eq!(
            kramdown_rule.heading_to_fragment_kramdown("The End - yay"),
            "the-end---yay"
        );

        // More test cases with hyphens
        assert_eq!(github_rule.heading_to_fragment_github("A - B - C"), "a---b---c");
        assert_eq!(kramdown_rule.heading_to_fragment_kramdown("A - B - C"), "a---b---c");

        assert_eq!(
            github_rule.heading_to_fragment_github("Pre-Existing-Hyphens"),
            "pre-existing-hyphens"
        );
        assert_eq!(
            kramdown_rule.heading_to_fragment_kramdown("Pre-Existing-Hyphens"),
            "pre-existing-hyphens"
        );
    }

    #[test]
    fn test_valid_internal_link() {
        let rule = MD051LinkFragments::new();
        let content = "# Hello World\n\n[link](#hello-world)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_invalid_internal_link() {
        let rule = MD051LinkFragments::new();
        let content = "# Hello World\n\n[link](#nonexistent)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("does not exist"));
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
        // For GitHub, underscores are preserved
        let content = "# respect_gitignore\n\n[correct](#respect_gitignore)\n[wrong](#respectgitignore)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Only the second link should trigger a warning
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("#respectgitignore"));
    }

    #[test]
    fn test_complex_heading_with_special_chars() {
        let rule = MD051LinkFragments::new();
        // Per GitHub spec: punctuation removed, spaces become hyphens (preserving consecutive)
        let content = "# FAQ: What's New & Improved?\n\n[faq](#faq-whats-new--improved)";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }
}
