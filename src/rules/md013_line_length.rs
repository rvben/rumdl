/// Rule MD013: Line length
///
/// See [docs/md013.md](../../docs/md013.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::calculate_excess_range;
use lazy_static::lazy_static;
use regex::Regex;
use toml;

mod md013_config;
use md013_config::MD013Config;

lazy_static! {
    static ref URL_PATTERN: Regex = Regex::new(r"^https?://\S+$").unwrap();
    static ref IMAGE_REF_PATTERN: Regex = Regex::new(r"^!\[.*?\]\[.*?\]$" ).unwrap();
    static ref LINK_REF_PATTERN: Regex = Regex::new(r"^\[.*?\]:\s*https?://\S+$").unwrap();

    // Pattern to find URLs anywhere in text
    static ref URL_IN_TEXT: Regex = Regex::new(r"https?://\S+").unwrap();

    // Pattern to find markdown links [text](url) for URL exclusion
    static ref MARKDOWN_LINK_PATTERN: Regex = Regex::new(r"\[([^\]]*)\]\(([^)]+)\)").unwrap();

    // Sentence splitting patterns
    static ref SENTENCE_END: Regex = Regex::new(r"[.!?]\s+[A-Z]").unwrap();
    static ref ABBREVIATION: Regex = Regex::new(r"\b(?:Mr|Mrs|Ms|Dr|Prof|Sr|Jr|vs|etc|i\.e|e\.g|Inc|Corp|Ltd|Co|St|Ave|Blvd|Rd|Ph\.D|M\.D|B\.A|M\.A|Ph\.D|U\.S|U\.K|U\.N|N\.Y|L\.A|D\.C)\.\s+[A-Z]").unwrap();
    static ref DECIMAL_NUMBER: Regex = Regex::new(r"\d+\.\s*\d+").unwrap();
    static ref LIST_ITEM: Regex = Regex::new(r"^\s*\d+\.\s+").unwrap();

    // Link detection patterns
    static ref INLINE_LINK: Regex = Regex::new(r"\[([^\]]*)\]\(([^)]*)\)").unwrap();
    static ref REFERENCE_LINK: Regex = Regex::new(r"\[([^\]]*)\]\[([^\]]*)\]").unwrap();
}

#[derive(Clone, Default)]
pub struct MD013LineLength {
    config: MD013Config,
}

impl MD013LineLength {
    pub fn new(line_length: usize, code_blocks: bool, tables: bool, headings: bool, strict: bool) -> Self {
        Self {
            config: MD013Config {
                line_length,
                code_blocks,
                tables,
                headings,
                strict,
            },
        }
    }

    pub fn from_config_struct(config: MD013Config) -> Self {
        Self { config }
    }

    fn is_in_table(lines: &[&str], current_line: usize) -> bool {
        // Check if current line is part of a table
        let current = lines[current_line].trim();
        if current.starts_with('|') || current.starts_with("|-") {
            return true;
        }

        // Check if line is between table markers
        if current_line > 0 && current_line + 1 < lines.len() {
            let prev = lines[current_line - 1].trim();
            let next = lines[current_line + 1].trim();
            if (prev.starts_with('|') || prev.starts_with("|-")) && (next.starts_with('|') || next.starts_with("|-")) {
                return true;
            }
        }
        false
    }

    fn should_ignore_line(
        &self,
        line: &str,
        _lines: &[&str],
        current_line: usize,
        structure: &DocumentStructure,
    ) -> bool {
        if self.config.strict {
            return false;
        }

        // Quick check for common patterns before expensive regex
        let trimmed = line.trim();

        // Only skip if the entire line is a URL (quick check first)
        if (trimmed.starts_with("http://") || trimmed.starts_with("https://")) && URL_PATTERN.is_match(trimmed) {
            return true;
        }

        // Only skip if the entire line is an image reference (quick check first)
        if trimmed.starts_with("![") && trimmed.ends_with(']') && IMAGE_REF_PATTERN.is_match(trimmed) {
            return true;
        }

        // Only skip if the entire line is a link reference (quick check first)
        if trimmed.starts_with('[') && trimmed.contains("]:") && LINK_REF_PATTERN.is_match(trimmed) {
            return true;
        }

        // Code blocks with long strings (only check if in code block)
        if structure.is_in_code_block(current_line + 1)
            && !trimmed.is_empty()
            && !line.contains(' ')
            && !line.contains('\t')
        {
            return true;
        }

        false
    }
}

