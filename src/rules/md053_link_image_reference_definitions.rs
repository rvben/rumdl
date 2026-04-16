use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::range_utils::calculate_line_range;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

// Shortcut reference links: [reference] - must not be followed by another bracket
// Allow references followed by punctuation like colon, period, comma (e.g., "[reference]:", "[reference].")
// Don't exclude references followed by ": " in the middle of a line (only at start of line)
static SHORTCUT_REFERENCE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]").unwrap());

// Link/image reference definition format: [reference]: URL
static REFERENCE_DEFINITION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\[([^\]]+)\]:\s+(.+)$").unwrap());

// Multi-line reference definition continuation pattern
static CONTINUATION_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s+(.+)$").unwrap());

/// Configuration for MD053 rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD053Config {
    /// List of reference names to keep even if unused
    #[serde(default = "default_ignored_definitions")]
    pub ignored_definitions: Vec<String>,
}

impl Default for MD053Config {
    fn default() -> Self {
        Self {
            ignored_definitions: default_ignored_definitions(),
        }
    }
}

fn default_ignored_definitions() -> Vec<String> {
    Vec::new()
}

impl RuleConfig for MD053Config {
    const RULE_NAME: &'static str = "MD053";
}

/// Rule MD053: Link and image reference definitions should be used
///
/// See [docs/md053.md](../../docs/md053.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when a link or image reference definition is declared but not used
/// anywhere in the document. Unused reference definitions can create confusion and clutter.
///
/// ## Supported Reference Formats
///
/// This rule handles the following reference formats:
///
/// - **Full reference links/images**: `[text][reference]` or `![text][reference]`
/// - **Collapsed reference links/images**: `[text][]` or `![text][]`
/// - **Shortcut reference links**: `[reference]` (must be defined elsewhere)
/// - **Reference definitions**: `[reference]: URL "Optional Title"`
/// - **Multi-line reference definitions**:
///   ```markdown
///   [reference]: URL
///      "Optional title continued on next line"
///   ```
///
/// ## Configuration Options
///
/// The rule supports the following configuration options:
///
/// ```yaml
/// MD053:
///   ignored_definitions: []  # List of reference definitions to ignore (never report as unused)
/// ```
///
/// ## Performance Optimizations
///
/// This rule implements various performance optimizations for handling large documents:
///
/// 1. **Caching**: The rule caches parsed definitions and references based on content hashing
/// 2. **Efficient Reference Matching**: Uses HashMaps for O(1) lookups of definitions
/// 3. **Smart Code Block Handling**: Efficiently skips references inside code blocks/spans
/// 4. **Lazy Evaluation**: Only processes necessary portions of the document
///
/// ## Edge Cases Handled
///
/// - **Case insensitivity**: References are matched case-insensitively
/// - **Escaped characters**: Properly processes escaped characters in references
/// - **Unicode support**: Handles non-ASCII characters in references and URLs
/// - **Code blocks**: Ignores references inside code blocks and spans
/// - **Special characters**: Properly handles references with special characters
///
/// ## Fix Behavior
///
/// This rule does not provide automatic fixes. Unused references must be manually reviewed
/// and removed, as they may be intentionally kept for future use or as templates.
#[derive(Clone)]
pub struct MD053LinkImageReferenceDefinitions {
    config: MD053Config,
}

impl MD053LinkImageReferenceDefinitions {
    /// Create a new instance of the MD053 rule
    pub fn new() -> Self {
        Self {
            config: MD053Config::default(),
        }
    }

    /// Create a new instance with the given configuration
    pub fn from_config_struct(config: MD053Config) -> Self {
        Self { config }
    }

