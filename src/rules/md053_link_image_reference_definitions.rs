use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::document_structure::DocumentStructure;
use crate::utils::range_utils::calculate_line_range;
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

lazy_static! {
    // Link reference format: [text][reference]
    // REMOVED: static ref LINK_REFERENCE_REGEX: FancyRegex = FancyRegex::new(r"\[([^\]]*)\]\s*\[([^\]]*)\]").unwrap();

    // Image reference format: ![text][reference]
    // REMOVED: static ref IMAGE_REFERENCE_REGEX: FancyRegex = FancyRegex::new(r"!\[([^\]]*)\]\s*\[([^\]]*)\]").unwrap();

    // Shortcut reference links: [reference] - must not be followed by a colon to avoid matching definitions
    static ref SHORTCUT_REFERENCE_REGEX: FancyRegex =
        FancyRegex::new(r"(?<!\!)\[([^\]]+)\](?!\s*[\[(:])").unwrap();

    // REMOVED: Empty reference links: [text][] or ![text][]
    // static ref EMPTY_LINK_REFERENCE_REGEX: Regex = Regex::new(r"\[([^\]]+)\]\s*\[\s*\]").unwrap();
    // static ref EMPTY_IMAGE_REFERENCE_REGEX: Regex = Regex::new(r"!\[([^\]]+)\]\s*\[\s*\]").unwrap();

    // Link/image reference definition format: [reference]: URL
    static ref REFERENCE_DEFINITION_REGEX: Regex =
        Regex::new(r"^\s*\[([^\]]+)\]:\s+(.+)$").unwrap();

    // Multi-line reference definition continuation pattern
    static ref CONTINUATION_REGEX: Regex = Regex::new(r"^\s+(.+)$").unwrap();

    // Code block regex
    static ref CODE_BLOCK_START_REGEX: Regex = Regex::new(r"^```").unwrap();
    static ref CODE_BLOCK_END_REGEX: Regex = Regex::new(r"^```\s*$").unwrap();
}

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
/// When fixing issues, this rule removes unused reference definitions while preserving
/// the document's structure, including handling proper blank line formatting around
/// the removed definitions.
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

    /// Unescape a reference string by removing backslashes before special characters.
    ///
    /// This allows matching references like `[example\-reference]` with definitions like
    /// `[example-reference]: http://example.com`
    ///
    /// Returns the unescaped reference string.
    fn unescape_reference(reference: &str) -> String {
        // Remove backslashes before special characters
        reference.replace("\\", "")
    }

    /// Find all link and image reference definitions in the content.
    ///
    /// This method returns a HashMap where the key is the normalized reference ID and the value is a vector of (start_line, end_line) tuples.
    fn find_definitions(
        &self,
        ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> HashMap<String, Vec<(usize, usize)>> {
        let mut definitions: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

        // First, add all reference definitions from context
        for ref_def in &ctx.reference_defs {
            // Apply unescape to handle escaped characters in definitions
            let normalized_id = Self::unescape_reference(&ref_def.id); // Already lowercase from context
            definitions
                .entry(normalized_id)
                .or_default()
                .push((ref_def.line - 1, ref_def.line - 1)); // Convert to 0-indexed
        }

        // Handle multi-line definitions that might not be fully captured by ctx.reference_defs
        let lines = &ctx.lines;
        let mut i = 0;
        while i < lines.len() {
            let line_info = &lines[i];
            let line = &line_info.content;

            // Skip code blocks and front matter using line info
            if line_info.in_code_block || doc_structure.is_in_front_matter(i + 1) {
                i += 1;
                continue;
            }

            // Check for multi-line continuation of existing definitions
            if i > 0 && CONTINUATION_REGEX.is_match(line) {
                // Find the reference definition this continues
                let mut def_start = i - 1;
                while def_start > 0 && !REFERENCE_DEFINITION_REGEX.is_match(&lines[def_start].content) {
                    def_start -= 1;
                }

                if let Some(caps) = REFERENCE_DEFINITION_REGEX.captures(&lines[def_start].content) {
                    let ref_id = caps.get(1).unwrap().as_str().trim();
                    let normalized_id = Self::unescape_reference(ref_id).to_lowercase();

                    // Update the end line for this definition
                    if let Some(ranges) = definitions.get_mut(&normalized_id)
                        && let Some(last_range) = ranges.last_mut()
                        && last_range.0 == def_start
                    {
                        last_range.1 = i;
                    }
                }
            }
            i += 1;
        }
        definitions
    }

    /// Find all link and image reference reference usages in the content.
    ///
    /// This method returns a HashSet of all normalized reference IDs found in usage.
    /// It leverages cached data from LintContext for efficiency.
    fn find_usages(
        &self,
        doc_structure: &DocumentStructure,
        ctx: &crate::lint_context::LintContext,
    ) -> HashSet<String> {
        let mut usages: HashSet<String> = HashSet::new();

        // 1. Add usages from cached reference links in LintContext
        for link in &ctx.links {
            if link.is_reference
                && let Some(ref_id) = &link.reference_id
            {
                // Ensure the link itself is not inside a code block line
                if !doc_structure.is_in_code_block(link.line) {
                    usages.insert(Self::unescape_reference(ref_id).to_lowercase());
                }
            }
        }

        // 2. Add usages from cached reference images in LintContext
        for image in &ctx.images {
            if image.is_reference
                && let Some(ref_id) = &image.reference_id
            {
                // Ensure the image itself is not inside a code block line
                if !doc_structure.is_in_code_block(image.line) {
                    usages.insert(Self::unescape_reference(ref_id).to_lowercase());
                }
            }
        }

        // 3. Find shortcut references [ref] not already handled by DocumentStructure.links
        //    and ensure they are not within code spans or code blocks.
        // Cache code spans once before the loop
        let code_spans = ctx.code_spans();

        for (i, line_info) in ctx.lines.iter().enumerate() {
            let line_num = i + 1; // 1-indexed

            // Skip lines in code blocks or front matter
            if line_info.in_code_block || doc_structure.is_in_front_matter(line_num) {
                continue;
            }

            // Find potential shortcut references
            for caps in SHORTCUT_REFERENCE_REGEX.captures_iter(&line_info.content).flatten() {
                if let Some(full_match) = caps.get(0)
                    && let Some(ref_id_match) = caps.get(1)
                {
                    // Check if the match is within a code span
                    let match_byte_offset = line_info.byte_offset + full_match.start();
                    let in_code_span = code_spans
                        .iter()
                        .any(|span| match_byte_offset >= span.byte_offset && match_byte_offset < span.byte_end);

                    if !in_code_span {
                        let ref_id = ref_id_match.as_str().trim();
                        let normalized_id = Self::unescape_reference(ref_id).to_lowercase();
                        usages.insert(normalized_id);
                    }
                }
            }
        }

        // NOTE: The complex recursive loop trying to find references within definitions
        // has been removed as it's not standard Markdown behavior for finding *usages*.
        // Usages refer to `[text][ref]`, `![alt][ref]`, `[ref]`, etc., in the main content,
        // not references potentially embedded within the URL or title of another definition.

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
            // If this id is not used anywhere and is not in the ignored list, all its ranges are unused
            if !usages.contains(id) && !self.is_ignored_definition(id) {
                for (start, end) in ranges {
                    unused.push((id.clone(), *start, *end));
                }
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

    /// Clean up multiple consecutive blank lines that might be created after removing references
    fn clean_up_blank_lines(&self, content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let mut result_lines = Vec::new();
        let mut consecutive_blanks = 0;

        for line in lines {
            if line.trim().is_empty() {
                consecutive_blanks += 1;
                if consecutive_blanks <= 1 {
                    // Allow up to 1 consecutive blank line
                    result_lines.push(line);
                }
            } else {
                consecutive_blanks = 0;
                result_lines.push(line);
            }
        }

        // Remove leading and trailing blank lines
        while !result_lines.is_empty() && result_lines[0].trim().is_empty() {
            result_lines.remove(0);
        }
        while !result_lines.is_empty() && result_lines[result_lines.len() - 1].trim().is_empty() {
            result_lines.pop();
        }

        // Don't add trailing newlines - let the content determine its own ending
        result_lines.join("\n")
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

    /// Check the content for unused link/image reference definitions.
    ///
    /// This implementation uses caching for improved performance on large documents.
    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        // Compute DocumentStructure once
        let doc_structure = DocumentStructure::new(content);

        // Find definitions and usages using DocumentStructure
        let definitions = self.find_definitions(ctx, &doc_structure);
        let usages = self.find_usages(&doc_structure, ctx);

        // Get unused references by comparing definitions and usages
        let unused_refs = self.get_unused_references(&definitions, &usages);

        let mut warnings = Vec::new();

        // Create warnings for unused references
        for (definition, start, _end) in unused_refs {
            let line_num = start + 1; // 1-indexed line numbers
            let line_content = ctx.lines.get(start).map(|l| l.content.as_str()).unwrap_or("");

            // Calculate precise character range for the entire reference definition line
            let (start_line, start_col, end_line, end_col) = calculate_line_range(line_num, line_content);

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!("Unused link/image reference: [{definition}]"),
                severity: Severity::Warning,
                fix: Some(Fix {
                    // Remove the entire line including the newline
                    range: {
                        let line_start = ctx.line_to_byte_offset(line_num).unwrap_or(0);
                        let line_end = if line_num < ctx.lines.len() {
                            ctx.line_to_byte_offset(line_num + 1).unwrap_or(content.len())
                        } else {
                            content.len()
                        };
                        line_start..line_end
                    },
                    replacement: String::new(), // Remove the line
                }),
            });
        }

        Ok(warnings)
    }

    /// Fix the content by removing unused link/image reference definitions.
    ///
    /// This implementation uses caching for improved performance on large documents.
    /// It optimizes the process by:
    /// 1. Using cached definitions to avoid re-parsing the document
    /// 2. Preserving document structure while removing unused references
    /// 3. Cleaning up any formatting issues created by the removals
    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let doc_structure = DocumentStructure::new(content);

        // Find definitions and usages using DocumentStructure
        let definitions = self.find_definitions(ctx, &doc_structure);
        let usages = self.find_usages(&doc_structure, ctx);

        // Get unused references by comparing definitions and usages
        let unused_refs = self.get_unused_references(&definitions, &usages);

        // If no unused references, return original content
        if unused_refs.is_empty() {
            return Ok(content.to_string());
        }

        // Collect all line ranges to remove (sort by start line descending)
        let mut lines_to_remove: Vec<(usize, usize)> =
            unused_refs.iter().map(|(_, start, end)| (*start, *end)).collect();
        lines_to_remove.sort_by(|a, b| b.0.cmp(&a.0)); // Sort descending by start line

        // Remove lines from end to beginning to preserve line numbers
        let lines: Vec<&str> = ctx.lines.iter().map(|l| l.content.as_str()).collect();
        let mut result_lines: Vec<&str> = lines.clone();

        for (start_line, end_line) in lines_to_remove {
            // Remove lines from start_line to end_line (inclusive)
            if start_line < result_lines.len() && end_line < result_lines.len() {
                result_lines.drain(start_line..=end_line);
            }
        }

        // Join the remaining lines
        let mut result = result_lines.join("\n");

        // Preserve original ending (with or without newline)
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        // Clean up multiple consecutive blank lines that might have been created
        let cleaned = self.clean_up_blank_lines(&result);

        Ok(cleaned)
    }

    /// Check if this rule should be skipped for performance
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no reference definitions
        ctx.content.is_empty() || !ctx.content.contains("]:")
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_unused_reference_definition() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[unused]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Unused link/image reference: [unused]"));
    }

    #[test]
    fn test_used_reference_image() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "![alt][img]\n\n[img]: image.jpg";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_case_insensitive_matching() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[Text][REF]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_shortcut_reference() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_collapsed_reference() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref][]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_multiple_unused_definitions() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[unused1]: url1\n[unused2]: url2\n[unused3]: url3";
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("unused"));
    }

    #[test]
    fn test_multiline_definition() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref]: https://example.com\n  \"Title on next line\"";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1); // Still unused
    }

    #[test]
    fn test_reference_in_code_block() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "```\n[ref]\n```\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Reference used only in code block is still considered unused
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_reference_in_inline_code() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "`[ref]`\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Reference in inline code is not a usage
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_escaped_reference() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[example\\-ref]\n\n[example-ref]: https://example.com";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should match despite escaping
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_duplicate_definitions() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref]: url1\n[ref]: url2\n\n[ref]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Both definitions are used (Markdown uses the first one)
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_removes_unused_definition() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[used]\n\n[used]: url1\n[unused]: url2\n\nMore content";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("[used]: url1"));
        assert!(!fixed.contains("[unused]: url2"));
        assert!(fixed.contains("More content"));
    }

    #[test]
    fn test_fix_preserves_blank_lines() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "Content\n\n[unused]: url\n\nMore content";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "Content\n\nMore content");
    }

    #[test]
    fn test_fix_multiple_consecutive_definitions() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[unused1]: url1\n[unused2]: url2\n[unused3]: url3";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "");
    }

    #[test]
    fn test_special_characters_in_reference() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref-with_special.chars]\n\n[ref-with_special.chars]: url";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_find_definitions() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[ref1]: url1\n[ref2]: url2\nSome text\n[ref3]: url3";
        let ctx = LintContext::new(content);
        let doc = DocumentStructure::new(content);
        let defs = rule.find_definitions(&ctx, &doc);

        assert_eq!(defs.len(), 3);
        assert!(defs.contains_key("ref1"));
        assert!(defs.contains_key("ref2"));
        assert!(defs.contains_key("ref3"));
    }

    #[test]
    fn test_find_usages() {
        let rule = MD053LinkImageReferenceDefinitions::new();
        let content = "[text][ref1] and [ref2] and ![img][ref3]";
        let ctx = LintContext::new(content);
        let doc = DocumentStructure::new(content);
        let usages = rule.find_usages(&doc, &ctx);

        assert!(usages.contains("ref1"));
        assert!(usages.contains("ref2"));
        assert!(usages.contains("ref3"));
    }

    #[test]
    fn test_clean_up_blank_lines() {
        let rule = MD053LinkImageReferenceDefinitions::new();

        // Test multiple consecutive blank lines
        assert_eq!(rule.clean_up_blank_lines("text\n\n\n\nmore text"), "text\n\nmore text");

        // Test leading/trailing blank lines
        assert_eq!(rule.clean_up_blank_lines("\n\ntext\n\n"), "text");
    }

    #[test]
    fn test_ignored_definitions_config() {
        // Test with ignored definitions
        let config = MD053Config {
            ignored_definitions: vec!["todo".to_string(), "draft".to_string()],
        };
        let rule = MD053LinkImageReferenceDefinitions::from_config_struct(config);

        let content = "[todo]: https://example.com/todo\n[draft]: https://example.com/draft\n[unused]: https://example.com/unused";
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
    fn test_fix_respects_ignored_definitions() {
        // Test that fix respects ignored definitions
        let config = MD053Config {
            ignored_definitions: vec!["template".to_string()],
        };
        let rule = MD053LinkImageReferenceDefinitions::from_config_struct(config);

        let content = "[template]: https://example.com/template\n[unused]: https://example.com/unused\n\nSome content.";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Should keep template but remove unused
        assert!(fixed.contains("[template]: https://example.com/template"));
        assert!(!fixed.contains("[unused]: https://example.com/unused"));
        assert!(fixed.contains("Some content."));
    }
}
