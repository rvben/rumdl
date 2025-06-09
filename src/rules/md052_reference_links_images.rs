use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_match_range;
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::{HashMap, HashSet};

lazy_static! {
    // Pattern to match reference definitions [ref]: url (standard regex is fine)
    static ref REF_REGEX: Regex = Regex::new(r"^\s*\[([^\]]+)\]:\s*\S+").unwrap();

    // Pattern to match reference links and images ONLY: [text][reference] or ![text][reference]
    // These need lookbehind for escaped brackets
    static ref REF_LINK_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)\[([^\]]+)\]\[([^\]]*)\]").unwrap();
    static ref REF_IMAGE_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)!\[([^\]]+)\]\[([^\]]*)\]").unwrap();

    // Pattern for shortcut reference links [reference]
    static ref SHORTCUT_REF_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)\[([^\]]+)\](?!\s*[\[\(])").unwrap();

    // Pattern to match inline links and images (to exclude them)
    static ref INLINE_LINK_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)\[([^\]]+)\]\(([^)]+)\)").unwrap();
    static ref INLINE_IMAGE_REGEX: FancyRegex = FancyRegex::new(r"(?<!\\)!\[([^\]]+)\]\(([^)]+)\)").unwrap();

    // Pattern for list items to exclude from reference checks (standard regex is fine)
    static ref LIST_ITEM_REGEX: Regex = Regex::new(r"^\s*[-*+]\s+(?:\[[xX\s]\]\s+)?").unwrap();

    // Pattern for code blocks (standard regex is fine)
    static ref FENCED_CODE_START: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();

    // Pattern for output example sections (standard regex is fine)
    static ref OUTPUT_EXAMPLE_START: Regex = Regex::new(r"^#+\s*(?:Output|Example|Output Style|Output Format)\s*$").unwrap();
}

/// Rule MD052: Reference links and images should use reference style
///
/// See [docs/md052.md](../../docs/md052.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when a reference link or image uses a reference that isn't defined.
#[derive(Clone)]
pub struct MD052ReferenceLinkImages;

impl Default for MD052ReferenceLinkImages {
    fn default() -> Self {
        Self::new()
    }
}

impl MD052ReferenceLinkImages {
    pub fn new() -> Self {
        Self
    }


    fn extract_references(&self, content: &str) -> HashSet<String> {
        let mut references = HashSet::new();
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();

        for line in content.lines() {
            // Handle code block boundaries
            if let Some(cap) = FENCED_CODE_START.captures(line) {
                if let Some(marker) = cap.get(0) {
                    let marker_str = marker.as_str().to_string();
                    if !in_code_block {
                        in_code_block = true;
                        code_fence_marker = marker_str;
                    } else if line.trim().starts_with(&code_fence_marker) {
                        in_code_block = false;
                        code_fence_marker.clear();
                    }
                }
                continue;
            }

            // Skip lines in code blocks
            if in_code_block {
                continue;
            }

            if let Some(cap) = REF_REGEX.captures(line) {
                // Store references in lowercase for case-insensitive comparison
                if let Some(reference) = cap.get(1) {
                    references.insert(reference.as_str().to_lowercase());
                }
            }
        }

        references
    }

