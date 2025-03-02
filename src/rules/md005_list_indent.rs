use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct MD005ListIndent;

impl MD005ListIndent {
    fn get_list_marker_info(line: &str) -> Option<(usize, char, usize)> {
        let indentation = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();
        
        // Check for unordered list markers
        if let Some(c) = trimmed.chars().next() {
            if c == '*' || c == '-' || c == '+' {
                if trimmed.len() > 1 && trimmed.chars().nth(1).map_or(false, |c| c.is_whitespace()) {
                    return Some((indentation, c, 1)); // 1 char marker
                }
            }
        }
        
        // Check for ordered list markers
        let re = Regex::new(r"^\d+[.)]").unwrap();
        if re.is_match(trimmed) {
            let marker_match = re.find(trimmed).unwrap();
            let marker_char = trimmed.chars().nth(marker_match.end() - 1).unwrap();
            if trimmed.len() > marker_match.end() && 
               trimmed.chars().nth(marker_match.end()).map_or(false, |c| c.is_whitespace()) {
                return Some((indentation, marker_char, marker_match.end()));
            }
        }
        
        None
    }
    
    fn is_blank_line(line: &str) -> bool {
        line.trim().is_empty()
    }
    
    fn is_in_code_block(lines: &[&str], current_line: usize) -> bool {
        let mut in_code_block = false;
        
        for (i, line) in lines.iter().take(current_line + 1).enumerate() {
            if line.trim().starts_with("```") || line.trim().starts_with("~~~") {
                in_code_block = !in_code_block;
            }
            
            if i == current_line {
                return in_code_block;
            }
        }
        
        false
    }
    
    // Determine the expected indentation for a list item at a specific level
    fn get_expected_indent(level: usize) -> usize {
        if level == 1 {
            0 // Top level items should be at the start of the line
        } else {
            2 * (level - 1) // Nested items should be indented by 2 spaces per level
        }
    }
    
    // Determine if a line is a continuation of a list item
    fn is_list_continuation(prev_line: &str, current_line: &str) -> bool {
        // If the previous line is a list item and the current line has more indentation
        // but is not a list item itself, it's a continuation
        if let Some((prev_indent, _, _)) = Self::get_list_marker_info(prev_line) {
            let current_indent = current_line.len() - current_line.trim_start().len();
            return current_indent > prev_indent && Self::get_list_marker_info(current_line).is_none();
        }
        false
    }
}

