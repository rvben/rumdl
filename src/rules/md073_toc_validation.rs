//! MD073: Table of Contents validation rule
//!
//! Validates that TOC sections match the actual document headings.

use crate::lint_context::LintContext;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::anchor_styles::AnchorStyle;
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Regex for TOC start marker: `<!-- toc -->` with optional whitespace variations
static TOC_START_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<!--\s*toc\s*-->").unwrap());

/// Regex for TOC stop marker: `<!-- tocstop -->` or `<!-- /toc -->`
static TOC_STOP_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<!--\s*(?:tocstop|/toc)\s*-->").unwrap());

/// Regex for extracting TOC entries: `- [text](#anchor)` or `* [text](#anchor)`
/// with optional leading whitespace for nested items
/// Handles nested brackets like `[`check [PATHS...]`](#check-paths)`
static TOC_ENTRY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)[-*]\s+\[([^\[\]]*(?:\[[^\[\]]*\][^\[\]]*)*)\]\(#([^)]+)\)").unwrap());

/// Represents a detected TOC region in the document
#[derive(Debug, Clone)]
struct TocRegion {
    /// 1-indexed start line of the TOC content (after the marker)
    start_line: usize,
    /// 1-indexed end line of the TOC content (before the stop marker)
    end_line: usize,
    /// Byte offset where TOC content starts
    content_start: usize,
    /// Byte offset where TOC content ends
    content_end: usize,
}

/// A parsed TOC entry from the existing TOC
#[derive(Debug, Clone)]
struct TocEntry {
    /// Display text of the link
    text: String,
    /// Anchor/fragment (without #)
    anchor: String,
    /// Number of leading whitespace characters (for indentation checking)
    indent_spaces: usize,
}

/// An expected TOC entry generated from document headings
#[derive(Debug, Clone)]
struct ExpectedTocEntry {
    /// 1-indexed line number of the heading
    heading_line: usize,
    /// Heading level (1-6)
    level: u8,
    /// Heading text (for display)
    text: String,
    /// Generated anchor
    anchor: String,
}

/// Types of mismatches between actual and expected TOC
#[derive(Debug)]
enum TocMismatch {
    /// Entry exists in TOC but heading doesn't exist
    StaleEntry { entry: TocEntry },
    /// Heading exists but no TOC entry for it
    MissingEntry { expected: ExpectedTocEntry },
    /// TOC entry text doesn't match heading text
    TextMismatch {
        entry: TocEntry,
        expected: ExpectedTocEntry,
    },
    /// TOC entries are in wrong order
    OrderMismatch { entry: TocEntry, expected_position: usize },
    /// TOC entry has wrong indentation level
    IndentationMismatch {
        entry: TocEntry,
        actual_indent: usize,
        expected_indent: usize,
    },
}

/// Regex patterns for stripping markdown formatting from heading text
static MARKDOWN_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap());
static MARKDOWN_REF_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\[[^\]]*\]").unwrap());
static MARKDOWN_IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[([^\]]*)\]\([^)]+\)").unwrap());
static MARKDOWN_CODE_SPAN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]+)`").unwrap());
static MARKDOWN_BOLD_ASTERISK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*([^*]+)\*\*").unwrap());
static MARKDOWN_BOLD_UNDERSCORE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"__([^_]+)__").unwrap());
static MARKDOWN_ITALIC_ASTERISK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*([^*]+)\*").unwrap());
// Match underscore italic at word boundaries (space or start/end)
// Handles: "_text_", " _text_ ", "start _text_", "_text_ end"
static MARKDOWN_ITALIC_UNDERSCORE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|[^a-zA-Z0-9])_([^_]+)_([^a-zA-Z0-9]|$)").unwrap());

/// Strip markdown formatting from text, preserving plain text content.
/// Used for TOC entry display text.
///
/// Examples:
/// - `[terminal](url)` → `terminal`
/// - `**bold**` → `bold`
/// - `` `code` `` → `code`
/// - `Tool: [terminal](url)` → `Tool: terminal`
fn strip_markdown_formatting(text: &str) -> String {
    let mut result = text.to_string();

    // Strip images first (before links, since images use similar syntax)
    result = MARKDOWN_IMAGE.replace_all(&result, "$1").to_string();

    // Strip links: [text](url) → text
    result = MARKDOWN_LINK.replace_all(&result, "$1").to_string();

    // Strip reference links: [text][ref] → text
    result = MARKDOWN_REF_LINK.replace_all(&result, "$1").to_string();

    // Strip code spans: `code` → code
    result = MARKDOWN_CODE_SPAN.replace_all(&result, "$1").to_string();

    // Strip bold (do double before single to handle nested)
    result = MARKDOWN_BOLD_ASTERISK.replace_all(&result, "$1").to_string();
    result = MARKDOWN_BOLD_UNDERSCORE.replace_all(&result, "$1").to_string();

    // Strip italic
    result = MARKDOWN_ITALIC_ASTERISK.replace_all(&result, "$1").to_string();
    // Underscore italic: preserve boundary chars, extract content
    result = MARKDOWN_ITALIC_UNDERSCORE.replace_all(&result, "$1$2$3").to_string();

    result
}

/// MD073: Table of Contents Validation
///
/// This rule validates that TOC sections match the actual document headings.
/// It detects TOC regions via markers (`<!-- toc -->...<!-- tocstop -->`).
///
/// To opt into TOC validation, add markers to your document:
/// ```markdown
/// <!-- toc -->
/// - [Section](#section)
/// <!-- tocstop -->
/// ```
///
/// ## Configuration
///
/// ```toml
/// [MD073]
/// # Enable the rule (opt-in, disabled by default)
/// enabled = true
/// # Minimum heading level to include (default: 2)
/// min-level = 2
/// # Maximum heading level to include (default: 4)
/// max-level = 4
/// # Whether TOC order must match document order (default: true)
/// enforce-order = true
/// # Indent size per nesting level (default: from MD007 config, or 2)
/// indent = 2
/// ```
#[derive(Clone)]
pub struct MD073TocValidation {
    /// Whether this rule is enabled (default: false - opt-in rule)
    enabled: bool,
    /// Minimum heading level to include
    min_level: u8,
    /// Maximum heading level to include
    max_level: u8,
    /// Whether to enforce order matching
    enforce_order: bool,
    /// Indent size per nesting level (reads from MD007 config by default)
    pub indent: usize,
}