    /// Returns true if this pattern should be skipped during reference detection
    fn should_skip_pattern(text: &str) -> bool {
        // Don't skip pure numeric patterns - they could be footnote references like [1]
        // Only skip numeric ranges like [1:3], [0:10], etc.
        if text.contains(':') && text.chars().all(|c| c.is_ascii_digit() || c == ':') {
            return true;
        }

        // Skip glob/wildcard patterns like [*], [...], [**]
        if text == "*" || text == "..." || text == "**" {
            return true;
        }

        // Skip patterns that are just punctuation or operators
        if text.chars().all(|c| !c.is_alphanumeric() && c != ' ') {
            return true;
        }

        // Skip very short non-word patterns (likely operators or syntax)
        // But allow single digits (could be footnotes) and single letters
        if text.len() <= 2 && !text.chars().all(char::is_alphanumeric) {
            return true;
        }

        // Skip descriptive prose patterns with colon like [default: the project root]
        // But allow reference-style patterns like [RFC: 1234], [Issue: 42], [See: Section 2]
        // These are distinguished by having a short prefix (typically 1-2 words) before the colon
        if text.contains(':') && text.contains(' ') && !text.contains('`') {
            // Check if this looks like a reference pattern (short prefix before colon)
            // vs a prose description (longer text before colon)
            if let Some((before_colon, _)) = text.split_once(':') {
                let before_trimmed = before_colon.trim();
                // Count words before colon - references typically have 1-2 words
                let word_count = before_trimmed.split_whitespace().count();
                // If there are 3+ words before the colon, it's likely prose
                if word_count >= 3 {
                    return true;
                }
            }
        }

        // Skip alert/admonition patterns like [!WARN], [!NOTE], etc.
        if text.starts_with('!') {
            return true;
        }

        // Note: We don't filter out patterns with backticks because backticks in reference names
        // are valid markdown syntax, e.g., [`dataclasses.InitVar`] is a valid reference name

        // Also don't filter out references with dots - these are legitimate reference names
        // like [tool.ruff] or [os.path] which are valid markdown references

        // Note: We don't filter based on word count anymore because legitimate references
        // can have many words, like "python language reference for import statements"
        // Word count filtering was causing false positives where valid references were
        // being incorrectly flagged as unused

        false
    }

    /// Unescape a reference string by removing backslashes before special characters.
    ///
    /// This allows matching references like `[example\-reference]` with definitions like
    /// `[example-reference]: http://example.com`
    ///
    /// Returns the unescaped reference string.
    fn unescape_reference(reference: &str) -> String {
        // Remove backslashes before special characters
        reference.replace('\\', "")
    }

    /// Check if a reference definition is likely a comment-style reference.
    ///
    /// This recognizes common community patterns for comments in markdown:
    /// - `[//]: # (comment)` - Most popular pattern
    /// - `[comment]: # (text)` - Semantic pattern
    /// - `[note]: # (text)` - Documentation pattern
    /// - `[todo]: # (text)` - Task tracking pattern
    /// - Any reference with just `#` as the URL (fragment-only, often unused)
    ///
    /// While not part of any official markdown spec (CommonMark, GFM), these patterns
    /// are widely used across 23+ markdown implementations as documented in the community.
    ///
    /// # Arguments
    /// * `ref_id` - The reference ID (already normalized to lowercase)
    /// * `url` - The URL from the reference definition
    ///
    /// # Returns
    /// `true` if this looks like a comment-style reference that should be ignored
    fn is_likely_comment_reference(ref_id: &str, url: &str) -> bool {
        // Common comment reference labels used in the community
        const COMMENT_LABELS: &[&str] = &[
            "//",      // [//]: # (comment) - most popular
            "comment", // [comment]: # (text)
            "note",    // [note]: # (text)
            "todo",    // [todo]: # (text)
            "fixme",   // [fixme]: # (text)
            "hack",    // [hack]: # (text)
        ];

        let normalized_id = ref_id.trim().to_lowercase();
        let normalized_url = url.trim();

        // Pattern 1: Known comment labels with fragment URLs
        // e.g., [//]: # (comment), [comment]: #section
        if COMMENT_LABELS.contains(&normalized_id.as_str()) && normalized_url.starts_with('#') {
            return true;
        }

        // Pattern 2: Any reference with just "#" as the URL
        // This is often used as a comment placeholder or unused anchor
        if normalized_url == "#" {
            return true;
        }

        false
    }

    /// Find all link and image reference definitions in the content.
    ///
    /// This method returns a HashMap where the key is the normalized reference ID and the value is a vector of (start_line, end_line) tuples.
    fn find_definitions(&self, ctx: &crate::lint_context::LintContext) -> HashMap<String, Vec<(usize, usize)>> {
        let mut definitions: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

        // First, add all reference definitions from context
        for ref_def in &ctx.reference_defs {
            // Skip comment-style references (e.g., [//]: # (comment))
            if Self::is_likely_comment_reference(&ref_def.id, &ref_def.url) {
                continue;
            }

            // Apply unescape to handle escaped characters in definitions
            let normalized_id = Self::unescape_reference(&ref_def.id); // Already lowercase from context
            definitions
                .entry(normalized_id)
                .or_default()
                .push((ref_def.line - 1, ref_def.line - 1)); // Convert to 0-indexed
        }

        // Handle multi-line definitions by tracking the last definition seen
        let lines = &ctx.lines;
        let mut last_def_line: Option<usize> = None;
        let mut last_def_id: Option<String> = None;

        for (i, line_info) in lines.iter().enumerate() {
            if line_info.in_code_block || line_info.in_front_matter {
                last_def_line = None;
                last_def_id = None;
                continue;
            }

            let line = line_info.content(ctx.content);

            if let Some(caps) = REFERENCE_DEFINITION_REGEX.captures(line) {
                // Track this definition for potential continuation
                let ref_id = caps.get(1).unwrap().as_str().trim();
                let normalized_id = Self::unescape_reference(ref_id).to_lowercase();
                last_def_line = Some(i);
                last_def_id = Some(normalized_id);
            } else if let Some(def_start) = last_def_line
                && let Some(ref def_id) = last_def_id
                && CONTINUATION_REGEX.is_match(line)
            {
                // Extend the definition's end line
                if let Some(ranges) = definitions.get_mut(def_id.as_str())
                    && let Some(last_range) = ranges.last_mut()
                    && last_range.0 == def_start
                {
                    last_range.1 = i;
                }
            } else {
                // Non-continuation, non-definition line resets tracking
                last_def_line = None;
                last_def_id = None;
            }
        }
        definitions
    }

