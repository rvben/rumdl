/// Rule MD013: Line length
///
/// See [docs/md013.md](../../docs/md013.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::mkdocs_attr_list::is_standalone_attr_list;
use crate::utils::mkdocs_snippets::is_snippet_block_delimiter;
use crate::utils::range_utils::LineIndex;
use crate::utils::range_utils::calculate_excess_range;
use crate::utils::regex_cache::{
    IMAGE_REF_PATTERN, INLINE_LINK_REGEX as MARKDOWN_LINK_PATTERN, LINK_REF_PATTERN, URL_IN_TEXT, URL_PATTERN,
};
use crate::utils::table_utils::TableUtils;
use crate::utils::text_reflow::split_into_sentences;
use toml;

mod helpers;
pub mod md013_config;
use helpers::{
    extract_list_marker_and_content, has_hard_break, is_horizontal_rule, is_list_item, is_template_directive_only,
    split_into_segments, trim_preserving_hard_break,
};
pub use md013_config::MD013Config;
use md013_config::{LengthMode, ReflowMode};

#[cfg(test)]
mod tests;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Default)]
pub struct MD013LineLength {
    pub(crate) config: MD013Config,
}

impl MD013LineLength {
    pub fn new(line_length: usize, code_blocks: bool, tables: bool, headings: bool, strict: bool) -> Self {
        Self {
            config: MD013Config {
                line_length: crate::types::LineLength::new(line_length),
                code_blocks,
                tables,
                headings,
                paragraphs: true, // Default to true for backwards compatibility
                strict,
                reflow: false,
                reflow_mode: ReflowMode::default(),
                length_mode: LengthMode::default(),
                abbreviations: Vec::new(),
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
        ctx: &crate::lint_context::LintContext,
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
        if ctx.line_info(current_line + 1).is_some_and(|info| info.in_code_block)
            && !trimmed.is_empty()
            && !line.contains(' ')
            && !line.contains('\t')
        {
            return true;
        }

        false
    }

    /// Check if rule should skip based on provided config (used for inline config support)
    fn should_skip_with_config(&self, ctx: &crate::lint_context::LintContext, config: &MD013Config) -> bool {
        // Skip if content is empty
        if ctx.content.is_empty() {
            return true;
        }

        // For sentence-per-line or normalize mode, never skip based on line length
        if config.reflow
            && (config.reflow_mode == ReflowMode::SentencePerLine || config.reflow_mode == ReflowMode::Normalize)
        {
            return false;
        }

        // Quick check: if total content is shorter than line limit, definitely skip
        if ctx.content.len() <= config.line_length.get() {
            return true;
        }

        // Skip if no line exceeds the limit
        !ctx.lines.iter().any(|line| line.byte_len > config.line_length.get())
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

        // Parse inline configuration FIRST so we can use effective config for should_skip
        let inline_config = crate::inline_config::InlineConfig::from_content(content);
        let config_override = inline_config.get_rule_config("MD013");

        // Apply configuration override if present
        let effective_config = if let Some(json_config) = config_override {
            if let Some(obj) = json_config.as_object() {
                let mut config = self.config.clone();
                if let Some(line_length) = obj.get("line_length").and_then(|v| v.as_u64()) {
                    config.line_length = crate::types::LineLength::new(line_length as usize);
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
                        "sentence-per-line" => ReflowMode::SentencePerLine,
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

        // Fast early return using should_skip with EFFECTIVE config (after inline overrides)
        // But don't skip if we're in reflow mode with Normalize or SentencePerLine
        if self.should_skip_with_config(ctx, &effective_config)
            && !(effective_config.reflow
                && (effective_config.reflow_mode == ReflowMode::Normalize
                    || effective_config.reflow_mode == ReflowMode::SentencePerLine))
        {
            return Ok(Vec::new());
        }

        // Direct implementation without DocumentStructure
        let mut warnings = Vec::new();

        // Special handling: line_length = 0 means "no line length limit"
        // Skip all line length checks, but still allow reflow if enabled
        let skip_length_checks = effective_config.line_length.is_unlimited();

        // Pre-filter lines that could be problematic to avoid processing all lines
        let mut candidate_lines = Vec::new();
        if !skip_length_checks {
            for (line_idx, line_info) in ctx.lines.iter().enumerate() {
                // Skip front matter - it should never be linted
                if line_info.in_front_matter {
                    continue;
                }

                // Quick length check first
                if line_info.byte_len > effective_config.line_length.get() {
                    candidate_lines.push(line_idx);
                }
            }
        }

        // If no candidate lines and not in normalize or sentence-per-line mode, early return
        if candidate_lines.is_empty()
            && !(effective_config.reflow
                && (effective_config.reflow_mode == ReflowMode::Normalize
                    || effective_config.reflow_mode == ReflowMode::SentencePerLine))
        {
            return Ok(warnings);
        }

        // Use ctx.lines if available for better performance
        let lines: Vec<&str> = if !ctx.lines.is_empty() {
            ctx.lines.iter().map(|l| l.content(ctx.content)).collect()
        } else {
            content.lines().collect()
        };

        // Create a quick lookup set for heading lines
        // We need this for both the heading skip check AND the paragraphs check
        let heading_lines_set: std::collections::HashSet<usize> = ctx
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.heading.is_some())
            .map(|(idx, _)| idx + 1)
            .collect();

        // Use pre-computed table blocks from context
        // We need this for both the table skip check AND the paragraphs check
        let table_blocks = &ctx.table_blocks;
        let mut table_lines_set = std::collections::HashSet::new();
        for table in table_blocks {
            table_lines_set.insert(table.header_line + 1);
            table_lines_set.insert(table.delimiter_line + 1);
            for &line in &table.content_lines {
                table_lines_set.insert(line + 1);
            }
        }

        // Process candidate lines for line length checks
        for &line_idx in &candidate_lines {
            let line_number = line_idx + 1;
            let line = lines[line_idx];

            // Calculate effective length excluding unbreakable URLs
            let effective_length = self.calculate_effective_length(line);

            // Use single line length limit for all content
            let line_limit = effective_config.line_length.get();

            // Skip short lines immediately (double-check after effective length calculation)
            if effective_length <= line_limit {
                continue;
            }

            // Skip mkdocstrings blocks (already handled by LintContext)
            if ctx.lines[line_idx].in_mkdocstrings {
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
                    || (!effective_config.code_blocks
                        && ctx.line_info(line_number).is_some_and(|info| info.in_code_block))
                    || (!effective_config.tables && table_lines_set.contains(&line_number))
                    || ctx.lines[line_number - 1].blockquote.is_some()
                    || ctx.line_info(line_number).is_some_and(|info| info.in_html_block)
                    || ctx.line_info(line_number).is_some_and(|info| info.in_html_comment)
                    || ctx.line_info(line_number).is_some_and(|info| info.in_esm_block)
                    || ctx.line_info(line_number).is_some_and(|info| info.in_jsx_expression)
                    || ctx.line_info(line_number).is_some_and(|info| info.in_mdx_comment)
                {
                    continue;
                }

                // Check if this is a paragraph/regular text line
                // If paragraphs = false, skip lines that are NOT in special blocks
                if !effective_config.paragraphs {
                    let is_special_block = heading_lines_set.contains(&line_number)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_code_block)
                        || table_lines_set.contains(&line_number)
                        || ctx.lines[line_number - 1].blockquote.is_some()
                        || ctx.line_info(line_number).is_some_and(|info| info.in_html_block)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_html_comment)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_esm_block)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_jsx_expression)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_mdx_comment);

                    // Skip regular paragraph text when paragraphs = false
                    if !is_special_block {
                        continue;
                    }
                }

                // Skip lines that are only a URL, image ref, or link ref
                if self.should_ignore_line(line, &lines, line_idx, ctx) {
                    continue;
                }
            }

            // In sentence-per-line mode, check if this is a single long sentence
            // If so, emit a warning without a fix (user must manually rephrase)
            if effective_config.reflow_mode == ReflowMode::SentencePerLine {
                let sentences = split_into_sentences(line.trim());
                if sentences.len() == 1 {
                    // Single sentence that's too long - warn but don't auto-fix
                    let message = format!("Line length {effective_length} exceeds {line_limit} characters");

                    let (start_line, start_col, end_line, end_col) =
                        calculate_excess_range(line_number, line, line_limit);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        message,
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        fix: None, // No auto-fix for long single sentences
                    });
                    continue;
                }
                // Multiple sentences will be handled by paragraph-based reflow
                continue;
            }

            // Don't provide fix for individual lines when reflow is enabled
            // Paragraph-based fixes will be handled separately
            let fix = None;

            let message = format!("Line length {effective_length} exceeds {line_limit} characters");

            // Calculate precise character range for the excess portion
            let (start_line, start_col, end_line, end_col) = calculate_excess_range(line_number, line, line_limit);

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message,
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                severity: Severity::Warning,
                fix,
            });
        }

        // If reflow is enabled, generate paragraph-based fixes
        if effective_config.reflow {
            let paragraph_warnings = self.generate_paragraph_fixes(ctx, &effective_config, &lines);
            // Merge paragraph warnings with line warnings, removing duplicates
            for pw in paragraph_warnings {
                // Remove any line warnings that overlap with this paragraph
                warnings.retain(|w| w.line < pw.line || w.line > pw.end_line);
                warnings.push(pw);
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // For CLI usage, apply fixes from warnings
        // LSP will use the warning-based fixes directly
        let warnings = self.check(ctx)?;

        // If there are no fixes, return content unchanged
        if !warnings.iter().any(|w| w.fix.is_some()) {
            return Ok(ctx.content.to_string());
        }

        // Apply warning-based fixes
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(|e| LintError::FixFailed(format!("Failed to apply fixes: {e}")))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        self.should_skip_with_config(ctx, &self.config)
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
        // Use global line_length if rule-specific config still has default value
        if rule_config.line_length.get() == 80 {
            rule_config.line_length = config.global.line_length;
        }
        Box::new(Self::from_config_struct(rule_config))
    }
}