impl Default for MD073TocValidation {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default - opt-in rule
            min_level: 2,
            max_level: 4,
            enforce_order: true,
            indent: 2, // Default indent, can be overridden by MD007 config
        }
    }
}

impl std::fmt::Debug for MD073TocValidation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MD073TocValidation")
            .field("enabled", &self.enabled)
            .field("min_level", &self.min_level)
            .field("max_level", &self.max_level)
            .field("enforce_order", &self.enforce_order)
            .field("indent", &self.indent)
            .finish()
    }
}

impl MD073TocValidation {
    /// Create a new rule with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Detect TOC region using markers
    fn detect_by_markers(&self, ctx: &LintContext) -> Option<TocRegion> {
        let mut start_line = None;
        let mut start_byte = None;

        for (idx, line_info) in ctx.lines.iter().enumerate() {
            let line_num = idx + 1;
            let content = line_info.content(ctx.content);

            // Skip if in code block or front matter
            if line_info.in_code_block || line_info.in_front_matter {
                continue;
            }

            // Look for start marker or stop marker
            if let (Some(s_line), Some(s_byte)) = (start_line, start_byte) {
                // We have a start, now look for stop marker
                if TOC_STOP_MARKER.is_match(content) {
                    let end_line = line_num - 1;
                    let content_end = line_info.byte_offset;

                    // Handle case where there's no content between markers
                    if end_line < s_line {
                        return Some(TocRegion {
                            start_line: s_line,
                            end_line: s_line,
                            content_start: s_byte,
                            content_end: s_byte,
                        });
                    }

                    return Some(TocRegion {
                        start_line: s_line,
                        end_line,
                        content_start: s_byte,
                        content_end,
                    });
                }
            } else if TOC_START_MARKER.is_match(content) {
                // TOC content starts on the next line
                if idx + 1 < ctx.lines.len() {
                    start_line = Some(line_num + 1);
                    start_byte = Some(ctx.lines[idx + 1].byte_offset);
                }
            }
        }

        None
    }

    /// Detect TOC region using markers
    fn detect_toc_region(&self, ctx: &LintContext) -> Option<TocRegion> {
        self.detect_by_markers(ctx)
    }

    /// Extract TOC entries from the detected region
    fn extract_toc_entries(&self, ctx: &LintContext, region: &TocRegion) -> Vec<TocEntry> {
        let mut entries = Vec::new();

        for idx in (region.start_line - 1)..region.end_line.min(ctx.lines.len()) {
            let line_info = &ctx.lines[idx];
            let content = line_info.content(ctx.content);

            if let Some(caps) = TOC_ENTRY_PATTERN.captures(content) {
                let indent_spaces = caps.get(1).map_or(0, |m| m.as_str().len());
                let text = caps.get(2).map_or("", |m| m.as_str()).to_string();
                let anchor = caps.get(3).map_or("", |m| m.as_str()).to_string();

                entries.push(TocEntry {
                    text,
                    anchor,
                    indent_spaces,
                });
            }
        }

        entries
    }

    /// Build expected TOC entries from document headings
    fn build_expected_toc(&self, ctx: &LintContext, toc_region: &TocRegion) -> Vec<ExpectedTocEntry> {
        let mut entries = Vec::new();
        let mut fragment_counts: HashMap<String, usize> = HashMap::new();

        for (idx, line_info) in ctx.lines.iter().enumerate() {
            let line_num = idx + 1;

            // Skip headings before/within the TOC region
            if line_num <= toc_region.end_line {
                // Also skip the TOC heading itself for heading-based detection
                continue;
            }

            // Skip code blocks, front matter, HTML blocks
            if line_info.in_code_block || line_info.in_front_matter || line_info.in_html_block {
                continue;
            }

            if let Some(heading) = &line_info.heading {
                // Filter by min/max level
                if heading.level < self.min_level || heading.level > self.max_level {
                    continue;
                }

                // Use custom ID if available, otherwise generate GitHub-style anchor
                let base_anchor = if let Some(custom_id) = &heading.custom_id {
                    custom_id.clone()
                } else {
                    AnchorStyle::GitHub.generate_fragment(&heading.text)
                };

                // Handle duplicate anchors
                let anchor = if let Some(count) = fragment_counts.get_mut(&base_anchor) {
                    let suffix = *count;
                    *count += 1;
                    format!("{base_anchor}-{suffix}")
                } else {
                    fragment_counts.insert(base_anchor.clone(), 1);
                    base_anchor
                };

                entries.push(ExpectedTocEntry {
                    heading_line: line_num,
                    level: heading.level,
                    text: heading.text.clone(),
                    anchor,
                });
            }
        }

        entries
    }

