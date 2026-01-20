use crate::utils::fast_hash;
use crate::utils::regex_cache::{escape_regex, get_cached_fancy_regex};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use fancy_regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};

mod md044_config;
use md044_config::MD044Config;

static HTML_COMMENT_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<!--([\s\S]*?)-->").unwrap());
// Reference definition pattern - matches [ref]: url "title"
static REF_DEF_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?m)^[ ]{0,3}\[([^\]]+)\]:\s*([^\s]+)(?:\s+(?:"([^"]*)"|'([^']*)'))?$"#).unwrap()
});

type WarningPosition = (usize, usize, String); // (line, column, found_name)

/// Rule MD044: Proper names should be capitalized
///
/// See [docs/md044.md](../../docs/md044.md) for full documentation, configuration, and examples.
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
///   code-blocks: false       # Whether to check code blocks (default: false)
/// ```
///
/// Example configuration:
///
/// ```yaml
/// MD044:
///   names: ["JavaScript", "Node.js", "TypeScript"]
///   code-blocks: true
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
/// - **Code Blocks**: Optionally checks code blocks (controlled by code-blocks setting)
/// - **Markdown Formatting**: Handles proper names within Markdown formatting elements
///
/// ## Fix Behavior
///
/// When fixing issues, this rule replaces incorrect capitalization with the correct form
/// as defined in the configuration.
///
#[derive(Clone)]
pub struct MD044ProperNames {
    config: MD044Config,
    // Cache the combined regex pattern string
    combined_pattern: Option<String>,
    // Precomputed lowercase name variants for fast pre-checks
    name_variants: Vec<String>,
    // Cache for name violations by content hash
    content_cache: Arc<Mutex<HashMap<u64, Vec<WarningPosition>>>>,
}

