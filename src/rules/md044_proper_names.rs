use crate::utils::fast_hash;
use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use fancy_regex::Regex;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

lazy_static! {
    static ref CODE_BLOCK_FENCE: regex::Regex = regex::Regex::new(r"^```").unwrap();
    static ref INDENTED_CODE_BLOCK: regex::Regex = regex::Regex::new(r"^    ").unwrap();
}

type WarningPosition = (usize, usize, String); // (line, column, found_name)

/// Rule MD044: Proper names should have the correct capitalization
///
/// This rule is triggered when proper names are not capitalized correctly in the document.
/// For example, if you have defined "JavaScript" as a proper name, the rule will flag any
/// occurrences of "javascript" or "Javascript" as violations.
///
/// ## Purpose
///
/// Ensuring consistent capitalization of proper names improves document quality and
/// professionalism. This is especially important for technical documentation where
/// product names, programming languages, and technologies often have specific
/// capitalization conventions.
///
/// ## Configuration Options
///
/// The rule supports the following configuration options:
///
/// ```yaml
/// MD044:
///   names: []                # List of proper names to check for correct capitalization
///   code_blocks_excluded: true  # Whether to exclude code blocks from checking
/// ```
///
/// Example configuration:
///
/// ```yaml
/// MD044:
///   names: ["JavaScript", "Node.js", "TypeScript"]
///   code_blocks_excluded: true
/// ```
///
/// ## Performance Optimizations
///
/// This rule implements several performance optimizations:
///
/// 1. **Regex Caching**: Pre-compiles and caches regex patterns for each proper name
/// 2. **Content Caching**: Caches results based on content hashing for repeated checks
/// 3. **Efficient Text Processing**: Uses optimized algorithms to avoid redundant text processing
/// 4. **Smart Code Block Detection**: Efficiently identifies and optionally excludes code blocks
///
/// ## Edge Cases Handled
///
/// - **Word Boundaries**: Only matches complete words, not substrings within other words
/// - **Case Sensitivity**: Properly handles case-specific matching
/// - **Code Blocks**: Optionally excludes code blocks where capitalization may be intentionally different
/// - **Markdown Formatting**: Handles proper names within Markdown formatting elements
///
/// ## Fix Behavior
///
/// When fixing issues, this rule replaces incorrect capitalization with the correct form
/// as defined in the configuration.
///
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
        // Create variations for case-insensitive matching and handling dots
        let lower_name = name.to_lowercase();
        let lower_name_no_dots = lower_name.replace('.', "");

        // Build the pattern using lookarounds that explicitly exclude underscore
        // to avoid issues with emphasis characters if \w includes _.
        // (?<![a-zA-Z0-9]) - Negative lookbehind for an ASCII alphanumeric character
        // (?i) - Case-insensitive flag
        // (?:...|...) - Non-capturing group for variations
        // (?![a-zA-Z0-9]) - Negative lookahead for an ASCII alphanumeric character
        format!(
            r"(?<![a-zA-Z0-9])(?i)(?:{}|{})(?![a-zA-Z0-9])",
            fancy_regex::escape(&lower_name),
            fancy_regex::escape(&lower_name_no_dots)
        )
    }

    // Get compiled regex from cache or compile it
    fn get_compiled_regex(&self, name: &str) -> Regex {
        let pattern = self.create_safe_pattern(name);
        let mut cache = self.regex_cache.borrow_mut();

        // Use entry API for cleaner cache logic
        cache.entry(pattern.clone()).or_insert_with(|| {
            Regex::new(&pattern).unwrap_or_else(|e| {
                // Provide more context on regex compilation failure
                panic!(
                    "Failed to compile regex pattern '{}' for name '{}': {}",
                    pattern,
                    name,
                    e
                )
            })
        }).clone()
    }

    // Find all name violations in the content and return positions
    fn find_name_violations(&self, content: &str) -> Vec<WarningPosition> {
        // Check if we have cached results
        let hash = fast_hash(content);
        {
            // Use a separate scope for borrowing to minimize lock time
            let cache = self.content_cache.borrow();
            if let Some(cached) = cache.get(&hash) {
                return cached.clone();
            }
        }

        let mut violations = Vec::new();
        let mut in_code_block = false;

        // Pre-compile and prepare regex patterns before the line loop
        let patterns: Vec<(&String, Regex)> = self
            .names
            .iter()
            .map(|name| (name, self.get_compiled_regex(name)))
            .collect();

        for (line_num, line) in content.lines().enumerate() {
            // Handle code blocks (using standard regex for simple fence matching)
            if CODE_BLOCK_FENCE.is_match(line.trim_start()) {
                in_code_block = !in_code_block;
                continue;
            }

            if self.code_blocks_excluded && self.is_code_block(line, in_code_block) {
                continue;
            }

            // Use the pre-compiled fancy_regex patterns
            for (name, regex) in &patterns {
                // Use find_iter from fancy_regex, which yields Result<Match, Error>
                for cap_result in regex.find_iter(line) {
                    match cap_result {
                        Ok(cap) => {
                            let found_name = &line[cap.start()..cap.end()];
                            // Ensure the found name isn't the correct one (case-sensitive compare)
                            if found_name != **name {
                                violations.push((
                                    line_num + 1,
                                    cap.start() + 1,
                                    found_name.to_string(),
                                ));
                            }
                        }
                        Err(e) => {
                            // Log or handle regex execution error if necessary
                            eprintln!(
                                "Regex execution error on line {} for pattern matching '{}': {}",
                                line_num + 1,
                                name,
                                e
                            );
                        }
                    }
                }
            }
        }

        // Store in cache
        self.content_cache
            .borrow_mut()
            .insert(hash, violations.clone());
        violations
    }

    // Get the proper name that should be used for a found name
    fn get_proper_name_for(&self, found_name: &str) -> Option<String> {
        // Iterate through the configured proper names
        for name in &self.names {
            // Perform a case-insensitive comparison between the found name
            // and the configured proper name (and its dotless variation).
            let lower_name = name.to_lowercase();
            let lower_name_no_dots = lower_name.replace('.', "");
            let found_lower = found_name.to_lowercase();

            if found_lower == lower_name || found_lower == lower_name_no_dots {
                // If they match case-insensitively, return the correctly capitalized name
                return Some(name.clone());
            }
        }
        // If no match is found after checking all configured names, return None
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
                self.get_proper_name_for(&found_name)
                    .map(|proper_name| LintWarning {
                        rule_name: Some(self.name()),
                        line,
                        column,
                        message: format!(
                            "Proper name '{}' should be '{}'",
                            found_name, proper_name
                        ),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line, column),
                            replacement: proper_name,
                        }),
                    })
            })
            .collect();

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.is_empty() || self.names.is_empty() {
            return Ok(content.to_string());
        }

        let mut violations = self.find_name_violations(content);
        if violations.is_empty() {
            return Ok(content.to_string());
        }

        // Sort violations in reverse order (by line, then by column) to apply fixes
        // from end to beginning, avoiding range invalidation.
        violations.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

        let mut fixed_content = content.to_string();
        let line_index = LineIndex::new(content.to_string()); // Recreate for accurate byte ranges

        for (line_num, col_num, found_name) in violations {
            if let Some(proper_name) = self.get_proper_name_for(&found_name) {
                // Calculate the byte range for the violation
                let range = line_index.line_col_to_byte_range(line_num, col_num);
                let start_byte = range.start;
                let end_byte = start_byte + found_name.len();

                // Ensure the calculated range is valid within the current fixed_content
                if end_byte <= fixed_content.len()
                    && fixed_content.is_char_boundary(start_byte)
                    && fixed_content.is_char_boundary(end_byte)
                {
                    // Perform the replacement directly on the string using byte offsets
                    fixed_content.replace_range(start_byte..end_byte, &proper_name);
                } else {
                    // Log error or handle invalid range - potentially due to overlapping fixes or calculation errors
                    eprintln!(
                        "Warning: Skipping fix for '{}' at {}:{} due to invalid byte range [{}..{}], content length {}.",
                        found_name, line_num, col_num, start_byte, end_byte, fixed_content.len()
                    );
                }
            }
        }

        Ok(fixed_content)
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}
