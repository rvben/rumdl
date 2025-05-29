pub mod config;
pub mod init;
pub mod lint_context;
pub mod lsp;
pub mod markdownlint_config;
pub mod parallel;
pub mod performance;
pub mod profiling;
pub mod rule;
pub mod rules;
pub mod utils;

#[cfg(feature = "python")]
pub mod python;

pub use rules::heading_utils::{Heading, HeadingStyle};
pub use rules::*;

pub use crate::lint_context::LintContext;
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
        let mut chars = Self {
            ..Default::default()
        };

        // Quick single-pass analysis
        let mut has_atx_heading = false;
        let mut has_setext_heading = false;

        for line in content.lines() {
            let trimmed = line.trim();

            // Headings: ATX (#) or Setext (underlines)
            if !has_atx_heading && trimmed.starts_with('#') {
                has_atx_heading = true;
            }
            if !has_setext_heading
                && (trimmed.chars().all(|c| c == '=' || c == '-') && trimmed.len() > 1)
            {
                has_setext_heading = true;
            }

            // Quick character-based detection (more efficient than regex)
            if !chars.has_lists
                && (line.contains("* ") || line.contains("- ") || line.contains("+ "))
            {
                chars.has_lists = true;
            }
            if !chars.has_lists
                && line.chars().next().map_or(false, |c| c.is_ascii_digit())
                && line.contains(". ")
            {
                chars.has_lists = true;
            }
            if !chars.has_links && (line.contains('[') || line.contains("http://") || line.contains("https://") || line.contains("ftp://")) {
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
    let ast_rules_count = applicable_rules
        .iter()
        .filter(|rule| rule.uses_ast())
        .count();
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
                // Filter out warnings for rules disabled via comments
                let filtered_warnings: Vec<_> = rule_warnings
                    .into_iter()
                    .filter(|warning| {
                        !crate::rule::is_rule_disabled_at_line(
                            content,
                            rule.name(),
                            warning.line.saturating_sub(1), // Convert to 0-based line index
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
            log::debug!(
                "Skipped {} of {} rules based on content analysis",
                skipped_rules,
                _total_rules
            );
        }
        if ast.is_some() {
            log::debug!("Used shared AST for {} rules", ast_rules_count);
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
        report.push_str(&format!("  Total usage: {}\n", total_usage));

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
        report.push_str(&format!("  Total usage: {}\n", total_usage));

        if total_usage > ast_stats.len() as u64 {
            let cache_hit_rate =
                ((total_usage - ast_stats.len() as u64) as f64 / total_usage as f64) * 100.0;
            report.push_str(&format!("  Cache hit rate: {:.1}%\n", cache_hit_rate));
        }
    }

    report
}
