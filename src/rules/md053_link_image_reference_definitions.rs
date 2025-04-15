use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use crate::utils::document_structure::DocumentStructure;

lazy_static! {
    // Link reference format: [text][reference]
    static ref LINK_REFERENCE_REGEX: FancyRegex = FancyRegex::new(r"\[([^\]]*)\]\s*\[([^\]]*)\]").unwrap();

    // Image reference format: ![text][reference]
    static ref IMAGE_REFERENCE_REGEX: FancyRegex = FancyRegex::new(r"!\[([^\]]*)\]\s*\[([^\]]*)\]").unwrap();

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

type DefinitionCache = Rc<RefCell<HashMap<u64, Vec<(String, usize, usize, usize)>>>>;

// Move the ReferenceDefinition struct to the module level
struct ReferenceDefinition {
    id: String,
    line: usize,
    start_col: usize,
    end_col: usize,
}

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
#[derive(Clone)]
pub struct MD053LinkImageReferenceDefinitions {
    ignored_definitions: HashSet<String>,
    content_cache: DefinitionCache,
}

impl Default for MD053LinkImageReferenceDefinitions {
    fn default() -> Self {
        Self {
            ignored_definitions: HashSet::new(),
            content_cache: Rc::new(RefCell::new(HashMap::new())),
        }
    }
}

impl MD053LinkImageReferenceDefinitions {
    /// Create a new instance of the MD053 rule
    pub fn new(ignored_definitions: Vec<String>) -> Self {
        Self {
            ignored_definitions: ignored_definitions
                .into_iter()
                .map(|s| s.to_lowercase())
                .collect(),
            content_cache: Rc::new(RefCell::new(HashMap::new())),
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
        let document_structure = DocumentStructure::new(content);
        document_structure.code_blocks.iter().map(|block| (block.start_line, block.end_line)).collect()
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

    /// Find all link and image reference definitions in the content.
    ///
    /// This method returns a HashMap where the key is the normalized reference ID and the value is a vector of (start_line, end_line) tuples.
    fn find_definitions(&self, content: &str) -> HashMap<String, Vec<(usize, usize)>> {
        let mut definitions: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        let lines: Vec<&str> = content.lines().collect();
        let code_blocks = self.find_code_blocks(content);
        let mut in_front_matter = false;
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            // Front matter detection (YAML)
            if i == 0 && line.trim_start().starts_with("---") {
                in_front_matter = true;
                i += 1;
                while i < lines.len() && !lines[i].trim_start().starts_with("---") {
                    i += 1;
                }
                i += 1;
                continue;
            }
            if in_front_matter {
                i += 1;
                continue;
            }
            // Skip code blocks
            if self.is_in_code_block(i, &code_blocks) {
                i += 1;
                continue;
            }
            // Reference definition
            if let Some(caps) = REFERENCE_DEFINITION_REGEX.captures(line) {
                let ref_id = caps.get(1).unwrap().as_str().trim();
                let mut start_line = i;
                let mut end_line = i;
                // Multi-line continuation
                i += 1;
                while i < lines.len() && CONTINUATION_REGEX.is_match(lines[i]) {
                    end_line = i;
                    i += 1;
                }
                // Normalize reference id: trim, unescape, and lowercase
                let normalized_id = Self::unescape_reference(ref_id).to_lowercase();
                definitions.entry(normalized_id).or_insert_with(Vec::new).push((start_line, end_line));
                continue;
            }
            i += 1;
        }
        definitions
    }

    /// Find all link and image reference usages in the content.
    ///
    /// This method returns a HashSet of all normalized reference IDs found in usage (not in code blocks, code spans, or front matter).
    fn find_usages(&self, content: &str) -> HashSet<String> {
        let lines: Vec<&str> = content.lines().collect();
        let code_blocks = self.find_code_blocks(content);
        let mut in_front_matter = false;
        let mut usages: HashSet<String> = HashSet::new();
        let mut in_code_span = false;

        // Helper function to recursively extract all reference usages from a string
        fn extract_references(s: &str, usages: &mut HashSet<String>) {
            // Find all reference usages in this string
            for caps in LINK_REFERENCE_REGEX.captures_iter(s).flatten() {
                let ref_id = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("");
                let normalized_id = MD053LinkImageReferenceDefinitions::unescape_reference(ref_id).to_lowercase();
                usages.insert(normalized_id.clone());
                // Recursively scan the link text for more references
                if let Some(link_text) = caps.get(1) {
                    extract_references(link_text.as_str(), usages);
                }
            }
            for caps in IMAGE_REFERENCE_REGEX.captures_iter(s).flatten() {
                let ref_id = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("");
                let normalized_id = MD053LinkImageReferenceDefinitions::unescape_reference(ref_id).to_lowercase();
                usages.insert(normalized_id.clone());
                // Recursively scan the alt text for more references
                if let Some(alt_text) = caps.get(1) {
                    extract_references(alt_text.as_str(), usages);
                }
            }
            for caps in SHORTCUT_REFERENCE_REGEX.captures_iter(s).flatten() {
                let ref_id = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                let normalized_id = MD053LinkImageReferenceDefinitions::unescape_reference(ref_id).to_lowercase();
                usages.insert(normalized_id);
            }
            for caps in EMPTY_LINK_REFERENCE_REGEX.captures_iter(s) {
                if let Some(cap) = caps.get(1) {
                    let normalized_id = MD053LinkImageReferenceDefinitions::unescape_reference(cap.as_str().trim()).to_lowercase();
                    usages.insert(normalized_id);
                }
            }
            for caps in EMPTY_IMAGE_REFERENCE_REGEX.captures_iter(s) {
                if let Some(cap) = caps.get(1) {
                    let normalized_id = MD053LinkImageReferenceDefinitions::unescape_reference(cap.as_str().trim()).to_lowercase();
                    usages.insert(normalized_id);
                }
            }
        }

        for (i, line) in lines.iter().enumerate() {
            // Front matter detection (YAML)
            if i == 0 && line.trim_start().starts_with("---") {
                in_front_matter = true;
                continue;
            }
            if in_front_matter {
                if line.trim_start().starts_with("---") {
                    in_front_matter = false;
                }
                continue;
            }
            // Skip code blocks
            if self.is_in_code_block(i, &code_blocks) {
                continue;
            }
            // Code span detection (inline backticks)
            let mut chars = line.chars().peekable();
            let mut backtick_count = 0;
            while let Some(c) = chars.next() {
                if c == '`' {
                    backtick_count += 1;
                    while let Some('`') = chars.peek() {
                        chars.next();
                        backtick_count += 1;
                    }
                    in_code_span = !in_code_span;
                }
            }
            if in_code_span {
                continue;
            }
            // Recursively extract all reference usages from this line
            extract_references(line, &mut usages);
        }
        // Recursively mark as used any references that are referenced by other used references (nested references in definitions)
        let definitions = self.find_definitions(content);
        let mut all_usages = usages.clone();
        let mut stack: Vec<String> = usages.iter().cloned().collect();
        while let Some(used_id) = stack.pop() {
            if let Some(ranges) = definitions.get(&used_id) {
                for (start, end) in ranges {
                    for line_idx in *start..=*end {
                        if let Some(line) = lines.get(line_idx) {
                            extract_references(line, &mut all_usages);
                            // If new usages are found, add them to the stack
                            for id in all_usages.iter() {
                                if !stack.contains(id) && !usages.contains(id) {
                                    stack.push(id.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        all_usages
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
        let usages = self.find_usages(content);
        let definitions = self.find_definitions(content);
        let mut unused = Vec::new();
        for (id, ranges) in definitions {
            // Ignore if in ignored_definitions
            if self.ignored_definitions.contains(&id) {
                continue;
            }
            // If this id is not used anywhere, all its ranges are unused
            if !usages.contains(&id) {
                for (start, end) in ranges {
                    unused.push((id.clone(), start, end));
                }
            }
        }
        unused
    }

    // Get cached definitions for the given content.
    ///
    /// This method uses a cache to store the definitions for each content hash.
    /// If the definitions for the given content are already cached, they are returned.
    /// Otherwise, the definitions are computed, cached, and then returned.
    fn get_cached_definitions(&self, content: &str) -> Vec<(String, usize, usize, usize)> {
        let hash = Self::content_hash(content);

        // First check if we already have this content cached
        let cache = self.content_cache.borrow();
        if let Some(cached) = cache.get(&hash) {
            return cached.clone();
        }

        // If not cached, release the borrow and compute the definitions
        drop(cache);

        // Compute the definitions
        let definitions: Vec<(String, usize, usize, usize)> = self
            .find_definitions(content)
            .into_iter()
            .flat_map(|(key, defs)| {
                defs.into_iter().map(move |def| (key.clone(), def.0, def.1, def.1))
            })
            .collect();

        // Update the cache with the computed definitions
        self.content_cache
            .borrow_mut()
            .insert(hash, definitions.clone());

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
        for (definition, start, end) in unused_refs {
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

        // Split the content into lines
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        // Create a set of line numbers to remove (unused references)
        let mut to_remove = std::collections::HashSet::with_capacity(unused_refs.len() * 2);
        for (_, start, end) in &unused_refs {
            for line in *start..=*end {
                to_remove.insert(line);
            }
        }

        // Build the result, skipping unused definitions
        let mut result = Vec::with_capacity(lines.len() - to_remove.len());
        for (i, line) in lines.into_iter().enumerate() {
            if !to_remove.contains(&i) {
                result.push(line);
            }
        }

        // Clean up formatting issues created by removals
        self.clean_up_document_structure(&mut result);

        Ok(result.join("\n"))
    }
}
