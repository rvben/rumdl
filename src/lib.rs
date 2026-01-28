pub mod config;
pub mod exit_codes;
pub mod filtered_lines;
pub mod fix_coordinator;
pub mod inline_config;
pub mod lint_context;
pub mod markdownlint_config;
pub mod profiling;
pub mod rule;
#[cfg(feature = "native")]
pub mod vscode;
pub mod workspace_index;
#[macro_use]
pub mod rule_config;
#[macro_use]
pub mod rule_config_serde;
pub mod rules;
pub mod types;
pub mod utils;

// Native-only modules (require tokio, tower-lsp, etc.)
#[cfg(feature = "native")]
pub mod lsp;
#[cfg(feature = "native")]
pub mod output;
#[cfg(feature = "native")]
pub mod parallel;
#[cfg(feature = "native")]
pub mod performance;

// WASM module
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub mod wasm;

pub use rules::heading_utils::{Heading, HeadingStyle};
pub use rules::*;

pub use crate::lint_context::{LineInfo, LintContext, ListItemInfo};
use crate::rule::{LintResult, Rule, RuleCategory};
use crate::utils::element_cache::ElementCache;
#[cfg(not(target_arch = "wasm32"))]
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

/// Check if a line has enough leading whitespace to be an indented code block.
/// Indented code blocks require 4+ columns of leading whitespace (with proper tab expansion).
fn has_potential_indented_code_indent(line: &str) -> bool {
    ElementCache::calculate_indentation_width_default(line) >= 4
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
            // Include patterns without spaces to enable user-intention detection (MD030)
            if !chars.has_lists
                && (line.contains("* ")
                    || line.contains("- ")
                    || line.contains("+ ")
                    || trimmed.starts_with("* ")
                    || trimmed.starts_with("- ")
                    || trimmed.starts_with("+ ")
                    || trimmed.starts_with('*')
                    || trimmed.starts_with('-')
                    || trimmed.starts_with('+'))
            {
                chars.has_lists = true;
            }
            // Ordered lists: line starts with digit, or blockquote line contains digit followed by period
            if !chars.has_lists
                && ((line.chars().next().is_some_and(|c| c.is_ascii_digit())
                    && (line.contains(". ") || line.contains('.')))
                    || (trimmed.starts_with('>')
                        && trimmed.chars().any(|c| c.is_ascii_digit())
                        && (trimmed.contains(". ") || trimmed.contains('.'))))
            {
                chars.has_lists = true;
            }
            if !chars.has_links
                && (line.contains('[')
                    || line.contains("http://")
                    || line.contains("https://")
                    || line.contains("ftp://")
                    || line.contains("www."))
            {
                chars.has_links = true;
            }
            if !chars.has_images && line.contains("![") {
                chars.has_images = true;
            }
            if !chars.has_code
                && (line.contains('`') || line.contains("~~~") || has_potential_indented_code_indent(line))
            {
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

/// Compute content hash for incremental indexing change detection
///
/// Uses blake3 for native builds (fast, cryptographic-strength hash)
/// Falls back to std::hash for WASM builds
#[cfg(feature = "native")]
fn compute_content_hash(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex().to_string()
}

/// Compute content hash for WASM builds using std::hash
#[cfg(not(feature = "native"))]
fn compute_content_hash(content: &str) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Lint a file against the given rules with intelligent rule filtering
/// Assumes the provided `rules` vector contains the final,
/// configured, and filtered set of rules to be executed.
pub fn lint(
    content: &str,
    rules: &[Box<dyn Rule>],
    verbose: bool,
    flavor: crate::config::MarkdownFlavor,
    config: Option<&crate::config::Config>,
) -> LintResult {
    // Use lint_and_index but discard the FileIndex for backward compatibility
    let (result, _file_index) = lint_and_index(content, rules, verbose, flavor, None, config);
    result
}

/// Build FileIndex only (no linting) for cross-file analysis on cache hits
///
/// This is a lightweight function that only builds the FileIndex without running
/// any rules. Used when we have a cache hit but still need the FileIndex for
/// cross-file validation.
///
/// This avoids the overhead of re-running all rules when only the index data is needed.
pub fn build_file_index_only(
    content: &str,
    rules: &[Box<dyn Rule>],
    flavor: crate::config::MarkdownFlavor,
) -> crate::workspace_index::FileIndex {
    // Compute content hash for change detection
    let content_hash = compute_content_hash(content);
    let mut file_index = crate::workspace_index::FileIndex::with_hash(content_hash);

    // Early return for empty content
    if content.is_empty() {
        return file_index;
    }

    // Parse LintContext once with the provided flavor
    let lint_ctx = crate::lint_context::LintContext::new(content, flavor, None);

    // Only call contribute_to_index for cross-file rules (no rule checking!)
    for rule in rules {
        if rule.cross_file_scope() == crate::rule::CrossFileScope::Workspace {
            rule.contribute_to_index(&lint_ctx, &mut file_index);
        }
    }

    file_index
}

/// Lint a file and contribute to workspace index for cross-file analysis
///
/// This variant performs linting and optionally populates a `FileIndex` with data
/// needed for cross-file validation. The FileIndex is populated during linting,
/// avoiding duplicate parsing.
///
/// Returns: (warnings, FileIndex) - the FileIndex contains headings/links for cross-file rules
pub fn lint_and_index(
    content: &str,
    rules: &[Box<dyn Rule>],
    _verbose: bool,
    flavor: crate::config::MarkdownFlavor,
    source_file: Option<std::path::PathBuf>,
    config: Option<&crate::config::Config>,
) -> (LintResult, crate::workspace_index::FileIndex) {
    let mut warnings = Vec::new();
    // Compute content hash for change detection
    let content_hash = compute_content_hash(content);
    let mut file_index = crate::workspace_index::FileIndex::with_hash(content_hash);

    #[cfg(not(target_arch = "wasm32"))]
    let _overall_start = Instant::now();

    // Early return for empty content
    if content.is_empty() {
        return (Ok(warnings), file_index);
    }

    // Parse inline configuration comments once
    let inline_config = crate::inline_config::InlineConfig::from_content(content);

    // Export inline config data to FileIndex for cross-file rule filtering
    let (file_disabled, line_disabled) = inline_config.export_for_file_index();
    file_index.file_disabled_rules = file_disabled;
    file_index.line_disabled_rules = line_disabled;

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

    // Parse LintContext once with the provided flavor
    let lint_ctx = crate::lint_context::LintContext::new(content, flavor, source_file);

    #[cfg(not(target_arch = "wasm32"))]
    let profile_rules = std::env::var("RUMDL_PROFILE_RULES").is_ok();
    #[cfg(target_arch = "wasm32")]
    let profile_rules = false;

    for rule in &applicable_rules {
        #[cfg(not(target_arch = "wasm32"))]
        let _rule_start = Instant::now();

        // Skip rules that indicate they should be skipped (opt-in rules, content-based skipping)
        if rule.should_skip(&lint_ctx) {
            continue;
        }

        // Run single-file check
        let result = rule.check(&lint_ctx);

        match result {
            Ok(rule_warnings) => {
                // Filter out warnings for rules disabled via inline comments
                let filtered_warnings: Vec<_> = rule_warnings
                    .into_iter()
                    .filter(|warning| {
                        // Use the warning's rule_name if available, otherwise use the rule's name
                        let rule_name_to_check = warning.rule_name.as_deref().unwrap_or(rule.name());

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
                    .map(|mut warning| {
                        // Apply severity override from config if present
                        if let Some(cfg) = config {
                            let rule_name_to_check = warning.rule_name.as_deref().unwrap_or(rule.name());
                            if let Some(override_severity) = cfg.get_rule_severity(rule_name_to_check) {
                                warning.severity = override_severity;
                            }
                        }
                        warning
                    })
                    .collect();
                warnings.extend(filtered_warnings);
            }
            Err(e) => {
                log::error!("Error checking rule {}: {}", rule.name(), e);
                return (Err(e), file_index);
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let rule_duration = _rule_start.elapsed();
            if profile_rules {
                eprintln!("[RULE] {:6} {:?}", rule.name(), rule_duration);
            }

            #[cfg(not(test))]
            if _verbose && rule_duration.as_millis() > 500 {
                log::debug!("Rule {} took {:?}", rule.name(), rule_duration);
            }
        }
    }

    // Contribute to index for cross-file rules (done after all rules checked)
    // NOTE: We iterate over ALL rules (not just applicable_rules) because cross-file
    // rules need to extract data from every file in the workspace, regardless of whether
    // that file has content that would trigger the rule. For example, MD051 needs to
    // index headings from files that have no links (like target.md) so that links
    // FROM other files TO those headings can be validated.
    for rule in rules {
        if rule.cross_file_scope() == crate::rule::CrossFileScope::Workspace {
            rule.contribute_to_index(&lint_ctx, &mut file_index);
        }
    }

    #[cfg(not(test))]
    if _verbose {
        let skipped_rules = _total_rules - _applicable_count;
        if skipped_rules > 0 {
            log::debug!("Skipped {skipped_rules} of {_total_rules} rules based on content analysis");
        }
    }

    (Ok(warnings), file_index)
}

/// Run cross-file checks for rules that need workspace-wide validation
///
/// This should be called after all files have been linted and the WorkspaceIndex
/// has been built from the accumulated FileIndex data.
///
/// Note: This takes the FileIndex instead of content to avoid re-parsing each file.
/// The FileIndex was already populated during contribute_to_index in the linting phase.
///
/// Rules can use workspace_index methods for cross-file validation:
/// - `get_file(path)` - to look up headings in target files (for MD051)
///
/// Returns additional warnings from cross-file validation.
pub fn run_cross_file_checks(
    file_path: &std::path::Path,
    file_index: &crate::workspace_index::FileIndex,
    rules: &[Box<dyn Rule>],
    workspace_index: &crate::workspace_index::WorkspaceIndex,
    config: Option<&crate::config::Config>,
) -> LintResult {
    use crate::rule::CrossFileScope;

    let mut warnings = Vec::new();

    // Only check rules that need cross-file analysis
    for rule in rules {
        if rule.cross_file_scope() != CrossFileScope::Workspace {
            continue;
        }

        match rule.cross_file_check(file_path, file_index, workspace_index) {
            Ok(rule_warnings) => {
                // Filter cross-file warnings based on inline config stored in file_index
                let filtered: Vec<_> = rule_warnings
                    .into_iter()
                    .filter(|w| !file_index.is_rule_disabled_at_line(rule.name(), w.line))
                    .map(|mut warning| {
                        // Apply severity override from config if present
                        if let Some(cfg) = config
                            && let Some(override_severity) = cfg.get_rule_severity(rule.name())
                        {
                            warning.severity = override_severity;
                        }
                        warning
                    })
                    .collect();
                warnings.extend(filtered);
            }
            Err(e) => {
                log::error!("Error in cross-file check for rule {}: {}", rule.name(), e);
                return Err(e);
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::Rule;
    use crate::rules::{MD001HeadingIncrement, MD009TrailingSpaces};

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

        // Test indented code blocks (4 spaces)
        let chars = ContentCharacteristics::analyze("Text\n\n    indented code\n\nMore text");
        assert!(chars.has_code);

        // Test tab-indented code blocks
        let chars = ContentCharacteristics::analyze("Text\n\n\ttab indented code\n\nMore text");
        assert!(chars.has_code);

        // Test mixed whitespace indented code (2 spaces + tab = 4 columns)
        let chars = ContentCharacteristics::analyze("Text\n\n  \tmixed indent code\n\nMore text");
        assert!(chars.has_code);

        // Test 1 space + tab (also 4 columns due to tab expansion)
        let chars = ContentCharacteristics::analyze("Text\n\n \ttab after space\n\nMore text");
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
        let heading_rule = MD001HeadingIncrement::default();
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
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD001HeadingIncrement::default())];

        let result = lint("", &rules, false, crate::config::MarkdownFlavor::Standard, None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_lint_with_violations() {
        let content = "## Level 2\n#### Level 4"; // Skips level 3
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD001HeadingIncrement::default())];

        let result = lint(content, &rules, false, crate::config::MarkdownFlavor::Standard, None);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert!(!warnings.is_empty());
        // Check the rule field of LintWarning struct
        assert_eq!(warnings[0].rule_name.as_deref(), Some("MD001"));
    }

    #[test]
    fn test_lint_with_inline_disable() {
        let content = "<!-- rumdl-disable MD001 -->\n## Level 2\n#### Level 4";
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD001HeadingIncrement::default())];

        let result = lint(content, &rules, false, crate::config::MarkdownFlavor::Standard, None);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert!(warnings.is_empty()); // Should be disabled by inline comment
    }

    #[test]
    fn test_lint_rule_filtering() {
        // Content with no lists
        let content = "# Heading\nJust text";
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MD001HeadingIncrement::default()),
            // A list-related rule would be skipped
        ];

        let result = lint(content, &rules, false, crate::config::MarkdownFlavor::Standard, None);
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
    fn test_content_characteristics_edge_cases() {
        // Test setext heading edge case
        let chars = ContentCharacteristics::analyze("-"); // Single dash, not a heading
        assert!(!chars.has_headings);

        let chars = ContentCharacteristics::analyze("--"); // Two dashes, valid setext
        assert!(chars.has_headings);

        // Test list detection - we now include potential list patterns (with or without space)
        // to support user-intention detection in MD030
        let chars = ContentCharacteristics::analyze("*emphasis*"); // Could be list or emphasis
        assert!(chars.has_lists); // Run list rules to be safe

        let chars = ContentCharacteristics::analyze("1.Item"); // Could be list without space
        assert!(chars.has_lists); // Run list rules for user-intention detection

        // Test blockquote must be at start of line
        let chars = ContentCharacteristics::analyze("text > not a quote");
        assert!(!chars.has_blockquotes);
    }
}