    /// Find all link and image reference reference usages in the content.
    ///
    /// This method returns a HashSet of all normalized reference IDs found in usage.
    /// It leverages cached data from LintContext for efficiency.
    fn find_usages(&self, ctx: &crate::lint_context::LintContext) -> HashSet<String> {
        let mut usages: HashSet<String> = HashSet::new();

        // 1. Add usages from cached reference links in LintContext
        for link in &ctx.links {
            if link.is_reference
                && let Some(ref_id) = &link.reference_id
                && !ctx.line_info(link.line).is_some_and(|info| info.in_code_block)
            {
                usages.insert(Self::unescape_reference(ref_id).to_lowercase());
            }
        }

        // 2. Add usages from cached reference images in LintContext
        for image in &ctx.images {
            if image.is_reference
                && let Some(ref_id) = &image.reference_id
                && !ctx.line_info(image.line).is_some_and(|info| info.in_code_block)
            {
                usages.insert(Self::unescape_reference(ref_id).to_lowercase());
            }
        }

        // 3. Add usages from footnote references (e.g., [^1], [^note])
        for footnote_ref in &ctx.footnote_refs {
            if !ctx.line_info(footnote_ref.line).is_some_and(|info| info.in_code_block) {
                let ref_id = format!("^{}", footnote_ref.id);
                usages.insert(ref_id.to_lowercase());
            }
        }

        // 4. Find shortcut references [ref] not already handled by DocumentStructure.links
        //    and ensure they are not within code spans or code blocks.
        let code_spans = ctx.code_spans();

        // Build sorted array of code span byte ranges for binary search
        let mut span_ranges: Vec<(usize, usize)> = code_spans
            .iter()
            .map(|span| (span.byte_offset, span.byte_end))
            .collect();
        span_ranges.sort_unstable_by_key(|&(start, _)| start);

        for line_info in ctx.lines.iter() {
            if line_info.in_code_block || line_info.in_front_matter {
                continue;
            }

            let line_content = line_info.content(ctx.content);

            // Quick check: skip lines without '[' (no possible references)
            if !line_content.contains('[') {
                continue;
            }

            // Skip lines that are reference definitions
            if REFERENCE_DEFINITION_REGEX.is_match(line_content) {
                continue;
            }

            for caps in SHORTCUT_REFERENCE_REGEX.captures_iter(line_content) {
                if let Some(full_match) = caps.get(0)
                    && let Some(ref_id_match) = caps.get(1)
                {
                    let match_start = full_match.start();

                    // Negative lookbehind: skip if preceded by ! (image syntax)
                    if match_start > 0 && line_content.as_bytes()[match_start - 1] == b'!' {
                        continue;
                    }

                    // Negative lookahead: skip if followed by [ (full reference link)
                    let match_end = full_match.end();
                    if match_end < line_content.len() && line_content.as_bytes()[match_end] == b'[' {
                        continue;
                    }

                    let match_byte_offset = line_info.byte_offset + match_start;

                    // Binary search for code span containment
                    let in_code_span = span_ranges
                        .binary_search_by(|&(start, end)| {
                            if match_byte_offset < start {
                                std::cmp::Ordering::Greater
                            } else if match_byte_offset >= end {
                                std::cmp::Ordering::Less
                            } else {
                                std::cmp::Ordering::Equal
                            }
                        })
                        .is_ok();

                    if !in_code_span {
                        let ref_id = ref_id_match.as_str().trim();

                        if !Self::should_skip_pattern(ref_id) {
                            let normalized_id = Self::unescape_reference(ref_id).to_lowercase();
                            usages.insert(normalized_id);
                        }
                    }
                }
            }
        }

        usages
    }

