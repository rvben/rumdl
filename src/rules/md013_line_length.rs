/// Rule MD013: Line length
///
/// See [docs/md013.md](../../docs/md013.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::calculate_excess_range;
use crate::utils::regex_cache::{
    IMAGE_REF_PATTERN, INLINE_LINK_REGEX as MARKDOWN_LINK_PATTERN, LINK_REF_PATTERN, URL_IN_TEXT, URL_PATTERN,
};
use toml;

pub mod md013_config;
use md013_config::MD013Config;

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
                heading_line_length: None,
                code_block_line_length: None,
                stern: false,
                enable_reflow: false,
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
        if self.config.strict || self.config.stern {
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

        // Quick check: if total content is shorter than line limit, definitely no violations
        if content.len() <= self.config.line_length {
            return Ok(Vec::new());
        }

        // More aggressive early return - check if any line could possibly be long
        let has_long_lines = if !ctx.lines.is_empty() {
            ctx.lines
                .iter()
                .any(|line| line.content.len() > self.config.line_length)
        } else {
            // Fallback: do a quick scan for newlines to estimate max line length
            let mut max_line_len = 0;
            let mut current_line_len = 0;
            for ch in content.chars() {
                if ch == '\n' {
                    max_line_len = max_line_len.max(current_line_len);
                    current_line_len = 0;
                } else {
                    current_line_len += 1;
                }
            }
            max_line_len = max_line_len.max(current_line_len);
            max_line_len > self.config.line_length
        };

        if !has_long_lines {
            return Ok(Vec::new());
        }

        // Create structure manually
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

        // Early return was already done in check(), so we know there are long lines

        // Check for inline configuration overrides
        let inline_config = crate::inline_config::InlineConfig::from_content(content);
        let config_override = inline_config.get_rule_config("MD013");

        // Apply configuration override if present
        let effective_config = if let Some(json_config) = config_override {
            if let Some(obj) = json_config.as_object() {
                let mut config = self.config.clone();
                if let Some(line_length) = obj.get("line_length").and_then(|v| v.as_u64()) {
                    config.line_length = line_length as usize;
                }
                if let Some(code_blocks) = obj.get("code_blocks").and_then(|v| v.as_bool()) {
                    config.code_blocks = code_blocks;
                }
                if let Some(tables) = obj.get("tables").and_then(|v| v.as_bool()) {
                    config.tables = tables;
                }
                if let Some(headings) = obj.get("headings").and_then(|v| v.as_bool()) {
                    config.headings = headings;
                }
                if let Some(strict) = obj.get("strict").and_then(|v| v.as_bool()) {
                    config.strict = strict;
                }
                if let Some(stern) = obj.get("stern").and_then(|v| v.as_bool()) {
                    config.stern = stern;
                }
                if let Some(heading_line_length) = obj.get("heading_line_length").and_then(|v| v.as_u64()) {
                    config.heading_line_length = Some(heading_line_length as usize);
                }
                if let Some(code_block_line_length) = obj.get("code_block_line_length").and_then(|v| v.as_u64()) {
                    config.code_block_line_length = Some(code_block_line_length as usize);
                }
                config
            } else {
                self.config.clone()
            }
        } else {
            self.config.clone()
        };

        // Use ctx.lines if available for better performance
        let lines: Vec<&str> = if !ctx.lines.is_empty() {
            ctx.lines.iter().map(|l| l.content.as_str()).collect()
        } else {
            content.lines().collect()
        };

        // Create a quick lookup set for heading lines
        let heading_lines_set: std::collections::HashSet<usize> = structure.heading_lines.iter().cloned().collect();

        // Pre-compute table lines for efficiency instead of calling is_in_table for each line
        let table_lines_set: std::collections::HashSet<usize> = if effective_config.tables {
            let mut table_lines = std::collections::HashSet::new();
            let mut in_table = false;

            for (i, line) in lines.iter().enumerate() {
                let line_number = i + 1;

                // Quick check if in code block using pre-computed blocks from context or structure
                let in_code = if !ctx.code_blocks.is_empty() {
                    ctx.code_blocks
                        .iter()
                        .any(|(start, end)| *start <= line_number && line_number <= *end)
                } else {
                    structure.is_in_code_block(line_number)
                };

                if !in_code && line.contains('|') {
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

            // Determine the appropriate line length limit based on line type
            let line_limit = if heading_lines_set.contains(&line_number) {
                effective_config
                    .heading_line_length
                    .unwrap_or(effective_config.line_length)
            } else if structure.is_in_code_block(line_number) {
                effective_config
                    .code_block_line_length
                    .unwrap_or(effective_config.line_length)
            } else {
                effective_config.line_length
            };

            // Skip short lines immediately
            if effective_length <= line_limit {
                continue;
            }

            // Skip various block types efficiently
            if !effective_config.strict && !effective_config.stern {
                // Skip setext heading underlines
                if !line.trim().is_empty() && line.trim().chars().all(|c| c == '=' || c == '-') {
                    continue;
                }

                // Skip block elements according to config flags (optimized checks)
                if (effective_config.headings
                    && heading_lines_set.contains(&line_number)
                    && effective_config.heading_line_length.is_none())
                    || (!effective_config.code_blocks
                        && structure.is_in_code_block(line_number)
                        && effective_config.code_block_line_length.is_none())
                    || (effective_config.tables && table_lines_set.contains(&line_number))
                    || structure.is_in_blockquote(line_number)
                    || structure.is_in_html_block(line_number)
                {
                    continue;
                }

                // Skip lines that are only a URL, image ref, or link ref
                if self.should_ignore_line(line, &lines, line_num, structure) {
                    continue;
                }
            } else if effective_config.stern {
                // In stern mode, only skip if explicitly configured
                if (effective_config.headings
                    && heading_lines_set.contains(&line_number)
                    && effective_config.heading_line_length.is_none())
                    || (!effective_config.code_blocks
                        && structure.is_in_code_block(line_number)
                        && effective_config.code_block_line_length.is_none())
                    || (effective_config.tables && table_lines_set.contains(&line_number))
                {
                    continue;
                }
            }

            // Only provide a fix if reflow is enabled
            let fix = if self.config.enable_reflow && !self.should_skip_line_for_fix(line, line_num, structure) {
                // Provide a placeholder fix to indicate that reflow will happen
                // The actual reflow is done in the fix() method
                Some(crate::rule::Fix {
                    range: 0..0,                // Placeholder range
                    replacement: String::new(), // Placeholder replacement
                })
            } else {
                None
            };

            let message = format!("Line length {effective_length} exceeds {line_limit} characters");

            // Calculate precise character range for the excess portion
            let (start_line, start_col, end_line, end_col) = calculate_excess_range(line_number, line, line_limit);

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
        // Only fix if reflow is enabled
        if self.config.enable_reflow {
            let reflow_options = crate::utils::text_reflow::ReflowOptions {
                line_length: self.config.line_length,
                break_on_sentences: true,
                preserve_breaks: false,
            };

            return Ok(crate::utils::text_reflow::reflow_markdown(ctx.content, &reflow_options));
        }

        // Without reflow, MD013 has no fixes available
        Ok(ctx.content.to_string())
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

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty
        if ctx.content.is_empty() {
            return true;
        }

        // Quick check: if total content is shorter than line limit, definitely skip
        if ctx.content.len() <= self.config.line_length {
            return true;
        }

        // Use more efficient check - any() with early termination instead of all()
        !ctx.lines
            .iter()
            .any(|line| line.content.len() > self.config.line_length)
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
        if self.config.strict || self.config.stern {
            // In strict or stern mode, count everything
            return line.chars().count();
        }

        // Quick check: if line doesn't contain "http" or "[", it can't have URLs or markdown links
        if !line.contains("http") && !line.contains('[') {
            return line.chars().count();
        }

        let mut effective_line = line.to_string();

        // First handle markdown links to avoid double-counting URLs
        // Pattern: [text](very-long-url) -> [text](url)
        if line.contains('[') && line.contains("](") {
            for cap in MARKDOWN_LINK_PATTERN.captures_iter(&effective_line.clone()) {
                if let (Some(full_match), Some(text), Some(url)) = (cap.get(0), cap.get(1), cap.get(2))
                    && url.as_str().len() > 15
                {
                    let replacement = format!("[{}](url)", text.as_str());
                    effective_line = effective_line.replacen(full_match.as_str(), &replacement, 1);
                }
            }
        }

        // Then replace bare URLs with a placeholder of reasonable length
        // This allows lines with long URLs to pass if the rest of the content is reasonable
        if effective_line.contains("http") {
            for url_match in URL_IN_TEXT.find_iter(&effective_line.clone()) {
                let url = url_match.as_str();
                // Skip if this URL is already part of a markdown link we handled
                if !effective_line.contains(&format!("({url})")) {
                    // Replace URL with placeholder that represents a "reasonable" URL length
                    // Using 15 chars as a reasonable URL placeholder (e.g., "https://ex.com")
                    let placeholder = "x".repeat(15.min(url.len()));
                    effective_line = effective_line.replacen(url, &placeholder, 1);
                }
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
        assert!(rule.config.code_blocks); // Default is true
        assert!(rule.config.tables); // Default is true
        assert!(rule.config.headings);
        assert!(!rule.config.strict);
    }

    #[test]
    fn test_custom_config() {
        let rule = MD013LineLength::new(100, true, true, false, true);
        assert_eq!(rule.config.line_length, 100);
        assert!(rule.config.code_blocks);
        assert!(rule.config.tables);
        assert!(!rule.config.headings);
        assert!(rule.config.strict);
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

        assert!(!result.is_empty());
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
            "| Value 1  | Value 2  |",
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
    fn test_no_fix_without_reflow() {
        let rule = MD013LineLength::new(60, false, false, false, false);
        let content = "This line has trailing whitespace that makes it too long      ";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        // Without enable_reflow, no fix is provided
        assert!(result[0].fix.is_none());

        // Fix method returns content unchanged
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
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
    fn test_fix_without_reflow_preserves_content() {
        let rule = MD013LineLength::new(50, false, false, false, false);
        let content = "Line 1\nThis line has trailing spaces and is too long      \nLine 3";
        let ctx = LintContext::new(content);

        // Without enable_reflow, content is unchanged
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
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

    #[test]
    fn test_text_reflow_simple() {
        let config = MD013Config {
            line_length: 30,
            enable_reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This is a very long line that definitely exceeds thirty characters and needs to be wrapped.";
        let ctx = LintContext::new(content);

        let fixed = rule.fix(&ctx).unwrap();

        // Verify all lines are under 30 chars
        for line in fixed.lines() {
            assert!(
                line.chars().count() <= 30,
                "Line too long: {} (len={})",
                line,
                line.chars().count()
            );
        }

        // Verify content is preserved
        let fixed_words: Vec<&str> = fixed.split_whitespace().collect();
        let original_words: Vec<&str> = content.split_whitespace().collect();
        assert_eq!(fixed_words, original_words);
    }

    #[test]
    fn test_text_reflow_preserves_markdown_elements() {
        let config = MD013Config {
            line_length: 40,
            enable_reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This paragraph has **bold text** and *italic text* and [a link](https://example.com) that should be preserved.";
        let ctx = LintContext::new(content);

        let fixed = rule.fix(&ctx).unwrap();

        // Verify markdown elements are preserved
        assert!(fixed.contains("**bold text**"), "Bold text not preserved in: {fixed}");
        assert!(fixed.contains("*italic text*"), "Italic text not preserved in: {fixed}");
        assert!(
            fixed.contains("[a link](https://example.com)"),
            "Link not preserved in: {fixed}"
        );

        // Verify all lines are under 40 chars
        for line in fixed.lines() {
            assert!(line.len() <= 40, "Line too long: {line}");
        }
    }

    #[test]
    fn test_text_reflow_preserves_code_blocks() {
        let config = MD013Config {
            line_length: 30,
            enable_reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = r#"Here is some text.

```python
def very_long_function_name_that_exceeds_limit():
    return "This should not be wrapped"
```

More text after code block."#;
        let ctx = LintContext::new(content);

        let fixed = rule.fix(&ctx).unwrap();

        // Verify code block is preserved
        assert!(fixed.contains("def very_long_function_name_that_exceeds_limit():"));
        assert!(fixed.contains("```python"));
        assert!(fixed.contains("```"));
    }

    #[test]
    fn test_text_reflow_preserves_lists() {
        let config = MD013Config {
            line_length: 30,
            enable_reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = r#"Here is a list:

1. First item with a very long line that needs wrapping
2. Second item is short
3. Third item also has a long line that exceeds the limit

And a bullet list:

- Bullet item with very long content that needs wrapping
- Short bullet"#;
        let ctx = LintContext::new(content);

        let fixed = rule.fix(&ctx).unwrap();

        // Verify list structure is preserved
        assert!(fixed.contains("1. "));
        assert!(fixed.contains("2. "));
        assert!(fixed.contains("3. "));
        assert!(fixed.contains("- "));

        // Verify proper indentation for wrapped lines
        let lines: Vec<&str> = fixed.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.trim().starts_with("1.") || line.trim().starts_with("2.") || line.trim().starts_with("3.") {
                // Check if next line is a continuation (should be indented with 3 spaces for numbered lists)
                if i + 1 < lines.len()
                    && !lines[i + 1].trim().is_empty()
                    && !lines[i + 1].trim().starts_with(char::is_numeric)
                    && !lines[i + 1].trim().starts_with("-")
                {
                    // Numbered list continuation lines should have 3 spaces
                    assert!(lines[i + 1].starts_with("   ") || lines[i + 1].trim().is_empty());
                }
            } else if line.trim().starts_with("-") {
                // Check if next line is a continuation (should be indented with 2 spaces for dash lists)
                if i + 1 < lines.len()
                    && !lines[i + 1].trim().is_empty()
                    && !lines[i + 1].trim().starts_with(char::is_numeric)
                    && !lines[i + 1].trim().starts_with("-")
                {
                    // Dash list continuation lines should have 2 spaces
                    assert!(lines[i + 1].starts_with("  ") || lines[i + 1].trim().is_empty());
                }
            }
        }
    }

    #[test]
    fn test_text_reflow_disabled_by_default() {
        let rule = MD013LineLength::new(30, false, false, false, false);

        let content = "This is a very long line that definitely exceeds thirty characters.";
        let ctx = LintContext::new(content);

        let fixed = rule.fix(&ctx).unwrap();

        // Without reflow enabled, it should only trim whitespace (if any)
        // Since there's no trailing whitespace, content should be unchanged
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_reflow_with_hard_line_breaks() {
        // Test that lines with exactly 2 trailing spaces are preserved as hard breaks
        let config = MD013Config {
            line_length: 40,
            enable_reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Test with exactly 2 spaces (hard line break)
        let content = "This line has a hard break at the end  \nAnd this continues on the next line that is also quite long and needs wrapping";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Should preserve the hard line break (2 spaces)
        assert!(
            fixed.contains("  \n"),
            "Hard line break with exactly 2 spaces should be preserved"
        );
    }

    #[test]
    fn test_reflow_preserves_reference_links() {
        let config = MD013Config {
            line_length: 40,
            enable_reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This is a very long line with a [reference link][ref] that should not be broken apart when reflowing the text.

[ref]: https://example.com";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Reference link should remain intact
        assert!(fixed.contains("[reference link][ref]"));
        assert!(!fixed.contains("[ reference link]"));
        assert!(!fixed.contains("[ref ]"));
    }

    #[test]
    fn test_reflow_with_nested_markdown_elements() {
        let config = MD013Config {
            line_length: 35,
            enable_reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This text has **bold with `code` inside** and should handle it properly when wrapping";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Nested elements should be preserved
        assert!(fixed.contains("**bold with `code` inside**"));
    }

    #[test]
    fn test_reflow_with_unbalanced_markdown() {
        // Test edge case with unbalanced markdown
        let config = MD013Config {
            line_length: 30,
            enable_reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This has **unbalanced bold that goes on for a very long time without closing";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Should handle gracefully without panic
        // The text reflow handles unbalanced markdown by treating it as a bold element
        // Check that the content is properly reflowed without panic
        assert!(!fixed.is_empty());
        // Verify the content is wrapped to 30 chars
        for line in fixed.lines() {
            assert!(line.len() <= 30 || line.starts_with("**"), "Line exceeds limit: {line}");
        }
    }

    #[test]
    fn test_reflow_fix_indicator() {
        // Test that enable_reflow provides fix indicators
        let config = MD013Config {
            line_length: 30,
            enable_reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This is a very long line that definitely exceeds the thirty character limit";
        let ctx = LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();

        // Should have a fix indicator when enable_reflow is true
        assert!(!warnings.is_empty());
        assert!(
            warnings[0].fix.is_some(),
            "Should provide fix indicator when enable_reflow is true"
        );
    }

    #[test]
    fn test_no_fix_indicator_without_reflow() {
        // Test that without enable_reflow, no fix is provided
        let config = MD013Config {
            line_length: 30,
            enable_reflow: false,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This is a very long line that definitely exceeds the thirty character limit";
        let ctx = LintContext::new(content);
        let warnings = rule.check(&ctx).unwrap();

        // Should NOT have a fix indicator when enable_reflow is false
        assert!(!warnings.is_empty());
        assert!(
            warnings[0].fix.is_none(),
            "Should not provide fix when enable_reflow is false"
        );
    }

    #[test]
    fn test_reflow_preserves_all_reference_link_types() {
        let config = MD013Config {
            line_length: 40,
            enable_reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "Test [full reference][ref] and [collapsed][] and [shortcut] reference links in a very long line.

[ref]: https://example.com
[collapsed]: https://example.com
[shortcut]: https://example.com";

        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // All reference link types should be preserved
        assert!(fixed.contains("[full reference][ref]"));
        assert!(fixed.contains("[collapsed][]"));
        assert!(fixed.contains("[shortcut]"));
    }

    #[test]
    fn test_reflow_handles_images_correctly() {
        let config = MD013Config {
            line_length: 40,
            enable_reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This line has an ![image alt text](https://example.com/image.png) that should not be broken when reflowing.";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Image should remain intact
        assert!(fixed.contains("![image alt text](https://example.com/image.png)"));
    }
}
