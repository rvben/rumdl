use crate::utils::range_utils::LineIndex;
use crate::utils::fast_hash;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

lazy_static! {
    static ref CODE_BLOCK_FENCE: Regex = Regex::new(r"^```").unwrap();
    static ref INDENTED_CODE_BLOCK: Regex = Regex::new(r"^    ").unwrap();
}

type WarningPosition = (usize, usize, String); // (line, column, found_name)

/// Rule MD044: Proper names should have the correct capitalization
///
/// This rule is triggered when proper names are not capitalized correctly.
#[derive(Clone)]
pub struct MD044ProperNames {
    names: HashSet<String>,
    code_blocks_excluded: bool,
    // Cache for compiled regexes
    regex_cache: RefCell<HashMap<String, Regex>>,
    // Cache for content hash to warnings
    content_cache: RefCell<HashMap<u64, Vec<WarningPosition>>>,
}

impl MD044ProperNames {
    pub fn new(names: Vec<String>, code_blocks_excluded: bool) -> Self {
        Self {
            names: names.into_iter().collect(),
            code_blocks_excluded,
            regex_cache: RefCell::new(HashMap::new()),
            content_cache: RefCell::new(HashMap::new()),
        }
    }

    // Helper method for checking code blocks
    fn is_code_block(&self, line: &str, in_code_block: bool) -> bool {
        in_code_block || INDENTED_CODE_BLOCK.is_match(line)
    }

    // Create a regex-safe version of the name for word boundary matches
    fn create_safe_pattern(&self, name: &str) -> String {
        // Create variations of the name with and without dots
        let variations = vec![name.to_lowercase(), name.to_lowercase().replace(".", "")];

        // Create a pattern that matches any of the variations with word boundaries
        let pattern = variations
            .iter()
            .map(|v| regex::escape(v))
            .collect::<Vec<_>>()
            .join("|");

        format!(r"(?i)\b({})\b", pattern)
    }

    // Get compiled regex from cache or compile it
    fn get_compiled_regex(&self, name: &str) -> Regex {
        let pattern = self.create_safe_pattern(name);
        let mut cache = self.regex_cache.borrow_mut();
        
        if let Some(regex) = cache.get(&pattern) {
            regex.clone()
        } else {
            let regex = Regex::new(&pattern).unwrap();
            cache.insert(pattern, regex.clone());
            regex
        }
    }

    // Find all name violations in the content and return positions
    fn find_name_violations(&self, content: &str) -> Vec<WarningPosition> {
        // Check if we have cached results
        let hash = fast_hash(content);
        let cache_result = self.content_cache.borrow().get(&hash).cloned();
        
        if let Some(cached) = cache_result {
            return cached;
        }

        let mut violations = Vec::new();
        let mut in_code_block = false;

        for (line_num, line) in content.lines().enumerate() {
            // Handle code blocks
            if CODE_BLOCK_FENCE.is_match(line.trim_start()) {
                in_code_block = !in_code_block;
                continue;
            }

            if self.code_blocks_excluded && self.is_code_block(line, in_code_block) {
                continue;
            }

            for name in &self.names {
                let regex = self.get_compiled_regex(name);

                for cap in regex.find_iter(line) {
                    let found_name = &line[cap.start()..cap.end()];
                    if found_name != name {
                        violations.push((
                            line_num + 1,
                            cap.start() + 1,
                            found_name.to_string(),
                        ));
                    }
                }
            }
        }

        // Store in cache
        self.content_cache.borrow_mut().insert(hash, violations.clone());
        violations
    }

    // Get the proper name that should be used for a found name
    fn get_proper_name_for(&self, found_name: &str) -> Option<String> {
        for name in &self.names {
            let regex = self.get_compiled_regex(name);
            if regex.is_match(found_name) {
                return Some(name.clone());
            }
        }
        None
    }
}

impl Rule for MD044ProperNames {
    fn name(&self) -> &'static str {
        "MD044"
    }

    fn description(&self) -> &'static str {
        "Proper names should have the correct capitalization"
    }

    fn check(&self, content: &str) -> LintResult {
        if content.is_empty() || self.names.is_empty() {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let violations = self.find_name_violations(content);
        
        let warnings = violations
            .into_iter()
            .filter_map(|(line, column, found_name)| {
                if let Some(proper_name) = self.get_proper_name_for(&found_name) {
                    Some(LintWarning {
                        line,
                        column,
                        message: format!("Proper name '{}' should be '{}'", found_name, proper_name),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line, column),
                            replacement: proper_name,
                        }),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.is_empty() || self.names.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut new_lines = Vec::with_capacity(lines.len());
        let mut in_code_block = false;

        for line in lines {
            let mut current_line = line.to_string();

            // Handle code blocks
            if CODE_BLOCK_FENCE.is_match(line.trim_start()) {
                in_code_block = !in_code_block;
                new_lines.push(current_line);
                continue;
            }

            if self.code_blocks_excluded && self.is_code_block(line, in_code_block) {
                new_lines.push(current_line);
                continue;
            }

            // Apply all name replacements to this line
            for name in &self.names {
                let regex = self.get_compiled_regex(name);
                current_line = regex.replace_all(&current_line, name.as_str()).to_string();
            }

            new_lines.push(current_line);
        }

        Ok(new_lines.join("\n"))
    }
}