    /// Get unused references with their line ranges.
    ///
    /// This method uses the cached definitions to improve performance.
    ///
    /// Note: References that are only used inside code blocks are still considered unused,
    /// as code blocks are treated as examples or documentation rather than actual content.
    fn get_unused_references(
        &self,
        definitions: &HashMap<String, Vec<(usize, usize)>>,
        usages: &HashSet<String>,
    ) -> Vec<(String, usize, usize)> {
        let mut unused = Vec::new();
        for (id, ranges) in definitions {
            // If this id is not used anywhere and is not in the ignored list
            if !usages.contains(id) && !self.is_ignored_definition(id) {
                // Only report as unused if there's exactly one definition
                // Multiple definitions are already reported as duplicates
                if ranges.len() == 1 {
                    let (start, end) = ranges[0];
                    unused.push((id.clone(), start, end));
                }
                // If there are multiple definitions (duplicates), don't report them as unused
                // They're already being reported as duplicate definitions
            }
        }
        unused
    }

    /// Check if a definition should be ignored (kept even if unused)
    fn is_ignored_definition(&self, definition_id: &str) -> bool {
        self.config
            .ignored_definitions
            .iter()
            .any(|ignored| ignored.eq_ignore_ascii_case(definition_id))
    }
}

impl Default for MD053LinkImageReferenceDefinitions {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for MD053LinkImageReferenceDefinitions {
    fn name(&self) -> &'static str {
        "MD053"
    }

    fn description(&self) -> &'static str {
        "Link and image reference definitions should be needed"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    /// Check the content for unused and duplicate link/image reference definitions.
    ///
    /// This implementation uses caching for improved performance on large documents.
    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Find definitions and usages using LintContext
        let definitions = self.find_definitions(ctx);
        let usages = self.find_usages(ctx);

        // Get unused references by comparing definitions and usages
        let unused_refs = self.get_unused_references(&definitions, &usages);

        let mut warnings = Vec::new();

        // Check for duplicate definitions (case-insensitive per CommonMark spec)
        let mut seen_definitions: HashMap<String, (String, usize)> = HashMap::new(); // lowercase -> (original, first_line)