impl Rule for MD013LineLength {
    fn name(&self) -> &'static str {
        "MD013"
    }

    fn description(&self) -> &'static str {
        "Line length should not be excessive"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    /// Optimized check using pre-computed document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Early return if no lines could possibly exceed the limit
        if lines.iter().all(|line| line.len() <= self.config.line_length) {
            return Ok(Vec::new());
        }

        // Pre-compute LineIndex for efficient byte range calculations
        let line_index = crate::utils::range_utils::LineIndex::new(content.to_string());

        // Create a quick lookup set for heading lines
        let heading_lines_set: std::collections::HashSet<usize> = structure.heading_lines.iter().cloned().collect();

        // Pre-compute table lines for efficiency instead of calling is_in_table for each line
        let table_lines_set: std::collections::HashSet<usize> = if self.config.tables {
            let mut table_lines = std::collections::HashSet::new();
            let mut in_table = false;
            for (i, line) in lines.iter().enumerate() {
                let line_number = i + 1;
                if line.contains('|') && !structure.is_in_code_block(line_number) {
                    in_table = true;
                    table_lines.insert(line_number);
                } else if in_table && line.trim().is_empty() {
                    in_table = false;
                } else if in_table {
                    table_lines.insert(line_number);
                }
            }
            table_lines
        } else {
            std::collections::HashSet::new()
        };

        for (line_num, line) in lines.iter().enumerate() {
            let line_number = line_num + 1;

            // Calculate effective length excluding unbreakable URLs
            let effective_length = self.calculate_effective_length(line);

            // Skip short lines immediately
            if effective_length <= self.config.line_length {
                continue;
            }

            // Skip various block types efficiently
            if !self.config.strict {
                // Skip setext heading underlines
                if !line.trim().is_empty() && line.trim().chars().all(|c| c == '=' || c == '-') {
                    continue;
                }

                // Skip block elements according to config flags (optimized checks)
                if (self.config.headings && heading_lines_set.contains(&line_number))
                    || (!self.config.code_blocks && structure.is_in_code_block(line_number))
                    || (self.config.tables && table_lines_set.contains(&line_number))
                    || structure.is_in_blockquote(line_number)
                    || structure.is_in_html_block(line_number)
                {
                    continue;
                }

                // Skip lines that are only a URL, image ref, or link ref
                if self.should_ignore_line(line, &lines, line_num, structure) {
                    continue;
                }
            }

            // Generate simplified fix (avoid expensive sentence splitting for now)
            let fix = if !self.should_skip_line_for_fix(line, line_num, structure) {
                // First try trimming trailing whitespace
                let trimmed = line.trim_end();
                if trimmed.len() <= self.config.line_length && trimmed != *line {
                    let line_start = line_index.line_col_to_byte_range(line_number, 1).start;
                    let line_end = if line_number < lines.len() {
                        line_index.line_col_to_byte_range(line_number + 1, 1).start - 1
                    } else {
                        content.len()
                    };
                    Some(crate::rule::Fix {
                        range: line_start..line_end,
                        replacement: trimmed.to_string(),
                    })
                } else {
                    None // Skip expensive sentence splitting for performance
                }
            } else {
                None
            };

            let message = if let Some(ref _fix_obj) = fix {
                format!(
                    "Line length {} exceeds {} characters (can trim whitespace)",
                    effective_length, self.config.line_length
                )
            } else {
                format!(
                    "Line length {} exceeds {} characters",
                    effective_length, self.config.line_length
                )
            };

            // Calculate precise character range for the excess portion
            let (start_line, start_col, end_line, end_col) =
                calculate_excess_range(line_number, line, self.config.line_length);

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                message,
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                severity: Severity::Warning,
                fix,
            });
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Get all warnings with their fixes
        let warnings = self.check(ctx)?;

        // If no warnings, return original content
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Collect all fixes and sort by range start (descending) to apply from end to beginning
        let mut fixes: Vec<_> = warnings
            .iter()
            .filter_map(|w| w.fix.as_ref().map(|f| (f.range.start, f.range.end, &f.replacement)))
            .collect();
        fixes.sort_by(|a, b| b.0.cmp(&a.0));

        // Apply fixes from end to beginning to preserve byte offsets
        let mut result = ctx.content.to_string();
        for (start, end, replacement) in fixes {
            if start < result.len() && end <= result.len() && start <= end {
                result.replace_range(start..end, replacement);
            }
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD013Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD013Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let mut rule_config = crate::rule_config_serde::load_rule_config::<MD013Config>(config);
        // Special handling for line_length from global config
        if rule_config.line_length == 80 {
            // default value
            rule_config.line_length = config.global.line_length as usize;
        }
        Box::new(Self::from_config_struct(rule_config))
    }
}

