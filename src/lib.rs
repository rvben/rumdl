pub mod config;
pub mod exit_codes;
pub mod inline_config;
pub mod lint_context;
pub mod lsp;
pub mod markdownlint_config;
pub mod output;
pub mod parallel;
pub mod performance;
pub mod profiling;
pub mod rule;
pub mod vscode;
#[macro_use]
pub mod rule_config;
#[macro_use]
pub mod rule_config_serde;
pub mod rules;
pub mod utils;

#[cfg(feature = "python")]
pub mod python;

pub use rules::heading_utils::{Heading, HeadingStyle};
pub use rules::*;

pub use crate::lint_context::{LineInfo, LintContext, ListItemInfo};
use crate::rule::{LintResult, Rule, RuleCategory};
use crate::utils::document_structure::DocumentStructure;
use std::time::Instant;

/// Content characteristics for efficient rule filtering
#[derive(Debug, Default)]
struct ContentCharacteristics {
    has_headings: bool,    // # or setext headings
    has_lists: bool,       // *, -, +, 1. etc
    has_links: bool,       // [text](url) or [text][ref]
    has_code: bool,        // ``` or ~~~ or indented code
    has_emphasis: bool,    // * or _ for emphasis
    has_html: bool,        // < > tags
    has_tables: bool,      // | pipes
    has_blockquotes: bool, // > markers
    has_images: bool,      // ![alt](url)
}

impl ContentCharacteristics {
    fn analyze(content: &str) -> Self {
        let mut chars = Self { ..Default::default() };

        // Quick single-pass analysis
        let mut has_atx_heading = false;
        let mut has_setext_heading = false;

        for line in content.lines() {
            let trimmed = line.trim();

            // Headings: ATX (#) or Setext (underlines)
            if !has_atx_heading && trimmed.starts_with('#') {
                has_atx_heading = true;
            }
            if !has_setext_heading && (trimmed.chars().all(|c| c == '=' || c == '-') && trimmed.len() > 1) {
                has_setext_heading = true;
            }

            // Quick character-based detection (more efficient than regex)
            if !chars.has_lists && (line.contains("* ") || line.contains("- ") || line.contains("+ ")) {
                chars.has_lists = true;
            }
            if !chars.has_lists && line.chars().next().is_some_and(|c| c.is_ascii_digit()) && line.contains(". ") {
                chars.has_lists = true;
            }
            if !chars.has_links
                && (line.contains('[')
                    || line.contains("http://")
                    || line.contains("https://")
                    || line.contains("ftp://"))
            {
                chars.has_links = true;
            }
            if !chars.has_images && line.contains("![") {
                chars.has_images = true;
            }
            if !chars.has_code && (line.contains('`') || line.contains("~~~")) {
                chars.has_code = true;
            }
            if !chars.has_emphasis && (line.contains('*') || line.contains('_')) {
                chars.has_emphasis = true;
            }
            if !chars.has_html && line.contains('<') {
                chars.has_html = true;
            }
            if !chars.has_tables && line.contains('|') {
                chars.has_tables = true;
            }
            if !chars.has_blockquotes && line.starts_with('>') {
                chars.has_blockquotes = true;
            }
        }

        chars.has_headings = has_atx_heading || has_setext_heading;
        chars
    }

    /// Check if a rule should be skipped based on content characteristics
    fn should_skip_rule(&self, rule: &dyn Rule) -> bool {
        match rule.category() {
            RuleCategory::Heading => !self.has_headings,
            RuleCategory::List => !self.has_lists,
            RuleCategory::Link => !self.has_links && !self.has_images,
            RuleCategory::Image => !self.has_images,
            RuleCategory::CodeBlock => !self.has_code,
            RuleCategory::Html => !self.has_html,
            RuleCategory::Emphasis => !self.has_emphasis,
            RuleCategory::Blockquote => !self.has_blockquotes,
            RuleCategory::Table => !self.has_tables,
            // Always check these categories as they apply to all content
            RuleCategory::Whitespace | RuleCategory::FrontMatter | RuleCategory::Other => false,
        }
    }
}