        for (definition_id, ranges) in &definitions {
            // Skip ignored definitions for duplicate checking
            if self.is_ignored_definition(definition_id) {
                continue;
            }

            if ranges.len() > 1 {
                // Multiple definitions with exact same ID (already lowercase)
                for (i, &(start_line, _)) in ranges.iter().enumerate() {
                    if i > 0 {
                        // Skip the first occurrence, report all others
                        let line_num = start_line + 1;
                        let line_content = ctx.lines.get(start_line).map_or("", |l| l.content(ctx.content));
                        let (start_line_1idx, start_col, end_line, end_col) =
                            calculate_line_range(line_num, line_content);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: start_line_1idx,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!("Duplicate link or image reference definition: [{definition_id}]"),
                            severity: Severity::Warning,
                            fix: None,
                        });
                    }
                }
            }

            // Track for case-variant duplicates
            if let Some(&(start_line, _)) = ranges.first() {
                // Find the original case version from the line
                if let Some(line_info) = ctx.lines.get(start_line)
                    && let Some(caps) = REFERENCE_DEFINITION_REGEX.captures(line_info.content(ctx.content))
                {
                    let original_id = caps.get(1).unwrap().as_str().trim();
                    let lower_id = original_id.to_lowercase();

                    if let Some((first_original, first_line)) = seen_definitions.get(&lower_id) {
                        // Found a case-variant duplicate
                        if first_original != original_id {
                            let line_num = start_line + 1;
                            let line_content = line_info.content(ctx.content);
                            let (start_line_1idx, start_col, end_line, end_col) =
                                calculate_line_range(line_num, line_content);

                            warnings.push(LintWarning {
                                    rule_name: Some(self.name().to_string()),
                                    line: start_line_1idx,
                                    column: start_col,
                                    end_line,
                                    end_column: end_col,
                                    message: format!("Duplicate link or image reference definition: [{}] (conflicts with [{}] on line {})",
                                                   original_id, first_original, first_line + 1),
                                    severity: Severity::Warning,
                                    fix: None,
                                });
                        }
                    } else {
                        seen_definitions.insert(lower_id, (original_id.to_string(), start_line));
                    }
                }
            }
        }

        // Create warnings for unused references
        for (definition, start, _end) in unused_refs {
            let line_num = start + 1; // 1-indexed line numbers
            let line_content = ctx.lines.get(start).map_or("", |l| l.content(ctx.content));

            // Calculate precise character range for the entire reference definition line
            let (start_line, start_col, end_line, end_col) = calculate_line_range(line_num, line_content);

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!("Unused link/image reference: [{definition}]"),
                severity: Severity::Warning,
                fix: None, // MD053 is warning-only, no automatic fixes
            });
        }

        Ok(warnings)
    }

    /// MD053 does not provide automatic fixes
    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // This rule is warning-only, no automatic fixes provided
        Ok(ctx.content.to_string())
    }

    /// Check if this rule should be skipped for performance
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no links/images
        ctx.content.is_empty() || !ctx.likely_has_links_or_images()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD053Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;
        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD053Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD053Config>(config);
        Box::new(MD053LinkImageReferenceDefinitions::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_used_reference_link() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[text][ref]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_unused_reference_definition() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[unused]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Unused link/image reference: [unused]"));
    }

    #[test]
    fn test_used_reference_image() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "![alt][img]\n\n[img]: image.jpg";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_case_insensitive_matching() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[Text][REF]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_shortcut_reference() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_collapsed_reference() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref][]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_multiple_unused_definitions() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[unused1]: url1\n[unused2]: url2\n[unused3]: url3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3);

        // The warnings might not be in the same order, so collect all messages
        let messages: Vec<String> = result.iter().map(|w| w.message.clone()).collect();
        assert!(messages.iter().any(|m| m.contains("unused1")));
        assert!(messages.iter().any(|m| m.contains("unused2")));
        assert!(messages.iter().any(|m| m.contains("unused3")));
    }

    #[test]
    fn test_mixed_used_and_unused() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[used]\n\n[used]: url1\n[unused]: url2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("unused"));
    }

    #[test]
    fn test_multiline_definition() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref]: https://example.com\n  \"Title on next line\"";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1); // Still unused
    }

    #[test]
    fn test_reference_in_code_block() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "```\n[ref]\n```\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Reference used only in code block is still considered unused
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_reference_in_inline_code() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "`[ref]`\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Reference in inline code is not a usage
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_escaped_reference() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[example\\-ref]\n\n[example-ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should match despite escaping
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_duplicate_definitions() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref]: url1\n[ref]: url2\n\n[ref]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag the duplicate definition even though it's used (matches markdownlint)
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_fix_returns_original() {
        // MD053 is warning-only, fix should return original content
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[used]\n\n[used]: url1\n[unused]: url2\n\nMore content";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_preserves_content() {
        // MD053 is warning-only, fix should preserve all content
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "Content\n\n[unused]: url\n\nMore content";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_does_not_remove() {
        // MD053 is warning-only, fix should not remove anything
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[unused1]: url1\n[unused2]: url2\n[unused3]: url3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, content);
    }

    #[test]
    fn test_special_characters_in_reference() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref-with_special.chars]\n\n[ref-with_special.chars]: url";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_find_definitions() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref1]: url1\n[ref2]: url2\nSome text\n[ref3]: url3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let defs = rule.find_definitions(&ctx);

        assert_eq!(defs.len(), 3);
        assert!(defs.contains_key("ref1"));
        assert!(defs.contains_key("ref2"));
        assert!(defs.contains_key("ref3"));
    }

    #[test]
    fn test_find_usages() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[text][ref1] and [ref2] and ![img][ref3]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let usages = rule.find_usages(&ctx);

        assert!(usages.contains("ref1"));
        assert!(usages.contains("ref2"));
        assert!(usages.contains("ref3"));
    }

    #[test]
    fn test_ignored_definitions_config() {
        // Test with ignored definitions
        let config = MD053Config {
            ignored_definitions: vec!["todo".to_string(), "draft".to_string()],
        };
        let rule = MD053LinkImageReferenceDefinitions::from_config_struct(config);

        let content = "[todo]: https://example.com/todo\n[draft]: https://example.com/draft\n[unused]: https://example.com/unused";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only flag "unused", not "todo" or "draft"
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("unused"));
        assert!(!result[0].message.contains("todo"));
        assert!(!result[0].message.contains("draft"));
    }

    #[test]
    fn test_ignored_definitions_case_insensitive() {
        // Test case-insensitive matching of ignored definitions
        let config = MD053Config {
            ignored_definitions: vec!["TODO".to_string()],
        };
        let rule = MD053LinkImageReferenceDefinitions::from_config_struct(config);

        let content = "[todo]: https://example.com/todo\n[unused]: https://example.com/unused";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only flag "unused", not "todo" (matches "TODO" case-insensitively)
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("unused"));
        assert!(!result[0].message.contains("todo"));
    }

    #[test]
    fn test_default_config_section() {
        let rule = MD053LinkImageReferenceDefinitions::default();
        let config_section = rule.default_config_section();

        assert!(config_section.is_some());
        let (name, value) = config_section.unwrap();
        assert_eq!(name, "MD053");

        // Should contain the ignored_definitions option with default empty array
        if let toml::Value::Table(table) = value {
            assert!(table.contains_key("ignored-definitions"));
            assert_eq!(table["ignored-definitions"], toml::Value::Array(vec![]));
        } else {
            panic!("Expected TOML table");
        }
    }

    #[test]
    fn test_fix_with_ignored_definitions() {
        // MD053 is warning-only, fix should not remove anything even with ignored definitions
        let config = MD053Config {
            ignored_definitions: vec!["template".to_string()],
        };
        let rule = MD053LinkImageReferenceDefinitions::from_config_struct(config);

        let content = "[template]: https://example.com/template\n[unused]: https://example.com/unused\n\nSome content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should keep everything since MD053 doesn't fix
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_duplicate_definitions_exact_case() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref]: url1\n[ref]: url2\n[ref]: url3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should have 2 duplicate warnings (for the 2nd and 3rd definitions)
        // Plus 1 unused warning
        let duplicate_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Duplicate")).collect();
        assert_eq!(duplicate_warnings.len(), 2);
        assert_eq!(duplicate_warnings[0].line, 2);
        assert_eq!(duplicate_warnings[1].line, 3);
    }

    #[test]
    fn test_duplicate_definitions_case_variants() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content =
            "[method resolution order]: url1\n[Method Resolution Order]: url2\n[METHOD RESOLUTION ORDER]: url3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should have 2 duplicate warnings (for the 2nd and 3rd definitions)
        // Note: These are treated as exact duplicates since they normalize to the same ID
        let duplicate_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Duplicate")).collect();
        assert_eq!(duplicate_warnings.len(), 2);

        // The exact duplicate messages don't include "conflicts with"
        // Only case-variant duplicates with different normalized forms would
        assert_eq!(duplicate_warnings[0].line, 2);
        assert_eq!(duplicate_warnings[1].line, 3);
    }

    #[test]
    fn test_duplicate_and_unused() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[used]\n[used]: url1\n[used]: url2\n[unused]: url3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should have 1 duplicate warning and 1 unused warning
        let duplicate_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Duplicate")).collect();
        let unused_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Unused")).collect();

        assert_eq!(duplicate_warnings.len(), 1);
        assert_eq!(unused_warnings.len(), 1);
        assert_eq!(duplicate_warnings[0].line, 3); // Second [used] definition
        assert_eq!(unused_warnings[0].line, 4); // [unused] definition
    }

    #[test]
    fn test_duplicate_with_usage() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        // Even if used, duplicates should still be reported
        let content = "[ref]\n\n[ref]: url1\n[ref]: url2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should have 1 duplicate warning (no unused since it's referenced)
        let duplicate_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Duplicate")).collect();
        let unused_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Unused")).collect();

        assert_eq!(duplicate_warnings.len(), 1);
        assert_eq!(unused_warnings.len(), 0);
        assert_eq!(duplicate_warnings[0].line, 4);
    }

    #[test]
    fn test_no_duplicate_different_ids() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref1]: url1\n[ref2]: url2\n[ref3]: url3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should have no duplicate warnings, only unused warnings
        let duplicate_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("Duplicate")).collect();
        assert_eq!(duplicate_warnings.len(), 0);
    }

    #[test]
    fn test_comment_style_reference_double_slash() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        // Most popular comment pattern: [//]: # (comment)
        let content = "[//]: # (This is a comment)\n\nSome regular text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not report as unused - it's recognized as a comment
        assert_eq!(result.len(), 0, "Comment-style reference [//]: # should not be flagged");
    }

    #[test]
    fn test_comment_style_reference_comment_label() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        // Semantic comment pattern: [comment]: # (text)
        let content = "[comment]: # (This is a semantic comment)\n\n[note]: # (This is a note)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not report either as unused
        assert_eq!(result.len(), 0, "Comment-style references should not be flagged");
    }

    #[test]
    fn test_comment_style_reference_todo_fixme() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        // Task tracking patterns: [todo]: # and [fixme]: #
        let content = "[todo]: # (Add more examples)\n[fixme]: # (Fix this later)\n[hack]: # (Temporary workaround)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not report any as unused
        assert_eq!(result.len(), 0, "TODO/FIXME comment patterns should not be flagged");
    }

    #[test]
    fn test_comment_style_reference_fragment_only() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        // Any reference with just "#" as URL should be treated as a comment
        let content = "[anything]: #\n[ref]: #\n\nSome text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not report as unused - fragment-only URLs are often comments
        assert_eq!(result.len(), 0, "References with just '#' URL should not be flagged");
    }

    #[test]
    fn test_comment_vs_real_reference() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        // Mix of comment and real reference - only real one should be flagged if unused
        let content = "[//]: # (This is a comment)\n[real-ref]: https://example.com\n\nSome text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only report the real reference as unused
        assert_eq!(result.len(), 1, "Only real unused references should be flagged");
        assert!(result[0].message.contains("real-ref"), "Should flag the real reference");
    }

    #[test]
    fn test_comment_with_fragment_section() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        // Comment pattern with a fragment section (still a comment)
        let content = "[//]: #section (Comment about section)\n\nSome text.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not report as unused - it's still a comment pattern
        assert_eq!(result.len(), 0, "Comment with fragment section should not be flagged");
    }

    #[test]
    fn test_is_likely_comment_reference_helper() {
        // Test the helper function directly
        assert!(
            MD053LinkImageReferenceDefinitions::is_likely_comment_reference("//", "#"),
            "[//]: # should be recognized as comment"
        );
        assert!(
            MD053LinkImageReferenceDefinitions::is_likely_comment_reference("comment", "#section"),
            "[comment]: #section should be recognized as comment"
        );
        assert!(
            MD053LinkImageReferenceDefinitions::is_likely_comment_reference("note", "#"),
            "[note]: # should be recognized as comment"
        );
        assert!(
            MD053LinkImageReferenceDefinitions::is_likely_comment_reference("todo", "#"),
            "[todo]: # should be recognized as comment"
        );
        assert!(
            MD053LinkImageReferenceDefinitions::is_likely_comment_reference("anything", "#"),
            "Any label with just '#' should be recognized as comment"
        );
        assert!(
            !MD053LinkImageReferenceDefinitions::is_likely_comment_reference("ref", "https://example.com"),
            "Real URL should not be recognized as comment"
        );
        assert!(
            !MD053LinkImageReferenceDefinitions::is_likely_comment_reference("link", "http://test.com"),
            "Real URL should not be recognized as comment"
        );
    }

    #[test]
    fn test_reference_with_colon_in_name() {
        // References containing colons and spaces should be recognized as valid references
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "Check [RFC: 1234] for specs.\n\n[RFC: 1234]: https://example.com\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Reference with colon should be recognized as used, got warnings: {result:?}"
        );
    }

    #[test]
    fn test_reference_with_colon_various_styles() {
        // Test various RFC-style and similar references with colons
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = r#"See [RFC: 1234] and [Issue: 42] and [PR: 100].

[RFC: 1234]: https://example.com/rfc1234
[Issue: 42]: https://example.com/issue42
[PR: 100]: https://example.com/pr100
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "All colon-style references should be recognized as used, got warnings: {result:?}"
        );
    }

    #[test]
    fn test_should_skip_pattern_allows_rfc_style() {
        // Verify that should_skip_pattern does NOT skip RFC-style references with colons
        // This tests the fix for the bug where references with ": " were incorrectly skipped
        assert!(
            !MD053LinkImageReferenceDefinitions::should_skip_pattern("RFC: 1234"),
            "RFC-style references should NOT be skipped"
        );
        assert!(
            !MD053LinkImageReferenceDefinitions::should_skip_pattern("Issue: 42"),
            "Issue-style references should NOT be skipped"
        );
        assert!(
            !MD053LinkImageReferenceDefinitions::should_skip_pattern("PR: 100"),
            "PR-style references should NOT be skipped"
        );
        assert!(
            !MD053LinkImageReferenceDefinitions::should_skip_pattern("See: Section 2"),
            "References with 'See:' should NOT be skipped"
        );
        assert!(
            !MD053LinkImageReferenceDefinitions::should_skip_pattern("foo:bar"),
            "References without space after colon should NOT be skipped"
        );
    }

    #[test]
    fn test_should_skip_pattern_skips_prose() {
        // Verify that prose-like patterns (3+ words before colon) are still skipped
        assert!(
            MD053LinkImageReferenceDefinitions::should_skip_pattern("default value is: something"),
            "Prose with 3+ words before colon SHOULD be skipped"
        );
        assert!(
            MD053LinkImageReferenceDefinitions::should_skip_pattern("this is a label: description"),
            "Prose with 4 words before colon SHOULD be skipped"
        );
        assert!(
            MD053LinkImageReferenceDefinitions::should_skip_pattern("the project root: path/to/dir"),
            "Prose-like descriptions SHOULD be skipped"
        );
    }

    #[test]
    fn test_many_code_spans_with_shortcut_references() {
        // Exercises the binary search path for code span containment.
        // With many code spans, linear search would be slow; binary search stays O(log n).
        let rule = MD053LinkImageReferenceDefinitions::new();

        let mut lines = Vec::new();
        // Generate many code spans interleaved with shortcut references
        for i in 0..100 {
            lines.push(format!("Some `code{i}` text and [used_ref] here"));
        }
        lines.push(String::new());
        lines.push("[used_ref]: https://example.com".to_string());
        lines.push("[unused_ref]: https://unused.com".to_string());

        let content = lines.join("\n");
        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // used_ref is referenced 100 times, so only unused_ref should be reported
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("unused_ref"));
    }

    #[test]
    fn test_multiline_definition_continuation_tracking() {
        // Exercises the forward-tracking for multi-line definitions.
        // Definitions with title on the next line should be treated as a single unit.
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "\
[ref1]: https://example.com
   \"Title on next line\"

[ref2]: https://example2.com
   \"Another title\"

Some text using [ref1] here.
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // ref1 is used, ref2 is not
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("ref2"));
    }

    #[test]
    fn test_code_span_at_boundary_does_not_hide_reference() {
        // A reference immediately after a code span should still be detected
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "`code`[ref]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // [ref] is outside the code span, so it counts as a usage
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_reference_inside_code_span_not_counted() {
        // A reference inside a code span should NOT be counted as usage
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "Use `[ref]` in code\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // [ref] is inside a code span, so the definition is unused
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_shortcut_ref_at_byte_zero() {
        let rule = MD053LinkImageReferenceDefinitions::default();
        let content = "[example]\n\n[example]: https://example.com\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "[ref] at byte 0 should be recognized as usage: {result:?}"
        );
    }

    #[test]
    fn test_shortcut_ref_at_end_of_line() {
        let rule = MD053LinkImageReferenceDefinitions::default();
        let content = "Text [example]\n\n[example]: https://example.com\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "[ref] at end of line should be recognized as usage: {result:?}"
        );
    }

    #[test]
    fn test_reference_in_multiline_footnote_not_false_positive() {
        // Issue #540: reference link inside multi-line footnote body
        // was falsely reported as unused because the 4-space-indented
        // footnote continuation was misidentified as an indented code block
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "\
# Greetings

This is a paragraph that has a footnote.[^footnote]

[^footnote]:
    This footnote is long enough that it doesn't fit on just one line.
    Here is my [website][web].

[web]: https://web.evanchen.cc
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Reference used inside multi-line footnote should not be flagged: {result:?}"
        );
    }

    #[test]
    fn test_reference_in_single_line_footnote() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "\
# Greetings

