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

/// Detection method for TOC regions
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TocDetection {
    /// Only detect using `<!-- toc -->...<!-- tocstop -->` markers
    #[default]
    Markers,
    /// Only detect using heading like "## Table of Contents"
    Heading,
    /// Try markers first, fall back to heading detection
    Both,
}

/// Represents a detected TOC region in the document
#[derive(Debug, Clone)]
struct TocRegion {
    /// 1-indexed start line of the TOC content (after the marker/heading)
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
}

/// MD073: Table of Contents Validation
///
/// This rule validates that TOC sections match the actual document headings.
/// It can detect TOC regions via markers (`<!-- toc -->...<!-- tocstop -->`)
/// or by heading patterns.
///
/// ## Configuration
///
/// ```toml
/// [MD073]
/// # Detection method: "markers", "heading", or "both"
/// detection = "both"
/// # Minimum heading level to include (default: 2)
/// min-level = 2
/// # Maximum heading level to include (default: 4)
/// max-level = 4
/// # Whether TOC order must match document order (default: true)
/// enforce-order = true
/// # Whether to use nested indentation (default: true)
/// nested = true
/// # Anchor generation style (default: "github")
/// anchor-style = "github"
/// # Headings that indicate a TOC section
/// toc-headings = ["Table of Contents", "Contents", "TOC"]
/// ```
#[derive(Clone)]
pub struct MD073TocValidation {
    /// How to detect TOC regions
    detection: TocDetection,
    /// Minimum heading level to include
    min_level: u8,
    /// Maximum heading level to include
    max_level: u8,
    /// Whether to enforce order matching
    enforce_order: bool,
    /// Whether to nest entries based on heading level
    nested: bool,
    /// Anchor generation style
    anchor_style: AnchorStyle,
    /// Heading patterns that indicate TOC sections
    toc_headings: Vec<String>,
}

impl Default for MD073TocValidation {
    fn default() -> Self {
        Self {
            detection: TocDetection::Both,
            min_level: 2,
            max_level: 4,
            enforce_order: true,
            nested: true,
            anchor_style: AnchorStyle::GitHub,
            toc_headings: vec![
                "Table of Contents".to_string(),
                "Contents".to_string(),
                "TOC".to_string(),
            ],
        }
    }
}

