use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use fancy_regex::Regex as FancyRegex;
use std::collections::{HashMap, HashSet};
use lazy_static::lazy_static;

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
}

impl Default for MD053LinkImageReferenceDefinitions {
    fn default() -> Self {
        Self {
            ignored_definitions: Vec::new(),
        }
    }
}

impl MD053LinkImageReferenceDefinitions {
    /// Create a new instance of the MD053 rule
    pub fn new(ignored_definitions: Vec<String>) -> Self {
        Self {
            ignored_definitions,
        }
    }

    // Find all code blocks in the content to avoid processing references within them
    fn find_code_blocks(&self, content: &str) -> Vec<(usize, usize)> {
        let mut code_blocks = Vec::new();
        let mut in_code_block = false;
        let mut start_line = 0;
        
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
    
    // Find all reference usages in the content
    fn find_usages(&self, content: &str) -> HashSet<String> {
        let mut usages = HashSet::new();
        let code_blocks = self.find_code_blocks(content);
        
        // Process each line
        for (line_idx, line) in content.lines().enumerate() {
            // Skip if line is in a code block
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
    
    // Get unused references with their line ranges
    fn get_unused_references(&self, content: &str) -> Vec<(String, usize, usize)> {
        let usages = self.find_usages(content);
        let definitions = self.find_definitions(content);
        let code_blocks = self.find_code_blocks(content);
        
        // Create a set of ignored definitions (case-insensitive)
        let mut ignored: HashSet<String> = self.ignored_definitions
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        
        // Extract and add reference IDs from code blocks to ignored list
        let lines: Vec<&str> = content.lines().collect();
        for (start, end) in &code_blocks {
            // Find any reference-like patterns within code blocks
            for i in *start..=*end {
                if i < lines.len() {
                    // Look for [link][id] pattern in code blocks
                    let line = lines[i];
                    
                    // Add any reference IDs from code blocks to ignored list
                    for cap in LINK_REFERENCE_REGEX.captures_iter(line) {
                        if let Some(reference_match) = cap.get(2) {
                            let reference = reference_match.as_str().trim();
                            if !reference.is_empty() {
                                ignored.insert(reference.to_lowercase());
                            }
                        }
                    }
                }
            }
        }
        
        let mut result = Vec::new();
        
        // Find unused references
        for (reference, positions) in definitions {
            // Skip if this reference should be ignored
            if ignored.contains(&reference) {
                continue;
            }
            
            // Skip if this reference is used in the content
            if usages.contains(&reference) {
                continue;
            }
            
            // Skip references defined inside code blocks
            for (start, end) in positions {
                if !code_blocks.iter().any(|(block_start, block_end)| 
                    start >= *block_start && start <= *block_end) {
                    result.push((reference.clone(), start, end));
                }
            }
        }
        
        // Sort by start line (ascending)
        result.sort_by_key(|&(_, start, _)| start);
        
        result
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
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        let unused_refs = self.get_unused_references(content);
        
        let mut warnings = Vec::new();
        
        // Create warnings for unused references
        for (reference, start_line, _) in unused_refs {
            warnings.push(LintWarning {
                line: start_line + 1, // 1-indexed line numbers
                column: 1,
                message: format!("Unused reference definition: [{}]", reference),
                fix: Some(Fix {
                    line: start_line + 1,
                    column: 1,
                    replacement: String::new(),
                }),
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
        
        // Split the content into lines
        let lines: Vec<&str> = content.lines().collect();
        
        // Create a set of line numbers to remove
        let mut to_remove = HashSet::new();
        for (_, start, end) in &unused_refs {
            for line in *start..=*end {
                to_remove.insert(line);
            }
        }
        
        // Build the result line by line
        let mut result = Vec::with_capacity(lines.len());
        let mut i = 0;
        while i < lines.len() {
            if to_remove.contains(&i) {
                // Skip this line as it's part of an unused definition
                i += 1;
                continue;
            }
            
            // Add the line to the result
            result.push(lines[i].to_string());
            i += 1;
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