impl MD013LineLength {
    /// Generate paragraph-based fixes
    fn generate_paragraph_fixes(
        &self,
        ctx: &crate::lint_context::LintContext,
        config: &MD013Config,
        lines: &[&str],
    ) -> Vec<LintWarning> {
        let mut warnings = Vec::new();
        let line_index = LineIndex::new(ctx.content);

        let mut i = 0;
        while i < lines.len() {
            let line_num = i + 1;

            // Skip special structures
            let should_skip_due_to_line_info = ctx.line_info(line_num).is_some_and(|info| {
                info.in_code_block
                    || info.in_front_matter
                    || info.in_html_block
                    || info.in_html_comment
                    || info.in_esm_block
                    || info.in_jsx_expression
                    || info.in_mdx_comment
            });

            if should_skip_due_to_line_info
                || (line_num > 0 && line_num <= ctx.lines.len() && ctx.lines[line_num - 1].blockquote.is_some())
                || lines[i].trim().starts_with('#')
                || TableUtils::is_potential_table_row(lines[i])
                || lines[i].trim().is_empty()
                || is_horizontal_rule(lines[i].trim())
                || is_template_directive_only(lines[i])
            {
                i += 1;
                continue;
            }

            // Helper function to detect semantic line markers
            let is_semantic_line = |content: &str| -> bool {
                let trimmed = content.trim_start();
                let semantic_markers = [
                    "NOTE:",
                    "WARNING:",
                    "IMPORTANT:",
                    "CAUTION:",
                    "TIP:",
                    "DANGER:",
                    "HINT:",
                    "INFO:",
                ];
                semantic_markers.iter().any(|marker| trimmed.starts_with(marker))
            };

            // Helper function to detect fence markers (opening or closing)
            let is_fence_marker = |content: &str| -> bool {
                let trimmed = content.trim_start();
                trimmed.starts_with("```") || trimmed.starts_with("~~~")
            };

            // Check if this is a list item - handle it specially
            let trimmed = lines[i].trim();
            if is_list_item(trimmed) {
                // Collect the entire list item including continuation lines
                let list_start = i;
                let (marker, first_content) = extract_list_marker_and_content(lines[i]);
                let marker_len = marker.len();

                // Track lines and their types (content, code block, fence, nested list)
                #[derive(Clone)]
                enum LineType {
                    Content(String),
                    CodeBlock(String, usize),      // content and original indent
                    NestedListItem(String, usize), // full line content and original indent
                    SemanticLine(String),          // Lines starting with NOTE:, WARNING:, etc that should stay separate
                    SnippetLine(String),           // MkDocs Snippets delimiters (-8<-) that must stay on their own line
                    Empty,
                }

                let mut actual_indent: Option<usize> = None;
                let mut list_item_lines: Vec<LineType> = vec![LineType::Content(first_content)];
                i += 1;

                // Collect continuation lines using ctx.lines for metadata
                while i < lines.len() {
                    let line_info = &ctx.lines[i];

                    // Use pre-computed is_blank from ctx
                    if line_info.is_blank {
                        // Empty line - check if next line is indented (part of list item)
                        if i + 1 < lines.len() {
                            let next_info = &ctx.lines[i + 1];

                            // Check if next line is indented enough to be continuation
                            if !next_info.is_blank && next_info.indent >= marker_len {
                                // This blank line is between paragraphs/blocks in the list item
                                list_item_lines.push(LineType::Empty);
                                i += 1;
                                continue;
                            }
                        }
                        // No indented line after blank, end of list item
                        break;
                    }

                    // Use pre-computed indent from ctx
                    let indent = line_info.indent;

                    // Valid continuation must be indented at least marker_len
                    if indent >= marker_len {
                        let trimmed = line_info.content(ctx.content).trim();

                        // Use pre-computed in_code_block from ctx
                        if line_info.in_code_block {
                            list_item_lines.push(LineType::CodeBlock(
                                line_info.content(ctx.content)[indent..].to_string(),
                                indent,
                            ));
                            i += 1;
                            continue;
                        }

                        // Check if this is a SIBLING list item (breaks parent)
                        // Nested lists are indented >= marker_len and are PART of the parent item
                        // Siblings are at indent < marker_len (at or before parent marker)
                        if is_list_item(trimmed) && indent < marker_len {
                            // This is a sibling item at same or higher level - end parent item
                            break;
                        }

                        // Check if this is a NESTED list item marker
                        // Nested lists should be processed separately UNLESS they're part of a
                        // multi-paragraph list item (indicated by a blank line before them OR
                        // it's a continuation of an already-started nested list)
                        if is_list_item(trimmed) && indent >= marker_len {
                            // Check if there was a blank line before this (multi-paragraph context)
                            let has_blank_before = matches!(list_item_lines.last(), Some(LineType::Empty));

                            // Check if we've already seen nested list content (another nested item)
                            let has_nested_content = list_item_lines.iter().any(|line| {
                                matches!(line, LineType::Content(c) if is_list_item(c.trim()))
                                    || matches!(line, LineType::NestedListItem(_, _))
                            });

                            if !has_blank_before && !has_nested_content {
                                // Single-paragraph context with no prior nested items: starts a new item
                                // End parent collection; nested list will be processed next
                                break;
                            }
                            // else: multi-paragraph context or continuation of nested list, keep collecting
                            // Mark this as a nested list item to preserve its structure
                            list_item_lines.push(LineType::NestedListItem(
                                line_info.content(ctx.content)[indent..].to_string(),
                                indent,
                            ));
                            i += 1;
                            continue;
                        }

                        // Normal continuation: marker_len to marker_len+3
                        if indent <= marker_len + 3 {
                            // Set actual_indent from first non-code continuation if not set
                            if actual_indent.is_none() {
                                actual_indent = Some(indent);
                            }

                            // Extract content (remove indentation and trailing whitespace)
                            // Preserve hard breaks (2 trailing spaces) while removing excessive whitespace
                            // See: https://github.com/rvben/rumdl/issues/76
                            let content = trim_preserving_hard_break(&line_info.content(ctx.content)[indent..]);

                            // Check if this is a fence marker (opening or closing)
                            // These should be treated as code block lines, not paragraph content
                            if is_fence_marker(&content) {
                                list_item_lines.push(LineType::CodeBlock(content, indent));
                            }
                            // Check if this is a semantic line (NOTE:, WARNING:, etc.)
                            else if is_semantic_line(&content) {
                                list_item_lines.push(LineType::SemanticLine(content));
                            }
                            // Check if this is a snippet block delimiter (-8<- or --8<--)
                            // These must be preserved on their own lines for MkDocs Snippets extension
                            else if is_snippet_block_delimiter(&content) {
                                list_item_lines.push(LineType::SnippetLine(content));
                            } else {
                                list_item_lines.push(LineType::Content(content));
                            }
                            i += 1;
                        } else {
                            // indent >= marker_len + 4: indented code block
                            list_item_lines.push(LineType::CodeBlock(
                                line_info.content(ctx.content)[indent..].to_string(),
                                indent,
                            ));
                            i += 1;
                        }
                    } else {
                        // Not indented enough, end of list item
                        break;
                    }
                }

                // Use detected indent or fallback to marker length
                let indent_size = actual_indent.unwrap_or(marker_len);
                let expected_indent = " ".repeat(indent_size);

                // Split list_item_lines into blocks (paragraphs, code blocks, nested lists, semantic lines, and HTML blocks)
                #[derive(Clone)]
                enum Block {
                    Paragraph(Vec<String>),
                    Code {
                        lines: Vec<(String, usize)>, // (content, indent) pairs
                        has_preceding_blank: bool,   // Whether there was a blank line before this block
                    },
                    NestedList(Vec<(String, usize)>), // (content, indent) pairs for nested list items
                    SemanticLine(String), // Semantic markers like NOTE:, WARNING: that stay on their own line
                    SnippetLine(String),  // MkDocs Snippets delimiter that stays on its own line without extra spacing
                    Html {
                        lines: Vec<String>,        // HTML content preserved exactly as-is
                        has_preceding_blank: bool, // Whether there was a blank line before this block
                    },
                }

                // HTML tag detection helpers
                // Block-level HTML tags that should trigger HTML block detection
                const BLOCK_LEVEL_TAGS: &[&str] = &[
                    "div",
                    "details",
                    "summary",
                    "section",
                    "article",
                    "header",
                    "footer",
                    "nav",
                    "aside",
                    "main",
                    "table",
                    "thead",
                    "tbody",
                    "tfoot",
                    "tr",
                    "td",
                    "th",
                    "ul",
                    "ol",
                    "li",
                    "dl",
                    "dt",
                    "dd",
                    "pre",
                    "blockquote",
                    "figure",
                    "figcaption",
                    "form",
                    "fieldset",
                    "legend",
                    "hr",
                    "p",
                    "h1",
                    "h2",
                    "h3",
                    "h4",
                    "h5",
                    "h6",
                    "style",
                    "script",
                    "noscript",
                ];

                fn is_block_html_opening_tag(line: &str) -> Option<String> {
                    let trimmed = line.trim();

                    // Check for HTML comments
                    if trimmed.starts_with("<!--") {
                        return Some("!--".to_string());
                    }

                    // Check for opening tags
                    if trimmed.starts_with('<') && !trimmed.starts_with("</") && !trimmed.starts_with("<!") {
                        // Extract tag name from <tagname ...> or <tagname>
                        let after_bracket = &trimmed[1..];
                        if let Some(end) = after_bracket.find(|c: char| c.is_whitespace() || c == '>' || c == '/') {
                            let tag_name = after_bracket[..end].to_lowercase();

                            // Only treat as block if it's a known block-level tag
                            if BLOCK_LEVEL_TAGS.contains(&tag_name.as_str()) {
                                return Some(tag_name);
                            }
                        }
                    }
                    None
                }

                fn is_html_closing_tag(line: &str, tag_name: &str) -> bool {
                    let trimmed = line.trim();

                    // Special handling for HTML comments
                    if tag_name == "!--" {
                        return trimmed.ends_with("-->");
                    }

                    // Check for closing tags: </tagname> or </tagname ...>
                    trimmed.starts_with(&format!("</{tag_name}>"))
                        || trimmed.starts_with(&format!("</{tag_name}  "))
                        || (trimmed.starts_with("</") && trimmed[2..].trim_start().starts_with(tag_name))
                }

                fn is_self_closing_tag(line: &str) -> bool {
                    let trimmed = line.trim();
                    trimmed.ends_with("/>")
                }

                let mut blocks: Vec<Block> = Vec::new();
                let mut current_paragraph: Vec<String> = Vec::new();
                let mut current_code_block: Vec<(String, usize)> = Vec::new();
                let mut current_nested_list: Vec<(String, usize)> = Vec::new();
                let mut current_html_block: Vec<String> = Vec::new();
                let mut html_tag_stack: Vec<String> = Vec::new();
                let mut in_code = false;
                let mut in_nested_list = false;
                let mut in_html_block = false;
                let mut had_preceding_blank = false; // Track if we just saw an empty line
                let mut code_block_has_preceding_blank = false; // Track blank before current code block
                let mut html_block_has_preceding_blank = false; // Track blank before current HTML block

                for line in &list_item_lines {
                    match line {
                        LineType::Empty => {
                            if in_code {
                                current_code_block.push((String::new(), 0));
                            } else if in_nested_list {
                                current_nested_list.push((String::new(), 0));
                            } else if in_html_block {
                                // Allow blank lines inside HTML blocks
                                current_html_block.push(String::new());
                            } else if !current_paragraph.is_empty() {
                                blocks.push(Block::Paragraph(current_paragraph.clone()));
                                current_paragraph.clear();
                            }
                            // Mark that we saw a blank line
                            had_preceding_blank = true;
                        }
                        LineType::Content(content) => {
                            // Check if we're currently in an HTML block
                            if in_html_block {
                                current_html_block.push(content.clone());

                                // Check if this line closes any open HTML tags
                                if let Some(last_tag) = html_tag_stack.last() {
                                    if is_html_closing_tag(content, last_tag) {
                                        html_tag_stack.pop();

                                        // If stack is empty, HTML block is complete
                                        if html_tag_stack.is_empty() {
                                            blocks.push(Block::Html {
                                                lines: current_html_block.clone(),
                                                has_preceding_blank: html_block_has_preceding_blank,
                                            });
                                            current_html_block.clear();
                                            in_html_block = false;
                                        }
                                    } else if let Some(new_tag) = is_block_html_opening_tag(content) {
                                        // Nested opening tag within HTML block
                                        if !is_self_closing_tag(content) {
                                            html_tag_stack.push(new_tag);
                                        }
                                    }
                                }
                                had_preceding_blank = false;
                            } else {
                                // Not in HTML block - check if this line starts one
                                if let Some(tag_name) = is_block_html_opening_tag(content) {
                                    // Flush current paragraph before starting HTML block
                                    if in_code {
                                        blocks.push(Block::Code {
                                            lines: current_code_block.clone(),
                                            has_preceding_blank: code_block_has_preceding_blank,
                                        });
                                        current_code_block.clear();
                                        in_code = false;
                                    } else if in_nested_list {
                                        blocks.push(Block::NestedList(current_nested_list.clone()));
                                        current_nested_list.clear();
                                        in_nested_list = false;
                                    } else if !current_paragraph.is_empty() {
                                        blocks.push(Block::Paragraph(current_paragraph.clone()));
                                        current_paragraph.clear();
                                    }

                                    // Start new HTML block
                                    in_html_block = true;
                                    html_block_has_preceding_blank = had_preceding_blank;
                                    current_html_block.push(content.clone());

                                    // Check if it's self-closing or needs a closing tag
                                    if is_self_closing_tag(content) {
                                        // Self-closing tag - complete the HTML block immediately
                                        blocks.push(Block::Html {
                                            lines: current_html_block.clone(),
                                            has_preceding_blank: html_block_has_preceding_blank,
                                        });
                                        current_html_block.clear();
                                        in_html_block = false;
                                    } else {
                                        // Regular opening tag - push to stack
                                        html_tag_stack.push(tag_name);
                                    }
                                } else {
                                    // Regular content line - add to paragraph
                                    if in_code {
                                        // Switching from code to content
                                        blocks.push(Block::Code {
                                            lines: current_code_block.clone(),
                                            has_preceding_blank: code_block_has_preceding_blank,
                                        });
                                        current_code_block.clear();
                                        in_code = false;
                                    } else if in_nested_list {
                                        // Switching from nested list to content
                                        blocks.push(Block::NestedList(current_nested_list.clone()));
                                        current_nested_list.clear();
                                        in_nested_list = false;
                                    }
                                    current_paragraph.push(content.clone());
                                }
                                had_preceding_blank = false; // Reset after content
                            }
                        }
                        LineType::CodeBlock(content, indent) => {
                            if in_nested_list {
                                // Switching from nested list to code
                                blocks.push(Block::NestedList(current_nested_list.clone()));
                                current_nested_list.clear();
                                in_nested_list = false;
                            } else if in_html_block {
                                // Switching from HTML block to code (shouldn't happen normally, but handle it)
                                blocks.push(Block::Html {
                                    lines: current_html_block.clone(),
                                    has_preceding_blank: html_block_has_preceding_blank,
                                });
                                current_html_block.clear();
                                html_tag_stack.clear();
                                in_html_block = false;
                            }
                            if !in_code {
                                // Switching from content to code
                                if !current_paragraph.is_empty() {
                                    blocks.push(Block::Paragraph(current_paragraph.clone()));
                                    current_paragraph.clear();
                                }
                                in_code = true;
                                // Record whether there was a blank line before this code block
                                code_block_has_preceding_blank = had_preceding_blank;
                            }
                            current_code_block.push((content.clone(), *indent));
                            had_preceding_blank = false; // Reset after code
                        }
                        LineType::NestedListItem(content, indent) => {
                            if in_code {
                                // Switching from code to nested list
                                blocks.push(Block::Code {
                                    lines: current_code_block.clone(),
                                    has_preceding_blank: code_block_has_preceding_blank,
                                });
                                current_code_block.clear();
                                in_code = false;
                            } else if in_html_block {
                                // Switching from HTML block to nested list (shouldn't happen normally, but handle it)
                                blocks.push(Block::Html {
                                    lines: current_html_block.clone(),
                                    has_preceding_blank: html_block_has_preceding_blank,
                                });
                                current_html_block.clear();
                                html_tag_stack.clear();
                                in_html_block = false;
                            }
                            if !in_nested_list {
                                // Switching from content to nested list
                                if !current_paragraph.is_empty() {
                                    blocks.push(Block::Paragraph(current_paragraph.clone()));
                                    current_paragraph.clear();
                                }
                                in_nested_list = true;
                            }
                            current_nested_list.push((content.clone(), *indent));
                            had_preceding_blank = false; // Reset after nested list
                        }
                        LineType::SemanticLine(content) => {
                            // Semantic lines are standalone - flush any current block and add as separate block
                            if in_code {
                                blocks.push(Block::Code {
                                    lines: current_code_block.clone(),
                                    has_preceding_blank: code_block_has_preceding_blank,
                                });
                                current_code_block.clear();
                                in_code = false;
                            } else if in_nested_list {
                                blocks.push(Block::NestedList(current_nested_list.clone()));
                                current_nested_list.clear();
                                in_nested_list = false;
                            } else if in_html_block {
                                blocks.push(Block::Html {
                                    lines: current_html_block.clone(),
                                    has_preceding_blank: html_block_has_preceding_blank,
                                });
                                current_html_block.clear();
                                html_tag_stack.clear();
                                in_html_block = false;
                            } else if !current_paragraph.is_empty() {
                                blocks.push(Block::Paragraph(current_paragraph.clone()));
                                current_paragraph.clear();
                            }
                            // Add semantic line as its own block
                            blocks.push(Block::SemanticLine(content.clone()));
                            had_preceding_blank = false; // Reset after semantic line
                        }
                        LineType::SnippetLine(content) => {
                            // Snippet delimiters (-8<-) are standalone - flush any current block and add as separate block
                            // Unlike semantic lines, snippet lines don't add extra blank lines around them
                            if in_code {
                                blocks.push(Block::Code {
                                    lines: current_code_block.clone(),
                                    has_preceding_blank: code_block_has_preceding_blank,
                                });
                                current_code_block.clear();
                                in_code = false;
                            } else if in_nested_list {
                                blocks.push(Block::NestedList(current_nested_list.clone()));
                                current_nested_list.clear();
                                in_nested_list = false;
                            } else if in_html_block {
                                blocks.push(Block::Html {
                                    lines: current_html_block.clone(),
                                    has_preceding_blank: html_block_has_preceding_blank,
                                });
                                current_html_block.clear();
                                html_tag_stack.clear();
                                in_html_block = false;
                            } else if !current_paragraph.is_empty() {
                                blocks.push(Block::Paragraph(current_paragraph.clone()));
                                current_paragraph.clear();
                            }
                            // Add snippet line as its own block
                            blocks.push(Block::SnippetLine(content.clone()));
                            had_preceding_blank = false;
                        }
                    }
                }

                // Push remaining block
                if in_code && !current_code_block.is_empty() {
                    blocks.push(Block::Code {
                        lines: current_code_block,
                        has_preceding_blank: code_block_has_preceding_blank,
                    });
                } else if in_nested_list && !current_nested_list.is_empty() {
                    blocks.push(Block::NestedList(current_nested_list));
                } else if in_html_block && !current_html_block.is_empty() {
                    // If we still have an unclosed HTML block, push it anyway
                    // (malformed HTML - missing closing tag)
                    blocks.push(Block::Html {
                        lines: current_html_block,
                        has_preceding_blank: html_block_has_preceding_blank,
                    });
                } else if !current_paragraph.is_empty() {
                    blocks.push(Block::Paragraph(current_paragraph));
                }

                // Check if reflowing is needed (only for content paragraphs, not code blocks or nested lists)
                let content_lines: Vec<String> = list_item_lines
                    .iter()
                    .filter_map(|line| {
                        if let LineType::Content(s) = line {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                // Check if we need to reflow this list item
                // We check the combined content to see if it exceeds length limits
                let combined_content = content_lines.join(" ").trim().to_string();
                let full_line = format!("{marker}{combined_content}");

                // Helper to check if we should reflow in normalize mode
                let should_normalize = || {
                    // Don't normalize if the list item only contains nested lists, code blocks, or semantic lines
                    // DO normalize if it has plain text content that spans multiple lines
                    let has_nested_lists = blocks.iter().any(|b| matches!(b, Block::NestedList(_)));
                    let has_code_blocks = blocks.iter().any(|b| matches!(b, Block::Code { .. }));
                    let has_semantic_lines = blocks.iter().any(|b| matches!(b, Block::SemanticLine(_)));
                    let has_snippet_lines = blocks.iter().any(|b| matches!(b, Block::SnippetLine(_)));
                    let has_paragraphs = blocks.iter().any(|b| matches!(b, Block::Paragraph(_)));

                    // If we have nested lists, code blocks, semantic lines, or snippet lines but no paragraphs, don't normalize
                    if (has_nested_lists || has_code_blocks || has_semantic_lines || has_snippet_lines)
                        && !has_paragraphs
                    {
                        return false;
                    }

                    // If we have paragraphs, check if they span multiple lines or there are multiple blocks
                    if has_paragraphs {
                        let paragraph_count = blocks.iter().filter(|b| matches!(b, Block::Paragraph(_))).count();
                        if paragraph_count > 1 {
                            // Multiple paragraph blocks should be normalized
                            return true;
                        }

                        // Single paragraph block: normalize if it has multiple content lines
                        if content_lines.len() > 1 {
                            return true;
                        }
                    }

                    false
                };

                let needs_reflow = match config.reflow_mode {
                    ReflowMode::Normalize => {
                        // Only reflow if:
                        // 1. The combined line would exceed the limit, OR
                        // 2. The list item should be normalized (has multi-line plain text)
                        let combined_length = self.calculate_effective_length(&full_line);
                        if combined_length > config.line_length.get() {
                            true
                        } else {
                            should_normalize()
                        }
                    }
                    ReflowMode::SentencePerLine => {
                        // Check if list item has multiple sentences
                        let sentences = split_into_sentences(&combined_content);
                        sentences.len() > 1
                    }
                    ReflowMode::Default => {
                        // In default mode, only reflow if any individual line exceeds limit
                        // Check the original lines, not the combined content
                        (list_start..i)
                            .any(|line_idx| self.calculate_effective_length(lines[line_idx]) > config.line_length.get())
                    }
                };

                if needs_reflow {
                    let start_range = line_index.whole_line_range(list_start + 1);
                    let end_line = i - 1;
                    let end_range = if end_line == lines.len() - 1 && !ctx.content.ends_with('\n') {
                        line_index.line_text_range(end_line + 1, 1, lines[end_line].len() + 1)
                    } else {
                        line_index.whole_line_range(end_line + 1)
                    };
                    let byte_range = start_range.start..end_range.end;

                    // Reflow each block (paragraphs only, preserve code blocks)
                    // When line_length = 0 (no limit), use a very large value for reflow
                    let reflow_line_length = if config.line_length.is_unlimited() {
                        usize::MAX
                    } else {
                        config.line_length.get().saturating_sub(indent_size).max(1)
                    };
                    let reflow_options = crate::utils::text_reflow::ReflowOptions {
                        line_length: reflow_line_length,
                        break_on_sentences: true,
                        preserve_breaks: false,
                        sentence_per_line: config.reflow_mode == ReflowMode::SentencePerLine,
                        abbreviations: config.abbreviations_for_reflow(),
                    };

                    let mut result: Vec<String> = Vec::new();
                    let mut is_first_block = true;

                    for (block_idx, block) in blocks.iter().enumerate() {
                        match block {
                            Block::Paragraph(para_lines) => {
                                // Split the paragraph into segments at hard break boundaries
                                // Each segment can be reflowed independently
                                let segments = split_into_segments(para_lines);

                                for (segment_idx, segment) in segments.iter().enumerate() {
                                    // Check if this segment ends with a hard break and what type
                                    let hard_break_type = segment.last().and_then(|line| {
                                        let line = line.strip_suffix('\r').unwrap_or(line);
                                        if line.ends_with('\\') {
                                            Some("\\")
                                        } else if line.ends_with("  ") {
                                            Some("  ")
                                        } else {
                                            None
                                        }
                                    });

                                    // Join and reflow the segment (removing the hard break marker for processing)
                                    let segment_for_reflow: Vec<String> = segment
                                        .iter()
                                        .map(|line| {
                                            // Strip hard break marker (2 spaces or backslash) for reflow processing
                                            if line.ends_with('\\') {
                                                line[..line.len() - 1].trim_end().to_string()
                                            } else if line.ends_with("  ") {
                                                line[..line.len() - 2].trim_end().to_string()
                                            } else {
                                                line.clone()
                                            }
                                        })
                                        .collect();

                                    let segment_text = segment_for_reflow.join(" ").trim().to_string();
                                    if !segment_text.is_empty() {
                                        let reflowed =
                                            crate::utils::text_reflow::reflow_line(&segment_text, &reflow_options);

                                        if is_first_block && segment_idx == 0 {
                                            // First segment of first block starts with marker
                                            result.push(format!("{marker}{}", reflowed[0]));
                                            for line in reflowed.iter().skip(1) {
                                                result.push(format!("{expected_indent}{line}"));
                                            }
                                            is_first_block = false;
                                        } else {
                                            // Subsequent segments
                                            for line in reflowed {
                                                result.push(format!("{expected_indent}{line}"));
                                            }
                                        }

                                        // If this segment had a hard break, add it back to the last line
                                        // Preserve the original hard break format (backslash or two spaces)
                                        if let Some(break_marker) = hard_break_type
                                            && let Some(last_line) = result.last_mut()
                                        {
                                            last_line.push_str(break_marker);
                                        }
                                    }
                                }

                                // Add blank line after paragraph block if there's a next block
                                // BUT: check if next block is a code block that doesn't want a preceding blank
                                // Also don't add blank lines before snippet lines (they should stay tight)
                                if block_idx < blocks.len() - 1 {
                                    let next_block = &blocks[block_idx + 1];
                                    let should_add_blank = match next_block {
                                        Block::Code {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::SnippetLine(_) => false, // No blank line before snippet delimiters
                                        _ => true,                      // For all other blocks, add blank line
                                    };
                                    if should_add_blank {
                                        result.push(String::new());
                                    }
                                }
                            }
                            Block::Code {
                                lines: code_lines,
                                has_preceding_blank: _,
                            } => {
                                // Preserve code blocks as-is with original indentation
                                // NOTE: Blank line before code block is handled by the previous block
                                // (see paragraph block's logic above)

                                for (idx, (content, orig_indent)) in code_lines.iter().enumerate() {
                                    if is_first_block && idx == 0 {
                                        // First line of first block gets marker
                                        result.push(format!(
                                            "{marker}{}",
                                            " ".repeat(orig_indent - marker_len) + content
                                        ));
                                        is_first_block = false;
                                    } else if content.is_empty() {
                                        result.push(String::new());
                                    } else {
                                        result.push(format!("{}{}", " ".repeat(*orig_indent), content));
                                    }
                                }
                            }
                            Block::NestedList(nested_items) => {
                                // Preserve nested list items as-is with original indentation
                                if !is_first_block {
                                    result.push(String::new());
                                }

                                for (idx, (content, orig_indent)) in nested_items.iter().enumerate() {
                                    if is_first_block && idx == 0 {
                                        // First line of first block gets marker
                                        result.push(format!(
                                            "{marker}{}",
                                            " ".repeat(orig_indent - marker_len) + content
                                        ));
                                        is_first_block = false;
                                    } else if content.is_empty() {
                                        result.push(String::new());
                                    } else {
                                        result.push(format!("{}{}", " ".repeat(*orig_indent), content));
                                    }
                                }

                                // Add blank line after nested list if there's a next block
                                // Check if next block is a code block that doesn't want a preceding blank
                                if block_idx < blocks.len() - 1 {
                                    let next_block = &blocks[block_idx + 1];
                                    let should_add_blank = match next_block {
                                        Block::Code {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::SnippetLine(_) => false, // No blank line before snippet delimiters
                                        _ => true,                      // For all other blocks, add blank line
                                    };
                                    if should_add_blank {
                                        result.push(String::new());
                                    }
                                }
                            }
                            Block::SemanticLine(content) => {
                                // Preserve semantic lines (NOTE:, WARNING:, etc.) as-is on their own line
                                // Add blank line before if not first block
                                if !is_first_block {
                                    result.push(String::new());
                                }

                                if is_first_block {
                                    // First block starts with marker
                                    result.push(format!("{marker}{content}"));
                                    is_first_block = false;
                                } else {
                                    // Subsequent blocks use expected indent
                                    result.push(format!("{expected_indent}{content}"));
                                }

                                // Add blank line after semantic line if there's a next block
                                // Check if next block is a code block that doesn't want a preceding blank
                                if block_idx < blocks.len() - 1 {
                                    let next_block = &blocks[block_idx + 1];
                                    let should_add_blank = match next_block {
                                        Block::Code {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::SnippetLine(_) => false, // No blank line before snippet delimiters
                                        _ => true,                      // For all other blocks, add blank line
                                    };
                                    if should_add_blank {
                                        result.push(String::new());
                                    }
                                }
                            }
                            Block::SnippetLine(content) => {
                                // Preserve snippet delimiters (-8<-) as-is on their own line
                                // Unlike semantic lines, snippet lines don't add extra blank lines
                                if is_first_block {
                                    // First block starts with marker
                                    result.push(format!("{marker}{content}"));
                                    is_first_block = false;
                                } else {
                                    // Subsequent blocks use expected indent
                                    result.push(format!("{expected_indent}{content}"));
                                }
                                // No blank lines added before or after snippet delimiters
                            }
                            Block::Html {
                                lines: html_lines,
                                has_preceding_blank: _,
                            } => {
                                // Preserve HTML blocks exactly as-is with original indentation
                                // NOTE: Blank line before HTML block is handled by the previous block

                                for (idx, line) in html_lines.iter().enumerate() {
                                    if is_first_block && idx == 0 {
                                        // First line of first block gets marker
                                        result.push(format!("{marker}{line}"));
                                        is_first_block = false;
                                    } else if line.is_empty() {
                                        // Preserve blank lines inside HTML blocks
                                        result.push(String::new());
                                    } else {
                                        // Preserve lines with their original content (already includes indentation)
                                        result.push(format!("{expected_indent}{line}"));
                                    }
                                }

                                // Add blank line after HTML block if there's a next block
                                if block_idx < blocks.len() - 1 {
                                    let next_block = &blocks[block_idx + 1];
                                    let should_add_blank = match next_block {
                                        Block::Code {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::Html {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::SnippetLine(_) => false, // No blank line before snippet delimiters
                                        _ => true,                      // For all other blocks, add blank line
                                    };
                                    if should_add_blank {
                                        result.push(String::new());
                                    }
                                }
                            }
                        }
                    }

                    let reflowed_text = result.join("\n");

                    // Preserve trailing newline
                    let replacement = if end_line < lines.len() - 1 || ctx.content.ends_with('\n') {
                        format!("{reflowed_text}\n")
                    } else {
                        reflowed_text
                    };

                    // Get the original text to compare
                    let original_text = &ctx.content[byte_range.clone()];

                    // Only generate a warning if the replacement is different from the original
                    if original_text != replacement {
                        // Generate an appropriate message based on why reflow is needed
                        let message = match config.reflow_mode {
                            ReflowMode::SentencePerLine => {
                                let num_sentences = split_into_sentences(&combined_content).len();
                                let num_lines = content_lines.len();
                                if num_lines == 1 {
                                    // Single line with multiple sentences
                                    format!("Line contains {num_sentences} sentences (one sentence per line required)")
                                } else {
                                    // Multiple lines - could be split sentences or mixed
                                    format!(
                                        "Paragraph should have one sentence per line (found {num_sentences} sentences across {num_lines} lines)"
                                    )
                                }
                            }
                            ReflowMode::Normalize => {
                                let combined_length = self.calculate_effective_length(&full_line);
                                if combined_length > config.line_length.get() {
                                    format!(
                                        "Line length {} exceeds {} characters",
                                        combined_length,
                                        config.line_length.get()
                                    )
                                } else {
                                    "Multi-line content can be normalized".to_string()
                                }
                            }
                            ReflowMode::Default => {
                                let combined_length = self.calculate_effective_length(&full_line);
                                format!(
                                    "Line length {} exceeds {} characters",
                                    combined_length,
                                    config.line_length.get()
                                )
                            }
                        };

                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            message,
                            line: list_start + 1,
                            column: 1,
                            end_line: end_line + 1,
                            end_column: lines[end_line].len() + 1,
                            severity: Severity::Warning,
                            fix: Some(crate::rule::Fix {
                                range: byte_range,
                                replacement,
                            }),
                        });
                    }
                }
                continue;
            }

            // Found start of a paragraph - collect all lines in it
            let paragraph_start = i;
            let mut paragraph_lines = vec![lines[i]];
            i += 1;

            while i < lines.len() {
                let next_line = lines[i];
                let next_line_num = i + 1;
                let next_trimmed = next_line.trim();

                // Stop at paragraph boundaries
                if next_trimmed.is_empty()
                    || ctx.line_info(next_line_num).is_some_and(|info| info.in_code_block)
                    || ctx.line_info(next_line_num).is_some_and(|info| info.in_front_matter)
                    || ctx.line_info(next_line_num).is_some_and(|info| info.in_html_block)
                    || ctx.line_info(next_line_num).is_some_and(|info| info.in_html_comment)
                    || ctx.line_info(next_line_num).is_some_and(|info| info.in_esm_block)
                    || ctx.line_info(next_line_num).is_some_and(|info| info.in_jsx_expression)
                    || ctx.line_info(next_line_num).is_some_and(|info| info.in_mdx_comment)
                    || (next_line_num > 0
                        && next_line_num <= ctx.lines.len()
                        && ctx.lines[next_line_num - 1].blockquote.is_some())
                    || next_trimmed.starts_with('#')
                    || TableUtils::is_potential_table_row(next_line)
                    || is_list_item(next_trimmed)
                    || is_horizontal_rule(next_trimmed)
                    || (next_trimmed.starts_with('[') && next_line.contains("]:"))
                    || is_template_directive_only(next_line)
                    || is_standalone_attr_list(next_line)
                    || is_snippet_block_delimiter(next_line)
                {
                    break;
                }

                // Check if the previous line ends with a hard break (2+ spaces or backslash)
                if i > 0 && has_hard_break(lines[i - 1]) {
                    // Don't include lines after hard breaks in the same paragraph
                    break;
                }

                paragraph_lines.push(next_line);
                i += 1;
            }

            // Combine paragraph lines into a single string for processing
            // This must be done BEFORE the needs_reflow check for sentence-per-line mode
            let paragraph_text = paragraph_lines.join(" ");

            // Skip reflowing if this paragraph contains definition list items
            // Definition lists are multi-line structures that should not be joined
            let contains_definition_list = paragraph_lines
                .iter()
                .any(|line| crate::utils::is_definition_list_item(line));

            if contains_definition_list {
                // Don't reflow definition lists - skip this paragraph
                i = paragraph_start + paragraph_lines.len();
                continue;
            }

            // Skip reflowing if this paragraph contains MkDocs Snippets markers
            // Snippets blocks (-8<- ... -8<-) should be preserved exactly
            let contains_snippets = paragraph_lines.iter().any(|line| is_snippet_block_delimiter(line));

            if contains_snippets {
                // Don't reflow Snippets blocks - skip this paragraph
                i = paragraph_start + paragraph_lines.len();
                continue;
            }

            // Check if this paragraph needs reflowing
            let needs_reflow = match config.reflow_mode {
                ReflowMode::Normalize => {
                    // In normalize mode, reflow multi-line paragraphs
                    paragraph_lines.len() > 1
                }
                ReflowMode::SentencePerLine => {
                    // In sentence-per-line mode, check if the JOINED paragraph has multiple sentences
                    // Note: we check the joined text because sentences can span multiple lines
                    let sentences = split_into_sentences(&paragraph_text);

                    // Always reflow if multiple sentences on one line
                    if sentences.len() > 1 {
                        true
                    } else if paragraph_lines.len() > 1 {
                        // For single-sentence paragraphs spanning multiple lines:
                        // Reflow if they COULD fit on one line (respecting line-length constraint)
                        if config.line_length.is_unlimited() {
                            // No line-length constraint - always join single sentences
                            true
                        } else {
                            // Only join if it fits within line-length
                            let effective_length = self.calculate_effective_length(&paragraph_text);
                            effective_length <= config.line_length.get()
                        }
                    } else {
                        false
                    }
                }
                ReflowMode::Default => {
                    // In default mode, only reflow if lines exceed limit
                    paragraph_lines
                        .iter()
                        .any(|line| self.calculate_effective_length(line) > config.line_length.get())
                }
            };

            if needs_reflow {
                // Calculate byte range for this paragraph
                // Use whole_line_range for each line and combine
                let start_range = line_index.whole_line_range(paragraph_start + 1);
                let end_line = paragraph_start + paragraph_lines.len() - 1;

                // For the last line, we want to preserve any trailing newline
                let end_range = if end_line == lines.len() - 1 && !ctx.content.ends_with('\n') {
                    // Last line without trailing newline - use line_text_range
                    line_index.line_text_range(end_line + 1, 1, lines[end_line].len() + 1)
                } else {
                    // Not the last line or has trailing newline - use whole_line_range
                    line_index.whole_line_range(end_line + 1)
                };

                let byte_range = start_range.start..end_range.end;

                // Check if the paragraph ends with a hard break and what type
                let hard_break_type = paragraph_lines.last().and_then(|line| {
                    let line = line.strip_suffix('\r').unwrap_or(line);
                    if line.ends_with('\\') {
                        Some("\\")
                    } else if line.ends_with("  ") {
                        Some("  ")
                    } else {
                        None
                    }
                });

                // Reflow the paragraph
                // When line_length = 0 (no limit), use a very large value for reflow
                let reflow_line_length = if config.line_length.is_unlimited() {
                    usize::MAX
                } else {
                    config.line_length.get()
                };
                let reflow_options = crate::utils::text_reflow::ReflowOptions {
                    line_length: reflow_line_length,
                    break_on_sentences: true,
                    preserve_breaks: false,
                    sentence_per_line: config.reflow_mode == ReflowMode::SentencePerLine,
                    abbreviations: config.abbreviations_for_reflow(),
                };
                let mut reflowed = crate::utils::text_reflow::reflow_line(&paragraph_text, &reflow_options);

                // If the original paragraph ended with a hard break, preserve it
                // Preserve the original hard break format (backslash or two spaces)
                if let Some(break_marker) = hard_break_type
                    && !reflowed.is_empty()
                {
                    let last_idx = reflowed.len() - 1;
                    if !has_hard_break(&reflowed[last_idx]) {
                        reflowed[last_idx].push_str(break_marker);
                    }
                }

                let reflowed_text = reflowed.join("\n");

                // Preserve trailing newline if the original paragraph had one
                let replacement = if end_line < lines.len() - 1 || ctx.content.ends_with('\n') {
                    format!("{reflowed_text}\n")
                } else {
                    reflowed_text
                };

                // Get the original text to compare
                let original_text = &ctx.content[byte_range.clone()];

                // Only generate a warning if the replacement is different from the original
                if original_text != replacement {
                    // Create warning with actual fix
                    // In default mode, report the specific line that violates
                    // In normalize mode, report the whole paragraph
                    // In sentence-per-line mode, report the entire paragraph
                    let (warning_line, warning_end_line) = match config.reflow_mode {
                        ReflowMode::Normalize => (paragraph_start + 1, end_line + 1),
                        ReflowMode::SentencePerLine => {
                            // Highlight the entire paragraph that needs reformatting
                            (paragraph_start + 1, paragraph_start + paragraph_lines.len())
                        }
                        ReflowMode::Default => {
                            // Find the first line that exceeds the limit
                            let mut violating_line = paragraph_start;
                            for (idx, line) in paragraph_lines.iter().enumerate() {
                                if self.calculate_effective_length(line) > config.line_length.get() {
                                    violating_line = paragraph_start + idx;
                                    break;
                                }
                            }
                            (violating_line + 1, violating_line + 1)
                        }
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        message: match config.reflow_mode {
                            ReflowMode::Normalize => format!(
                                "Paragraph could be normalized to use line length of {} characters",
                                config.line_length.get()
                            ),
                            ReflowMode::SentencePerLine => {
                                let num_sentences = split_into_sentences(&paragraph_text).len();
                                if paragraph_lines.len() == 1 {
                                    // Single line with multiple sentences
                                    format!("Line contains {num_sentences} sentences (one sentence per line required)")
                                } else {
                                    let num_lines = paragraph_lines.len();
                                    // Multiple lines - could be split sentences or mixed
                                    format!("Paragraph should have one sentence per line (found {num_sentences} sentences across {num_lines} lines)")
                                }
                            },
                            ReflowMode::Default => format!("Line length exceeds {} characters", config.line_length.get()),
                        },
                        line: warning_line,
                        column: 1,
                        end_line: warning_end_line,
                        end_column: lines[warning_end_line.saturating_sub(1)].len() + 1,
                        severity: Severity::Warning,
                        fix: Some(crate::rule::Fix {
                            range: byte_range,
                            replacement,
                        }),
                    });
                }
            }
        }

        warnings
    }

    /// Calculate string length based on the configured length mode
    fn calculate_string_length(&self, s: &str) -> usize {
        match self.config.length_mode {
            LengthMode::Chars => s.chars().count(),
            LengthMode::Visual => s.width(),
            LengthMode::Bytes => s.len(),
        }
    }

    /// Calculate effective line length excluding unbreakable URLs
    fn calculate_effective_length(&self, line: &str) -> usize {
        if self.config.strict {
            // In strict mode, count everything
            return self.calculate_string_length(line);
        }

        // Quick byte-level check: if line doesn't contain "http" or "[", it can't have URLs or markdown links
        let bytes = line.as_bytes();
        if !bytes.contains(&b'h') && !bytes.contains(&b'[') {
            return self.calculate_string_length(line);
        }

        // More precise check for URLs and links
        if !line.contains("http") && !line.contains('[') {
            return self.calculate_string_length(line);
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

        self.calculate_string_length(&effective_line)
    }
}