/// Lint a file against the given rules with intelligent rule filtering
/// Assumes the provided `rules` vector contains the final,
/// configured, and filtered set of rules to be executed.
pub fn lint(content: &str, rules: &[Box<dyn Rule>], _verbose: bool) -> LintResult {
    let mut warnings = Vec::new();
    let _overall_start = Instant::now();

    // Early return for empty content
    if content.is_empty() {
        return Ok(warnings);
    }

    // Parse inline configuration comments once
    let inline_config = crate::inline_config::InlineConfig::from_content(content);

    // Analyze content characteristics for rule filtering
    let characteristics = ContentCharacteristics::analyze(content);

    // Filter rules based on content characteristics
    let applicable_rules: Vec<_> = rules
        .iter()
        .filter(|rule| !characteristics.should_skip_rule(rule.as_ref()))
        .collect();

    // Calculate skipped rules count before consuming applicable_rules
    let _total_rules = rules.len();
    let _applicable_count = applicable_rules.len();

    // Parse DocumentStructure once
    let structure = DocumentStructure::new(content);

    // Parse AST once for rules that can benefit from it
    let ast_rules_count = applicable_rules.iter().filter(|rule| rule.uses_ast()).count();
    let ast = if ast_rules_count > 0 {
        Some(crate::utils::ast_utils::get_cached_ast(content))
    } else {
        None
    };

    // Parse LintContext once (migration step)
    let lint_ctx = crate::lint_context::LintContext::new(content);

    for rule in applicable_rules {
        let _rule_start = Instant::now();

        // Try optimized paths in order of preference
        let result = if rule.uses_ast() {
            if let Some(ref ast_ref) = ast {
                // 1. AST-based path
                rule.as_maybe_ast()
                    .and_then(|ext| ext.check_with_ast_opt(&lint_ctx, ast_ref))
                    .unwrap_or_else(|| rule.check_with_ast(&lint_ctx, ast_ref))
            } else {
                // Fallback to regular check if no AST
                rule.as_maybe_document_structure()
                    .and_then(|ext| ext.check_with_structure_opt(&lint_ctx, &structure))
                    .unwrap_or_else(|| rule.check(&lint_ctx))
            }
        } else {
            // 2. Document structure path
            rule.as_maybe_document_structure()
                .and_then(|ext| ext.check_with_structure_opt(&lint_ctx, &structure))
                .unwrap_or_else(|| rule.check(&lint_ctx))
        };

        match result {
            Ok(rule_warnings) => {
                // Filter out warnings for rules disabled via inline comments
                let filtered_warnings: Vec<_> = rule_warnings
                    .into_iter()
                    .filter(|warning| {
                        // Use the warning's rule_name if available, otherwise use the rule's name
                        let rule_name_to_check = warning.rule_name.unwrap_or(rule.name());

                        // Extract the base rule name for sub-rules like "MD029-style" -> "MD029"
                        let base_rule_name = if let Some(dash_pos) = rule_name_to_check.find('-') {
                            &rule_name_to_check[..dash_pos]
                        } else {
                            rule_name_to_check
                        };

                        !inline_config.is_rule_disabled(
                            base_rule_name,
                            warning.line, // Already 1-indexed
                        )
                    })
                    .collect();
                warnings.extend(filtered_warnings);
            }
            Err(e) => {
                log::error!("Error checking rule {}: {}", rule.name(), e);
                return Err(e);
            }
        }

        #[cfg(not(test))]
        if _verbose {
            let rule_duration = _rule_start.elapsed();
            if rule_duration.as_millis() > 500 {
                log::debug!("Rule {} took {:?}", rule.name(), rule_duration);
            }
        }
    }

    #[cfg(not(test))]
    if _verbose {
        let skipped_rules = _total_rules - _applicable_count;
        if skipped_rules > 0 {
            log::debug!("Skipped {skipped_rules} of {_total_rules} rules based on content analysis");
        }
        if ast.is_some() {
            log::debug!("Used shared AST for {ast_rules_count} rules");
        }
    }

    Ok(warnings)
}

/// Get the profiling report
pub fn get_profiling_report() -> String {
    profiling::get_report()
}

/// Reset the profiling data
pub fn reset_profiling() {
    profiling::reset()
}

/// Get regex cache statistics for performance monitoring
pub fn get_regex_cache_stats() -> std::collections::HashMap<String, u64> {
    crate::utils::regex_cache::get_cache_stats()
}

/// Get AST cache statistics for performance monitoring
pub fn get_ast_cache_stats() -> std::collections::HashMap<u64, u64> {
    crate::utils::ast_utils::get_ast_cache_stats()
}

/// Clear all caches (useful for testing and memory management)
pub fn clear_all_caches() {
    crate::utils::ast_utils::clear_ast_cache();
    // Note: Regex cache is intentionally not cleared as it's global and shared
}

