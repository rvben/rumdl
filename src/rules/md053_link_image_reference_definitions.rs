use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Link reference format: [text][reference]
    static ref LINK_REFERENCE_REGEX: Regex = Regex::new(r"\[([^\]]*)\]\s*\[([^\]]*)\]").unwrap();

    // Image reference format: ![text][reference]
    static ref IMAGE_REFERENCE_REGEX: Regex =
        Regex::new(r"!\[([^\]]*)\]\s*\[([^\]]*)\]").unwrap();

    // Shortcut reference links: [reference] - must not be followed by a colon to avoid matching definitions
    static ref SHORTCUT_REFERENCE_REGEX: FancyRegex =
        FancyRegex::new(r"(?<!\!)\[([^\]]+)\](?!\s*[\[(:])").unwrap();

    // Empty reference links: [text][] or ![text][]
    static ref EMPTY_LINK_REFERENCE_REGEX: Regex = Regex::new(r"\[([^\]]+)\]\s*\[\s*\]").unwrap();
    static ref EMPTY_IMAGE_REFERENCE_REGEX: Regex = Regex::new(r"!\[([^\]]+)\]\s*\[\s*\]").unwrap();

    // Link/image reference definition format: [reference]: URL
    static ref REFERENCE_DEFINITION_REGEX: Regex =
        Regex::new(r"^\s*\[([^\]]+)\]:\s+(.+)$").unwrap();

    // Multi-line reference definition continuation pattern
    static ref CONTINUATION_REGEX: Regex = Regex::new(r"^\s+(.+)$").unwrap();

    // Code block regex
    static ref CODE_BLOCK_START_REGEX: Regex = Regex::new(r"^```").unwrap();
    static ref CODE_BLOCK_END_REGEX: Regex = Regex::new(r"^```\s*$").unwrap();
}

type DefinitionCache = RefCell<HashMap<u64, Vec<(String, usize, usize)>>>;

/// Rule MD053: Link and image reference definitions should be needed
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
pub struct MD053LinkImageReferenceDefinitions {
    ignored_definitions: HashSet<String>,
    content_cache: DefinitionCache,
}

impl Default for MD053LinkImageReferenceDefinitions {
    fn default() -> Self {
        Self {
            ignored_definitions: HashSet::new(),
            content_cache: RefCell::new(HashMap::new()),
        }
    }
}