    fn find_undefined_references(
        &self,
        content: &str,
        references: &HashSet<String>,
        ctx: &crate::lint_context::LintContext,
    ) -> Vec<(usize, usize, usize, String)> {
        let mut undefined = Vec::new();
        let mut reported_refs = HashMap::new();
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();
        let mut in_example_section = false;
        
        // Use cached data for reference links and images
        for link in &ctx.links {
            if !link.is_reference {
                continue; // Skip inline links
            }
            
            if let Some(ref_id) = &link.reference_id {
                let reference_lower = ref_id.to_lowercase();
                
                // Check if reference is defined
                if !references.contains(&reference_lower) && !reported_refs.contains_key(&reference_lower) {
                    // Check if the line is in an example section or list item
                    if let Some(line_info) = ctx.line_info(link.line) {
                        if OUTPUT_EXAMPLE_START.is_match(&line_info.content) {
                            in_example_section = true;
                            continue;
                        }
                        
                        if in_example_section {
                            continue;
                        }
                        
                        // Skip list items
                        if LIST_ITEM_REGEX.is_match(&line_info.content) {
                            continue;
                        }
                    }
                    
                    let match_len = link.byte_end - link.byte_offset;
                    undefined.push((link.line - 1, link.start_col, match_len, ref_id.clone()));
                    reported_refs.insert(reference_lower, true);
                }
            }
        }
        
        // Use cached data for reference images
        for image in &ctx.images {
            if !image.is_reference {
                continue; // Skip inline images
            }
            
            if let Some(ref_id) = &image.reference_id {
                let reference_lower = ref_id.to_lowercase();
                
                // Check if reference is defined
                if !references.contains(&reference_lower) && !reported_refs.contains_key(&reference_lower) {
                    // Check if the line is in an example section or list item
                    if let Some(line_info) = ctx.line_info(image.line) {
                        if OUTPUT_EXAMPLE_START.is_match(&line_info.content) {
                            in_example_section = true;
                            continue;
                        }
                        
                        if in_example_section {
                            continue;
                        }
                        
                        // Skip list items
                        if LIST_ITEM_REGEX.is_match(&line_info.content) {
                            continue;
                        }
                    }
                    
                    let match_len = image.byte_end - image.byte_offset;
                    undefined.push((image.line - 1, image.start_col, match_len, ref_id.clone()));
                    reported_refs.insert(reference_lower, true);
                }
            }
        }
        
        // Handle shortcut references [text] which aren't captured in ctx.links
        // Need to use regex for these
        let lines: Vec<&str> = content.lines().collect();
        in_example_section = false; // Reset for line-by-line processing
        
        for (line_num, line) in lines.iter().enumerate() {
            // Handle code blocks
            if let Some(cap) = FENCED_CODE_START.captures(line) {
                if let Some(marker) = cap.get(0) {
                    let marker_str = marker.as_str().to_string();
                    if !in_code_block {
                        in_code_block = true;
                        code_fence_marker = marker_str;
                    } else if line.trim().starts_with(&code_fence_marker) {
                        in_code_block = false;
                        code_fence_marker.clear();
                    }
                }
                continue;
            }
            
            if in_code_block {
                continue;
            }
            
            // Check for example sections
            if OUTPUT_EXAMPLE_START.is_match(line) {
                in_example_section = true;
                continue;
            }
            
            if in_example_section {
                // Check if we're exiting the example section (another heading)
                if line.starts_with('#') && !OUTPUT_EXAMPLE_START.is_match(line) {
                    in_example_section = false;
                } else {
                    continue;
                }
            }
            
            // Skip list items
            if LIST_ITEM_REGEX.is_match(line) {
                continue;
            }
            
            // Check shortcut references: [reference]
            if let Ok(captures) = SHORTCUT_REF_REGEX.captures_iter(line).collect::<Result<Vec<_>, _>>() {
                for cap in captures {
                    if let Some(ref_match) = cap.get(1) {
                        let reference = ref_match.as_str();
                        let reference_lower = reference.to_lowercase();
                        
                        if !references.contains(&reference_lower) && !reported_refs.contains_key(&reference_lower) {
                            let full_match = cap.get(0).unwrap();
                            let col = full_match.start();
                            let match_len = full_match.end() - full_match.start();
                            undefined.push((line_num, col, match_len, reference.to_string()));
                            reported_refs.insert(reference_lower, true);
                        }
                    }
                }
            }
        }
        
        undefined
    }
}

impl Rule for MD052ReferenceLinkImages {
    fn name(&self) -> &'static str {
        "MD052"
    }

    fn description(&self) -> &'static str {
        "Reference links and images should use a reference that exists"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();
        let references = self.extract_references(content);

        // Use optimized detection method with cached link/image data
        for (line_num, col, match_len, reference) in
            self.find_undefined_references(content, &references, ctx)
        {
            let lines: Vec<&str> = content.lines().collect();
            let line_content = lines.get(line_num).unwrap_or(&"");

            // Calculate precise character range for the entire undefined reference
            let (start_line, start_col, end_line, end_col) =
                calculate_match_range(line_num + 1, line_content, col, match_len);

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!(
                    "Reference '{}' not found",
                    reference
                ),
                severity: Severity::Warning,
                fix: None,
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        // No automatic fix available for undefined references
        Ok(content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD052ReferenceLinkImages::new())
    }
}