This is a paragraph that has a footnote.[^footnote]

[^footnote]: Here is my [website][web].

[web]: https://web.evanchen.cc
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Reference used inside single-line footnote should not be flagged: {result:?}"
        );
    }

    #[test]
    fn test_shortcut_reference_in_multiline_footnote() {
        // Shortcut reference [web] (not full [text][web]) inside footnote
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "\
Text with footnote.[^note]

[^note]:
    See [web] for details.

[web]: https://example.com
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Shortcut reference inside multi-line footnote should not be flagged: {result:?}"
        );
    }

    #[test]
    fn test_unused_reference_not_in_footnote_still_flagged() {
        // Ensure we don't accidentally suppress real unused references
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "\
# Greetings

This is a paragraph that has a footnote.[^footnote]

[^footnote]:
    This footnote is long enough.

[unused]: https://example.com
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("unused"));
    }

    #[test]
    fn test_image_reference_in_multiline_footnote() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "\
Text with footnote.[^note]

[^note]:
    Here is a diagram:
    ![diagram][img]

[img]: https://example.com/diagram.png
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Image reference inside multi-line footnote should not be flagged: {result:?}"
        );
    }

    #[test]
    fn test_multiple_references_in_one_footnote() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "\
Text.[^note]

[^note]:
    See [link1][ref1] and [link2][ref2] for details.

[ref1]: https://example.com
[ref2]: https://example.org
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multiple references inside one footnote should all be recognized: {result:?}"
        );
    }

    #[test]
    fn test_reference_in_code_block_inside_footnote_not_counted() {
        // Fenced code blocks within footnotes should still be treated as code.
        // A [ref] pattern inside a fenced code block is not a usage.
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "\
Text.[^code]

[^code]:
    ```python
    x = [ref_like_syntax]
    ```

[ref_like_syntax]: https://example.com
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Reference inside fenced code block within footnote should still be unused: {result:?}"
        );
        assert!(result[0].message.contains("ref_like_syntax"));
    }

    #[test]
    fn test_nested_list_in_footnote_with_reference() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "\
Text.[^deep]

[^deep]:
    - List item
        - Nested with [link text][deep-ref]

[deep-ref]: https://example.com
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Reference in nested list inside footnote should not be flagged: {result:?}"
        );
    }
}
