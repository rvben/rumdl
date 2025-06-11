use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::regex_cache::*;
use std::collections::HashSet;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pre-compiled optimized patterns for quick checks
    static ref QUICK_LINK_CHECK: Regex = Regex::new(r"\[.*?\]\([^)]*#").unwrap();
    static ref QUICK_EXTERNAL_CHECK: Regex = Regex::new(r"^https?://|^ftp://|^www\.").unwrap();
    static ref QUICK_MARKDOWN_CHECK: Regex = Regex::new(r"[*_`\[\]]").unwrap();

    // Optimized single-pass markdown stripping (faster than multiple regex calls)
    static ref MARKDOWN_STRIP: Regex = Regex::new(r"\*\*([^*]+)\*\*|__([^_]+)__|~~([^~]+)~~|\*([^*]+)\*|_([^_]+)_|`([^`]+)`|\[([^\]]+)\]\([^)]+\)").unwrap();
}

/// Rule MD051: Link anchors should match document headings
///
/// See [docs/md051.md](../../docs/md051.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when a link anchor (the part after #) doesn't exist in the current document.
/// This only applies to internal document links (like #heading), not to external URLs or cross-file links (like file.md#heading).
#[derive(Clone)]
pub struct MD051LinkFragments;

impl Default for MD051LinkFragments {
    fn default() -> Self {
        Self::new()
    }
}

impl MD051LinkFragments {
    pub fn new() -> Self {
        Self
    }


    /// Extract headings from cached LintContext information
    fn extract_headings_from_context(
        &self,
        ctx: &crate::lint_context::LintContext,
    ) -> HashSet<String> {
        let mut headings = HashSet::with_capacity(32); // Pre-allocate reasonable capacity
        let mut in_toc = false;

        // Single pass through lines, only processing lines with headings
        for line_info in &ctx.lines {
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

                // Use optimized fragment generation
                let fragment = self.heading_to_fragment_fast(&heading.text);
                if !fragment.is_empty() {
                    headings.insert(fragment);
                }
            }
        }

        headings
    }

    /// Optimized fragment generation with minimal allocations
    #[inline]
    fn heading_to_fragment_fast(&self, heading: &str) -> String {
        // Early return for empty headings
        if heading.is_empty() {
            return String::new();
        }

        // Quick check: if no markdown formatting, use fast path
        let needs_markdown_stripping = QUICK_MARKDOWN_CHECK.is_match(heading);

        let text = if needs_markdown_stripping {
            self.strip_markdown_formatting_fast(heading)
        } else {
            heading.to_string()
        };

        // Optimized character processing using byte iteration for ASCII
        let mut fragment = String::with_capacity(text.len());
        let mut prev_was_hyphen = false;

        for c in text.to_lowercase().chars() {
            match c {
                // Keep alphanumeric characters
                'a'..='z' | '0'..='9' => {
                    fragment.push(c);
                    prev_was_hyphen = false;
                }
                // Ampersand becomes double hyphen (special case)
                '&' => {
                    if !prev_was_hyphen {
                        fragment.push_str("--");
                    } else {
                        fragment.push('-'); // Make it double
                    }
                    prev_was_hyphen = true;
                }
                // Spaces and other characters become single hyphen (but avoid consecutive hyphens)
                _ => {
                    if !prev_was_hyphen {
                        fragment.push('-');
                        prev_was_hyphen = true;
                    }
                }
            }
        }

        // Remove leading and trailing hyphens
        fragment.trim_matches('-').to_string()
    }

    /// Optimized markdown stripping using single-pass regex
    #[inline]
    fn strip_markdown_formatting_fast(&self, text: &str) -> String {
        // Fast path: if no markdown characters, return as-is
        if !QUICK_MARKDOWN_CHECK.is_match(text) {
            return text.to_string();
        }

        // Use single regex to capture all markdown formatting at once
        let result = MARKDOWN_STRIP.replace_all(text, |caps: &regex::Captures| {
            // Return the captured content (group 1-7 for different formatting types)
            for i in 1..=7 {
                if let Some(content) = caps.get(i) {
                    return content.as_str().to_string();
                }
            }
            caps.get(0).unwrap().as_str().to_string()
        });

        // Remove any remaining backticks only if they exist
        if result.contains('`') {
            result.replace('`', "")
        } else {
            result.to_string()
        }
    }

        /// Check if a path has a file extension indicating it's a file reference
    fn has_file_extension(path: &str) -> bool {
        // First, strip query parameters and other URL components
        // Split on ? to remove query parameters, and on & to handle other URL components
        let clean_path = path.split('?').next().unwrap_or(path)
                            .split('&').next().unwrap_or(path);

        // Common file extensions that indicate cross-file references
        let file_extensions = [
            // Markdown and documentation formats
            ".md", ".markdown", ".mdown", ".mkdn", ".mdx", ".md2", ".mdtext",
            ".rst", ".txt", ".adoc", ".asciidoc", ".org",

            // Web formats
            ".html", ".htm", ".xhtml", ".xml", ".svg",

            // Data and config formats
            ".json", ".yaml", ".yml", ".toml", ".ini", ".cfg", ".conf",

            // Office documents
            ".pdf", ".doc", ".docx", ".odt", ".rtf",

            // Programming and script files (often contain documentation)
            ".py", ".js", ".ts", ".rs", ".go", ".java", ".cpp", ".c", ".h",
            ".sh", ".bash", ".zsh", ".fish", ".ps1", ".bat", ".cmd",

            // Other common file types that might have fragments
            ".tex", ".bib", ".csv", ".tsv", ".log"
        ];

        // Case-insensitive extension matching
        let path_lower = clean_path.to_lowercase();
        for ext in &file_extensions {
            if path_lower.ends_with(ext) {
                return true;
            }
        }

                        // Also check for any extension pattern (dot followed by 2-10 alphanumeric characters)
        // This catches extensions not in our known list like .backup, .tmp, .orig, etc.
        if let Some(last_dot) = path_lower.rfind('.') {
            // Special case: if path starts with a dot, it might be a hidden file
            // Only treat it as having an extension if there's a second dot
            if path_lower.starts_with('.') {
                // For hidden files like .gitignore, .bashrc, we need a second dot to be a file extension
                // e.g., .config.json has extension .json, but .gitignore has no extension
                if last_dot == 0 {
                    // Only one dot at the beginning - not a file extension
                    return false;
                }
            }

            let potential_ext = &path_lower[last_dot + 1..];
            // Valid extension: 2-10 characters, alphanumeric (allows for longer extensions like .backup)
            if potential_ext.len() >= 2 && potential_ext.len() <= 10 &&
               potential_ext.chars().all(|c| c.is_ascii_alphanumeric()) {
                return true;
            }
        }

        false
    }

    /// Fast external URL detection with optimized patterns
    #[inline]
    fn is_external_url_fast(&self, url: &str) -> bool {
        // Quick byte-level check for common prefixes
        let bytes = url.as_bytes();
        if bytes.len() < 4 {
            return false;
        }

        // Check for http:// (7 chars minimum)
        if bytes.len() >= 7 && &bytes[..7] == b"http://" {
            return true;
        }

        // Check for https:// (8 chars minimum)
        if bytes.len() >= 8 && &bytes[..8] == b"https://" {
            return true;
        }

        // Check for ftp:// (6 chars minimum)
        if bytes.len() >= 6 && &bytes[..6] == b"ftp://" {
            return true;
        }

        // Check for www. (4 chars minimum)
        if bytes.len() >= 4 && &bytes[..4] == b"www." {
            return true;
        }

        false
    }
}