impl MD044ProperNames {
    pub fn new(names: Vec<String>, code_blocks: bool) -> Self {
        let config = MD044Config {
            names,
            code_blocks,
            html_elements: true, // Default to checking HTML elements
            html_comments: true, // Default to checking HTML comments
        };
        let combined_pattern = Self::create_combined_pattern(&config);
        let name_variants = Self::build_name_variants(&config);
        Self {
            config,
            combined_pattern,
            name_variants,
            content_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Helper function for consistent ASCII normalization
    fn ascii_normalize(s: &str) -> String {
        s.replace(['é', 'è', 'ê', 'ë'], "e")
            .replace(['à', 'á', 'â', 'ä', 'ã', 'å'], "a")
            .replace(['ï', 'î', 'í', 'ì'], "i")
            .replace(['ü', 'ú', 'ù', 'û'], "u")
            .replace(['ö', 'ó', 'ò', 'ô', 'õ'], "o")
            .replace('ñ', "n")
            .replace('ç', "c")
    }

    pub fn from_config_struct(config: MD044Config) -> Self {
        let combined_pattern = Self::create_combined_pattern(&config);
        let name_variants = Self::build_name_variants(&config);
        Self {
            config,
            combined_pattern,
            name_variants,
            content_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Create a combined regex pattern for all proper names
    fn create_combined_pattern(config: &MD044Config) -> Option<String> {
        if config.names.is_empty() {
            return None;
        }

        // Create patterns for all names and their variations
        let mut patterns: Vec<String> = config
            .names
            .iter()
            .flat_map(|name| {
                let mut variations = vec![];
                let lower_name = name.to_lowercase();

                // Add the lowercase version
                variations.push(escape_regex(&lower_name));

                // Add version without dots
                let lower_name_no_dots = lower_name.replace('.', "");
                if lower_name != lower_name_no_dots {
                    variations.push(escape_regex(&lower_name_no_dots));
                }

                // Add ASCII-normalized versions for common accented characters
                let ascii_normalized = Self::ascii_normalize(&lower_name);

                if ascii_normalized != lower_name {
                    variations.push(escape_regex(&ascii_normalized));

                    // Also add version without dots
                    let ascii_no_dots = ascii_normalized.replace('.', "");
                    if ascii_normalized != ascii_no_dots {
                        variations.push(escape_regex(&ascii_no_dots));
                    }
                }

                variations
            })
            .collect();

        // Sort patterns by length (longest first) to avoid shorter patterns matching within longer ones
        patterns.sort_by_key(|b| std::cmp::Reverse(b.len()));

        // Combine all patterns into a single regex with capture groups
        // Don't use \b as it doesn't work with Unicode - we'll check boundaries manually
        Some(format!(r"(?i)({})", patterns.join("|")))
    }

    fn build_name_variants(config: &MD044Config) -> Vec<String> {
        let mut variants = HashSet::new();
        for name in &config.names {
            let lower_name = name.to_lowercase();
            variants.insert(lower_name.clone());

            let lower_no_dots = lower_name.replace('.', "");
            if lower_name != lower_no_dots {
                variants.insert(lower_no_dots);
            }

            let ascii_normalized = Self::ascii_normalize(&lower_name);
            if ascii_normalized != lower_name {
                variants.insert(ascii_normalized.clone());

                let ascii_no_dots = ascii_normalized.replace('.', "");
                if ascii_normalized != ascii_no_dots {
                    variants.insert(ascii_no_dots);
                }
            }
        }

        variants.into_iter().collect()
    }

    // Find all name violations in the content and return positions
    fn find_name_violations(&self, content: &str, ctx: &crate::lint_context::LintContext) -> Vec<WarningPosition> {
        // Early return: if no names configured or content is empty
        if self.config.names.is_empty() || content.is_empty() || self.combined_pattern.is_none() {
            return Vec::new();
        }

        // Early return: quick check if any of the configured names might be in content
        let content_lower = if content.is_ascii() {
            content.to_ascii_lowercase()
        } else {
            content.to_lowercase()
        };
        let has_potential_matches = self.name_variants.iter().any(|name| content_lower.contains(name));

        if !has_potential_matches {
            return Vec::new();
        }

        // Check if we have cached results
        let hash = fast_hash(content);
        {
            // Use a separate scope for borrowing to minimize lock time
            if let Ok(cache) = self.content_cache.lock()
                && let Some(cached) = cache.get(&hash)
            {
                return cached.clone();
            }
        }

        let mut violations = Vec::new();

        // Get the regex from global cache
        let combined_regex = match &self.combined_pattern {
            Some(pattern) => match get_cached_fancy_regex(pattern) {
                Ok(regex) => regex,
                Err(_) => return Vec::new(),
            },
            None => return Vec::new(),
        };

        // Use ctx.lines for better performance
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            let line_num = line_idx + 1;
            let line = line_info.content(ctx.content);

            // Skip code fence lines (```language or ~~~language)
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                continue;
            }

            // Skip if in code block (when code_blocks = false)
            if !self.config.code_blocks && line_info.in_code_block {
                continue;
            }

            // Skip if in HTML block (when html_elements = false)
            if !self.config.html_elements && line_info.in_html_block {
                continue;
            }

            // Check if we should skip HTML comments
            let in_html_comment = if !self.config.html_comments {
                // Check if this position is within an HTML comment
                self.is_in_html_comment(content, line_info.byte_offset)
            } else {
                false
            };

            if in_html_comment {
                continue;
            }

            // Skip JSX expressions and MDX comments (MDX flavor)
            if line_info.in_jsx_expression || line_info.in_mdx_comment {
                continue;
            }

            // Early return: skip lines that don't contain any potential matches
            let line_lower = line.to_lowercase();
            let has_line_matches = self.name_variants.iter().any(|name| line_lower.contains(name));

            if !has_line_matches {
                continue;
            }

            // Use the combined regex to find all matches in one pass
            for cap_result in combined_regex.find_iter(line) {
                match cap_result {
                    Ok(cap) => {
                        let found_name = &line[cap.start()..cap.end()];

                        // Check word boundaries manually for Unicode support
                        let start_pos = cap.start();
                        let end_pos = cap.end();

                        if !self.is_at_word_boundary(line, start_pos, true)
                            || !self.is_at_word_boundary(line, end_pos, false)
                        {
                            continue; // Not at word boundary
                        }

                        // Skip if in inline code when code_blocks is false
                        if !self.config.code_blocks {
                            let byte_pos = line_info.byte_offset + cap.start();
                            if ctx.is_in_code_block_or_span(byte_pos) {
                                continue;
                            }
                        }

                        // Skip if in link (inline links, reference links, or reference definitions)
                        let byte_pos = line_info.byte_offset + cap.start();
                        if self.is_in_link(ctx, byte_pos) {
                            continue;
                        }

                        // Find which proper name this matches
                        if let Some(proper_name) = self.get_proper_name_for(found_name) {
                            // Only flag if it's not already correct
                            if found_name != proper_name {
                                violations.push((line_num, cap.start() + 1, found_name.to_string()));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Regex execution error on line {line_num}: {e}");
                    }
                }
            }
        }

        // Store in cache (ignore if mutex is poisoned)
        if let Ok(mut cache) = self.content_cache.lock() {
            cache.insert(hash, violations.clone());
        }
        violations
    }

    // Check if a byte position is within an HTML comment
    fn is_in_html_comment(&self, content: &str, byte_pos: usize) -> bool {
        for m in HTML_COMMENT_REGEX.find_iter(content).flatten() {
            if m.start() <= byte_pos && byte_pos < m.end() {
                return true;
            }
        }
        false
    }

    /// Check if a byte position is within a link (inline links, reference links, or reference definitions)
    fn is_in_link(&self, ctx: &crate::lint_context::LintContext, byte_pos: usize) -> bool {
        // Check inline and reference links
        for link in &ctx.links {
            if link.byte_offset <= byte_pos && byte_pos < link.byte_end {
                return true;
            }
        }

        // Check images (which use similar syntax)
        for image in &ctx.images {
            if image.byte_offset <= byte_pos && byte_pos < image.byte_end {
                return true;
            }
        }

        // Check reference definitions [ref]: url "title" using regex pattern
        for m in REF_DEF_REGEX.find_iter(ctx.content) {
            if m.start() <= byte_pos && byte_pos < m.end() {
                return true;
            }
        }

        false
    }

    // Check if a character is a word boundary (handles Unicode)
    fn is_word_boundary_char(c: char) -> bool {
        !c.is_alphanumeric()
    }

    // Check if position is at a word boundary
    fn is_at_word_boundary(&self, content: &str, pos: usize, is_start: bool) -> bool {
        let chars: Vec<char> = content.chars().collect();
        let char_indices: Vec<(usize, char)> = content.char_indices().collect();

        // Find the character position
        let char_pos = char_indices.iter().position(|(idx, _)| *idx == pos);
        if char_pos.is_none() {
            return true; // If we can't find position, assume boundary
        }
        let char_pos = char_pos.unwrap();

        if is_start {
            // Check character before position
            if char_pos == 0 {
                return true; // Start of string
            }
            Self::is_word_boundary_char(chars[char_pos - 1])
        } else {
            // Check character after position
            if char_pos >= chars.len() {
                return true; // End of string
            }
            Self::is_word_boundary_char(chars[char_pos])
        }
    }

    // Get the proper name that should be used for a found name
    fn get_proper_name_for(&self, found_name: &str) -> Option<String> {
        let found_lower = found_name.to_lowercase();

        // Iterate through the configured proper names
        for name in &self.config.names {
            let lower_name = name.to_lowercase();
            let lower_name_no_dots = lower_name.replace('.', "");

            // Direct match
            if found_lower == lower_name || found_lower == lower_name_no_dots {
                return Some(name.clone());
            }

            // Check ASCII-normalized version
            let ascii_normalized = Self::ascii_normalize(&lower_name);

            let ascii_no_dots = ascii_normalized.replace('.', "");

            if found_lower == ascii_normalized || found_lower == ascii_no_dots {
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

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        if self.config.names.is_empty() {
            return true;
        }
        // Quick check if any configured names exist (case-insensitive)
        let content_lower = ctx.content.to_lowercase();
        !self
            .config
            .names
            .iter()
            .any(|name| content_lower.contains(&name.to_lowercase()))
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        if content.is_empty() || self.config.names.is_empty() || self.combined_pattern.is_none() {
            return Ok(Vec::new());
        }

        // Early return: quick check if any of the configured names might be in content
        let content_lower = content.to_lowercase();
        let has_potential_matches = self.config.names.iter().any(|name| {
            let name_lower = name.to_lowercase();
            let name_no_dots = name_lower.replace('.', "");

            // Check direct match
            if content_lower.contains(&name_lower) || content_lower.contains(&name_no_dots) {
                return true;
            }

            // Also check ASCII-normalized version
            let ascii_normalized = Self::ascii_normalize(&name_lower);

            if ascii_normalized != name_lower {
                if content_lower.contains(&ascii_normalized) {
                    return true;
                }
                let ascii_no_dots = ascii_normalized.replace('.', "");
                if ascii_normalized != ascii_no_dots && content_lower.contains(&ascii_no_dots) {
                    return true;
                }
            }

            false
        });

        if !has_potential_matches {
            return Ok(Vec::new());
        }

        let line_index = &ctx.line_index;
        let violations = self.find_name_violations(content, ctx);

        let warnings = violations
            .into_iter()
            .filter_map(|(line, column, found_name)| {
                self.get_proper_name_for(&found_name).map(|proper_name| LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line,
                    column,
                    end_line: line,
                    end_column: column + found_name.len(),
                    message: format!("Proper name '{found_name}' should be '{proper_name}'"),
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

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if content.is_empty() || self.config.names.is_empty() {
            return Ok(content.to_string());
        }

        let violations = self.find_name_violations(content, ctx);
        if violations.is_empty() {
            return Ok(content.to_string());
        }

        // Process lines and build the fixed content
        let mut fixed_lines = Vec::new();

        // Group violations by line
        let mut violations_by_line: HashMap<usize, Vec<(usize, String)>> = HashMap::new();
        for (line_num, col_num, found_name) in violations {
            violations_by_line
                .entry(line_num)
                .or_default()
                .push((col_num, found_name));
        }

        // Sort violations within each line in reverse order
        for violations in violations_by_line.values_mut() {
            violations.sort_by_key(|b| std::cmp::Reverse(b.0));
        }

        // Process each line
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            let line_num = line_idx + 1;

            if let Some(line_violations) = violations_by_line.get(&line_num) {
                // This line has violations, fix them
                let mut fixed_line = line_info.content(ctx.content).to_string();

                for (col_num, found_name) in line_violations {
                    if let Some(proper_name) = self.get_proper_name_for(found_name) {
                        let start_col = col_num - 1; // Convert to 0-based
                        let end_col = start_col + found_name.len();

                        if end_col <= fixed_line.len()
                            && fixed_line.is_char_boundary(start_col)
                            && fixed_line.is_char_boundary(end_col)
                        {
                            fixed_line.replace_range(start_col..end_col, &proper_name);
                        }
                    }
                }

                fixed_lines.push(fixed_line);
            } else {
                // No violations on this line, keep it as is
                fixed_lines.push(line_info.content(ctx.content).to_string());
            }
        }

        // Join lines with newlines, preserving the original ending
        let mut result = fixed_lines.join("\n");
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD044Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    fn create_context(content: &str) -> LintContext<'_> {
        LintContext::new(content, crate::config::MarkdownFlavor::Standard, None)
    }

    #[test]
    fn test_correctly_capitalized_names() {
        let rule = MD044ProperNames::new(
            vec![
                "JavaScript".to_string(),
                "TypeScript".to_string(),
                "Node.js".to_string(),
            ],
            true,
        );

        let content = "This document uses JavaScript, TypeScript, and Node.js correctly.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag correctly capitalized names");
    }

    #[test]
    fn test_incorrectly_capitalized_names() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string(), "TypeScript".to_string()], true);

        let content = "This document uses javascript and typescript incorrectly.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Should flag two incorrect capitalizations");
        assert_eq!(result[0].message, "Proper name 'javascript' should be 'JavaScript'");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 20);
        assert_eq!(result[1].message, "Proper name 'typescript' should be 'TypeScript'");
        assert_eq!(result[1].line, 1);
        assert_eq!(result[1].column, 35);
    }

    #[test]
    fn test_names_at_beginning_of_sentences() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string(), "Python".to_string()], true);

