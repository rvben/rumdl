use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use crate::utils::fast_hash;

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

/// Rule MD053: Link and image reference definitions should be needed
///
/// This rule checks that all link and image reference definitions are used at least
/// once in the document.
pub struct MD053LinkImageReferenceDefinitions {
    ignored_definitions: HashSet<String>,
    cache: RefCell<HashMap<u64, Vec<(String, usize, usize)>>>,
}

impl Default for MD053LinkImageReferenceDefinitions {
    fn default() -> Self {
        Self {
            ignored_definitions: HashSet::new(),
            cache: RefCell::new(HashMap::new()),
        }
    }
}

impl MD053LinkImageReferenceDefinitions {
    /// Create a new instance of the MD053 rule
    pub fn new(ignored_definitions: Vec<String>) -> Self {
        let mut ignored_set = HashSet::new();
        for def in ignored_definitions {
            ignored_set.insert(def.to_lowercase());
        }

        Self {
            ignored_definitions: ignored_set,
            cache: RefCell::new(HashMap::new()),
        }
    }

    // Find all code blocks in the content to avoid processing references within them
    fn find_code_blocks(&self, content: &str) -> Vec<(usize, usize)> {
        let mut code_blocks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        let mut start_line = 0;

        for (i, line) in lines.iter().enumerate() {
            if CODE_BLOCK_START_REGEX.is_match(line) && !in_code_block {
                in_code_block = true;
                start_line = i;
            } else if CODE_BLOCK_END_REGEX.is_match(line) && in_code_block {
                code_blocks.push((start_line, i));
                in_code_block = false;
            }
        }

        // Handle unclosed code blocks
        if in_code_block {
            code_blocks.push((start_line, lines.len() - 1));
        }

        code_blocks
    }

    // Check if a line is inside a code block
    fn is_in_code_block(&self, line_idx: usize, code_blocks: &[(usize, usize)]) -> bool {
        for &(start, end) in code_blocks {
            if line_idx >= start && line_idx <= end {
                return true;
            }
        }
        false
    }

    // Check if a line range overlaps with any code block
    fn is_inside_code_block(&self, start: usize, end: usize, code_blocks: &[(usize, usize)]) -> bool {
        for &(block_start, block_end) in code_blocks {
            if start <= block_end && end >= block_start {
                return true;
            }
        }
        false
    }

