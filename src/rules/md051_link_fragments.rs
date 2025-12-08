use crate::rule::{CrossFileScope, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::anchor_styles::AnchorStyle;
use crate::workspace_index::{CrossFileLinkIndex, FileIndex, HeadingIndex};
use pulldown_cmark::LinkType;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;
// HTML tags with id or name attributes (supports any HTML element, not just <a>)
// This pattern only captures the first id/name attribute in a tag
static HTML_ANCHOR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b(?:id|name)\s*=\s*["']([^"']+)["']"#).unwrap());

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
/// This rule validates that link anchors (the part after #) exist in the current document.
/// Only applies to internal document links (like #heading), not to external URLs or cross-file links.
#[derive(Clone)]
pub struct MD051LinkFragments {
    /// Anchor style to use for validation
    anchor_style: AnchorStyle,
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

    /// Extract all valid heading anchors from the document
    /// Returns (markdown_anchors, html_anchors) where markdown_anchors are lowercased
    /// for case-insensitive matching, and html_anchors are case-sensitive
    fn extract_headings_from_context(
        &self,
        ctx: &crate::lint_context::LintContext,
    ) -> (HashSet<String>, HashSet<String>) {
        let mut markdown_headings = HashSet::with_capacity(32);
        let mut html_anchors = HashSet::with_capacity(16);
        let mut fragment_counts = std::collections::HashMap::new();

        for line_info in &ctx.lines {
            if line_info.in_front_matter {
                continue;
            }

            // Extract HTML anchor tags with id/name attributes
            if !line_info.in_code_block {
                let content = line_info.content(ctx.content);
                let bytes = content.as_bytes();

                // Skip lines without HTML tags or id/name attributes
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
                                            html_anchors.insert(id.to_string());
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
            }

            // Extract markdown heading anchors
            if let Some(heading) = &line_info.heading {
                // Custom ID from {#custom-id} syntax
                if let Some(custom_id) = &heading.custom_id {
                    markdown_headings.insert(custom_id.to_lowercase());
                }

                // Generate anchor from heading text
                // The anchor generation algorithm handles markdown formatting and HTML tags correctly
                let fragment = self.anchor_style.generate_fragment(&heading.text);

                if !fragment.is_empty() {
                    // Handle duplicate headings by appending -1, -2, etc.
                    let final_fragment = if let Some(count) = fragment_counts.get_mut(&fragment) {
                        let suffix = *count;
                        *count += 1;
                        format!("{fragment}-{suffix}")
                    } else {
                        fragment_counts.insert(fragment.clone(), 1);
                        fragment
                    };
                    markdown_headings.insert(final_fragment);
                }
            }
        }

        (markdown_headings, html_anchors)
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

            // Check if it's an absolute path (starts with /)
            // These are links to other pages on the same site
            if path_part.starts_with('/') {
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
                            // Could be ".ext" (file extension) or ".hidden" (hidden file)
                            // Treat short alphanumeric suffixes as file extensions
                            !after_dot.is_empty() && after_dot.len() <= 10 &&
                            after_dot.chars().all(|c| c.is_ascii_alphanumeric())
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
        if !ctx.likely_has_links_or_images() {
            return true;
        }
        // Check for # character (fragments)
        !ctx.has_char('#')
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        if ctx.content.is_empty() || ctx.links.is_empty() || self.should_skip(ctx) {
            return Ok(warnings);
        }

        let (markdown_headings, html_anchors) = self.extract_headings_from_context(ctx);

        for link in &ctx.links {
            if link.is_reference {
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

            // Validate fragment against document headings
            // HTML anchors are case-sensitive, markdown anchors are case-insensitive
            let found = if html_anchors.contains(fragment) {
                true
            } else {
                let fragment_lower = fragment.to_lowercase();
                markdown_headings.contains(&fragment_lower)
            };

            if !found {
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: format!("Link anchor '#{fragment}' does not exist in document headings"),
                    line: link.line,
                    column: link.start_col + 1,
                    end_line: link.line,
                    end_column: link.end_col + 1,
                    severity: Severity::Warning,
                    fix: None,
                });
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
                    "kramdown" => AnchorStyle::Kramdown,
                    "kramdown-gfm" => AnchorStyle::KramdownGfm,
                    "jekyll" => AnchorStyle::KramdownGfm, // Backward compatibility alias
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

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn cross_file_scope(&self) -> CrossFileScope {
        CrossFileScope::Workspace
    }

    fn contribute_to_index(&self, ctx: &crate::lint_context::LintContext, file_index: &mut FileIndex) {
        let mut fragment_counts = HashMap::new();

        // Extract headings (for other files to reference)
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            if line_info.in_front_matter {
                continue;
            }

            if let Some(heading) = &line_info.heading {
                let fragment = self.anchor_style.generate_fragment(&heading.text);

                if !fragment.is_empty() {
                    // Handle duplicate headings
                    let final_fragment = if let Some(count) = fragment_counts.get_mut(&fragment) {
                        let suffix = *count;
                        *count += 1;
                        format!("{fragment}-{suffix}")
                    } else {
                        fragment_counts.insert(fragment.clone(), 1);
                        fragment
                    };

                    file_index.add_heading(HeadingIndex {
                        text: heading.text.clone(),
                        auto_anchor: final_fragment,
                        custom_anchor: heading.custom_id.clone(),
                        line: line_idx + 1, // 1-indexed
                    });
                }
            }
        }

        // Extract cross-file links (for validation against other files)
        for link in &ctx.links {
            if link.is_reference {
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
    }

    fn cross_file_check(
        &self,
        file_path: &Path,
        file_index: &FileIndex,
        workspace_index: &crate::workspace_index::WorkspaceIndex,
    ) -> LintResult {
        let mut warnings = Vec::new();

        // Check each cross-file link in this file
        for cross_link in &file_index.cross_file_links {
            // Skip cross-file links without fragments - nothing to validate
            if cross_link.fragment.is_empty() {
                continue;
            }

            // Resolve the target file path relative to the current file
            let target_path = if let Some(parent) = file_path.parent() {
                parent.join(&cross_link.target_path)
            } else {
                Path::new(&cross_link.target_path).to_path_buf()
            };

            // Normalize the path (remove . and ..)
            let target_path = normalize_path(&target_path);

            // Look up the target file in the workspace index
            if let Some(target_file_index) = workspace_index.get_file(&target_path) {
                // Check if the fragment matches any heading in the target file (O(1) lookup)
                if !target_file_index.has_anchor(&cross_link.fragment) {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: cross_link.line,
                        column: cross_link.column,
                        end_line: cross_link.line,
                        end_column: cross_link.column + cross_link.target_path.len() + 1 + cross_link.fragment.len(),
                        message: format!(
                            "Link fragment '{}' not found in '{}'",
                            cross_link.fragment, cross_link.target_path
                        ),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
            }
            // If target file not in index, skip (could be external file or not in workspace)
        }

        Ok(warnings)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let value: toml::Value = toml::from_str(
            r#"
# Anchor generation style to match your target platform
# Options: "github" (default), "kramdown-gfm", "kramdown"
# Note: "jekyll" is accepted as an alias for "kramdown-gfm" (backward compatibility)
anchor-style = "github"
"#,
        )
        .ok()?;
        Some(("MD051".to_string(), value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

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
}