        let content = "javascript is a great language. python is also popular.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Should flag names at beginning of sentences");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 1);
        assert_eq!(result[1].line, 1);
        assert_eq!(result[1].column, 33);
    }

    #[test]
    fn test_names_in_code_blocks_checked_by_default() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = r#"Here is some text with JavaScript.

```javascript
// This javascript should be checked
const lang = "javascript";
```

But this javascript should be flagged."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "Should flag javascript inside and outside code blocks");
        assert_eq!(result[0].line, 4);
        assert_eq!(result[1].line, 5);
        assert_eq!(result[2].line, 8);
    }

    #[test]
    fn test_names_in_code_blocks_ignored_when_disabled() {
        let rule = MD044ProperNames::new(
            vec!["JavaScript".to_string()],
            false, // code_blocks = false means skip code blocks
        );

        let content = r#"```
javascript in code block
```"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag javascript in code blocks when code_blocks is false"
        );
    }

    #[test]
    fn test_names_in_inline_code_checked_by_default() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = "This is `javascript` in inline code and javascript outside.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // When code_blocks=true, inline code should be checked
        assert_eq!(result.len(), 2, "Should flag javascript inside and outside inline code");
        assert_eq!(result[0].column, 10); // javascript in inline code
        assert_eq!(result[1].column, 41); // javascript outside
    }

    #[test]
    fn test_multiple_names_in_same_line() {
        let rule = MD044ProperNames::new(
            vec!["JavaScript".to_string(), "TypeScript".to_string(), "React".to_string()],
            true,
        );

        let content = "I use javascript, typescript, and react in my projects.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "Should flag all three incorrect names");
        assert_eq!(result[0].message, "Proper name 'javascript' should be 'JavaScript'");
        assert_eq!(result[1].message, "Proper name 'typescript' should be 'TypeScript'");
        assert_eq!(result[2].message, "Proper name 'react' should be 'React'");
    }

    #[test]
    fn test_case_sensitivity() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = "JAVASCRIPT, Javascript, javascript, and JavaScript variations.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "Should flag all incorrect case variations");
        // JavaScript (correct) should not be flagged
        assert!(result.iter().all(|w| w.message.contains("should be 'JavaScript'")));
    }

    #[test]
    fn test_configuration_with_custom_name_list() {
        let config = MD044Config {
            names: vec!["GitHub".to_string(), "GitLab".to_string(), "DevOps".to_string()],
            code_blocks: true,
            html_elements: true,
            html_comments: true,
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "We use github, gitlab, and devops for our workflow.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "Should flag all custom names");
        assert_eq!(result[0].message, "Proper name 'github' should be 'GitHub'");
        assert_eq!(result[1].message, "Proper name 'gitlab' should be 'GitLab'");
        assert_eq!(result[2].message, "Proper name 'devops' should be 'DevOps'");
    }

    #[test]
    fn test_empty_configuration() {
        let rule = MD044ProperNames::new(vec![], true);

        let content = "This has javascript and typescript but no configured names.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Should not flag anything with empty configuration");
    }

    #[test]
    fn test_names_with_special_characters() {
        let rule = MD044ProperNames::new(
            vec!["Node.js".to_string(), "ASP.NET".to_string(), "C++".to_string()],
            true,
        );

        let content = "We use nodejs, asp.net, ASP.NET, and c++ in our stack.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // nodejs should match Node.js (dotless variation)
        // asp.net should be flagged (wrong case)
        // ASP.NET should not be flagged (correct)
        // c++ should be flagged
        assert_eq!(result.len(), 3, "Should handle special characters correctly");

        let messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();
        assert!(messages.contains(&"Proper name 'nodejs' should be 'Node.js'"));
        assert!(messages.contains(&"Proper name 'asp.net' should be 'ASP.NET'"));
        assert!(messages.contains(&"Proper name 'c++' should be 'C++'"));
    }

    #[test]
    fn test_word_boundaries() {
        let rule = MD044ProperNames::new(vec!["Java".to_string(), "Script".to_string()], true);

        let content = "JavaScript is not java or script, but Java and Script are separate.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Should only flag lowercase "java" and "script" as separate words
        assert_eq!(result.len(), 2, "Should respect word boundaries");
        assert!(result.iter().any(|w| w.column == 19)); // "java" position
        assert!(result.iter().any(|w| w.column == 27)); // "script" position
    }

    #[test]
    fn test_fix_method() {
        let rule = MD044ProperNames::new(
            vec![
                "JavaScript".to_string(),
                "TypeScript".to_string(),
                "Node.js".to_string(),
            ],
            true,
        );

        let content = "I love javascript, typescript, and nodejs!";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "I love JavaScript, TypeScript, and Node.js!");
    }

    #[test]
    fn test_fix_multiple_occurrences() {
        let rule = MD044ProperNames::new(vec!["Python".to_string()], true);

        let content = "python is great. I use python daily. PYTHON is powerful.";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "Python is great. I use Python daily. Python is powerful.");
    }

    #[test]
    fn test_fix_checks_code_blocks_by_default() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = r#"I love javascript.

```
const lang = "javascript";
```

More javascript here."#;

        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = r#"I love JavaScript.

```
const lang = "JavaScript";
```

More JavaScript here."#;

        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_multiline_content() {
        let rule = MD044ProperNames::new(vec!["Rust".to_string(), "Python".to_string()], true);

        let content = r#"First line with rust.
Second line with python.
Third line with RUST and PYTHON."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 4, "Should flag all incorrect occurrences");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
        assert_eq!(result[2].line, 3);
        assert_eq!(result[3].line, 3);
    }

    #[test]
    fn test_default_config() {
        let config = MD044Config::default();
        assert!(config.names.is_empty());
        assert!(!config.code_blocks); // Default is false (skip code blocks)
    }

    #[test]
    fn test_performance_with_many_names() {
        let mut names = vec![];
        for i in 0..50 {
            names.push(format!("ProperName{i}"));
        }

        let rule = MD044ProperNames::new(names, true);

        let content = "This has propername0, propername25, and propername49 incorrectly.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "Should handle many configured names efficiently");
    }

    #[test]
    fn test_large_name_count_performance() {
        // Verify MD044 can handle large numbers of names without regex limitations
        // This test confirms that fancy-regex handles large patterns well
        let names = (0..1000).map(|i| format!("ProperName{i}")).collect::<Vec<_>>();

        let rule = MD044ProperNames::new(names, true);

        // The combined pattern should be created successfully
        assert!(rule.combined_pattern.is_some());

        // Should be able to check content without errors
        let content = "This has propername0 and propername999 in it.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Should detect both incorrect names
        assert_eq!(result.len(), 2, "Should handle 1000 names without issues");
    }

    #[test]
    fn test_cache_behavior() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = "Using javascript here.";
        let ctx = create_context(content);

        // First check
        let result1 = rule.check(&ctx).unwrap();
        assert_eq!(result1.len(), 1);

        // Second check should use cache
        let result2 = rule.check(&ctx).unwrap();
        assert_eq!(result2.len(), 1);

        // Results should be identical
        assert_eq!(result1[0].line, result2[0].line);
        assert_eq!(result1[0].column, result2[0].column);
    }

    #[test]
    fn test_html_comments_not_checked_when_disabled() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string()],
            code_blocks: true,    // Check code blocks
            html_elements: true,  // Check HTML elements
            html_comments: false, // Don't check HTML comments
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = r#"Regular javascript here.
<!-- This javascript in HTML comment should be ignored -->
More javascript outside."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Should only flag javascript outside HTML comments");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_html_comments_checked_when_enabled() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string()],
            code_blocks: true,   // Check code blocks
            html_elements: true, // Check HTML elements
            html_comments: true, // Check HTML comments
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = r#"Regular javascript here.
<!-- This javascript in HTML comment should be checked -->
More javascript outside."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            3,
            "Should flag all javascript occurrences including in HTML comments"
        );
    }

    #[test]
    fn test_multiline_html_comments() {
        let config = MD044Config {
            names: vec!["Python".to_string(), "JavaScript".to_string()],
            code_blocks: true,    // Check code blocks
            html_elements: true,  // Check HTML elements
            html_comments: false, // Don't check HTML comments
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = r#"Regular python here.
<!--
This is a multiline comment
with javascript and python
that should be ignored
-->
More javascript outside."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Should only flag names outside HTML comments");
        assert_eq!(result[0].line, 1); // python
        assert_eq!(result[1].line, 7); // javascript
    }

    #[test]
    fn test_fix_preserves_html_comments_when_disabled() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string()],
            code_blocks: true,    // Check code blocks
            html_elements: true,  // Check HTML elements
            html_comments: false, // Don't check HTML comments
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = r#"javascript here.
<!-- javascript in comment -->
More javascript."#;

        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = r#"JavaScript here.
<!-- javascript in comment -->
More JavaScript."#;

        assert_eq!(
            fixed, expected,
            "Should not fix names inside HTML comments when disabled"
        );
    }

    #[test]
    fn test_proper_names_in_links_not_flagged() {
        let rule = MD044ProperNames::new(
            vec!["JavaScript".to_string(), "Node.js".to_string(), "Python".to_string()],
            true,
        );

        let content = r#"Check this [javascript documentation](https://javascript.info) for info.

Visit [node.js homepage](https://nodejs.org) and [python tutorial](https://python.org).

Real javascript should be flagged.

Also see the [typescript guide][ts-ref] for more.

Real python should be flagged too.

[ts-ref]: https://typescript.org/handbook"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Only the real standalone proper names should be flagged
        assert_eq!(
            result.len(),
            2,
            "Expected exactly 2 warnings for standalone proper names"
        );
        assert!(result[0].message.contains("'javascript' should be 'JavaScript'"));
        assert!(result[1].message.contains("'python' should be 'Python'"));
        // Should be on lines with standalone instances
        assert!(result[0].line == 5); // "Real javascript should be flagged."
        assert!(result[1].line == 9); // "Real python should be flagged too."
    }

    #[test]
    fn test_proper_names_in_images_not_flagged() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = r#"Here is a ![javascript logo](javascript.png "javascript icon") image.

Real javascript should be flagged."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Only the standalone proper name should be flagged
        assert_eq!(result.len(), 1, "Expected exactly 1 warning for standalone proper name");
        assert!(result[0].message.contains("'javascript' should be 'JavaScript'"));
        assert!(result[0].line == 3); // "Real javascript should be flagged."
    }

    #[test]
    fn test_proper_names_in_reference_definitions_not_flagged() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string(), "TypeScript".to_string()], true);

        let content = r#"Check the [javascript guide][js-ref] for details.

Real javascript should be flagged.

[js-ref]: https://javascript.info/typescript/guide"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Only the standalone proper name should be flagged
        assert_eq!(result.len(), 1, "Expected exactly 1 warning for standalone proper name");
        assert!(result[0].message.contains("'javascript' should be 'JavaScript'"));
        assert!(result[0].line == 3); // "Real javascript should be flagged."
    }
}