impl MD013LineLength {
    /// Check if a line should be skipped for fixing
    fn should_skip_line_for_fix(&self, line: &str, line_num: usize, structure: &DocumentStructure) -> bool {
        let line_number = line_num + 1; // 1-based

        // Skip code blocks
        if structure.is_in_code_block(line_number) {
            return true;
        }

        // Skip HTML blocks
        if structure.is_in_html_block(line_number) {
            return true;
        }

        // Skip tables (they have complex formatting)
        if Self::is_in_table(&[line], 0) {
            return true;
        }

        // Skip lines that are only URLs (can't be wrapped)
        if line.trim().starts_with("http://") || line.trim().starts_with("https://") {
            return true;
        }

        // Skip setext heading underlines
        if !line.trim().is_empty() && line.trim().chars().all(|c| c == '=' || c == '-') {
            return true;
        }

        false
    }

    /// Calculate effective line length excluding unbreakable URLs
    fn calculate_effective_length(&self, line: &str) -> usize {
        if self.config.strict {
            // In strict mode, count everything
            return line.chars().count();
        }

        let mut effective_line = line.to_string();

        // First handle markdown links to avoid double-counting URLs
        // Pattern: [text](very-long-url) -> [text](url)
        for cap in MARKDOWN_LINK_PATTERN.captures_iter(&effective_line.clone()) {
            if let (Some(full_match), Some(text), Some(url)) = (cap.get(0), cap.get(1), cap.get(2)) {
                if url.as_str().len() > 15 {
                    let replacement = format!("[{}](url)", text.as_str());
                    effective_line = effective_line.replacen(full_match.as_str(), &replacement, 1);
                }
            }
        }

        // Then replace bare URLs with a placeholder of reasonable length
        // This allows lines with long URLs to pass if the rest of the content is reasonable
        for url_match in URL_IN_TEXT.find_iter(&effective_line.clone()) {
            let url = url_match.as_str();
            // Skip if this URL is already part of a markdown link we handled
            if !effective_line.contains(&format!("({})", url)) {
                // Replace URL with placeholder that represents a "reasonable" URL length
                // Using 15 chars as a reasonable URL placeholder (e.g., "https://ex.com")
                let placeholder = "x".repeat(15.min(url.len()));
                effective_line = effective_line.replacen(url, &placeholder, 1);
            }
        }

        effective_line.chars().count()
    }
}

