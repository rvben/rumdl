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
use crate::utils::table_utils::TableUtils;
use toml;

pub mod md013_config;
use md013_config::{MD013Config, ReflowMode};

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
                reflow: false,
                reflow_mode: ReflowMode::default(),
            },
        }
    }

    pub fn from_config_struct(config: MD013Config) -> Self {
        Self { config }
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

        // Quick check: if total content is shorter than line limit, definitely no violations
        // BUT: in normalize mode with reflow, we still want to check for multi-line paragraphs
        if content.len() <= self.config.line_length
            && !(self.config.reflow && self.config.reflow_mode == ReflowMode::Normalize)
        {
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

        // In normalize mode, we want to continue even if no long lines
        if !(has_long_lines || self.config.reflow && self.config.reflow_mode == ReflowMode::Normalize) {
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
                if let Some(reflow) = obj.get("reflow").and_then(|v| v.as_bool()) {
                    config.reflow = reflow;
                }
                if let Some(reflow_mode) = obj.get("reflow_mode").and_then(|v| v.as_str()) {
                    config.reflow_mode = match reflow_mode {
                        "default" => ReflowMode::Default,
                        "normalize" => ReflowMode::Normalize,
                        _ => ReflowMode::default(),
                    };
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

        // Use TableUtils to find all table blocks in the document
        let table_blocks = TableUtils::find_table_blocks(content, ctx);

        // Pre-compute table lines from the table blocks
        let table_lines_set: std::collections::HashSet<usize> = {
            let mut table_lines = std::collections::HashSet::new();

            for table in &table_blocks {
                // Add header line
                table_lines.insert(table.header_line + 1); // Convert 0-indexed to 1-indexed
                // Add delimiter line
                table_lines.insert(table.delimiter_line + 1);
                // Add all content lines
                for &line in &table.content_lines {
                    table_lines.insert(line + 1); // Convert 0-indexed to 1-indexed
                }
            }
            table_lines
        };

        for (line_num, line) in lines.iter().enumerate() {
            let line_number = line_num + 1;

            // Calculate effective length excluding unbreakable URLs
            let effective_length = self.calculate_effective_length(line);

            // Use single line length limit for all content
            let line_limit = effective_config.line_length;

            // Skip short lines immediately
            if effective_length <= line_limit {
                continue;
            }

            // Skip various block types efficiently
            if !effective_config.strict {
                // Skip setext heading underlines
                if !line.trim().is_empty() && line.trim().chars().all(|c| c == '=' || c == '-') {
                    continue;
                }

                // Skip block elements according to config flags
                // The flags mean: true = check these elements, false = skip these elements
                // So we skip when the flag is FALSE and the line is in that element type
                if (!effective_config.headings && heading_lines_set.contains(&line_number))
                    || (!effective_config.code_blocks && structure.is_in_code_block(line_number))
                    || (!effective_config.tables && table_lines_set.contains(&line_number))
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

            // Only provide a fix if reflow is enabled
            let fix = if effective_config.reflow && !self.should_skip_line_for_fix(line, line_num, structure) {
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

        // In normalize mode with reflow enabled, also flag paragraphs that could be better reflowed
        if effective_config.reflow && effective_config.reflow_mode == ReflowMode::Normalize {
            // Find paragraph blocks that could benefit from normalization
            let normalize_warnings = self.find_normalizable_paragraphs(ctx, structure, &effective_config);
            for warning in normalize_warnings {
                if !warnings.iter().any(|w| w.line == warning.line) {
                    warnings.push(warning);
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Check for inline configuration overrides (same as in check())
        let inline_config = crate::inline_config::InlineConfig::from_content(ctx.content);
        let config_override = inline_config.get_rule_config("MD013");

        // Apply configuration override if present
        let effective_config = if let Some(json_config) = config_override {
            if let Some(obj) = json_config.as_object() {
                let mut config = self.config.clone();
                if let Some(line_length) = obj.get("line_length").and_then(|v| v.as_u64()) {
                    config.line_length = line_length as usize;
                }
                if let Some(reflow) = obj.get("reflow").and_then(|v| v.as_bool()) {
                    config.reflow = reflow;
                }
                if let Some(reflow_mode) = obj.get("reflow_mode").and_then(|v| v.as_str()) {
                    config.reflow_mode = match reflow_mode {
                        "default" => ReflowMode::Default,
                        "normalize" => ReflowMode::Normalize,
                        _ => ReflowMode::default(),
                    };
                }
                config
            } else {
                self.config.clone()
            }
        } else {
            self.config.clone()
        };

        // Only fix if reflow is enabled
        if effective_config.reflow {
            // In default mode, only reflow if there are actual violations
            if effective_config.reflow_mode == ReflowMode::Default {
                // Check if there are any violations that need fixing
                let warnings = self.check(ctx)?;
                if warnings.is_empty() {
                    // No violations, don't change anything
                    return Ok(ctx.content.to_string());
                }
            }

            // In normalize mode, set preserve_breaks to false to allow combining short lines
            let preserve_breaks = effective_config.reflow_mode != ReflowMode::Normalize;

            let reflow_options = crate::utils::text_reflow::ReflowOptions {
                line_length: effective_config.line_length,
                break_on_sentences: true,
                preserve_breaks,
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

    fn config_aliases(&self) -> Option<std::collections::HashMap<String, String>> {
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("enable_reflow".to_string(), "reflow".to_string());
        Some(aliases)
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
    /// Find paragraphs that could benefit from normalization
    fn find_normalizable_paragraphs(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
        config: &MD013Config,
    ) -> Vec<LintWarning> {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = if !ctx.lines.is_empty() {
            ctx.lines.iter().map(|l| l.content.as_str()).collect()
        } else {
            ctx.content.lines().collect()
        };

        let mut i = 0;
        while i < lines.len() {
            let line_num = i + 1;

            // Skip if in code block, table, or other special structures
            if structure.is_in_code_block(line_num)
                || structure.is_in_html_block(line_num)
                || structure.is_in_blockquote(line_num)
                || TableUtils::is_potential_table_row(lines[i])
            {
                i += 1;
                continue;
            }

            // Skip headings
            if lines[i].trim().starts_with('#') || structure.heading_lines.contains(&line_num) {
                i += 1;
                continue;
            }

            // Skip empty lines
            if lines[i].trim().is_empty() {
                i += 1;
                continue;
            }

            // Check if this is the start of a paragraph that could be normalized
            let mut paragraph_end = i;
            while paragraph_end < lines.len() {
                let next_line_num = paragraph_end + 1;

                // Stop at empty lines
                if paragraph_end >= lines.len() || lines[paragraph_end].trim().is_empty() {
                    break;
                }

                // Stop at special structures
                if structure.is_in_code_block(next_line_num)
                    || structure.is_in_html_block(next_line_num)
                    || structure.is_in_blockquote(next_line_num)
                    || (lines[paragraph_end].trim().starts_with('#'))
                    || TableUtils::is_potential_table_row(lines[paragraph_end])
                {
                    break;
                }

                paragraph_end += 1;
            }

            // Check if paragraph has multiple lines that could be combined
            if paragraph_end - i > 1 {
                // Multiple lines in paragraph - always flag in normalize mode
                // (user explicitly wants to normalize paragraphs to use full line length)
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!(
                        "Paragraph could be normalized to use line length of {} characters",
                        config.line_length
                    ),
                    line: line_num,
                    column: 1,
                    end_line: paragraph_end,
                    end_column: lines[paragraph_end - 1].len() + 1,
                    severity: Severity::Warning,
                    fix: Some(crate::rule::Fix {
                        range: 0..0,
                        replacement: String::new(),
                    }),
                });
            }

            i = paragraph_end;
        }

        warnings
    }

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
        // Check if line looks like a table row
        if TableUtils::is_potential_table_row(line) {
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
        assert!(rule.config.headings); // Default is true
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Line length"));
        assert!(result[0].message.contains("exceeds 50 characters"));
    }

    #[test]
    fn test_no_violation_under_limit() {
        let rule = MD013LineLength::new(100, false, false, false, false);
        let content = "Short line.\nAnother short line.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_multiple_violations() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "This line is definitely longer than thirty chars.\nThis is also a line that exceeds the limit.\nShort line.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
    }

    #[test]
    fn test_code_blocks_exemption() {
        // With code_blocks = false, code blocks should be skipped
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "```\nThis is a very long line inside a code block that should be ignored.\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_code_blocks_not_exempt_when_configured() {
        // With code_blocks = true, code blocks should be checked
        let rule = MD013LineLength::new(30, true, false, false, false);
        let content = "```\nThis is a very long line inside a code block that should NOT be ignored.\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty());
    }

    #[test]
    fn test_heading_checked_when_enabled() {
        let rule = MD013LineLength::new(30, false, false, true, false);
        let content = "# This is a very long heading that would normally exceed the limit";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_heading_exempt_when_disabled() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "# This is a very long heading that should trigger a warning";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_table_checked_when_enabled() {
        let rule = MD013LineLength::new(30, false, true, false, false);
        let content = "| This is a very long table header | Another long column header |\n|-----------------------------------|-------------------------------|";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2); // Both table lines exceed limit
    }

    #[test]
    fn test_issue_78_tables_after_fenced_code_blocks() {
        // Test for GitHub issue #78 - tables with tables=false after fenced code blocks
        let rule = MD013LineLength::new(20, false, false, false, false); // tables=false
        let content = r#"# heading

```plain
some code block longer than 20 chars length
```

this is a very long line

| column A | column B |
| -------- | -------- |
| `var` | `val` |
| value 1 | value 2 |

correct length line"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should only flag line 7 ("this is a very long line"), not the table lines
        assert_eq!(result.len(), 1, "Should only flag 1 line (the non-table long line)");
        assert_eq!(result[0].line, 7, "Should flag line 7");
        assert!(result[0].message.contains("24 exceeds 20"));
    }

    #[test]
    fn test_issue_78_tables_with_inline_code() {
        // Test that tables with inline code (backticks) are properly detected as tables
        let rule = MD013LineLength::new(20, false, false, false, false); // tables=false
        let content = r#"| column A | column B |
| -------- | -------- |
| `var with very long name` | `val exceeding limit` |
| value 1 | value 2 |

This line exceeds limit"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should only flag the last line, not the table lines
        assert_eq!(result.len(), 1, "Should only flag the non-table line");
        assert_eq!(result[0].line, 6, "Should flag line 6");
    }

    #[test]
    fn test_issue_78_indented_code_blocks() {
        // Test with indented code blocks instead of fenced
        let rule = MD013LineLength::new(20, false, false, false, false); // tables=false
        let content = r#"# heading

    some code block longer than 20 chars length

this is a very long line

| column A | column B |
| -------- | -------- |
| value 1 | value 2 |

correct length line"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should only flag line 5 ("this is a very long line"), not the table lines
        assert_eq!(result.len(), 1, "Should only flag 1 line (the non-table long line)");
        assert_eq!(result[0].line, 5, "Should flag line 5");
    }

    #[test]
    fn test_url_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "https://example.com/this/is/a/very/long/url/that/exceeds/the/limit";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_image_reference_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "![This is a very long image alt text that exceeds limit][reference]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_link_reference_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "[reference]: https://example.com/very/long/url/that/exceeds/limit";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_strict_mode() {
        let rule = MD013LineLength::new(30, false, false, false, true);
        let content = "https://example.com/this/is/a/very/long/url/that/exceeds/the/limit";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // In strict mode, even URLs trigger warnings
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_blockquote_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "> This is a very long line inside a blockquote that should be ignored.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_setext_heading_underline_exemption() {
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = "Heading\n========================================";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // The underline should be exempt
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_no_fix_without_reflow() {
        let rule = MD013LineLength::new(60, false, false, false, false);
        let content = "This line has trailing whitespace that makes it too long      ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        // Without reflow, no fix is provided
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD013LineLength::default();
        let ctx = LintContext::new("", crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_excess_range_calculation() {
        let rule = MD013LineLength::new(10, false, false, false, false);
        let content = "12345678901234567890"; // 20 chars, limit is 10
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // HTML blocks should be exempt
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_mixed_content() {
        // code_blocks=false, tables=false, headings=false (all skipped/exempt)
        let rule = MD013LineLength::new(30, false, false, false, false);
        let content = r#"# This heading is very long but should be exempt

This regular paragraph line is too long and should trigger.

```
Code block line that is very long but exempt.
```

| Table | With very long content |
|-------|------------------------|

Another long line that should trigger a warning."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        // Without reflow, content is unchanged
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_has_relevant_elements() {
        let rule = MD013LineLength::default();
        let structure = DocumentStructure::new("test");

        let ctx = LintContext::new("Some content", crate::config::MarkdownFlavor::Standard);
        assert!(rule.has_relevant_elements(&ctx, &structure));

        let empty_ctx = LintContext::new("", crate::config::MarkdownFlavor::Standard);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should not flag because effective length (with URL placeholder) is under 50
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_multiple_urls_in_line() {
        let rule = MD013LineLength::new(50, false, false, false, false);

        // Line with multiple URLs
        let content = "See https://first-url.com/long and https://second-url.com/also/very/long here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let result = rule.check(&ctx).unwrap();

        // Should not flag because effective length is reasonable
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_markdown_link_with_long_url() {
        let rule = MD013LineLength::new(50, false, false, false, false);

        // Markdown link with very long URL
        let content = "Check the [documentation](https://example.com/very/long/path/to/documentation/page) for details";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should not flag because effective length counts link as short
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_line_too_long_even_without_urls() {
        let rule = MD013LineLength::new(50, false, false, false, false);

        // Line that's too long even after URL exclusion
        let content = "This is a very long line with lots of text and https://url.com that still exceeds the limit";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should flag because even with URL placeholder, line is too long
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_strict_mode_counts_urls() {
        let rule = MD013LineLength::new(50, false, false, false, true); // strict=true

        // Same line that passes in non-strict mode
        let content = "Check the docs at https://example.com/very/long/url/that/exceeds/limit for info";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // In strict mode, should flag because full URL is counted
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_documentation_example_from_md051() {
        let rule = MD013LineLength::new(80, false, false, false, false);

        // This is the actual line from md051.md that was causing issues
        let content = r#"For more information, see the [CommonMark specification](https://spec.commonmark.org/0.30/#link-reference-definitions)."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should not flag because the URL is in a markdown link
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_text_reflow_simple() {
        let config = MD013Config {
            line_length: 30,
            reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This is a very long line that definitely exceeds thirty characters and needs to be wrapped.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

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
            reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This paragraph has **bold text** and *italic text* and [a link](https://example.com) that should be preserved.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

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
            reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = r#"Here is some text.

```python
def very_long_function_name_that_exceeds_limit():
    return "This should not be wrapped"
```

More text after code block."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

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
            reflow: true,
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

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
    fn test_issue_83_numbered_list_with_backticks() {
        // Test for issue #83: enable_reflow was incorrectly handling numbered lists
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // The exact case from issue #83
        let content = "1. List `manifest` to find the manifest with the largest ID. Say it's `00000000000000000002.manifest` in this example.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();

        // The expected output: properly wrapped at 100 chars with correct list formatting
        // After the fix, it correctly accounts for "1. " (3 chars) leaving 97 for content
        let expected = "1. List `manifest` to find the manifest with the largest ID. Say it's\n   `00000000000000000002.manifest` in this example.";

        assert_eq!(
            fixed, expected,
            "List should be properly reflowed with correct marker and indentation.\nExpected:\n{expected}\nGot:\n{fixed}"
        );
    }

    #[test]
    fn test_text_reflow_disabled_by_default() {
        let rule = MD013LineLength::new(30, false, false, false, false);

        let content = "This is a very long line that definitely exceeds thirty characters.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

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
            reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Test with exactly 2 spaces (hard line break)
        let content = "This line has a hard break at the end  \nAnd this continues on the next line that is also quite long and needs wrapping";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
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
            reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This is a very long line with a [reference link][ref] that should not be broken apart when reflowing the text.

[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
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
            reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This text has **bold with `code` inside** and should handle it properly when wrapping";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        // Nested elements should be preserved
        assert!(fixed.contains("**bold with `code` inside**"));
    }

    #[test]
    fn test_reflow_with_unbalanced_markdown() {
        // Test edge case with unbalanced markdown
        let config = MD013Config {
            line_length: 30,
            reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This has **unbalanced bold that goes on for a very long time without closing";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
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
        // Test that reflow provides fix indicators
        let config = MD013Config {
            line_length: 30,
            reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This is a very long line that definitely exceeds the thirty character limit";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();

        // Should have a fix indicator when reflow is true
        assert!(!warnings.is_empty());
        assert!(
            warnings[0].fix.is_some(),
            "Should provide fix indicator when reflow is true"
        );
    }

    #[test]
    fn test_no_fix_indicator_without_reflow() {
        // Test that without reflow, no fix is provided
        let config = MD013Config {
            line_length: 30,
            reflow: false,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This is a very long line that definitely exceeds the thirty character limit";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();

        // Should NOT have a fix indicator when reflow is false
        assert!(!warnings.is_empty());
        assert!(warnings[0].fix.is_none(), "Should not provide fix when reflow is false");
    }

    #[test]
    fn test_reflow_preserves_all_reference_link_types() {
        let config = MD013Config {
            line_length: 40,
            reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "Test [full reference][ref] and [collapsed][] and [shortcut] reference links in a very long line.

[ref]: https://example.com
[collapsed]: https://example.com
[shortcut]: https://example.com";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
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
            reflow: true,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This line has an ![image alt text](https://example.com/image.png) that should not be broken when reflowing.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        // Image should remain intact
        assert!(fixed.contains("![image alt text](https://example.com/image.png)"));
    }

    #[test]
    fn test_normalize_mode_flags_short_lines() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Content with short lines that could be combined
        let content = "This is a short line.\nAnother short line.\nA third short line that could be combined.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();

        // Should flag the paragraph as needing normalization
        assert!(!warnings.is_empty(), "Should flag paragraph for normalization");
        assert!(warnings[0].message.contains("normalized"));
    }

    #[test]
    fn test_normalize_mode_combines_short_lines() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Content with short lines that should be combined
        let content =
            "This is a line with\nmanual line breaks at\n80 characters that should\nbe combined into longer lines.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        // Should combine into a single line since it's under 100 chars total
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines.len(), 1, "Should combine into single line");
        assert!(lines[0].len() > 80, "Should use more of the 100 char limit");
    }

    #[test]
    fn test_normalize_mode_preserves_paragraph_breaks() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "First paragraph with\nshort lines.\n\nSecond paragraph with\nshort lines too.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        // Should preserve paragraph breaks (empty lines)
        assert!(fixed.contains("\n\n"), "Should preserve paragraph breaks");

        let paragraphs: Vec<&str> = fixed.split("\n\n").collect();
        assert_eq!(paragraphs.len(), 2, "Should have two paragraphs");
    }

    #[test]
    fn test_default_mode_only_fixes_violations() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Default, // Default mode
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Content with short lines that are NOT violations
        let content = "This is a short line.\nAnother short line.\nA third short line.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();

        // Should NOT flag anything in default mode
        assert!(warnings.is_empty(), "Should not flag short lines in default mode");

        // Fix should preserve the short lines
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed.lines().count(), 3, "Should preserve line breaks in default mode");
    }

    #[test]
    fn test_normalize_mode_with_lists() {
        let config = MD013Config {
            line_length: 80,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = r#"A paragraph with
short lines.

1. List item with
   short lines
2. Another item"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        // Should normalize the paragraph but preserve list structure
        let lines: Vec<&str> = fixed.lines().collect();
        assert!(lines[0].len() > 20, "First paragraph should be normalized");
        assert!(fixed.contains("1. "), "Should preserve list markers");
        assert!(fixed.contains("2. "), "Should preserve list markers");
    }

    #[test]
    fn test_normalize_mode_with_code_blocks() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = r#"A paragraph with
short lines.

```
code block should not be normalized
even with short lines
```

Another paragraph with
short lines."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        // Code block should be preserved as-is
        assert!(fixed.contains("code block should not be normalized\neven with short lines"));
        // But paragraphs should be normalized
        let lines: Vec<&str> = fixed.lines().collect();
        assert!(lines[0].len() > 20, "First paragraph should be normalized");
    }

    #[test]
    fn test_issue_76_use_case() {
        // This tests the exact use case from issue #76
        let config = MD013Config {
            line_length: 999999, // Set absurdly high
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Content with manual line breaks at 80 characters (typical markdown)
        let content = "We've decided to eliminate line-breaks in paragraphs. The obvious solution is\nto disable MD013, and call it good. However, that doesn't deal with the\nexisting content's line-breaks. My initial thought was to set line_length to\n999999 and enable_reflow, but realised after doing so, that it never triggers\nthe error, so nothing happens.";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        // Should flag for normalization even though no lines exceed limit
        let warnings = rule.check(&ctx).unwrap();
        assert!(!warnings.is_empty(), "Should flag paragraph for normalization");

        // Should combine into a single line
        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines.len(), 1, "Should combine into single line with high limit");
        assert!(!fixed.contains("\n"), "Should remove all line breaks within paragraph");
    }

    #[test]
    fn test_normalize_mode_single_line_unchanged() {
        // Single lines should not be flagged or changed
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This is a single line that should not be changed.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty(), "Single line should not be flagged");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Single line should remain unchanged");
    }

    #[test]
    fn test_normalize_mode_with_inline_code() {
        let config = MD013Config {
            line_length: 80,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content =
            "This paragraph has `inline code` and\nshould still be normalized properly\nwithout breaking the code.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let warnings = rule.check(&ctx).unwrap();
        assert!(!warnings.is_empty(), "Multi-line paragraph should be flagged");

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("`inline code`"), "Inline code should be preserved");
        assert!(fixed.lines().count() < 3, "Lines should be combined");
    }

    #[test]
    fn test_normalize_mode_with_emphasis() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This has **bold** and\n*italic* text that\nshould be preserved.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("**bold**"), "Bold should be preserved");
        assert!(fixed.contains("*italic*"), "Italic should be preserved");
        assert_eq!(fixed.lines().count(), 1, "Should be combined into one line");
    }

    #[test]
    fn test_normalize_mode_respects_hard_breaks() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Two spaces at end of line = hard break
        let content = "First line with hard break  \nSecond line after break\nThird line";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        // Hard break should be preserved
        assert!(fixed.contains("  \n"), "Hard break should be preserved");
        // But lines without hard break should be combined
        assert!(
            fixed.contains("Second line after break Third line"),
            "Lines without hard break should combine"
        );
    }

    #[test]
    fn test_normalize_mode_with_links() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content =
            "This has a [link](https://example.com) that\nshould be preserved when\nnormalizing the paragraph.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("[link](https://example.com)"),
            "Link should be preserved"
        );
        assert_eq!(fixed.lines().count(), 1, "Should be combined into one line");
    }

    #[test]
    fn test_normalize_mode_empty_lines_between_paragraphs() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "First paragraph\nwith multiple lines.\n\n\nSecond paragraph\nwith multiple lines.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        // Multiple empty lines should be preserved
        assert!(fixed.contains("\n\n\n"), "Multiple empty lines should be preserved");
        // Each paragraph should be normalized
        let parts: Vec<&str> = fixed.split("\n\n\n").collect();
        assert_eq!(parts.len(), 2, "Should have two parts");
        assert_eq!(parts[0].lines().count(), 1, "First paragraph should be one line");
        assert_eq!(parts[1].lines().count(), 1, "Second paragraph should be one line");
    }

    #[test]
    fn test_normalize_mode_mixed_list_types() {
        let config = MD013Config {
            line_length: 80,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = r#"Paragraph before list
with multiple lines.

- Bullet item
* Another bullet
+ Plus bullet

1. Numbered item
2. Another number

Paragraph after list
with multiple lines."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        // Lists should be preserved
        assert!(fixed.contains("- Bullet item"), "Dash list should be preserved");
        assert!(fixed.contains("* Another bullet"), "Star list should be preserved");
        assert!(fixed.contains("+ Plus bullet"), "Plus list should be preserved");
        assert!(fixed.contains("1. Numbered item"), "Numbered list should be preserved");

        // But paragraphs should be normalized
        assert!(
            fixed.starts_with("Paragraph before list with multiple lines."),
            "First paragraph should be normalized"
        );
        assert!(
            fixed.ends_with("Paragraph after list with multiple lines."),
            "Last paragraph should be normalized"
        );
    }

    #[test]
    fn test_normalize_mode_with_horizontal_rules() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "Paragraph before\nhorizontal rule.\n\n---\n\nParagraph after\nhorizontal rule.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("---"), "Horizontal rule should be preserved");
        assert!(
            fixed.contains("Paragraph before horizontal rule."),
            "First paragraph normalized"
        );
        assert!(
            fixed.contains("Paragraph after horizontal rule."),
            "Second paragraph normalized"
        );
    }

    #[test]
    fn test_normalize_mode_with_indented_code() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "Paragraph before\nindented code.\n\n    This is indented code\n    Should not be normalized\n\nParagraph after\nindented code.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("    This is indented code\n    Should not be normalized"),
            "Indented code preserved"
        );
        assert!(
            fixed.contains("Paragraph before indented code."),
            "First paragraph normalized"
        );
        assert!(
            fixed.contains("Paragraph after indented code."),
            "Second paragraph normalized"
        );
    }

    #[test]
    fn test_normalize_mode_disabled_without_reflow() {
        // Normalize mode should have no effect if reflow is disabled
        let config = MD013Config {
            line_length: 100,
            reflow: false, // Disabled
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This is a line\nwith breaks that\nshould not be changed.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty(), "Should not flag when reflow is disabled");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Content should be unchanged when reflow is disabled");
    }

    #[test]
    fn test_default_mode_with_long_lines() {
        // Default mode should only fix lines that exceed limit
        let config = MD013Config {
            line_length: 50,
            reflow: true,
            reflow_mode: ReflowMode::Default,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "Short line.\nThis is a very long line that definitely exceeds the fifty character limit and needs wrapping.\nAnother short line.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Should only flag the long line");
        assert_eq!(warnings[0].line, 2, "Should flag line 2");

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();
        // Should have more than 3 lines after wrapping the long one
        assert!(lines.len() > 3, "Long line should be wrapped");
        assert_eq!(lines[0], "Short line.", "First short line unchanged");
    }

    #[test]
    fn test_normalize_vs_default_mode_same_content() {
        let content = "This is a paragraph\nwith multiple lines\nthat could be combined.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        // Test default mode
        let default_config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Default,
            ..Default::default()
        };
        let default_rule = MD013LineLength::from_config_struct(default_config);
        let default_warnings = default_rule.check(&ctx).unwrap();
        let default_fixed = default_rule.fix(&ctx).unwrap();

        // Test normalize mode
        let normalize_config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let normalize_rule = MD013LineLength::from_config_struct(normalize_config);
        let normalize_warnings = normalize_rule.check(&ctx).unwrap();
        let normalize_fixed = normalize_rule.fix(&ctx).unwrap();

        // Verify different behavior
        assert!(default_warnings.is_empty(), "Default mode should not flag short lines");
        assert!(
            !normalize_warnings.is_empty(),
            "Normalize mode should flag multi-line paragraphs"
        );

        assert_eq!(
            default_fixed, content,
            "Default mode should not change content without violations"
        );
        assert_ne!(
            normalize_fixed, content,
            "Normalize mode should change multi-line paragraphs"
        );
        assert_eq!(
            normalize_fixed.lines().count(),
            1,
            "Normalize should combine into single line"
        );
    }

    #[test]
    fn test_normalize_mode_with_reference_definitions() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content =
            "This paragraph uses\na reference [link][ref]\nacross multiple lines.\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("[link][ref]"), "Reference link should be preserved");
        assert!(
            fixed.contains("[ref]: https://example.com"),
            "Reference definition should be preserved"
        );
        assert!(
            fixed.starts_with("This paragraph uses a reference [link][ref] across multiple lines."),
            "Paragraph should be normalized"
        );
    }

    #[test]
    fn test_normalize_mode_with_html_comments() {
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "Paragraph before\nHTML comment.\n\n<!-- This is a comment -->\n\nParagraph after\nHTML comment.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("<!-- This is a comment -->"),
            "HTML comment should be preserved"
        );
        assert!(
            fixed.contains("Paragraph before HTML comment."),
            "First paragraph normalized"
        );
        assert!(
            fixed.contains("Paragraph after HTML comment."),
            "Second paragraph normalized"
        );
    }

    #[test]
    fn test_normalize_mode_line_starting_with_number() {
        // Regression test for the bug we fixed where "80 characters" was treated as a list
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "This line mentions\n80 characters which\nshould not break the paragraph.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed.lines().count(), 1, "Should be combined into single line");
        assert!(
            fixed.contains("80 characters"),
            "Number at start of line should be preserved"
        );
    }

    #[test]
    fn test_default_mode_preserves_list_structure() {
        // In default mode, list continuation lines should be preserved
        let config = MD013Config {
            line_length: 80,
            reflow: true,
            reflow_mode: ReflowMode::Default,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = r#"- This is a bullet point that has
  some text on multiple lines
  that should stay separate

1. Numbered list item with
   multiple lines that should
   also stay separate"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        // In default mode, the structure should be preserved
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(
            lines[0], "- This is a bullet point that has",
            "First line should be unchanged"
        );
        assert_eq!(
            lines[1], "  some text on multiple lines",
            "Continuation should be preserved"
        );
        assert_eq!(
            lines[2], "  that should stay separate",
            "Second continuation should be preserved"
        );
    }

    #[test]
    fn test_normalize_mode_multi_line_list_items_no_extra_spaces() {
        // Test that multi-line list items don't get extra spaces when normalized
        let config = MD013Config {
            line_length: 80,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = r#"- This is a bullet point that has
  some text on multiple lines
  that should be combined

1. Numbered list item with
   multiple lines that need
   to be properly combined
2. Second item"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        // Check that there are no extra spaces in the combined list items
        assert!(
            !fixed.contains("lines  that"),
            "Should not have double spaces in bullet list"
        );
        assert!(
            !fixed.contains("need  to"),
            "Should not have double spaces in numbered list"
        );

        // Check that the list items are properly combined
        assert!(
            fixed.contains("- This is a bullet point that has some text on multiple lines that should be"),
            "Bullet list should be properly combined"
        );
        assert!(
            fixed.contains("1. Numbered list item with multiple lines that need to be properly combined"),
            "Numbered list should be properly combined"
        );
    }

    #[test]
    fn test_normalize_mode_actual_numbered_list() {
        // Ensure actual numbered lists are still detected correctly
        let config = MD013Config {
            line_length: 100,
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "Paragraph before list\nwith multiple lines.\n\n1. First item\n2. Second item\n10. Tenth item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("1. First item"), "Numbered list 1 should be preserved");
        assert!(fixed.contains("2. Second item"), "Numbered list 2 should be preserved");
        assert!(fixed.contains("10. Tenth item"), "Numbered list 10 should be preserved");
        assert!(
            fixed.starts_with("Paragraph before list with multiple lines."),
            "Paragraph should be normalized"
        );
    }
}