    /// Compare actual TOC entries against expected and find mismatches
    fn validate_toc(&self, actual: &[TocEntry], expected: &[ExpectedTocEntry]) -> Vec<TocMismatch> {
        let mut mismatches = Vec::new();

        // Build a map of expected anchors
        let expected_anchors: HashMap<&str, &ExpectedTocEntry> =
            expected.iter().map(|e| (e.anchor.as_str(), e)).collect();

        // Build a map of actual anchors
        let actual_anchors: HashMap<&str, &TocEntry> = actual.iter().map(|e| (e.anchor.as_str(), e)).collect();

        // Check for stale entries (in TOC but not in expected)
        for entry in actual {
            if !expected_anchors.contains_key(entry.anchor.as_str()) {
                mismatches.push(TocMismatch::StaleEntry { entry: entry.clone() });
            }
        }

        // Check for missing entries (in expected but not in TOC)
        for exp in expected {
            if !actual_anchors.contains_key(exp.anchor.as_str()) {
                mismatches.push(TocMismatch::MissingEntry { expected: exp.clone() });
            }
        }

        // Check for text mismatches (compare stripped versions)
        for entry in actual {
            if let Some(exp) = expected_anchors.get(entry.anchor.as_str()) {
                // Compare stripped text (removes markdown formatting like links, emphasis)
                let actual_stripped = strip_markdown_formatting(entry.text.trim());
                let expected_stripped = strip_markdown_formatting(exp.text.trim());
                if actual_stripped != expected_stripped {
                    mismatches.push(TocMismatch::TextMismatch {
                        entry: entry.clone(),
                        expected: (*exp).clone(),
                    });
                }
            }
        }

        // Check for indentation mismatches
        // Expected indentation is indent spaces per level difference from base level
        if !expected.is_empty() {
            let base_level = expected.iter().map(|e| e.level).min().unwrap_or(2);

            for entry in actual {
                if let Some(exp) = expected_anchors.get(entry.anchor.as_str()) {
                    let level_diff = exp.level.saturating_sub(base_level) as usize;
                    let expected_indent = level_diff * self.indent;

                    if entry.indent_spaces != expected_indent {
                        // Don't report indentation mismatch if already reported as text mismatch
                        let already_reported = mismatches.iter().any(|m| match m {
                            TocMismatch::TextMismatch { entry: e, .. } => e.anchor == entry.anchor,
                            TocMismatch::StaleEntry { entry: e } => e.anchor == entry.anchor,
                            _ => false,
                        });
                        if !already_reported {
                            mismatches.push(TocMismatch::IndentationMismatch {
                                entry: entry.clone(),
                                actual_indent: entry.indent_spaces,
                                expected_indent,
                            });
                        }
                    }
                }
            }
        }

        // Check order if enforce_order is enabled
        if self.enforce_order && !actual.is_empty() && !expected.is_empty() {
            let expected_order: Vec<&str> = expected.iter().map(|e| e.anchor.as_str()).collect();

            // Find entries that exist in both but are out of order
            let mut expected_idx = 0;
            for entry in actual {
                // Skip entries that don't exist in expected
                if !expected_anchors.contains_key(entry.anchor.as_str()) {
                    continue;
                }

                // Find where this anchor should be
                while expected_idx < expected_order.len() && expected_order[expected_idx] != entry.anchor {
                    expected_idx += 1;
                }

                if expected_idx >= expected_order.len() {
                    // This entry is after where it should be
                    let correct_pos = expected_order.iter().position(|a| *a == entry.anchor).unwrap_or(0);
                    // Only add order mismatch if not already reported as stale/text mismatch
                    let already_reported = mismatches.iter().any(|m| match m {
                        TocMismatch::StaleEntry { entry: e } => e.anchor == entry.anchor,
                        TocMismatch::TextMismatch { entry: e, .. } => e.anchor == entry.anchor,
                        _ => false,
                    });
                    if !already_reported {
                        mismatches.push(TocMismatch::OrderMismatch {
                            entry: entry.clone(),
                            expected_position: correct_pos + 1,
                        });
                    }
                } else {
                    expected_idx += 1;
                }
            }
        }

        mismatches
    }

    /// Generate a new TOC from expected entries (always uses nested indentation)
    fn generate_toc(&self, expected: &[ExpectedTocEntry]) -> String {
        if expected.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        let base_level = expected.iter().map(|e| e.level).min().unwrap_or(2);
        let indent_str = " ".repeat(self.indent);

        for entry in expected {
            let level_diff = entry.level.saturating_sub(base_level) as usize;
            let indent = indent_str.repeat(level_diff);

            // Strip markdown formatting from heading text for clean TOC entries
            let display_text = strip_markdown_formatting(&entry.text);
            result.push_str(&format!("{indent}- [{display_text}](#{})\n", entry.anchor));
        }

        result
    }
}