impl DocumentStructureExtensions for MD013LineLength {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
        // This rule always applies unless content is empty
        !ctx.content.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_default_config() {
        let rule = MD013LineLength::default();
        assert_eq!(rule.config.line_length, 80);
        assert_eq!(rule.config.code_blocks, true);  // Default is true
        assert_eq!(rule.config.tables, true);       // Default is true
        assert_eq!(rule.config.headings, true);
        assert_eq!(rule.config.strict, false);
    }

    #[test]
    fn test_custom_config() {
        let rule = MD013LineLength::new(100, true, true, false, true);
        assert_eq!(rule.config.line_length, 100);
        assert_eq!(rule.config.code_blocks, true);
        assert_eq!(rule.config.tables, true);
        assert_eq!(rule.config.headings, false);
        assert_eq!(rule.config.strict, true);
    }

    #[test]
    fn test_basic_line_length_violation() {
        let rule = MD013LineLength::new(50, false, false, false, false);
        let content = "This is a line that is definitely longer than fifty characters and should trigger a warning.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Line length"));
        assert!(result[0].message.contains("exceeds 50 characters"));
    }

    #[test]
    fn test_no_violation_under_limit() {
        let rule = MD013LineLength::new(100, false, false, false, false);
        let content = "Short line.\nAnother short line.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_multiple_violations() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "This line is definitely longer than thirty chars.\nThis is also a line that exceeds the limit.\nShort line.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
    }

    #[test]
    fn test_code_blocks_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "```\nThis is a very long line inside a code block that should be ignored.\n```";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_code_blocks_not_exempt_when_configured() {
        let rule = MD013LineLength::new(30, true, false, false, false);
        let content = "```\nThis is a very long line inside a code block that should NOT be ignored.\n```";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert!(result.len() > 0);
    }

    #[test]
    fn test_heading_exemption() {
        let rule = MD013LineLength::new(30, false, false, true, false);
        let content = "# This is a very long heading that would normally exceed the limit";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_heading_not_exempt_when_configured() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "# This is a very long heading that should trigger a warning";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_table_detection() {
        let lines = vec![
            "| Column 1 | Column 2 |",
            "|----------|----------|",
            "| Value 1  | Value 2  |"
        ];

        assert!(MD013LineLength::is_in_table(&lines, 0));
        assert!(MD013LineLength::is_in_table(&lines, 1));
        assert!(MD013LineLength::is_in_table(&lines, 2));
    }

    #[test]
    fn test_table_exemption() {
        let rule = MD013LineLength::new(30, false, true, false, false);
        let content = "| This is a very long table header | Another long column header |\n|-----------------------------------|-------------------------------|";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_url_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "https://example.com/this/is/a/very/long/url/that/exceeds/the/limit";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_image_reference_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "![This is a very long image alt text that exceeds limit][reference]";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_link_reference_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "[reference]: https://example.com/very/long/url/that/exceeds/limit";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_strict_mode() {
        let rule = MD013LineLength::new(30, false, false, false, true);
        let content = "https://example.com/this/is/a/very/long/url/that/exceeds/the/limit";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // In strict mode, even URLs trigger warnings
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_blockquote_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "> This is a very long line inside a blockquote that should be ignored.";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_setext_heading_underline_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "Heading\n========================================";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // The underline should be exempt
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_trailing_whitespace_fix() {
        let rule = MD013LineLength::new(60, false, false, false, false);
        let content = "This line has trailing whitespace that makes it too long      ";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        // The line without spaces is 56 chars, within limit of 60
        assert!(result[0].fix.is_some());
        assert!(result[0].message.contains("can trim whitespace"));

        // Apply fix
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed.trim(), "This line has trailing whitespace that makes it too long");
    }

    #[test]
    fn test_character_vs_byte_counting() {
        let rule = MD013LineLength::new(10, false, false, false, false);
        // Unicode characters should count as 1 character each
        let content = "你好世界这是测试文字超过限制"; // 14 characters
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD013LineLength::default();
        let ctx = LintContext::new("");
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_excess_range_calculation() {
        let rule = MD013LineLength::new(10, false, false, false, false);
        let content = "12345678901234567890"; // 20 chars, limit is 10
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        // The warning should highlight from character 11 onwards
        assert_eq!(result[0].column, 11);
        assert_eq!(result[0].end_column, 21);
    }

    #[test]
    fn test_html_block_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "<div>\nThis is a very long line inside an HTML block that should be ignored.\n</div>";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // HTML blocks should be exempt
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_mixed_content() {
        // code_blocks=false, tables=true, headings=true
        let rule = MD013LineLength::new(30, false, true, true, false);
        let content = r#"# This heading is very long but should be exempt

This regular paragraph line is too long and should trigger.

```
Code block line that is very long but exempt.
```

| Table | With very long content |
|-------|------------------------|

Another long line that should trigger a warning."#;

        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should have warnings for the two regular paragraph lines only
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 3);
        assert_eq!(result[1].line, 12);
    }

    #[test]
    fn test_fix_preserves_content() {
        let rule = MD013LineLength::new(50, false, false, false, false);
        let content = "Line 1\nThis line has trailing spaces and is too long      \nLine 3";
        let ctx = LintContext::new(content);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("Line 1"));
        assert!(fixed.contains("Line 3"));
        assert!(!fixed.contains("      \n")); // Trailing spaces removed
    }

    #[test]
    fn test_has_relevant_elements() {
        let rule = MD013LineLength::default();
        let structure = DocumentStructure::new("test");

        let ctx = LintContext::new("Some content");
        assert!(rule.has_relevant_elements(&ctx, &structure));

        let empty_ctx = LintContext::new("");
        assert!(!rule.has_relevant_elements(&empty_ctx, &structure));
    }

    #[test]
    fn test_rule_metadata() {
        let rule = MD013LineLength::default();
        assert_eq!(rule.name(), "MD013");
        assert_eq!(rule.description(), "Line length should not be excessive");
        assert_eq!(rule.category(), RuleCategory::Whitespace);
    }

    #[test]
    fn test_url_embedded_in_text() {
        let rule = MD013LineLength::new(50, false, false, false, false);

        // This line would be 85 chars, but only ~45 without the URL
        let content = "Check the docs at https://example.com/very/long/url/that/exceeds/limit for info";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should not flag because effective length (with URL placeholder) is under 50
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_multiple_urls_in_line() {
        let rule = MD013LineLength::new(50, false, false, false, false);

        // Line with multiple URLs
        let content = "See https://first-url.com/long and https://second-url.com/also/very/long here";
        let ctx = LintContext::new(content);

        let result = rule.check(&ctx).unwrap();

        // Should not flag because effective length is reasonable
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_markdown_link_with_long_url() {
        let rule = MD013LineLength::new(50, false, false, false, false);

        // Markdown link with very long URL
        let content = "Check the [documentation](https://example.com/very/long/path/to/documentation/page) for details";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should not flag because effective length counts link as short
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_line_too_long_even_without_urls() {
        let rule = MD013LineLength::new(50, false, false, false, false);

        // Line that's too long even after URL exclusion
        let content = "This is a very long line with lots of text and https://url.com that still exceeds the limit";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should flag because even with URL placeholder, line is too long
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_strict_mode_counts_urls() {
        let rule = MD013LineLength::new(50, false, false, false, true); // strict=true

        // Same line that passes in non-strict mode
        let content = "Check the docs at https://example.com/very/long/url/that/exceeds/limit for info";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // In strict mode, should flag because full URL is counted
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_documentation_example_from_md051() {
        let rule = MD013LineLength::new(80, false, false, false, false);

        // This is the actual line from md051.md that was causing issues
        let content = r#"For more information, see the [CommonMark specification](https://spec.commonmark.org/0.30/#link-reference-definitions)."#;
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should not flag because the URL is in a markdown link
        assert_eq!(result.len(), 0);
    }
}