impl Rule for MD051LinkFragments {
    fn name(&self) -> &'static str {
        "MD051"
    }

    fn description(&self) -> &'static str {
        "Link anchors (# references) should exist in the current document"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early return: if no links at all, skip processing
        if !content.contains('[') || !content.contains('#') {
            return Ok(Vec::new());
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    /// Optimized check using pre-computed document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        _structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;

        // Early return: if no links at all, skip processing
        if !content.contains('[') || !content.contains('#') {
            return Ok(Vec::new());
        }

        // Extract headings once for the entire document
        let headings = self.extract_headings_from_context(ctx);

        // If no headings, no need to check TOC sections
        let has_headings = !headings.is_empty();

        let mut warnings = Vec::new();
        let mut in_toc_section = false;

        // Use centralized link parsing from LintContext
        for link in &ctx.links {
            // Skip external links
            let url = if link.is_reference {
                // Resolve reference URL
                if let Some(ref_id) = &link.reference_id {
                    ctx.get_reference_url(ref_id).unwrap_or("")
                } else {
                    ""
                }
            } else {
                &link.url
            };

            // Skip if external URL
            if self.is_external_url_fast(url) {
                continue;
            }

            // Check if URL has a fragment
            if let Some(hash_pos) = url.find('#') {
                let fragment = &url[hash_pos + 1..].to_lowercase();

                // Skip empty fragments
                if fragment.is_empty() {
                    continue;
                }

                // Skip cross-file fragment links - only validate fragments in same document
                // If URL contains a file path (has file extension like .md, .rst, .html, etc.), it's a cross-file link
                let path_before_hash = &url[..hash_pos];
                if Self::has_file_extension(path_before_hash) {
                    continue;
                }

                // Check if in TOC section
                if in_toc_section {
                    continue;
                }

                let line_info = &ctx.lines[link.line - 1];

                // Check if we're entering a TOC section
                if has_headings && TOC_SECTION_START.is_match(&line_info.content) {
                    in_toc_section = true;
                    continue;
                }

                // Check if we're exiting a TOC section (next heading)
                if in_toc_section && line_info.content.starts_with('#') && !TOC_SECTION_START.is_match(&line_info.content) {
                    in_toc_section = false;
                }

                // Check if the fragment exists in headings
                if !has_headings || !headings.contains(fragment) {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: link.line,
                        column: link.start_col + 1, // Convert to 1-indexed
                        end_line: link.line,
                        end_column: link.end_col + 1, // Convert to 1-indexed
                        message: format!(
                            "Link anchor '#{}' does not exist in document headings",
                            fragment
                        ),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // No automatic fix for missing fragments, just return content as-is
        Ok(ctx.content.to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD051LinkFragments::new())
    }
}

impl DocumentStructureExtensions for MD051LinkFragments {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
        // This rule is only relevant if there are both headings and links
        let has_headings = ctx.lines.iter().any(|line| line.heading.is_some());
        let has_links = ctx.content.contains('[') && ctx.content.contains(']');
        has_headings && has_links
    }
}
