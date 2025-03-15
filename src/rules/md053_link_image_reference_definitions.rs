use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use regex::Regex;
use fancy_regex::Regex as FancyRegex;
use std::collections::{HashMap, HashSet};
use lazy_static::lazy_static;
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use crate::utils::range_utils::line_col_to_byte_range;

fn fast_hash<T: Hash + ?Sized>(t: &T) -> u64 {
    let mut s = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

lazy_static! {
    // Link reference format: [text][reference]
    static ref LINK_REFERENCE_REGEX: Regex = Regex::new(r"\[([^\]]*)\]\s*\[([^\]]*)\]").unwrap();
    
    // Image reference format: ![alt][reference]
    static ref IMAGE_REFERENCE_REGEX: Regex = Regex::new(r"!\[([^\]]*)\]\s*\[([^\]]*)\]").unwrap();
    
    // Shortcut reference format: [reference] that is not followed by [] or (: 
    // Using fancy-regex for negative lookahead
    static ref SHORTCUT_REFERENCE_REGEX: FancyRegex = FancyRegex::new(r"(?<!\!)\[([^\]]+)\](?!\s*[\[(])").unwrap();
    
    // Reference definition format: [reference]: URL
    static ref REFERENCE_DEFINITION_REGEX: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s+.*$").unwrap();
}

/// Rule MD053: Link and image reference definitions should be needed
///
/// This rule checks that all link and image reference definitions are used at least
/// once in the document.
#[derive(Clone, Debug)]
pub struct MD053LinkImageReferenceDefinitions {
    ignored_definitions: Vec<String>,
    cache: RefCell<HashMap<u64, Vec<(String, usize, usize)>>>,

}

impl Default for MD053LinkImageReferenceDefinitions {
    fn default() -> Self {
        Self {
            ignored_definitions: Vec::new(),
            cache: RefCell::new(HashMap::new()),
        }
    }
}

impl MD053LinkImageReferenceDefinitions {
    /// Create a new instance of the MD053 rule
    pub fn new(ignored_definitions: Vec<String>) -> Self {
        Self {
            ignored_definitions: ignored_definitions.into_iter()
                .map(|s| s.to_lowercase())
                .collect(),
            cache: RefCell::new(HashMap::new()),
        }
    }

    // Find all code blocks in the content to avoid processing references within them
    fn find_code_blocks(&self, content: &str) -> Vec<(usize, usize)> {
        let mut code_blocks = Vec::new();
        let mut in_code_block = false;
        let mut start_line = 0;
        
        // Process each line to detect code block delimiters
        for (i, line) in content.lines().enumerate() {
            if line.trim().starts_with("```") {
                if in_code_block {
                    // End of code block
                    code_blocks.push((start_line, i));
                    in_code_block = false;
                } else {
                    // Start of code block
                    start_line = i;
                    in_code_block = true;
                }
            }
        }
        
        // Handle unclosed code block
        if in_code_block {
            let line_count = content.lines().count();
            if line_count > 0 {
                code_blocks.push((start_line, line_count - 1));
            }
        }
        
        code_blocks
    }
    
    // Check if a line is inside a code block
    fn is_in_code_block(&self, line_idx: usize, code_blocks: &[(usize, usize)]) -> bool {
        code_blocks.iter().any(|(start, end)| line_idx >= *start && line_idx <= *end)
    }
    
    // Check if a line range overlaps with any code block
    fn is_inside_code_block(&self, start: usize, end: usize, code_blocks: &[(usize, usize)]) -> bool {
        code_blocks.iter().any(|(cb_start, cb_end)| 
            // Check if any part of the line range overlaps with a code block
            (start <= *cb_end && end >= *cb_start)
        )
    }
    
    // Find all reference usages in the content, accounting for code blocks
    fn find_usages(&self, content: &str) -> HashSet<String> {
        let mut usages = HashSet::new();
        let code_blocks = self.find_code_blocks(content);
        
        // Process each line - first pass for normal content
        for (line_idx, line) in content.lines().enumerate() {
            // Skip if line is in a code block - references in code blocks have special handling
            if self.is_in_code_block(line_idx, &code_blocks) {
                continue;
            }
            
            // Extract references from standard link format [text][reference]
            for cap in LINK_REFERENCE_REGEX.captures_iter(line) {
                if let Some(reference_match) = cap.get(2) {
                    let reference = reference_match.as_str().trim();
                    if !reference.is_empty() {
                        usages.insert(reference.to_lowercase());
                    } else if let Some(text_match) = cap.get(1) {
                        // Handle empty reference format [text][]
                        usages.insert(text_match.as_str().trim().to_lowercase());
                    }
                }
            }
            
            // Extract references from image format ![alt][reference]
            for cap in IMAGE_REFERENCE_REGEX.captures_iter(line) {
                if let Some(reference_match) = cap.get(2) {
                    let reference = reference_match.as_str().trim();
                    if !reference.is_empty() {
                        usages.insert(reference.to_lowercase());
                    } else if let Some(alt_match) = cap.get(1) {
                        // Handle empty reference format ![alt][]
                        usages.insert(alt_match.as_str().trim().to_lowercase());
                    }
                }
            }
            
            // Extract shortcut references [reference] using fancy-regex
            if let Ok(captures) = SHORTCUT_REFERENCE_REGEX.captures_iter(line).collect::<Result<Vec<_>, _>>() {
                for cap in captures {
                    if let Some(reference_match) = cap.get(1) {
                        let reference = reference_match.as_str().trim();
                        // Don't add if this is actually a reference definition
                        if !line.trim().starts_with(&format!("[{}]:", reference)) {
                            usages.insert(reference.to_lowercase());
                        }
                    }
                }
            }
        }
        
        // Special case: Find reference usages inside code blocks
        // For MD053, references that appear in code blocks should be considered "used"
        for (start, end) in &code_blocks {
            for i in *start..=*end {
                if let Some(line) = content.lines().nth(i) {
                    // Find all reference mentions in the code block
                    for cap in LINK_REFERENCE_REGEX.captures_iter(line) {
                        if let Some(reference_match) = cap.get(2) {
                            let reference = reference_match.as_str().trim();
                            if !reference.is_empty() {
                                usages.insert(reference.to_lowercase());
                            } else if let Some(text_match) = cap.get(1) {
                                usages.insert(text_match.as_str().trim().to_lowercase());
                            }
                        }
                    }
                    
                    for cap in IMAGE_REFERENCE_REGEX.captures_iter(line) {
                        if let Some(reference_match) = cap.get(2) {
                            let reference = reference_match.as_str().trim();
                            if !reference.is_empty() {
                                usages.insert(reference.to_lowercase());
                            } else if let Some(alt_match) = cap.get(1) {
                                usages.insert(alt_match.as_str().trim().to_lowercase());
                            }
                        }
                    }
                    
                    // Extract shortcut references [reference] 
                    if let Ok(captures) = SHORTCUT_REFERENCE_REGEX.captures_iter(line).collect::<Result<Vec<_>, _>>() {
                        for cap in captures {
                            if let Some(reference_match) = cap.get(1) {
                                let reference = reference_match.as_str().trim();
                                if !line.trim().starts_with(&format!("[{}]:", reference)) {
                                    usages.insert(reference.to_lowercase());
                                }
                            }
                        }
                    }
                }
            }
        }
        
        usages
    }
    
    // Find all reference definitions in the content
    fn find_definitions(&self, content: &str) -> HashMap<String, Vec<(usize, usize)>> {
        let mut definitions = HashMap::new();
        let code_blocks = self.find_code_blocks(content);
        let lines: Vec<&str> = content.lines().collect();
        
        let mut i = 0;
        while i < lines.len() {
            // Skip lines in code blocks
            if self.is_in_code_block(i, &code_blocks) {
                i += 1;
                continue;
            }
            
            if let Some(cap) = REFERENCE_DEFINITION_REGEX.captures(lines[i]) {
                if let Some(reference_match) = cap.get(1) {
                    let reference = reference_match.as_str().trim().to_lowercase();
                    
                    // Look for multi-line definitions
                    let mut end_line = i;
                    while end_line + 1 < lines.len() {
                        let next_line = lines[end_line + 1];
                        if next_line.trim().is_empty() || 
                           !(next_line.starts_with("  ") || next_line.starts_with('\t')) {
                            break;
                        }
                        end_line += 1;
                    }
                    
                    definitions
                        .entry(reference)
                        .or_insert_with(Vec::new)
                        .push((i, end_line));
                    
                    i = end_line + 1;
                    continue;
                }
            }
            
            i += 1;
        }
        
        definitions
    }
    
    fn get_cached_definitions(&self, content: &str) -> Vec<(String, usize, usize)> {
        let hash = fast_hash(content);
        self.cache.borrow_mut()
            .entry(hash)
            .or_insert_with(|| 
                self.find_definitions(content)
                    .into_iter()
                    .flat_map(|(s, e_vec)| 
                        e_vec.into_iter().map(move |(start, end)| 
                            (s.clone(), start, end)
                        )
                    )
                    .collect()
            ).clone()
    }
    
    // Get unused references with their line ranges
    fn get_unused_references(&self, content: &str) -> Vec<(String, usize, usize)> {
        let usages = self.find_usages(content);
        
        let all_definitions = self.get_cached_definitions(content);
        
        let code_blocks = self.find_code_blocks(content);
        
        // Filter out used references, ignored definitions, and references inside code blocks
        let unused_refs = all_definitions
            .into_iter()
            .filter(|(ref_name, start, end)| {
                let is_used = usages.contains(ref_name);
                let is_ignored = self.ignored_definitions.iter().any(|ignored| ignored.eq_ignore_ascii_case(ref_name));
                let is_in_code_block = self.is_inside_code_block(*start, *end, &code_blocks);
                
                !is_used && !is_ignored && !is_in_code_block
            })
            .collect();
        
        unused_refs
    }

    // Check if a line is inside a code block
    fn check(&self, content: &str) -> LintResult {
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        let unused_refs = self.get_unused_references(content);
        
        let mut warnings = Vec::new();
        
        // Create warnings for unused references
        for definition in unused_refs {
            let match_start = content.find(&format!("[{}]:", definition.0)).unwrap();
            let start_line = content[..match_start].lines().count();
            warnings.push(LintWarning {
                line: start_line + 1, // 1-indexed line numbers
                column: 1,
                message: format!("Unused link/image reference definition: [{}]", definition.0),
                severity: Severity::Warning,
                fix: None,
            });
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.is_empty() {
            return Ok(String::new());
        }
        
        let unused_refs = self.get_unused_references(content);
        if unused_refs.is_empty() {
            return Ok(content.to_string());
        }
        
        // Find all code blocks to avoid modifying content in them
        let code_blocks = self.find_code_blocks(content);
        
        // Find all used reference definitions to preserve them
        let usages = self.find_usages(content);
        let all_definitions = self.get_cached_definitions(content);
        
        // Find reference definition lines that need to be preserved (used references)
        let mut keep_lines = HashSet::new();
        for (ref_name, start, end) in &all_definitions {
            if usages.contains(ref_name) || 
               self.ignored_definitions.iter().any(|ignored| ignored.eq_ignore_ascii_case(ref_name)) ||
               self.is_inside_code_block(*start, *end, &code_blocks) {
                for line in *start..=*end {
                    keep_lines.insert(line);
                }
            }
        }
        
        // Split the content into lines
        let lines: Vec<&str> = content.lines().collect();
        
        // Create a set of line numbers to remove (unused references not in code blocks)
        let mut to_remove = HashSet::new();
        for (_, start, end) in &unused_refs {
            if !self.is_inside_code_block(*start, *end, &code_blocks) {
                for line in *start..=*end {
                    to_remove.insert(line);
                }
            }
        }
        
        // Build the result, preserving used references and content structure
        let mut result = Vec::with_capacity(lines.len());
        
        for (i, line) in lines.iter().enumerate() {
            if to_remove.contains(&i) && !keep_lines.contains(&i) {
                // Skip this line as it's part of an unused definition
                continue;
            }
            
            // Add the line to the result
            result.push((*line).to_string());
        }
        
        // Clean up consecutive empty lines
        let mut cleaned = Vec::with_capacity(result.len());
        let mut prev_empty = false;
        
        for line in result {
            let current_empty = line.trim().is_empty();
            
            if current_empty && prev_empty {
                // Skip consecutive empty lines
                continue;
            }
            
            cleaned.push(line);
            prev_empty = current_empty;
        }
        
        // Remove trailing empty lines
        while !cleaned.is_empty() && cleaned.last().unwrap().trim().is_empty() {
            cleaned.pop();
        }
        
        // Remove leading empty lines
        while !cleaned.is_empty() && cleaned.first().unwrap().trim().is_empty() {
            cleaned.remove(0);
        }
        
        // Join the lines with newlines
        Ok(cleaned.join("\n"))
    }
}

impl Rule for MD053LinkImageReferenceDefinitions {
    fn name(&self) -> &'static str {
        "MD053"
    }

    fn description(&self) -> &'static str {
        "Link and image reference definitions should be needed"
    }

    fn check(&self, content: &str) -> LintResult {
        let unused_refs = self.get_unused_references(content);
        let mut warnings = Vec::new();
        
        for (ref_name, start, _end) in unused_refs {
            // Line numbers are 1-indexed for warnings
            warnings.push(LintWarning {
                line: start + 1,
                column: 1,
                message: format!("Unused link reference definition: [{}]", ref_name),
                severity: Severity::Warning,
                fix: None,
            });
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.is_empty() {
            return Ok(String::new());
        }
        
        let unused_refs = self.get_unused_references(content);
        if unused_refs.is_empty() {
            return Ok(content.to_string());
        }
        
        // Find all code blocks to avoid modifying content in them
        let code_blocks = self.find_code_blocks(content);
        
        // Find all used reference definitions to preserve them
        let usages = self.find_usages(content);
        let all_definitions = self.get_cached_definitions(content);
        
        // Find reference definition lines that need to be preserved (used references)
        let mut keep_lines = HashSet::new();
        for (ref_name, start, end) in &all_definitions {
            if usages.contains(ref_name) || 
               self.ignored_definitions.iter().any(|ignored| ignored.eq_ignore_ascii_case(ref_name)) ||
               self.is_inside_code_block(*start, *end, &code_blocks) {
                for line in *start..=*end {
                    keep_lines.insert(line);
                }
            }
        }
        
        // Split the content into lines
        let lines: Vec<&str> = content.lines().collect();
        
        // Create a set of line numbers to remove (unused references not in code blocks)
        let mut to_remove = HashSet::new();
        for (_, start, end) in &unused_refs {
            if !self.is_inside_code_block(*start, *end, &code_blocks) {
                for line in *start..=*end {
                    to_remove.insert(line);
                }
            }
        }
        
        // Build the result, preserving used references and content structure
        let mut result = Vec::with_capacity(lines.len());
        
        for (i, line) in lines.iter().enumerate() {
            if to_remove.contains(&i) && !keep_lines.contains(&i) {
                // Skip this line as it's part of an unused definition
                continue;
            }
            
            // Add the line to the result
            result.push((*line).to_string());
        }
        
        // Clean up consecutive empty lines
        let mut cleaned = Vec::with_capacity(result.len());
        let mut prev_empty = false;
        
        for line in result {
            let current_empty = line.trim().is_empty();
            
            if current_empty && prev_empty {
                // Skip consecutive empty lines
                continue;
            }
            
            cleaned.push(line);
            prev_empty = current_empty;
        }
        
        // Remove trailing empty lines
        while !cleaned.is_empty() && cleaned.last().unwrap().trim().is_empty() {
            cleaned.pop();
        }
        
        // Remove leading empty lines
        while !cleaned.is_empty() && cleaned.first().unwrap().trim().is_empty() {
            cleaned.remove(0);
        }
        
        // Join the lines with newlines
        Ok(cleaned.join("\n"))
    }
}