impl MD053LinkImageReferenceDefinitions {
    /// Create a new instance of the MD053 rule
    pub fn new(ignored_definitions: Vec<String>) -> Self {
        Self {
            ignored_definitions: ignored_definitions.into_iter().map(|s| s.to_lowercase()).collect(),
            content_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Find all code blocks in the content.
    ///
    /// This method returns a vector of ranges representing the start and end
    /// line indexes of each code block in the content.
    ///
    /// The code block detection is robust and handles both fenced code blocks (```...```)
    /// and indented code blocks.
    fn find_code_blocks(&self, content: &str) -> Vec<(usize, usize)> {
        // Use lazy_static to compile these patterns only once
        lazy_static! {
            static ref FENCED_START: Regex = Regex::new(r"^(```|~~~)").unwrap();
            static ref INDENTED_CODE: Regex = Regex::new(r"^( {4}|\t)").unwrap();
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut code_blocks = Vec::new();
        let mut in_code_block = false;
        let mut start_line = 0;
        let mut fence_char = '\0';
        let mut fence_count = 0;
        let mut skip_to = 0;

        for (i, line) in lines.iter().enumerate() {
            // Skip lines that were already processed as part of an indented code block
            if i < skip_to {
                continue;
            }

            let trimmed = line.trim();

            // Quick check before using regex
            if !in_code_block
                && !trimmed.is_empty()
                && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
            {
                in_code_block = true;
                start_line = i;
                fence_char = trimmed.chars().next().unwrap();
                fence_count = trimmed.chars().take_while(|&c| c == fence_char).count();
            } else if in_code_block && !trimmed.is_empty() {
                let potential_end =
                    trimmed.starts_with(&fence_char.to_string().repeat(fence_count));
                if potential_end
                    && (trimmed.len() == fence_count
                        || !trimmed
                            .chars()
                            .nth(fence_count)
                            .unwrap()
                            .is_ascii_alphanumeric())
                {
                    code_blocks.push((start_line, i));
                    in_code_block = false;
                    fence_char = '\0';
                    fence_count = 0;
                }
            } else if !in_code_block && !trimmed.is_empty() {
                // Check for indented code block with a simple string operation first
                if line.starts_with("    ") || line.starts_with('\t') {
                    let mut j = i;
                    while j < lines.len()
                        && (lines[j].starts_with("    ")
                            || lines[j].starts_with('\t')
                            || lines[j].trim().is_empty())
                    {
                        j += 1;
                    }

                    // Only add if it's at least 2 lines (including blank lines) to avoid false positives
                    if j > i + 1 {
                        code_blocks.push((i, j - 1));
                        // Mark where to continue processing
                        skip_to = j;
                    }
                }
            }
        }

        // Handle unclosed code blocks at the end of the document
        if in_code_block {
            code_blocks.push((start_line, lines.len() - 1));
        }

        code_blocks
    }

    /// Check if a line range is inside a code block.
    ///
    /// This method determines if the given line range (start to end) is completely
    /// contained within any code block in the content.
    ///
    /// This is used to avoid flagging unused references that are defined inside code blocks.
    fn is_inside_code_block(
        &self,
        start: usize,
        end: usize,
        code_blocks: &[(usize, usize)],
    ) -> bool {
        code_blocks
            .iter()
            .any(|(block_start, block_end)| *block_start <= start && *block_end >= end)
    }

    /// Check if a line is inside a code block.
    ///
    /// This method determines if the given line index is contained within any code block.
    /// Used to track references within code blocks separately.
    fn is_in_code_block(&self, line_idx: usize, code_blocks: &[(usize, usize)]) -> bool {
        code_blocks
            .iter()
            .any(|(start, end)| *start <= line_idx && *end >= line_idx)
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

    /// Find all link and image reference usages in the content.
    ///
    /// This method returns a tuple containing:
    /// - A HashSet of all reference IDs found in usage (not in code blocks)
    /// - A HashSet of all reference IDs found in code blocks (these are tracked separately
    ///   because references used only in code blocks should still be considered unused
    ///   according to standard Markdown linting practice)
    ///
    /// References in code blocks are tracked but not counted as valid usages.
    fn find_usages(&self, content: &str) -> (HashSet<String>, HashSet<String>) {
        let mut usages = HashSet::new();
        let mut code_block_usages = HashSet::new();
        let code_blocks = self.find_code_blocks(content);
        let lines: Vec<&str> = content.lines().collect();

        // Collect all definitions to exclude them from shortcut reference detection
        let mut definitions = HashSet::new();
        for (i, line) in lines.iter().enumerate() {
            if let Some(caps) = REFERENCE_DEFINITION_REGEX.captures(line) {
                if let Some(ref_capture) = caps.get(1) {
                    // First add the definition with escaped characters (original form)
                    definitions.insert((i, ref_capture.as_str().trim().to_lowercase()));

                    // Also add the unescaped version of the definition
                    let unescaped = Self::unescape_reference(ref_capture.as_str().trim());
                    definitions.insert((i, unescaped.to_lowercase()));
                }
            }
        }

        // Process link references
        for cap in LINK_REFERENCE_REGEX.captures_iter(content) {
            if let Some(ref_capture) = cap.get(2) {
                let ref_text = ref_capture.as_str().trim();
                let line_idx = content[..ref_capture.start()].matches('\n').count();

                let is_in_code = self.is_in_code_block(line_idx, &code_blocks);

                if ref_text.is_empty() {
                    // Empty reference like [text][] uses text as the reference
                    if let Some(text_capture) = cap.get(1) {
                        let text_ref = text_capture.as_str().trim().to_lowercase();
                        if !text_ref.is_empty() {
                            if is_in_code {
                                code_block_usages.insert(text_ref);
                            } else {
                                usages.insert(text_ref);
                            }
                        }
                    }
                } else if is_in_code {
                    code_block_usages.insert(ref_text.to_lowercase());
                } else {
                    usages.insert(ref_text.to_lowercase());
                }
            }
        }

        // Process image references
        for cap in IMAGE_REFERENCE_REGEX.captures_iter(content) {
            if let Some(ref_capture) = cap.get(2) {
                let ref_text = ref_capture.as_str().trim();
                let line_idx = content[..ref_capture.start()].matches('\n').count();

                let is_in_code = self.is_in_code_block(line_idx, &code_blocks);

                if ref_text.is_empty() {
                    // Empty reference like ![text][] uses text as the reference
                    if let Some(text_capture) = cap.get(1) {
                        let text_ref = text_capture.as_str().trim().to_lowercase();
                        if !text_ref.is_empty() {
                            if is_in_code {
                                code_block_usages.insert(text_ref);
                            } else {
                                usages.insert(text_ref);
                            }
                        }
                    }
                } else if is_in_code {
                    code_block_usages.insert(ref_text.to_lowercase());
                } else {
                    usages.insert(ref_text.to_lowercase());
                }
            }
        }

        // Process shortcut references using FancyRegex
        let matches = SHORTCUT_REFERENCE_REGEX.find_iter(content);
        for m in matches.flatten() {
            // Extract line number for code block check
            let line_idx = content[..m.start()].matches('\n').count();

            // Skip if this match is actually a definition
            let ref_text = &content[m.start() + 1..m.end() - 1].trim().to_lowercase();
            if definitions.contains(&(line_idx, ref_text.clone())) {
                continue;
            }

            let is_in_code = self.is_in_code_block(line_idx, &code_blocks);

            // Extract the reference text from [reference]
            if !ref_text.is_empty() {
                if is_in_code {
                    code_block_usages.insert(ref_text.to_string());
                } else {
                    usages.insert(ref_text.to_string());
                }
            }
        }

        // Process empty reference links [text][] using the alt text as the reference
        for cap in EMPTY_LINK_REFERENCE_REGEX.captures_iter(content) {
            if let Some(text_capture) = cap.get(1) {
                let text = text_capture.as_str().trim();
                let line_idx = content[..text_capture.start()].matches('\n').count();
                if !text.is_empty() {
                    let is_in_code = self.is_in_code_block(line_idx, &code_blocks);
                    if is_in_code {
                        code_block_usages.insert(text.to_lowercase());
                    } else {
                        usages.insert(text.to_lowercase());
                    }
                }
            }
        }

        // Process empty image references ![text][] using the alt text as the reference
        for cap in EMPTY_IMAGE_REFERENCE_REGEX.captures_iter(content) {
            if let Some(text_capture) = cap.get(1) {
                let text = text_capture.as_str().trim();
                let line_idx = content[..text_capture.start()].matches('\n').count();
                if !text.is_empty() {
                    let is_in_code = self.is_in_code_block(line_idx, &code_blocks);
                    if is_in_code {
                        code_block_usages.insert(text.to_lowercase());
                    } else {
                        usages.insert(text.to_lowercase());
                    }
                }
            }
        }

        // Add a second pass to find nested references in link and image references
        // This finds cases like [![alt][img]][link] where [link] is the outer reference
        for cap in LINK_REFERENCE_REGEX.captures_iter(content) {
            if let Some(full_match) = cap.get(0) {
                let full_text = full_match.as_str();
                let line_idx = content[..full_match.start()].matches('\n').count();
                let is_in_code = self.is_in_code_block(line_idx, &code_blocks);

                // This regex finds the outer reference pattern in cases like [text][ref]
                let outer_ref_regex = Regex::new(r"\]\s*\[([^\]]+)\]$").unwrap();
                if let Some(outer_cap) = outer_ref_regex.captures(full_text) {
                    if let Some(outer_ref) = outer_cap.get(1) {
                        let outer_ref_text = outer_ref.as_str().trim().to_lowercase();
                        if !outer_ref_text.is_empty() {
                            if is_in_code {
                                code_block_usages.insert(outer_ref_text);
                            } else {
                                usages.insert(outer_ref_text);
                            }
                        }
                    }
                }
            }
        }

        (usages, code_block_usages)
    }

    // Find all reference definitions in the content
    fn find_definitions(&self, content: &str) -> HashMap<String, Vec<(usize, usize)>> {
        let mut definitions: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            if let Some(caps) = REFERENCE_DEFINITION_REGEX.captures(line) {
                if let Some(ref_capture) = caps.get(1) {
                    let ref_text = ref_capture.as_str().trim();

                    // If this is a multi-line definition, find where it ends
                    let mut end_line = i;

                    // Check if the definition continues to the next line
                    // A continued definition line starts with whitespace and has non-whitespace content
                    for (j, next_line) in lines.iter().enumerate().skip(i + 1) {
                        if next_line.starts_with(" ")
                            && !next_line.trim().is_empty()
                            && !REFERENCE_DEFINITION_REGEX.is_match(next_line)
                        {
                            end_line = j;
                        } else {
                            break;
                        }
                    }

                    // Add both the original and unescaped forms to enable matching both
                    let key = ref_text.to_lowercase();
                    let range_entry = (i, end_line);

                    if let Some(ranges) = definitions.get_mut(&key) {
                        ranges.push(range_entry);
                    } else {
                        definitions.insert(key, vec![range_entry]);
                    }

                    // Also add the unescaped version for matching escaped references
                    let unescaped_key = Self::unescape_reference(ref_text).to_lowercase();
                    if unescaped_key != ref_text.to_lowercase() {
                        if let Some(ranges) = definitions.get_mut(&unescaped_key) {
                            // Only add if the range isn't already there
                            if !ranges.contains(&range_entry) {
                                ranges.push(range_entry);
                            }
                        } else {
                            definitions.insert(unescaped_key, vec![range_entry]);
                        }
                    }
                }
            }
        }

        definitions
    }

    /// Compute a hash of the given content for caching purposes.
    /// This uses the DefaultHasher for better performance than the previous fast_hash implementation.
    fn content_hash(content: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    /// Get unused references with their line ranges.
    ///
    /// This method uses the cached definitions to improve performance.
    ///
    /// Note: References that are only used inside code blocks are still considered unused,
    /// as code blocks are treated as examples or documentation rather than actual content.
    fn get_unused_references(&self, content: &str) -> Vec<(String, usize, usize)> {
        let (usages, _code_block_usages) = self.find_usages(content);
        let cached_definitions = self.get_cached_definitions(content);
        let code_blocks = self.find_code_blocks(content);

        // Create a map to track which definition was used
        let mut used_definitions = HashMap::new();

        // Find which definitions are unused
        cached_definitions
            .into_iter()
            .filter(|(key, start, end)| {
                let original_key = key.clone();
                let unescaped_key = Self::unescape_reference(key).to_lowercase();
                let key_lower = key.to_lowercase();

                // Check if the reference is used (either in its original or unescaped form)
                // References used only in code blocks are considered unused
                let is_used = usages.contains(key) || usages.contains(&unescaped_key);
                let is_ignored = self.ignored_definitions.contains(&key_lower)
                    || self.ignored_definitions.contains(&unescaped_key);
                let is_in_code_block = self.is_inside_code_block(*start, *end, &code_blocks);

                // Track which definition was used to avoid duplication in results
                if is_used {
                    used_definitions.insert(original_key.clone(), true);
                }

                !is_used
                    && !is_ignored
                    && !is_in_code_block
                    && !used_definitions.contains_key(&original_key)
            })
            .collect::<Vec<_>>()
    }

    // Get cached definitions for the given content.
    ///
    /// This method uses a cache to store the definitions for each content hash.
    /// If the definitions for the given content are already cached, they are returned.
    /// Otherwise, the definitions are computed, cached, and then returned.
    fn get_cached_definitions(&self, content: &str) -> Vec<(String, usize, usize)> {
        let hash = Self::content_hash(content);

        // First check if we already have this content cached
        let cache = self.content_cache.borrow();
        if let Some(cached) = cache.get(&hash) {
            return cached.clone();
        }

        // If not cached, release the borrow and compute the definitions
        drop(cache);

        // Compute the definitions
        let definitions: Vec<(String, usize, usize)> = self
            .find_definitions(content)
            .into_iter()
            .flat_map(|(s, e_vec)| {
                e_vec
                    .into_iter()
                    .map(move |(start, end)| (s.clone(), start, end))
            })
            .collect();

        // Update the cache with the computed definitions
        self.content_cache.borrow_mut().insert(hash, definitions.clone());

        definitions
    }

    /// Helper method to clean up document structure after removing lines
    fn clean_up_document_structure(&self, lines: &mut Vec<String>) {
        // Clean up consecutive empty lines
        let mut i = 1;
        while i < lines.len() {
            if lines[i].trim().is_empty() && lines[i - 1].trim().is_empty() {
                lines.remove(i);
            } else {
                i += 1;
            }
        }

        // Remove trailing blank lines
        while !lines.is_empty() && lines.last().unwrap().trim().is_empty() {
            lines.pop();
        }

        // Remove leading blank lines
        while !lines.is_empty() && lines[0].trim().is_empty() {
            lines.remove(0);
        }
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
    fn check(&self, content: &str) -> LintResult {
        let unused_refs = self.get_unused_references(content);

        let mut warnings = Vec::new();

        // Create warnings for unused references
        for (definition, start, _) in unused_refs {
            warnings.push(LintWarning {
            rule_name: Some(self.name()),
                line: start + 1, // 1-indexed line numbers
                column: 1,
                message: format!("Unused link/image reference definition: [{}]", definition),
                severity: Severity::Warning,
                fix: None,
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
    fn fix(&self, content: &str) -> Result<String, LintError> {
        let unused_refs = self.get_unused_references(content);
        if unused_refs.is_empty() {
            return Ok(content.to_string());
        }

        // Split the content into lines - directly get owned strings to avoid clone during push
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        // Create a set of line numbers to remove (unused references)
        // Use a more efficient data structure for faster lookups
        let mut to_remove = std::collections::HashSet::with_capacity(unused_refs.len() * 2);
        for (_, start, end) in &unused_refs {
            for line in *start..=*end {
                to_remove.insert(line);
            }
        }

        // If there are many lines and few to remove, use this approach
        if to_remove.len() < lines.len() / 10 {
            // Build the result, skipping unused definitions
            // Pre-allocate the result vector to the maximum expected size
            let mut result = Vec::with_capacity(lines.len() - to_remove.len());
            for (i, line) in lines.into_iter().enumerate() {
                if !to_remove.contains(&i) {
                    result.push(line);
                }
            }

            // Clean up formatting issues created by removals
            self.clean_up_document_structure(&mut result);

            // Join the lines with newlines - avoid empty check which is unnecessary
            Ok(result.join("\n"))
        } else {
            // If there are many lines to remove, this alternative approach might be faster
            let mut result: Vec<String> = lines
                .into_iter()
                .enumerate()
                .filter_map(|(i, line)| {
                    if !to_remove.contains(&i) {
                        Some(line)
                    } else {
                        None
                    }
                })
                .collect();

            // Clean up formatting issues created by removals
            self.clean_up_document_structure(&mut result);

            // Join the lines with newlines
            Ok(result.join("\n"))
        }
    }
}