impl Rule for MD005ListIndent {
    fn name(&self) -> &'static str {
        "MD005"
    }

    fn description(&self) -> &'static str {
        "List indentation should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let lines: Vec<&str> = content.lines().collect();
        let mut warnings = Vec::new();
        
        // Maps to store indentation by level for each list
        let mut current_list_id = 0;
        let mut in_list = false;
        
        // First pass: collect all list items and their indentation
        let mut list_items = Vec::new();
        for (line_num, line) in lines.iter().enumerate() {
            // Skip blank lines and code blocks
            if Self::is_blank_line(line) || Self::is_in_code_block(&lines, line_num) {
                continue;
            }
            
            // Check if this is a list item
            if let Some((indent, _marker, _)) = Self::get_list_marker_info(line) {
                // If the indent is 0, or this is the first list item, or much less indented
                // than the previous list item, consider it the start of a new list
                let is_new_list = !in_list || indent == 0 || 
                                 (list_items.last().map_or(false, |(_, prev_indent, _)| 
                                    prev_indent > &0 && indent < prev_indent / 2));
                
                if is_new_list {
                    current_list_id += 1;
                    in_list = true;
                }
                
                list_items.push((line_num, indent, current_list_id));
            } else {
                // Not a list item - check if it's a continuation or something else
                if list_items.is_empty() || !in_list {
                    continue;
                }
                
                let (prev_line_num, _, _) = list_items.last().unwrap();
                if !Self::is_list_continuation(lines[*prev_line_num], line) {
                    in_list = false;
                }
            }
        }
        
        // Second pass: determine levels for each list
        let mut list_level_map: HashMap<usize, HashMap<usize, usize>> = HashMap::new(); // list_id -> { indent -> level }
        let mut list_item_levels: Vec<(usize, usize, usize)> = Vec::new(); // (line_num, indent, level)
        
        for (line_num, indent, list_id) in &list_items {
            // Skip items in code blocks
            if Self::is_in_code_block(&lines, *line_num) {
                continue;
            }
            
            // Get or create the indent->level map for this list
            let level_map = list_level_map.entry(*list_id).or_insert_with(HashMap::new);
            
            // If it's the first item in this list, it's level 1
            if level_map.is_empty() {
                level_map.insert(*indent, 1);
                list_item_levels.push((*line_num, *indent, 1));
                continue;
            }
            
            // Find the deepest previous level with an indentation less than this item
            let mut level = 1; // Default to top level
            let mut parent_indent = 0;
            
            for (prev_indent, prev_level) in level_map.iter() {
                if prev_indent < indent && (*prev_level >= level || *prev_indent > parent_indent) {
                    level = *prev_level + 1;
                    parent_indent = *prev_indent;
                } else if prev_indent == indent {
                    // Same indentation means same level
                    level = *prev_level;
                    break;
                }
            }
            
            level_map.insert(*indent, level);
            list_item_levels.push((*line_num, *indent, level));
        }
        
        // Third pass: check if indentation matches the expected level for each item
        for (line_num, indent, _list_id) in &list_items {
            // Skip items in code blocks
            if Self::is_in_code_block(&lines, *line_num) {
                continue;
            }
            
            // Find level for this item
            let level = list_item_levels.iter()
                .find(|(ln, _, _)| ln == line_num)
                .map(|(_, _, lvl)| *lvl)
                .unwrap_or(1);
            
            let expected_indent = Self::get_expected_indent(level);
            
            if *indent != expected_indent {
                let inconsistent_message = format!(
                    "List item indentation is {} {}, expected {} for level {}",
                    indent,
                    if *indent == 1 { "space" } else { "spaces" },
                    expected_indent,
                    level
                );
                
                // Create a fixed version of the line with proper indentation
                let line = lines[*line_num];
                let trimmed = line.trim_start();
                let replacement = format!("{}{}", " ".repeat(expected_indent), trimmed);
                
                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: 1,
                    message: inconsistent_message,
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: 1,
                        replacement,
                    }),
                });
            }
        }
        
        // Check for consistency among items in the same level of the same list
        // This ensures that list items at the same level have consistent indentation
        let mut level_groups: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new(); // (list_id, level) -> [(line_num, indent)]
        
        // Group list items by list_id and level
        for (line_num, indent, level) in &list_item_levels {
            let list_id = list_items.iter()
                .find(|(ln, _, _)| ln == line_num)
                .map(|(_, _, id)| *id)
                .unwrap_or(1);
                
            level_groups.entry((list_id, *level))
                .or_insert_with(Vec::new)
                .push((*line_num, *indent));
        }
        
        // For each group, check for indentation consistency
        for ((_list_id, _level), items) in &level_groups {
            if items.len() <= 1 {
                continue; // Need at least 2 items to check consistency
            }
            
            // Get first item's indentation as the reference
            let reference_indent = items[0].1;
            
            // Check that all other items match this indentation
            for &(line_num, indent) in &items[1..] {
                if indent != reference_indent {
                    // Found inconsistent indentation
                    let inconsistent_message = format!(
                        "List item indentation is inconsistent with other items at the same level (found: {}, expected: {})",
                        indent, reference_indent
                    );
                    
                    // Create a fixed version
                    let line = lines[line_num];
                    let trimmed = line.trim_start();
                    let replacement = format!("{}{}", " ".repeat(reference_indent), trimmed);
                    
                    // Only add if we don't already have a warning for this line
                    if !warnings.iter().any(|w| w.line == line_num + 1) {
                        warnings.push(LintWarning {
                            line: line_num + 1,
                            column: 1,
                            message: inconsistent_message,
                            fix: Some(Fix {
                                line: line_num + 1,
                                column: 1,
                                replacement,
                            }),
                        });
                    }
                }
            }
        }
        
        // Check for nested items with insufficient indentation
        for (line_num, indent, level) in &list_item_levels {
            if *level <= 1 {
                continue; // Not nested
            }
            
            // Find parent level items
            let list_id = list_items.iter()
                .find(|(ln, _, _)| ln == line_num)
                .map(|(_, _, id)| *id)
                .unwrap_or(1);
                
            // Get items at the parent level
            let parent_items = level_groups.get(&(list_id, level - 1));
            
            if let Some(parent_items) = parent_items {
                if parent_items.is_empty() {
                    continue;
                }
                
                // Find parent indentation
                let parent_indent = parent_items[0].1;
                
                // Child should be more indented
                if *indent <= parent_indent {
                    let message = format!(
                        "Nested list item should be more indented than parent (parent: {}, child: {})",
                        parent_indent, indent
                    );
                    
                    // Create fix
                    let line = lines[*line_num];
                    let trimmed = line.trim_start();
                    let expected_indent = parent_indent + 2;
                    let replacement = format!("{}{}", " ".repeat(expected_indent), trimmed);
                    
                    // Only add if we don't already have a warning
                    if !warnings.iter().any(|w| w.line == line_num + 1) {
                        warnings.push(LintWarning {
                            line: line_num + 1,
                            column: 1,
                            message,
                            fix: Some(Fix {
                                line: line_num + 1,
                                column: 1,
                                replacement,
                            }),
                        });
                    }
                }
            }
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Get warnings from the check method
        let warnings = self.check(content)?;
        
        if warnings.is_empty() {
            return Ok(content.to_string());
        }
        
        // Create a map of line numbers to fixes
        let mut fix_map: HashMap<usize, &Fix> = HashMap::new();
        for warning in &warnings {
            if let Some(fix) = &warning.fix {
                fix_map.insert(fix.line, fix);
            }
        }
        
        // Apply fixes line by line
        let mut fixed_content = String::new();
        let lines: Vec<&str> = content.lines().collect();
        
        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            
            if let Some(fix) = fix_map.get(&line_num) {
                fixed_content.push_str(&fix.replacement);
            } else {
                fixed_content.push_str(line);
            }
            
            // Preserve trailing newlines
            if i < lines.len() - 1 || content.ends_with('\n') {
                fixed_content.push('\n');
            }
        }
        
        Ok(fixed_content)
    }
} 