impl std::fmt::Debug for MD073TocValidation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MD073TocValidation")
            .field("detection", &self.detection)
            .field("min_level", &self.min_level)
            .field("max_level", &self.max_level)
            .field("enforce_order", &self.enforce_order)
            .field("nested", &self.nested)
            .field("toc_headings", &self.toc_headings)
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

    /// Detect TOC region by heading pattern
    fn detect_by_heading(&self, ctx: &LintContext) -> Option<TocRegion> {
        let mut toc_heading_line = None;
        let mut content_start_line = None;
        let mut content_start_byte = None;
        let mut blank_streak = 0usize;

        for (idx, line_info) in ctx.lines.iter().enumerate() {
            let line_num = idx + 1;

            // Skip if in code block or front matter
            if line_info.in_code_block || line_info.in_front_matter {
                continue;
            }

            // Look for TOC heading
            if toc_heading_line.is_none() {
                if let Some(heading) = &line_info.heading {
                    // Check if heading text matches any TOC heading pattern
                    let heading_text = heading.text.trim();
                    if self.toc_headings.iter().any(|h| h.eq_ignore_ascii_case(heading_text)) {
                        toc_heading_line = Some(line_num);
                        // Content starts on the next line
                        if idx + 1 < ctx.lines.len() {
                            content_start_line = Some(line_num + 1);
                            content_start_byte = Some(ctx.lines[idx + 1].byte_offset);
                        }
                    }
                }
            } else if content_start_line.is_some() {
                // We found the TOC heading, now find where the TOC ends
                // TOC ends at:
                // 1. Next heading
                // 2. Two consecutive blank lines
                // 3. End of document

                // Check for next heading
                if line_info.heading.is_some() {
                    let end_line = line_num - 1;
                    let content_end = line_info.byte_offset;

                    // Skip backwards over trailing blank lines
                    let mut actual_end = end_line;
                    while actual_end >= content_start_line.unwrap() {
                        let check_idx = actual_end - 1;
                        if check_idx < ctx.lines.len() && ctx.lines[check_idx].is_blank {
                            actual_end -= 1;
                        } else {
                            break;
                        }
                    }

                    if actual_end < content_start_line.unwrap() {
                        actual_end = content_start_line.unwrap();
                    }

                    return Some(TocRegion {
                        start_line: content_start_line.unwrap(),
                        end_line: actual_end,
                        content_start: content_start_byte.unwrap(),
                        content_end,
                    });
                }

                // Check for two consecutive blank lines
                if line_info.is_blank {
                    blank_streak += 1;
                    if blank_streak >= 2 {
                        let start_line = content_start_line.unwrap();
                        let first_blank_idx = idx - 1;
                        let mut end_line = line_num.saturating_sub(2);
                        if end_line < start_line {
                            end_line = start_line;
                        }

                        return Some(TocRegion {
                            start_line,
                            end_line,
                            content_start: content_start_byte.unwrap(),
                            content_end: ctx.lines[first_blank_idx].byte_offset,
                        });
                    }
                } else {
                    blank_streak = 0;
                }
            }
        }

        // If we found a TOC heading but no subsequent heading, TOC goes to end
        if let (Some(start_line), Some(start_byte)) = (content_start_line, content_start_byte) {
            // Find last non-blank line
            let mut end_line = ctx.lines.len();
            while end_line > start_line {
                let check_idx = end_line - 1;
                if check_idx < ctx.lines.len() && ctx.lines[check_idx].is_blank {
                    end_line -= 1;
                } else {
                    break;
                }
            }

            return Some(TocRegion {
                start_line,
                end_line,
                content_start: start_byte,
                content_end: ctx.content.len(),
            });
        }

        None
    }

    /// Detect TOC region based on configured detection method
    fn detect_toc_region(&self, ctx: &LintContext) -> Option<TocRegion> {
        match self.detection {
            TocDetection::Markers => self.detect_by_markers(ctx),
            TocDetection::Heading => self.detect_by_heading(ctx),
            TocDetection::Both => {
                // Try markers first, then fall back to heading
                self.detect_by_markers(ctx).or_else(|| self.detect_by_heading(ctx))
            }
        }
    }

    /// Extract TOC entries from the detected region
    fn extract_toc_entries(&self, ctx: &LintContext, region: &TocRegion) -> Vec<TocEntry> {
        let mut entries = Vec::new();

        for idx in (region.start_line - 1)..region.end_line.min(ctx.lines.len()) {
            let line_info = &ctx.lines[idx];
            let content = line_info.content(ctx.content);

            if let Some(caps) = TOC_ENTRY_PATTERN.captures(content) {
                let text = caps.get(2).map_or("", |m| m.as_str()).to_string();
                let anchor = caps.get(3).map_or("", |m| m.as_str()).to_string();

                entries.push(TocEntry { text, anchor });
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

                // Use custom ID if available
                let base_anchor = if let Some(custom_id) = &heading.custom_id {
                    custom_id.clone()
                } else {
                    self.anchor_style.generate_fragment(&heading.text)
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

        // Check for text mismatches
        for entry in actual {
            if let Some(exp) = expected_anchors.get(entry.anchor.as_str()) {
                // Normalize comparison (trim, case-insensitive for very flexible matching)
                if entry.text.trim() != exp.text.trim() {
                    mismatches.push(TocMismatch::TextMismatch {
                        entry: entry.clone(),
                        expected: (*exp).clone(),
                    });
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

    /// Generate a new TOC from expected entries
    fn generate_toc(&self, expected: &[ExpectedTocEntry]) -> String {
        if expected.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        let base_level = expected.iter().map(|e| e.level).min().unwrap_or(2);

        for entry in expected {
            let indent = if self.nested {
                let level_diff = entry.level.saturating_sub(base_level) as usize;
                "  ".repeat(level_diff)
            } else {
                String::new()
            };

            result.push_str(&format!("{indent}- [{}](#{})\n", entry.text, entry.anchor));
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
        // Quick check: if no TOC markers or headings that could be TOC
        let has_toc_marker = ctx.content.contains("<!-- toc") || ctx.content.contains("<!--toc");
        let has_toc_heading = self
            .toc_headings
            .iter()
            .any(|h| ctx.content.to_lowercase().contains(&h.to_lowercase()));

        !has_toc_marker && !has_toc_heading
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
# Detection method: "markers", "heading", or "both"
detection = "both"
# Minimum heading level to include
min-level = 2
# Maximum heading level to include
max-level = 4
# Whether TOC order must match document order
enforce-order = true
# Whether to use nested indentation
nested = true
# Anchor generation style
anchor-style = "github"
# Headings that indicate a TOC section
toc-headings = ["Table of Contents", "Contents", "TOC"]
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

        if let Some(rule_config) = config.rules.get("MD073") {
            // Parse detection method
            if let Some(detection_str) = rule_config.values.get("detection").and_then(|v| v.as_str()) {
                rule.detection = match detection_str.to_lowercase().as_str() {
                    "markers" => TocDetection::Markers,
                    "heading" => TocDetection::Heading,
                    _ => TocDetection::Both,
                };
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

            // Parse nested
            if let Some(nested) = rule_config.values.get("nested").and_then(|v| v.as_bool()) {
                rule.nested = nested;
            }

            // Parse anchor-style
            if let Some(style_str) = rule_config.values.get("anchor-style").and_then(|v| v.as_str()) {
                rule.anchor_style = match style_str.to_lowercase().as_str() {
                    "kramdown" => AnchorStyle::Kramdown,
                    "kramdown-gfm" | "jekyll" => AnchorStyle::KramdownGfm,
                    _ => AnchorStyle::GitHub,
                };
            }

            // Parse toc-headings
            if let Some(headings) = rule_config.values.get("toc-headings").and_then(|v| v.as_array()) {
                let custom_headings: Vec<String> = headings
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if !custom_headings.is_empty() {
                    rule.toc_headings = custom_headings;
                }
            }
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
    fn test_detect_heading_table_of_contents() {
        let mut rule = MD073TocValidation::new();
        rule.detection = TocDetection::Heading;

        let content = r#"# Title

## Table of Contents

- [Heading 1](#heading-1)
- [Heading 2](#heading-2)

## Heading 1

Content.

## Heading 2

More content.
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_by_heading(&ctx);
        assert!(region.is_some());
        let region = region.unwrap();
        // Verify the TOC region was detected
        assert_eq!(region.start_line, 4);
    }

    #[test]
    fn test_detect_heading_ends_on_double_blank_lines() {
        let mut rule = MD073TocValidation::new();
        rule.detection = TocDetection::Heading;

        let content = r#"# Title

## Table of Contents

- [Heading 1](#heading-1)


This text is not part of TOC.

## Heading 1

Content.
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_by_heading(&ctx).unwrap();
        assert_eq!(region.start_line, 4);
        assert_eq!(region.end_line, 5);
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
        let rule = MD073TocValidation::new();
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
        let rule = MD073TocValidation::new();
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
        let rule = MD073TocValidation::new();
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
        let rule = MD073TocValidation::new();
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
    fn test_fix_heading_detection_stops_on_double_blank_lines() {
        let mut rule = MD073TocValidation::new();
        rule.detection = TocDetection::Heading;

        let content = r#"# Title

## Table of Contents

- [Heading 1](#heading-1)


This text is not part of TOC.

## Heading 1

Content.
"#;
        let ctx = create_ctx(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("- [Heading 1](#heading-1)"));
        assert!(fixed.contains("This text is not part of TOC."));
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
        let rule = MD073TocValidation::new();
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
        let rule = MD073TocValidation::new();
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
        let mut rule = MD073TocValidation::new();
        rule.nested = true;

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

        // Check indentation
        assert!(toc.contains("- [Level 2](#level-2)"));
        assert!(toc.contains("  - [Level 3](#level-3)"));
        assert!(toc.contains("    - [Level 4](#level-4)"));
        assert!(toc.contains("- [Another Level 2](#another-level-2)"));
    }

    #[test]
    fn test_flat_no_indentation() {
        let mut rule = MD073TocValidation::new();
        rule.nested = false;

        let content = r#"<!-- toc -->

<!-- tocstop -->

## Level 2

### Level 3

#### Level 4
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_toc_region(&ctx).unwrap();
        let expected = rule.build_expected_toc(&ctx, &region);
        let toc = rule.generate_toc(&expected);

        // All entries should have no indentation
        for line in toc.lines() {
            if !line.is_empty() {
                assert!(line.starts_with("- ["), "Line should start without indent: {line}");
            }
        }
    }

    // ========== Order Mismatch Tests ==========

    #[test]
    fn test_order_mismatch_detected() {
        let rule = MD073TocValidation::new();
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
        let mut rule = MD073TocValidation::new();
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
        let rule = MD073TocValidation::new();
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
        let rule = MD073TocValidation::new();
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
        let rule = MD073TocValidation::new();
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
        assert!(rule.nested);
    }

    // ========== Custom Anchor Tests ==========

    #[test]
    fn test_custom_anchor_id_respected() {
        let rule = MD073TocValidation::new();
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
        let rule = MD073TocValidation::new();
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
        let rule = MD073TocValidation::new();
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

    // ========== Anchor Style Tests ==========

    #[test]
    fn test_github_anchor_style_default() {
        let rule = MD073TocValidation::new();
        assert_eq!(rule.anchor_style, AnchorStyle::GitHub);

        let content = r#"<!-- toc -->

<!-- tocstop -->

## Test_With_Underscores

Content.
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_toc_region(&ctx).unwrap();
        let expected = rule.build_expected_toc(&ctx, &region);

        // GitHub preserves underscores
        assert_eq!(expected[0].anchor, "test_with_underscores");
    }

    #[test]
    fn test_kramdown_anchor_style() {
        let mut rule = MD073TocValidation::new();
        rule.anchor_style = AnchorStyle::Kramdown;

        let content = r#"<!-- toc -->

<!-- tocstop -->

## Test_With_Underscores

Content.
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_toc_region(&ctx).unwrap();
        let expected = rule.build_expected_toc(&ctx, &region);

        // Kramdown removes underscores
        assert_eq!(expected[0].anchor, "testwithunderscores");
    }

    #[test]
    fn test_kramdown_gfm_anchor_style() {
        let mut rule = MD073TocValidation::new();
        rule.anchor_style = AnchorStyle::KramdownGfm;

        let content = r#"<!-- toc -->

<!-- tocstop -->

## Test_With_Underscores

Content.
"#;
        let ctx = create_ctx(content);
        let region = rule.detect_toc_region(&ctx).unwrap();
        let expected = rule.build_expected_toc(&ctx, &region);

        // KramdownGfm preserves underscores
        assert_eq!(expected[0].anchor, "test_with_underscores");
    }

    // ========== Stress Tests ==========

    #[test]
    fn test_stress_many_headings() {
        let rule = MD073TocValidation::new();

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
        let rule = MD073TocValidation::new();
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
        let rule = MD073TocValidation::new();

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
