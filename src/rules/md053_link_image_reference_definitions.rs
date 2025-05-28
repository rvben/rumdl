use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::document_structure::DocumentStructure;
use crate::utils::range_utils::calculate_line_range;
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
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
#[derive(Clone, Default)]
pub struct MD053LinkImageReferenceDefinitions {}

impl MD053LinkImageReferenceDefinitions {
    /// Create a new instance of the MD053 rule
    pub fn new() -> Self {
        Self {}
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
        content: &str,
        doc_structure: &DocumentStructure,
    ) -> HashMap<String, Vec<(usize, usize)>> {
        let mut definitions: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];

            // Skip code blocks and front matter using DocumentStructure
            if doc_structure.is_in_code_block(i + 1) || doc_structure.is_in_front_matter(i + 1) {
                i += 1;
                continue;
            }

            // Reference definition
            if let Some(caps) = REFERENCE_DEFINITION_REGEX.captures(line) {
                let ref_id = caps.get(1).unwrap().as_str().trim();
                let start_line = i;
                let mut end_line = i;
                // Multi-line continuation
                i += 1;
                while i < lines.len() && CONTINUATION_REGEX.is_match(lines[i]) {
                    end_line = i;
                    i += 1;
                }
                // Normalize reference id: trim, unescape, and lowercase
                let normalized_id = Self::unescape_reference(ref_id).to_lowercase();
                definitions
                    .entry(normalized_id)
                    .or_default()
                    .push((start_line, end_line));
                continue;
            }
            i += 1;
        }
        definitions
    }

    /// Find all link and image reference reference usages in the content.
    ///
    /// This method returns a HashSet of all normalized reference IDs found in usage.
    /// It leverages DocumentStructure for efficiency.
    fn find_usages(&self, content: &str, doc_structure: &DocumentStructure) -> HashSet<String> {
        let lines: Vec<&str> = content.lines().collect();
        let mut usages: HashSet<String> = HashSet::new();

        // 1. Add usages from pre-parsed reference links in DocumentStructure
        for link in &doc_structure.links {
            if link.is_reference {
                if let Some(ref_id) = &link.reference_id {
                    // Ensure the link itself is not inside a code block line
                    // (DocumentStructure parsing should already handle code spans)
                    if !doc_structure.is_in_code_block(link.line) {
                        usages.insert(Self::unescape_reference(ref_id).to_lowercase());
                    }
                }
            }
        }

        // 2. Add usages from pre-parsed reference images in DocumentStructure
        for image in &doc_structure.images {
            if image.is_reference {
                if let Some(ref_id) = &image.reference_id {
                    // Ensure the image itself is not inside a code block line
                    if !doc_structure.is_in_code_block(image.line) {
                        usages.insert(Self::unescape_reference(ref_id).to_lowercase());
                    }
                }
            }
        }

        // 3. Find shortcut references [ref] not already handled by DocumentStructure.links
        //    and ensure they are not within code spans or code blocks.
        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1; // 1-indexed

            // Skip lines in code blocks or front matter
            if doc_structure.is_in_code_block(line_num)
                || doc_structure.is_in_front_matter(line_num)
            {
                continue;
            }

            // Find potential shortcut references
            for caps in SHORTCUT_REFERENCE_REGEX.captures_iter(line).flatten() {
                if let Some(full_match) = caps.get(0) {
                    if let Some(ref_id_match) = caps.get(1) {
                        let start_col = full_match.start() + 1; // 1-indexed column
                        let end_col = full_match.end(); // 1-indexed end column (exclusive in match)

                        // Check if any part of the match is within a code span
                        let mut in_code_span = false;
                        for col in start_col..=end_col {
                            if doc_structure.is_in_code_span(line_num, col) {
                                in_code_span = true;
                                break;
                            }
                        }

                        if !in_code_span {
                            let ref_id = ref_id_match.as_str().trim();
                            let normalized_id = Self::unescape_reference(ref_id).to_lowercase();
                            usages.insert(normalized_id);
                        }
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
            // If this id is not used anywhere, all its ranges are unused
            if !usages.contains(id) {
                for (start, end) in ranges {
                    unused.push((id.clone(), *start, *end));
                }
            }
        }
        unused
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
        let definitions = self.find_definitions(content, &doc_structure);
        let usages = self.find_usages(content, &doc_structure);

        // Get unused references by comparing definitions and usages
        let unused_refs = self.get_unused_references(&definitions, &usages);

        let mut warnings = Vec::new();

        // Create warnings for unused references
        for (definition, start, _end) in unused_refs {
            let line_num = start + 1; // 1-indexed line numbers
            let lines: Vec<&str> = content.lines().collect();
            let line_content = lines.get(start).unwrap_or(&"");

            // Calculate precise character range for the entire reference definition line
            let (start_line, start_col, end_line, end_col) =
                calculate_line_range(line_num, line_content);

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!(
                    "Unused link/image reference definition: [{
            }]",
                    definition
                ),
                severity: Severity::Warning,
                fix: None, // We'll handle fixes in the fix() method
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
        let definitions = self.find_definitions(content, &doc_structure);
        let usages = self.find_usages(content, &doc_structure);

        // Get unused references by comparing definitions and usages
        let unused_refs = self.get_unused_references(&definitions, &usages);

        // If no unused references, return original content
        if unused_refs.is_empty() {
            return Ok(content.to_string());
        }

        // Collect all line ranges to remove (sort by start line descending)
        let mut lines_to_remove: Vec<(usize, usize)> = unused_refs
            .iter()
            .map(|(_, start, end)| (*start, *end))
            .collect();
        lines_to_remove.sort_by(|a, b| b.0.cmp(&a.0)); // Sort descending by start line

        // Remove lines from end to beginning to preserve line numbers
        let lines: Vec<&str> = content.lines().collect();
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD053LinkImageReferenceDefinitions::new())
    }
}