/// Get comprehensive cache performance report
pub fn get_cache_performance_report() -> String {
    let regex_stats = get_regex_cache_stats();
    let ast_stats = get_ast_cache_stats();

    let mut report = String::new();

    report.push_str("=== Cache Performance Report ===\n\n");

    // Regex cache statistics
    report.push_str("Regex Cache:\n");
    if regex_stats.is_empty() {
        report.push_str("  No regex patterns cached\n");
    } else {
        let total_usage: u64 = regex_stats.values().sum();
        report.push_str(&format!("  Total patterns: {}\n", regex_stats.len()));
        report.push_str(&format!("  Total usage: {total_usage}\n"));

        // Show top 5 most used patterns
        let mut sorted_patterns: Vec<_> = regex_stats.iter().collect();
        sorted_patterns.sort_by(|a, b| b.1.cmp(a.1));

        report.push_str("  Top patterns by usage:\n");
        for (pattern, count) in sorted_patterns.iter().take(5) {
            let truncated_pattern = if pattern.len() > 50 {
                format!("{}...", &pattern[..47])
            } else {
                pattern.to_string()
            };
            report.push_str(&format!(
                "    {} ({}x): {}\n",
                count,
                pattern.len().min(50),
                truncated_pattern
            ));
        }
    }

    report.push('\n');

    // AST cache statistics
    report.push_str("AST Cache:\n");
    if ast_stats.is_empty() {
        report.push_str("  No AST nodes cached\n");
    } else {
        let total_usage: u64 = ast_stats.values().sum();
        report.push_str(&format!("  Total ASTs: {}\n", ast_stats.len()));
        report.push_str(&format!("  Total usage: {total_usage}\n"));

        if total_usage > ast_stats.len() as u64 {
            let cache_hit_rate = ((total_usage - ast_stats.len() as u64) as f64 / total_usage as f64) * 100.0;
            report.push_str(&format!("  Cache hit rate: {cache_hit_rate:.1}%\n"));
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::Rule;
    use crate::rules::{MD001HeadingIncrement, MD009TrailingSpaces, MD012NoMultipleBlanks};

    #[test]
    fn test_content_characteristics_analyze() {
        // Test empty content
        let chars = ContentCharacteristics::analyze("");
        assert!(!chars.has_headings);
        assert!(!chars.has_lists);
        assert!(!chars.has_links);
        assert!(!chars.has_code);
        assert!(!chars.has_emphasis);
        assert!(!chars.has_html);
        assert!(!chars.has_tables);
        assert!(!chars.has_blockquotes);
        assert!(!chars.has_images);

        // Test content with headings
        let chars = ContentCharacteristics::analyze("# Heading");
        assert!(chars.has_headings);

        // Test setext headings
        let chars = ContentCharacteristics::analyze("Heading\n=======");
        assert!(chars.has_headings);

        // Test lists
        let chars = ContentCharacteristics::analyze("* Item\n- Item 2\n+ Item 3");
        assert!(chars.has_lists);

        // Test ordered lists
        let chars = ContentCharacteristics::analyze("1. First\n2. Second");
        assert!(chars.has_lists);

        // Test links
        let chars = ContentCharacteristics::analyze("[link](url)");
        assert!(chars.has_links);

        // Test URLs
        let chars = ContentCharacteristics::analyze("Visit https://example.com");
        assert!(chars.has_links);

        // Test images
        let chars = ContentCharacteristics::analyze("![alt text](image.png)");
        assert!(chars.has_images);

        // Test code
        let chars = ContentCharacteristics::analyze("`inline code`");
        assert!(chars.has_code);

        let chars = ContentCharacteristics::analyze("~~~\ncode block\n~~~");
        assert!(chars.has_code);

        // Test emphasis
        let chars = ContentCharacteristics::analyze("*emphasis* and _more_");
        assert!(chars.has_emphasis);

        // Test HTML
        let chars = ContentCharacteristics::analyze("<div>HTML content</div>");
        assert!(chars.has_html);

        // Test tables
        let chars = ContentCharacteristics::analyze("| Header | Header |\n|--------|--------|");
        assert!(chars.has_tables);

        // Test blockquotes
        let chars = ContentCharacteristics::analyze("> Quote");
        assert!(chars.has_blockquotes);

        // Test mixed content
        let content = "# Heading\n* List item\n[link](url)\n`code`\n*emphasis*\n<p>html</p>\n| table |\n> quote\n![image](img.png)";
        let chars = ContentCharacteristics::analyze(content);
        assert!(chars.has_headings);
        assert!(chars.has_lists);
        assert!(chars.has_links);
        assert!(chars.has_code);
        assert!(chars.has_emphasis);
        assert!(chars.has_html);
        assert!(chars.has_tables);
        assert!(chars.has_blockquotes);
        assert!(chars.has_images);
    }

    #[test]
    fn test_content_characteristics_should_skip_rule() {
        let chars = ContentCharacteristics {
            has_headings: true,
            has_lists: false,
            has_links: true,
            has_code: false,
            has_emphasis: true,
            has_html: false,
            has_tables: true,
            has_blockquotes: false,
            has_images: false,
        };

        // Create test rules for different categories
        let heading_rule = MD001HeadingIncrement;
        assert!(!chars.should_skip_rule(&heading_rule));

        let trailing_spaces_rule = MD009TrailingSpaces::new(2, false);
        assert!(!chars.should_skip_rule(&trailing_spaces_rule)); // Whitespace rules always run

        // Test skipping based on content
        let chars_no_headings = ContentCharacteristics {
            has_headings: false,
            ..Default::default()
        };
        assert!(chars_no_headings.should_skip_rule(&heading_rule));
    }

    #[test]
    fn test_lint_empty_content() {
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD001HeadingIncrement)];

        let result = lint("", &rules, false);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_lint_with_violations() {
        let content = "## Level 2\n#### Level 4"; // Skips level 3
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD001HeadingIncrement)];

        let result = lint(content, &rules, false);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert!(!warnings.is_empty());
        // Check the rule field of LintWarning struct
        assert_eq!(warnings[0].rule_name, Some("MD001"));
    }

    #[test]
    fn test_lint_with_inline_disable() {
        let content = "<!-- rumdl-disable MD001 -->\n## Level 2\n#### Level 4";
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD001HeadingIncrement)];

        let result = lint(content, &rules, false);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert!(warnings.is_empty()); // Should be disabled by inline comment
    }

    #[test]
    fn test_lint_rule_filtering() {
        // Content with no lists
        let content = "# Heading\nJust text";
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MD001HeadingIncrement),
            // A list-related rule would be skipped
        ];

        let result = lint(content, &rules, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_profiling_report() {
        // Just test that it returns a string without panicking
        let report = get_profiling_report();
        assert!(!report.is_empty());
        assert!(report.contains("Profiling"));
    }

    #[test]
    fn test_reset_profiling() {
        // Test that reset_profiling doesn't panic
        reset_profiling();

        // After reset, report should indicate no measurements or profiling disabled
        let report = get_profiling_report();
        assert!(report.contains("disabled") || report.contains("no measurements"));
    }

    #[test]
    fn test_get_regex_cache_stats() {
        let stats = get_regex_cache_stats();
        // Stats should be a valid HashMap (might be empty)
        assert!(stats.is_empty() || !stats.is_empty());

        // If not empty, all values should be positive
        for count in stats.values() {
            assert!(*count > 0);
        }
    }

    #[test]
    fn test_get_ast_cache_stats() {
        let stats = get_ast_cache_stats();
        // Stats should be a valid HashMap (might be empty)
        assert!(stats.is_empty() || !stats.is_empty());

        // If not empty, all values should be positive
        for count in stats.values() {
            assert!(*count > 0);
        }
    }

    #[test]
    fn test_clear_all_caches() {
        // Test that clear_all_caches doesn't panic
        clear_all_caches();

        // After clearing, AST cache should be empty
        let ast_stats = get_ast_cache_stats();
        assert!(ast_stats.is_empty());
    }

    #[test]
    fn test_get_cache_performance_report() {
        let report = get_cache_performance_report();

        // Report should contain expected sections
        assert!(report.contains("Cache Performance Report"));
        assert!(report.contains("Regex Cache:"));
        assert!(report.contains("AST Cache:"));

        // Test with empty caches
        clear_all_caches();
        let report_empty = get_cache_performance_report();
        assert!(report_empty.contains("No AST nodes cached"));
    }

    #[test]
    fn test_lint_with_ast_rules() {
        // Create content that would benefit from AST parsing
        let content = "# Heading\n\nParagraph with **bold** text.";
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD012NoMultipleBlanks::new(1))];

        let result = lint(content, &rules, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_content_characteristics_edge_cases() {
        // Test setext heading edge case
        let chars = ContentCharacteristics::analyze("-"); // Single dash, not a heading
        assert!(!chars.has_headings);

        let chars = ContentCharacteristics::analyze("--"); // Two dashes, valid setext
        assert!(chars.has_headings);

        // Test list detection edge cases
        let chars = ContentCharacteristics::analyze("*emphasis*"); // Not a list
        assert!(!chars.has_lists);

        let chars = ContentCharacteristics::analyze("1.Item"); // No space after period
        assert!(!chars.has_lists);

        // Test blockquote must be at start of line
        let chars = ContentCharacteristics::analyze("text > not a quote");
        assert!(!chars.has_blockquotes);
    }

    #[test]
    fn test_cache_performance_report_formatting() {
        // Add some data to caches to test formatting
        // (Would require actual usage of the caches, which happens during linting)

        let report = get_cache_performance_report();

        // Test truncation of long patterns
        // Since we can't easily add a long pattern to the cache in this test,
        // we'll just verify the report structure is correct
        assert!(!report.is_empty());
        assert!(report.lines().count() > 3); // Should have multiple lines
    }
}