impl Rule for MD073TocValidation {
    fn name(&self) -> &'static str {
        "MD073"
    }

    fn description(&self) -> &'static str {
        "Table of Contents should match document headings"
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        // Skip if rule is disabled (opt-in rule)
        if !self.enabled {
            return true;
        }

        // Quick check: skip if no TOC markers
        let has_toc_marker = ctx.content.contains("<!-- toc") || ctx.content.contains("<!--toc");
        !has_toc_marker
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Detect TOC region
        let Some(region) = self.detect_toc_region(ctx) else {
            // No TOC found - nothing to validate
            return Ok(warnings);
        };

        // Extract actual TOC entries
        let actual_entries = self.extract_toc_entries(ctx, &region);

        // Build expected TOC from headings
        let expected_entries = self.build_expected_toc(ctx, &region);

        // If no expected entries and no actual entries, nothing to validate
        if expected_entries.is_empty() && actual_entries.is_empty() {
            return Ok(warnings);
        }

        // Validate
        let mismatches = self.validate_toc(&actual_entries, &expected_entries);

        if !mismatches.is_empty() {
            // Generate a single warning at the TOC region with details
            let mut details = Vec::new();

            for mismatch in &mismatches {
                match mismatch {
                    TocMismatch::StaleEntry { entry } => {
                        details.push(format!("Stale entry: '{}' (heading no longer exists)", entry.text));
                    }
                    TocMismatch::MissingEntry { expected } => {
                        details.push(format!(
                            "Missing entry: '{}' (line {})",
                            expected.text, expected.heading_line
                        ));
                    }
                    TocMismatch::TextMismatch { entry, expected } => {
                        details.push(format!(
                            "Text mismatch: TOC has '{}', heading is '{}'",
                            entry.text, expected.text
                        ));
                    }
                    TocMismatch::OrderMismatch {
                        entry,
                        expected_position,
                    } => {
                        details.push(format!(
                            "Order mismatch: '{}' should be at position {}",
                            entry.text, expected_position
                        ));
                    }
                    TocMismatch::IndentationMismatch {
                        entry,
                        actual_indent,
                        expected_indent,
                        ..
                    } => {
                        details.push(format!(
                            "Indentation mismatch: '{}' has {} spaces, expected {} spaces",
                            entry.text, actual_indent, expected_indent
                        ));
                    }
                }
            }

            let message = format!(
                "Table of Contents does not match document headings: {}",
                details.join("; ")
            );

            // Generate fix: replace entire TOC content
            let new_toc = self.generate_toc(&expected_entries);
            let fix_range = region.content_start..region.content_end;

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message,
                line: region.start_line,
                column: 1,
                end_line: region.end_line,
                end_column: 1,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: fix_range,
                    replacement: new_toc,
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        // Detect TOC region
        let Some(region) = self.detect_toc_region(ctx) else {
            // No TOC found - return unchanged
            return Ok(ctx.content.to_string());
        };

        // Build expected TOC from headings
        let expected_entries = self.build_expected_toc(ctx, &region);

        // Generate new TOC
        let new_toc = self.generate_toc(&expected_entries);

        // Replace the TOC content
        let mut result = String::with_capacity(ctx.content.len());
        result.push_str(&ctx.content[..region.content_start]);
        result.push_str(&new_toc);
        result.push_str(&ctx.content[region.content_end..]);

        Ok(result)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Other
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let value: toml::Value = toml::from_str(
            r#"
# Whether this rule is enabled (opt-in, disabled by default)
enabled = false
# Minimum heading level to include
min-level = 2
# Maximum heading level to include
max-level = 4
# Whether TOC order must match document order
enforce-order = true
# Indentation per nesting level (defaults to MD007's indent value)
indent = 2
"#,
        )
        .ok()?;
        Some(("MD073".to_string(), value))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let mut rule = MD073TocValidation::default();
        let mut indent_from_md073 = false;

        if let Some(rule_config) = config.rules.get("MD073") {
            // Parse enabled (opt-in rule, defaults to false)
            if let Some(enabled) = rule_config.values.get("enabled").and_then(|v| v.as_bool()) {
                rule.enabled = enabled;
            }

            // Parse min-level
            if let Some(min_level) = rule_config.values.get("min-level").and_then(|v| v.as_integer()) {
                rule.min_level = (min_level.clamp(1, 6)) as u8;
            }

            // Parse max-level
            if let Some(max_level) = rule_config.values.get("max-level").and_then(|v| v.as_integer()) {
                rule.max_level = (max_level.clamp(1, 6)) as u8;
            }

            // Parse enforce-order
            if let Some(enforce_order) = rule_config.values.get("enforce-order").and_then(|v| v.as_bool()) {
                rule.enforce_order = enforce_order;
            }

            // Parse indent (MD073-specific override)
            if let Some(indent) = rule_config.values.get("indent").and_then(|v| v.as_integer()) {
                rule.indent = (indent.clamp(1, 8)) as usize;
                indent_from_md073 = true;
            }
        }

        // If indent not explicitly set in MD073, read from MD007 config
        if !indent_from_md073
            && let Some(md007_config) = config.rules.get("MD007")
            && let Some(indent) = md007_config.values.get("indent").and_then(|v| v.as_integer())
        {
            rule.indent = (indent.clamp(1, 8)) as usize;
        }

        Box::new(rule)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    fn create_ctx(content: &str) -> LintContext<'_> {
        LintContext::new(content, MarkdownFlavor::Standard, None)
    }

    /// Create rule with enabled=true for tests that call check() directly
    fn create_enabled_rule() -> MD073TocValidation {
        MD073TocValidation {
            enabled: true,
            ..MD073TocValidation::default()
        }
    }

    // ========== Detection Tests ==========

    #[test]
    fn test_detect_markers_basic() {
        let rule = MD073TocValidation::new();
        let content = r#"# Title

<!-- toc -->

- [Heading 1](#heading-1)

<!-- tocstop -->

## Heading 1

Content here.
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_by_markers(&ctx);
        assert!(region.is_some());
        let region = region.unwrap();
        // Verify region boundaries are detected correctly
        assert_eq!(region.start_line, 4);
        assert_eq!(region.end_line, 6);
    }

    #[test]
    fn test_detect_markers_variations() {
        let rule = MD073TocValidation::new();

        // Test <!--toc--> (no spaces)
        let content1 = "<!--toc-->\n- [A](#a)\n<!--tocstop-->\n";
        let ctx1 = create_ctx(content1);
        assert!(rule.detect_by_markers(&ctx1).is_some());

        // Test <!-- TOC --> (uppercase)
        let content2 = "<!-- TOC -->\n- [A](#a)\n<!-- TOCSTOP -->\n";
        let ctx2 = create_ctx(content2);
        assert!(rule.detect_by_markers(&ctx2).is_some());

        // Test <!-- /toc --> (alternative stop marker)
        let content3 = "<!-- toc -->\n- [A](#a)\n<!-- /toc -->\n";
        let ctx3 = create_ctx(content3);
        assert!(rule.detect_by_markers(&ctx3).is_some());
    }

    #[test]
    fn test_no_toc_region() {
        let rule = MD073TocValidation::new();
        let content = r#"# Title

## Heading 1

Content here.

## Heading 2

More content.
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_toc_region(&ctx);
        assert!(region.is_none());
    }

    // ========== Validation Tests ==========

    #[test]
    fn test_toc_matches_headings() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

- [Heading 1](#heading-1)
- [Heading 2](#heading-2)

<!-- tocstop -->

## Heading 1

Content.

## Heading 2

More content.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for matching TOC");
    }

    #[test]
    fn test_missing_entry() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

- [Heading 1](#heading-1)

<!-- tocstop -->

## Heading 1

Content.

## Heading 2

New heading not in TOC.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Missing entry"));
        assert!(result[0].message.contains("Heading 2"));
    }

    #[test]
    fn test_stale_entry() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

- [Heading 1](#heading-1)
- [Deleted Heading](#deleted-heading)

<!-- tocstop -->

## Heading 1

Content.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Stale entry"));
        assert!(result[0].message.contains("Deleted Heading"));
    }

    #[test]
    fn test_text_mismatch() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

- [Old Name](#heading-1)

<!-- tocstop -->

## Heading 1

Content.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Text mismatch"));
    }

    // ========== Level Filtering Tests ==========

    #[test]
    fn test_min_level_excludes_h1() {
        let mut rule = MD073TocValidation::new();
        rule.min_level = 2;

        let content = r#"<!-- toc -->

<!-- tocstop -->

# Should Be Excluded

## Should Be Included

Content.
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_toc_region(&ctx).unwrap();
        let expected = rule.build_expected_toc(&ctx, &region);

        assert_eq!(expected.len(), 1);
        assert_eq!(expected[0].text, "Should Be Included");
    }

    #[test]
    fn test_max_level_excludes_h5_h6() {
        let mut rule = MD073TocValidation::new();
        rule.max_level = 4;

        let content = r#"<!-- toc -->

<!-- tocstop -->

## Level 2

### Level 3

#### Level 4

##### Level 5 Should Be Excluded

###### Level 6 Should Be Excluded
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_toc_region(&ctx).unwrap();
        let expected = rule.build_expected_toc(&ctx, &region);

        assert_eq!(expected.len(), 3);
        assert!(expected.iter().all(|e| e.level <= 4));
    }

    // ========== Fix Tests ==========

    #[test]
    fn test_fix_adds_missing_entry() {
        let rule = MD073TocValidation::new();
        let content = r#"# Title

<!-- toc -->

- [Heading 1](#heading-1)

<!-- tocstop -->

## Heading 1

Content.

## Heading 2

New heading.
"#;
        let ctx = create_ctx(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("- [Heading 2](#heading-2)"));
    }

    #[test]
    fn test_fix_removes_stale_entry() {
        let rule = MD073TocValidation::new();
        let content = r#"# Title

<!-- toc -->

- [Heading 1](#heading-1)
- [Deleted](#deleted)

<!-- tocstop -->

## Heading 1

Content.
"#;
        let ctx = create_ctx(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("- [Heading 1](#heading-1)"));
        assert!(!fixed.contains("Deleted"));
    }

    #[test]
    fn test_fix_idempotent() {
        let rule = MD073TocValidation::new();
        let content = r#"# Title

<!-- toc -->

- [Heading 1](#heading-1)
- [Heading 2](#heading-2)

<!-- tocstop -->

## Heading 1

Content.

## Heading 2

More.
"#;
        let ctx = create_ctx(content);
        let fixed1 = rule.fix(&ctx).unwrap();
        let ctx2 = create_ctx(&fixed1);
        let fixed2 = rule.fix(&ctx2).unwrap();

        // Second fix should produce same output
        assert_eq!(fixed1, fixed2);
    }

    #[test]
    fn test_fix_preserves_markers() {
        let rule = MD073TocValidation::new();
        let content = r#"# Title

<!-- toc -->

Old TOC content.

<!-- tocstop -->

## New Heading

Content.
"#;
        let ctx = create_ctx(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Markers should still be present
        assert!(fixed.contains("<!-- toc -->"));
        assert!(fixed.contains("<!-- tocstop -->"));
        // New content should be generated
        assert!(fixed.contains("- [New Heading](#new-heading)"));
    }

    #[test]
    fn test_fix_requires_markers() {
        let rule = create_enabled_rule();

        // Document without markers - no TOC detected, no changes
        let content_no_markers = r#"# Title

## Heading 1

Content.
"#;
        let ctx = create_ctx(content_no_markers);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content_no_markers);

        // Document with markers - TOC detected and fixed
        let content_markers = r#"# Title

<!-- toc -->

- [Old Entry](#old-entry)

<!-- tocstop -->

## Heading 1

Content.
"#;
        let ctx = create_ctx(content_markers);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("- [Heading 1](#heading-1)"));
        assert!(!fixed.contains("Old Entry"));
    }

    // ========== Anchor Tests ==========

    #[test]
    fn test_duplicate_heading_anchors() {
        let rule = MD073TocValidation::new();
        let content = r#"# Title

<!-- toc -->

<!-- tocstop -->

## Duplicate

Content.

## Duplicate

More content.

## Duplicate

Even more.
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_toc_region(&ctx).unwrap();
        let expected = rule.build_expected_toc(&ctx, &region);

        assert_eq!(expected.len(), 3);
        assert_eq!(expected[0].anchor, "duplicate");
        assert_eq!(expected[1].anchor, "duplicate-1");
        assert_eq!(expected[2].anchor, "duplicate-2");
    }

    // ========== Edge Cases ==========

    #[test]
    fn test_headings_in_code_blocks_ignored() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

- [Real Heading](#real-heading)

<!-- tocstop -->

## Real Heading

```markdown
## Fake Heading In Code
```

Content.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not report fake heading in code block");
    }

    #[test]
    fn test_empty_toc_region() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->
<!-- tocstop -->

## Heading 1

Content.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Missing entry"));
    }

    #[test]
    fn test_nested_indentation() {
        let rule = create_enabled_rule();

        let content = r#"<!-- toc -->

<!-- tocstop -->

## Level 2

### Level 3

#### Level 4

## Another Level 2
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_toc_region(&ctx).unwrap();
        let expected = rule.build_expected_toc(&ctx, &region);
        let toc = rule.generate_toc(&expected);

        // Check indentation (always nested)
        assert!(toc.contains("- [Level 2](#level-2)"));
        assert!(toc.contains("  - [Level 3](#level-3)"));
        assert!(toc.contains("    - [Level 4](#level-4)"));
        assert!(toc.contains("- [Another Level 2](#another-level-2)"));
    }

    // ========== Indentation Mismatch Tests ==========

    #[test]
    fn test_indentation_mismatch_detected() {
        let rule = create_enabled_rule();
        // TOC entries are all at same indentation level, but headings have different levels
        let content = r#"<!-- toc -->
- [Hello](#hello)
- [Another](#another)
- [Heading](#heading)
<!-- tocstop -->

## Hello

### Another

## Heading
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        // Should detect indentation mismatch - "Another" is level 3 but has no indent
        assert_eq!(result.len(), 1, "Should report indentation mismatch: {result:?}");
        assert!(
            result[0].message.contains("Indentation mismatch"),
            "Message should mention indentation: {}",
            result[0].message
        );
        assert!(
            result[0].message.contains("Another"),
            "Message should mention the entry: {}",
            result[0].message
        );
    }

    #[test]
    fn test_indentation_mismatch_fixed() {
        let rule = create_enabled_rule();
        // TOC entries are all at same indentation level, but headings have different levels
        let content = r#"<!-- toc -->
- [Hello](#hello)
- [Another](#another)
- [Heading](#heading)
<!-- tocstop -->

## Hello

### Another

## Heading
"#;
        let ctx = create_ctx(content);
        let fixed = rule.fix(&ctx).unwrap();
        // After fix, "Another" should be indented
        assert!(fixed.contains("- [Hello](#hello)"));
        assert!(fixed.contains("  - [Another](#another)")); // Indented with 2 spaces
        assert!(fixed.contains("- [Heading](#heading)"));
    }

    #[test]
    fn test_no_indentation_mismatch_when_correct() {
        let rule = create_enabled_rule();
        // TOC has correct indentation
        let content = r#"<!-- toc -->
- [Hello](#hello)
  - [Another](#another)
- [Heading](#heading)
<!-- tocstop -->

## Hello

### Another

## Heading
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        // Should not report any issues - indentation is correct
        assert!(result.is_empty(), "Should not report issues: {result:?}");
    }

    // ========== Order Mismatch Tests ==========

    #[test]
    fn test_order_mismatch_detected() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

- [Section B](#section-b)
- [Section A](#section-a)

<!-- tocstop -->

## Section A

Content A.

## Section B

Content B.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        // Should detect order mismatch - Section B appears before Section A in TOC
        // but Section A comes first in document
        assert!(!result.is_empty(), "Should detect order mismatch");
    }

    #[test]
    fn test_order_mismatch_ignored_when_disabled() {
        let mut rule = create_enabled_rule();
        rule.enforce_order = false;
        let content = r#"# Title

<!-- toc -->

- [Section B](#section-b)
- [Section A](#section-a)

<!-- tocstop -->

## Section A

Content A.

## Section B

Content B.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        // With enforce_order=false, order mismatches should be ignored
        assert!(result.is_empty(), "Should not report order mismatch when disabled");
    }

    // ========== Unicode and Special Characters Tests ==========

    #[test]
    fn test_unicode_headings() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

- [日本語の見出し](#日本語の見出し)
- [Émojis 🎉](#émojis-)

<!-- tocstop -->

## 日本語の見出し

Japanese content.

## Émojis 🎉

Content with emojis.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        // Should handle unicode correctly
        assert!(result.is_empty(), "Should handle unicode headings");
    }

    #[test]
    fn test_special_characters_in_headings() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

- [What's New?](#whats-new)
- [C++ Guide](#c-guide)

<!-- tocstop -->

## What's New?

News content.

## C++ Guide

C++ content.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should handle special characters");
    }

    #[test]
    fn test_code_spans_in_headings() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

- [`check [PATHS...]`](#check-paths)

<!-- tocstop -->

## `check [PATHS...]`

Command documentation.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should handle code spans in headings with brackets");
    }

    // ========== Config Tests ==========

    #[test]
    fn test_from_config_defaults() {
        let config = crate::config::Config::default();
        let rule = MD073TocValidation::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD073TocValidation>().unwrap();

        assert_eq!(rule.min_level, 2);
        assert_eq!(rule.max_level, 4);
        assert!(rule.enforce_order);
        assert_eq!(rule.indent, 2);
    }

    #[test]
    fn test_indent_from_md007_config() {
        use crate::config::{Config, RuleConfig};
        use std::collections::BTreeMap;

        let mut config = Config::default();

        // Set MD007 indent to 4
        let mut md007_values = BTreeMap::new();
        md007_values.insert("indent".to_string(), toml::Value::Integer(4));
        config.rules.insert(
            "MD007".to_string(),
            RuleConfig {
                severity: None,
                values: md007_values,
            },
        );

        let rule = MD073TocValidation::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD073TocValidation>().unwrap();

        assert_eq!(rule.indent, 4, "Should read indent from MD007 config");
    }

    #[test]
    fn test_indent_md073_overrides_md007() {
        use crate::config::{Config, RuleConfig};
        use std::collections::BTreeMap;

        let mut config = Config::default();

        // Set MD007 indent to 4
        let mut md007_values = BTreeMap::new();
        md007_values.insert("indent".to_string(), toml::Value::Integer(4));
        config.rules.insert(
            "MD007".to_string(),
            RuleConfig {
                severity: None,
                values: md007_values,
            },
        );

        // Set MD073 indent to 3 (should override MD007)
        let mut md073_values = BTreeMap::new();
        md073_values.insert("enabled".to_string(), toml::Value::Boolean(true));
        md073_values.insert("indent".to_string(), toml::Value::Integer(3));
        config.rules.insert(
            "MD073".to_string(),
            RuleConfig {
                severity: None,
                values: md073_values,
            },
        );

        let rule = MD073TocValidation::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD073TocValidation>().unwrap();

        assert_eq!(rule.indent, 3, "MD073 indent should override MD007");
    }

    #[test]
    fn test_generate_toc_with_4_space_indent() {
        let mut rule = create_enabled_rule();
        rule.indent = 4;

        let content = r#"<!-- toc -->

<!-- tocstop -->

## Level 2

### Level 3

#### Level 4

## Another Level 2
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_toc_region(&ctx).unwrap();
        let expected = rule.build_expected_toc(&ctx, &region);
        let toc = rule.generate_toc(&expected);

        // With 4-space indent:
        // Level 2 = 0 spaces (base level)
        // Level 3 = 4 spaces
        // Level 4 = 8 spaces
        assert!(toc.contains("- [Level 2](#level-2)"), "Level 2 should have no indent");
        assert!(
            toc.contains("    - [Level 3](#level-3)"),
            "Level 3 should have 4-space indent"
        );
        assert!(
            toc.contains("        - [Level 4](#level-4)"),
            "Level 4 should have 8-space indent"
        );
        assert!(toc.contains("- [Another Level 2](#another-level-2)"));
    }

    #[test]
    fn test_validate_toc_with_4_space_indent() {
        let mut rule = create_enabled_rule();
        rule.indent = 4;

        // TOC with correct 4-space indentation
        let content = r#"<!-- toc -->
- [Hello](#hello)
    - [Another](#another)
- [Heading](#heading)
<!-- tocstop -->

## Hello

### Another

## Heading
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should accept 4-space indent when configured: {result:?}"
        );
    }

    #[test]
    fn test_validate_toc_wrong_indent_with_4_space_config() {
        let mut rule = create_enabled_rule();
        rule.indent = 4;

        // TOC with 2-space indentation (wrong when 4-space is configured)
        let content = r#"<!-- toc -->
- [Hello](#hello)
  - [Another](#another)
- [Heading](#heading)
<!-- tocstop -->

## Hello

### Another

## Heading
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should detect wrong indent");
        assert!(
            result[0].message.contains("Indentation mismatch"),
            "Should report indentation mismatch: {}",
            result[0].message
        );
        assert!(
            result[0].message.contains("expected 4 spaces"),
            "Should mention expected 4 spaces: {}",
            result[0].message
        );
    }

    // ========== Markdown Stripping Tests ==========

    #[test]
    fn test_strip_markdown_formatting_link() {
        let result = strip_markdown_formatting("Tool: [terminal](https://example.com)");
        assert_eq!(result, "Tool: terminal");
    }

    #[test]
    fn test_strip_markdown_formatting_bold() {
        let result = strip_markdown_formatting("This is **bold** text");
        assert_eq!(result, "This is bold text");

        let result = strip_markdown_formatting("This is __bold__ text");
        assert_eq!(result, "This is bold text");
    }

    #[test]
    fn test_strip_markdown_formatting_italic() {
        let result = strip_markdown_formatting("This is *italic* text");
        assert_eq!(result, "This is italic text");

        let result = strip_markdown_formatting("This is _italic_ text");
        assert_eq!(result, "This is italic text");
    }

    #[test]
    fn test_strip_markdown_formatting_code_span() {
        let result = strip_markdown_formatting("Use the `format` function");
        assert_eq!(result, "Use the format function");
    }

    #[test]
    fn test_strip_markdown_formatting_image() {
        let result = strip_markdown_formatting("See ![logo](image.png) for details");
        assert_eq!(result, "See logo for details");
    }

    #[test]
    fn test_strip_markdown_formatting_reference_link() {
        let result = strip_markdown_formatting("See [documentation][docs] for details");
        assert_eq!(result, "See documentation for details");
    }

    #[test]
    fn test_strip_markdown_formatting_combined() {
        // Link is stripped first, leaving bold, then bold is stripped
        let result = strip_markdown_formatting("Tool: [**terminal**](https://example.com)");
        assert_eq!(result, "Tool: terminal");
    }

    #[test]
    fn test_toc_with_link_in_heading_matches_stripped_text() {
        let rule = create_enabled_rule();

        // TOC entry text matches the stripped heading text
        let content = r#"# Title

<!-- toc -->

- [Tool: terminal](#tool-terminal)

<!-- tocstop -->

## Tool: [terminal](https://example.com)

Content here.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Stripped heading text should match TOC entry: {result:?}"
        );
    }

    #[test]
    fn test_toc_with_simplified_text_still_mismatches() {
        let rule = create_enabled_rule();

        // TOC entry "terminal" does NOT match stripped heading "Tool: terminal"
        let content = r#"# Title

<!-- toc -->

- [terminal](#tool-terminal)

<!-- tocstop -->

## Tool: [terminal](https://example.com)

Content here.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should report text mismatch");
        assert!(result[0].message.contains("Text mismatch"));
    }

    #[test]
    fn test_fix_generates_stripped_toc_entries() {
        let rule = MD073TocValidation::new();
        let content = r#"# Title

<!-- toc -->

<!-- tocstop -->

## Tool: [busybox](https://www.busybox.net/)

Content.

## Tool: [mount](https://en.wikipedia.org/wiki/Mount)

More content.
"#;
        let ctx = create_ctx(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Generated TOC should have stripped text (links removed)
        assert!(
            fixed.contains("- [Tool: busybox](#tool-busybox)"),
            "TOC entry should have stripped link text"
        );
        assert!(
            fixed.contains("- [Tool: mount](#tool-mount)"),
            "TOC entry should have stripped link text"
        );
        // TOC entries should NOT contain the URL (the actual headings in the document still will)
        // Check only within the TOC region (between toc markers)
        let toc_start = fixed.find("<!-- toc -->").unwrap();
        let toc_end = fixed.find("<!-- tocstop -->").unwrap();
        let toc_content = &fixed[toc_start..toc_end];
        assert!(
            !toc_content.contains("busybox.net"),
            "TOC should not contain URLs: {toc_content}"
        );
        assert!(
            !toc_content.contains("wikipedia.org"),
            "TOC should not contain URLs: {toc_content}"
        );
    }

    #[test]
    fn test_fix_with_bold_in_heading() {
        let rule = MD073TocValidation::new();
        let content = r#"# Title

<!-- toc -->

<!-- tocstop -->

## **Important** Section

Content.
"#;
        let ctx = create_ctx(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Generated TOC should have stripped text (bold markers removed)
        assert!(fixed.contains("- [Important Section](#important-section)"));
    }

    #[test]
    fn test_fix_with_code_in_heading() {
        let rule = MD073TocValidation::new();
        let content = r#"# Title

<!-- toc -->

<!-- tocstop -->

## Using `async` Functions

Content.
"#;
        let ctx = create_ctx(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Generated TOC should have stripped text (backticks removed)
        assert!(fixed.contains("- [Using async Functions](#using-async-functions)"));
    }

    // ========== Custom Anchor Tests ==========

    #[test]
    fn test_custom_anchor_id_respected() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

- [My Section](#my-custom-anchor)

<!-- tocstop -->

## My Section {#my-custom-anchor}

Content here.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should respect custom anchor IDs: {result:?}");
    }

    #[test]
    fn test_custom_anchor_id_in_generated_toc() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

<!-- tocstop -->

## First Section {#custom-first}

Content.

## Second Section {#another-custom}

More content.
"#;
        let ctx = create_ctx(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("- [First Section](#custom-first)"));
        assert!(fixed.contains("- [Second Section](#another-custom)"));
    }

    #[test]
    fn test_mixed_custom_and_generated_anchors() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

- [Custom Section](#my-id)
- [Normal Section](#normal-section)

<!-- tocstop -->

## Custom Section {#my-id}

Content.

## Normal Section

More content.
"#;
        let ctx = create_ctx(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should handle mixed custom and generated anchors");
    }

    // ========== Anchor Generation Tests ==========

    #[test]
    fn test_github_anchor_style() {
        let rule = create_enabled_rule();

        let content = r#"<!-- toc -->

<!-- tocstop -->

## Test_With_Underscores

Content.
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_toc_region(&ctx).unwrap();
        let expected = rule.build_expected_toc(&ctx, &region);

        // GitHub-style anchors preserve underscores
        assert_eq!(expected[0].anchor, "test_with_underscores");
    }

    // ========== Stress Tests ==========

    #[test]
    fn test_stress_many_headings() {
        let rule = create_enabled_rule();

        // Generate a document with 150 headings
        let mut content = String::from("# Title\n\n<!-- toc -->\n\n<!-- tocstop -->\n\n");

        for i in 1..=150 {
            content.push_str(&format!("## Heading Number {i}\n\nContent for section {i}.\n\n"));
        }

        let ctx = create_ctx(&content);

        // Should not panic or timeout
        let result = rule.check(&ctx).unwrap();

        // Should report missing entries for all 150 headings
        assert_eq!(result.len(), 1, "Should report single warning for TOC");
        assert!(result[0].message.contains("Missing entry"));

        // Fix should generate TOC with 150 entries
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("- [Heading Number 1](#heading-number-1)"));
        assert!(fixed.contains("- [Heading Number 100](#heading-number-100)"));
        assert!(fixed.contains("- [Heading Number 150](#heading-number-150)"));
    }

    #[test]
    fn test_stress_deeply_nested() {
        let rule = create_enabled_rule();
        let content = r#"# Title

<!-- toc -->

<!-- tocstop -->

## Level 2 A

### Level 3 A

#### Level 4 A

## Level 2 B

### Level 3 B

#### Level 4 B

## Level 2 C

### Level 3 C

#### Level 4 C

## Level 2 D

### Level 3 D

#### Level 4 D
"#;
        let ctx = create_ctx(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Check nested indentation is correct
        assert!(fixed.contains("- [Level 2 A](#level-2-a)"));
        assert!(fixed.contains("  - [Level 3 A](#level-3-a)"));
        assert!(fixed.contains("    - [Level 4 A](#level-4-a)"));
        assert!(fixed.contains("- [Level 2 D](#level-2-d)"));
        assert!(fixed.contains("  - [Level 3 D](#level-3-d)"));
        assert!(fixed.contains("    - [Level 4 D](#level-4-d)"));
    }

    #[test]
    fn test_stress_many_duplicates() {
        let rule = create_enabled_rule();

        // Generate 50 headings with the same text
        let mut content = String::from("# Title\n\n<!-- toc -->\n\n<!-- tocstop -->\n\n");
        for _ in 0..50 {
            content.push_str("## FAQ\n\nContent.\n\n");
        }

        let ctx = create_ctx(&content);
        let region = rule.detect_toc_region(&ctx).unwrap();
        let expected = rule.build_expected_toc(&ctx, &region);

        // Should generate unique anchors for all 50
        assert_eq!(expected.len(), 50);
        assert_eq!(expected[0].anchor, "faq");
        assert_eq!(expected[1].anchor, "faq-1");
        assert_eq!(expected[49].anchor, "faq-49");
    }
}