    // Find all reference usages in the content, accounting for code blocks
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
                        let text = text_capture.as_str().trim();
                        if !text.is_empty() {
                            if is_in_code {
                                code_block_usages.insert(text.to_lowercase());
                            } else {
                                usages.insert(text.to_lowercase());
                            }
                        }
                    }
                } else {
                    if is_in_code {
                        code_block_usages.insert(ref_text.to_lowercase());
                    } else {
                        usages.insert(ref_text.to_lowercase());
                    }
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
                        let text = text_capture.as_str().trim();
                        if !text.is_empty() {
                            if is_in_code {
                                code_block_usages.insert(text.to_lowercase());
                            } else {
                                usages.insert(text.to_lowercase());
                            }
                        }
                    }
                } else {
                    if is_in_code {
                        code_block_usages.insert(ref_text.to_lowercase());
                    } else {
                        usages.insert(ref_text.to_lowercase());
                    }
                }
            }
        }

        // Process shortcut references using FancyRegex
        let matches = SHORTCUT_REFERENCE_REGEX.find_iter(content);
        for m_result in matches {
            if let Ok(m) = m_result {
                // Extract line number for code block check
                let line_idx = content[..m.start()].matches('\n').count();
                
                // Skip if this match is actually a definition
                let ref_text = &content[m.start()+1..m.end()-1].trim().to_lowercase();
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
    
    // Helper method to unescape backslashes in reference definitions
    fn unescape_reference(reference: &str) -> String {
        // Remove backslash escapes e.g., "foo\-bar" becomes "foo-bar"
        reference.replace("\\", "")
    }

    // Find all reference definitions in the content
    fn find_definitions(&self, content: &str) -> HashMap<String, Vec<(usize, usize)>> {
        let mut definitions = HashMap::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;
        
        while i < lines.len() {
            if let Some(caps) = REFERENCE_DEFINITION_REGEX.captures(lines[i]) {
                if let Some(ref_capture) = caps.get(1) {
                    let ref_text = ref_capture.as_str().trim();
                    let ref_key = ref_text.to_lowercase();
                    let unescaped_key = Self::unescape_reference(ref_text).to_lowercase();
                    
                    // Get the end of the definition, handling multi-line definitions
                    let mut end_line = i;
                    while end_line + 1 < lines.len() && CONTINUATION_REGEX.is_match(lines[end_line + 1]) {
                        end_line += 1;
                    }
                    
                    // Store both the original and unescaped versions of the reference key
                    definitions.entry(ref_key.clone()).or_insert_with(Vec::new).push((i, end_line));
                    
                    // If the unescaped key is different, store it as an alias
                    if unescaped_key != ref_key {
                        definitions.entry(unescaped_key).or_insert_with(Vec::new).push((i, end_line));
                    }
                }
            }
            i += 1;
        }
        
        definitions
    }

    // Get cached definitions for the given content.
    ///
    /// This method uses a cache to store the definitions for each content hash.
    /// If the definitions for the given content are already cached, they are returned.
    /// Otherwise, the definitions are computed, cached, and then returned.
    fn get_cached_definitions(&self, content: &str) -> Vec<(String, usize, usize)> {
        let hash = fast_hash(content);
        self.cache
            .borrow_mut()
            .entry(hash)
            .or_insert_with(|| {
                self.find_definitions(content)
                    .into_iter()
                    .flat_map(|(s, e_vec)| {
                        e_vec
                            .into_iter()
                            .map(move |(start, end)| (s.clone(), start, end))
                    })
                    .collect()
            })
            .clone()
    }

    /// Get unused references with their line ranges.
    ///
    /// This method uses the cached definitions to improve performance.
    fn get_unused_references(&self, content: &str) -> Vec<(String, usize, usize)> {
        let (usages, _) = self.find_usages(content);
        let cached_definitions = self.get_cached_definitions(content);
        let code_blocks = self.find_code_blocks(content);
        
        // Create a map to track which definition was used
        let mut used_definitions = HashMap::new();
        
        // Find which definitions are unused
        let unused = cached_definitions
            .into_iter()
            .filter(|(key, start, end)| {
                let original_key = key.clone();
                let unescaped_key = Self::unescape_reference(key).to_lowercase();
                
                // Check if the reference is used (either in its original or unescaped form)
                let is_used = usages.contains(key) || usages.contains(&unescaped_key);
                let is_ignored = self.ignored_definitions.contains(key) || 
                                self.ignored_definitions.contains(&unescaped_key);
                let is_in_code_block = self.is_inside_code_block(*start, *end, &code_blocks);
                
                // Track which definition was used to avoid duplication in results
                if is_used {
                    used_definitions.insert(original_key.clone(), true);
                }
                
                !is_used && !is_ignored && !is_in_code_block && !used_definitions.contains_key(&original_key)
            })
            .collect::<Vec<_>>();
        
        unused
    }

    /// Helper method to clean up document structure after removing lines
    fn clean_up_document_structure(&self, lines: &mut Vec<String>) {
        // Clean up consecutive empty lines
        let mut i = 1;
        while i < lines.len() {
            if lines[i].trim().is_empty() && lines[i-1].trim().is_empty() {
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
        "md053"
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
        let lines: Vec<&str> = content.lines().collect();

        // Create a set of line numbers to remove (unused references)
        let mut to_remove = std::collections::HashSet::new();
        for (_, start, end) in &unused_refs {
            for line in *start..=*end {
                to_remove.insert(line);
            }
        }

        // Build the result, skipping unused definitions
        let mut result = Vec::with_capacity(lines.len());
        for (i, line) in lines.iter().enumerate() {
            if !to_remove.contains(&i) {
                result.push((*line).to_string());
            }
        }

        // Clean up formatting issues created by removals
        self.clean_up_document_structure(&mut result);

        // Join the lines with newlines
        let output = if !result.is_empty() {
            result.join("\n")
        } else {
            "".to_string()
        };

        Ok(output)
    }
}
