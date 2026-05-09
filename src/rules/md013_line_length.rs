/// Rule MD013: Line length
///
/// See [docs/md013.md](../../docs/md013.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::mkdocs_admonitions;
use crate::utils::mkdocs_attr_list::is_standalone_attr_list;
use crate::utils::mkdocs_snippets::is_snippet_block_delimiter;
use crate::utils::mkdocs_tabs;
use crate::utils::range_utils::LineIndex;
use crate::utils::range_utils::calculate_excess_range;
use crate::utils::regex_cache::{IMAGE_REF_PATTERN, LINK_REF_PATTERN, URL_PATTERN};
use crate::utils::table_utils::TableUtils;
use crate::utils::text_reflow::{
    BlockquoteLineData, ReflowLengthMode, blockquote_continuation_style, dominant_blockquote_prefix,
    reflow_blockquote_content, split_into_sentences,
};
use pulldown_cmark::LinkType;
use toml;

mod block_builder;
mod helpers;
pub mod md013_config;
use crate::utils::is_template_directive_only;
use block_builder::{Block, BlockBuilder};
use helpers::{
    extract_list_marker_and_content, has_hard_break, is_github_alert_marker, is_horizontal_rule, is_html_only_line,
    is_list_item, is_standalone_link_or_image_line, is_unwrappable_line, split_into_segments,
    trim_preserving_hard_break,
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

/// Blockquote paragraph line collected for reflow, with original line index for range computation.
struct CollectedBlockquoteLine {
    line_idx: usize,
    data: BlockquoteLineData,
}

impl MD013LineLength {
    pub fn new(line_length: usize, code_blocks: bool, tables: bool, headings: bool, strict: bool) -> Self {
        Self {
            config: MD013Config {
                line_length: crate::types::LineLength::new(line_length),
                code_blocks,
                tables,
                headings,
                paragraphs: true,  // Default to true for backwards compatibility
                blockquotes: true, // Default to true for backwards compatibility
                strict,
                stern: false,
                heading_line_length: None,
                code_block_line_length: None,
                reflow: false,
                reflow_mode: ReflowMode::default(),
                length_mode: LengthMode::default(),
                abbreviations: Vec::new(),
                require_sentence_capital: true,
            },
        }
    }

    pub fn from_config_struct(config: MD013Config) -> Self {
        Self { config }
    }

    /// Return a clone with code block checking disabled.
    /// Used for doc comment linting where code blocks are Rust code managed by rustfmt.
    pub fn with_code_blocks_disabled(&self) -> Self {
        let mut clone = self.clone();
        clone.config.code_blocks = false;
        clone
    }

    /// Convert MD013 LengthMode to text_reflow ReflowLengthMode
    fn reflow_length_mode(&self) -> ReflowLengthMode {
        match self.config.length_mode {
            LengthMode::Chars => ReflowLengthMode::Chars,
            LengthMode::Visual => ReflowLengthMode::Visual,
            LengthMode::Bytes => ReflowLengthMode::Bytes,
        }
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

        // Note: link reference definitions are handled as always-exempt (even in strict mode)
        // in the main check loop, so they don't need to be checked here.

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

        // For sentence-per-line, semantic-line-breaks, or normalize mode, never skip based on line length
        if config.reflow
            && (config.reflow_mode == ReflowMode::SentencePerLine
                || config.reflow_mode == ReflowMode::SemanticLineBreaks
                || config.reflow_mode == ReflowMode::Normalize)
        {
            return false;
        }

        // Use the smallest applicable budget across line/heading/code-block
        // contexts so a stricter context-specific limit doesn't get masked by
        // the document-wide budget.
        let min_limit = config.min_effective_line_length();
        if min_limit.is_unlimited() {
            return true;
        }
        let min_limit_bytes = min_limit.get();

        // Quick check: if total content is shorter than the smallest line limit,
        // definitely skip.
        if ctx.content.len() <= min_limit_bytes {
            return true;
        }

        // Skip if no line exceeds the smallest applicable limit.
        !ctx.lines.iter().any(|line| line.byte_len > min_limit_bytes)
    }

    fn normalize_mode_needs_reflow<'a, I>(&self, lines: I, config: &MD013Config) -> bool
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut line_count = 0;
        let check_length = !config.line_length.is_unlimited();

        for line in lines {
            line_count += 1;
            if check_length && self.calculate_effective_length(line) > config.line_length.get() {
                return true;
            }
        }

        line_count > 1
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
        // Use pre-parsed inline config from LintContext
        let config_override = ctx.inline_config().get_rule_config("MD013");

        // Apply configuration override if present
        let effective_config = if let Some(json_config) = config_override {
            if let Some(obj) = json_config.as_object() {
                let mut config = self.config.clone();
                if let Some(line_length) = obj.get("line_length").and_then(serde_json::Value::as_u64) {
                    config.line_length = crate::types::LineLength::new(line_length as usize);
                }
                if let Some(code_blocks) = obj.get("code_blocks").and_then(serde_json::Value::as_bool) {
                    config.code_blocks = code_blocks;
                }
                if let Some(tables) = obj.get("tables").and_then(serde_json::Value::as_bool) {
                    config.tables = tables;
                }
                if let Some(headings) = obj.get("headings").and_then(serde_json::Value::as_bool) {
                    config.headings = headings;
                }
                if let Some(blockquotes) = obj.get("blockquotes").and_then(serde_json::Value::as_bool) {
                    config.blockquotes = blockquotes;
                }
                if let Some(strict) = obj.get("strict").and_then(serde_json::Value::as_bool) {
                    config.strict = strict;
                }
                if let Some(reflow) = obj.get("reflow").and_then(serde_json::Value::as_bool) {
                    config.reflow = reflow;
                }
                if let Some(reflow_mode) = obj.get("reflow_mode").and_then(|v| v.as_str()) {
                    config.reflow_mode = match reflow_mode {
                        "default" => ReflowMode::Default,
                        "normalize" => ReflowMode::Normalize,
                        "sentence-per-line" => ReflowMode::SentencePerLine,
                        "semantic-line-breaks" => ReflowMode::SemanticLineBreaks,
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
                    || effective_config.reflow_mode == ReflowMode::SentencePerLine
                    || effective_config.reflow_mode == ReflowMode::SemanticLineBreaks))
        {
            return Ok(Vec::new());
        }

        // Direct implementation without DocumentStructure
        let mut warnings = Vec::new();

        // Special handling: line_length = 0 means "no line length limit"
        // Skip all line length checks, but still allow reflow if enabled
        let skip_length_checks = effective_config.line_length.is_unlimited();

        // Pre-filter lines that could be problematic to avoid processing all lines.
        // Use the smallest applicable budget across line/heading/code-block contexts
        // so candidates aren't dropped when a stricter context-specific budget applies.
        let prefilter_limit = effective_config.min_effective_line_length();
        let prefilter_skip = prefilter_limit.is_unlimited();
        let mut candidate_lines = Vec::new();
        if !skip_length_checks && !prefilter_skip {
            for (line_idx, line_info) in ctx.lines.iter().enumerate() {
                // Skip front matter - it should never be linted
                if line_info.in_front_matter {
                    continue;
                }

                // Quick length check first
                if line_info.byte_len > prefilter_limit.get() {
                    candidate_lines.push(line_idx);
                }
            }
        }

        // If no candidate lines and not in normalize or sentence-per-line mode, early return
        if candidate_lines.is_empty()
            && !(effective_config.reflow
                && (effective_config.reflow_mode == ReflowMode::Normalize
                    || effective_config.reflow_mode == ReflowMode::SentencePerLine
                    || effective_config.reflow_mode == ReflowMode::SemanticLineBreaks))
        {
            return Ok(warnings);
        }

        let lines = ctx.raw_lines();

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
        'line_loop: for &line_idx in &candidate_lines {
            let line_number = line_idx + 1;
            let line = lines[line_idx];

            // Calculate actual line length (used in warning messages)
            let effective_length = self.calculate_effective_length(line);

            // Pick the context-specific limit: heading > code-block > paragraph.
            // Headings dominate over code-block context if a setext underline ever
            // overlaps a fenced range (defensive — these are mutually exclusive in
            // practice, but the explicit ordering documents intent).
            let is_heading_line = heading_lines_set.contains(&line_number);
            let in_code_block = ctx.line_info(line_number).is_some_and(|info| info.in_code_block);
            let line_limit = if is_heading_line {
                effective_config.effective_heading_line_length().get()
            } else if in_code_block {
                effective_config.effective_code_block_line_length().get()
            } else {
                effective_config.line_length.get()
            };

            // A context-specific limit of 0 means "unlimited for this context".
            if line_limit == 0 {
                continue;
            }

            // Stern mode: like default, but the trailing-token forgiveness is
            // disabled — a line with whitespace that exceeds the limit is a
            // violation even if the excess is the final token. The "unwrappable"
            // line exemption (single token, optionally prefixed by # or >) is
            // still honored. Strict overrides stern entirely.
            if effective_config.stern && !effective_config.strict && is_unwrappable_line(line) {
                continue;
            }

            // Trailing-token forgiveness: only in default mode (not strict, not stern).
            // If the line only exceeds the limit because of a long token at the end
            // (URL, link chain, identifier), it passes. This matches markdownlint's
            // behavior: line.replace(/\S*$/u, "#")
            let check_length = if effective_config.strict || effective_config.stern {
                effective_length
            } else {
                match line.rfind(char::is_whitespace) {
                    Some(pos) => {
                        let ws_char = line[pos..].chars().next().unwrap();
                        let prefix_end = pos + ws_char.len_utf8();
                        self.calculate_string_length(&line[..prefix_end]) + 1
                    }
                    None => 1, // No whitespace — entire line is a single token
                }
            };

            // Skip lines where the check length is within the limit
            if check_length <= line_limit {
                continue;
            }

            // Semantic link understanding: suppress when excess comes entirely from inline URLs
            if !effective_config.strict {
                let text_only_length = self.calculate_text_only_length(effective_length, line_number, ctx);
                if text_only_length <= line_limit {
                    continue;
                }
            }

            // Skip mkdocstrings and pymdown blocks (already handled by LintContext)
            if ctx.lines[line_idx].in_mkdocstrings || ctx.lines[line_idx].in_pymdown_block {
                continue;
            }

            // Link reference definitions are always exempt, even in strict mode.
            // There's no way to shorten them without breaking the URL.
            // Also check after stripping list markers, since list items may
            // contain link ref defs as their content.
            {
                let trimmed = line.trim();
                if trimmed.starts_with('[') && trimmed.contains("]:") && LINK_REF_PATTERN.is_match(trimmed) {
                    continue;
                }
                if is_list_item(trimmed) {
                    let (_, content) = extract_list_marker_and_content(trimmed);
                    let content_trimmed = content.trim();
                    if content_trimmed.starts_with('[')
                        && content_trimmed.contains("]:")
                        && LINK_REF_PATTERN.is_match(content_trimmed)
                    {
                        continue;
                    }
                }
            }

            // Skip various block types efficiently
            if !effective_config.strict {
                // Lines whose only content is a link/image are exempt.
                // After stripping list markers, blockquote markers, and emphasis,
                // if only a link or image remains, there is no way to shorten it.
                if is_standalone_link_or_image_line(line) {
                    continue;
                }

                // Lines consisting entirely of HTML tags are exempt.
                // Badge lines, images with attributes, and similar inline HTML
                // are long due to URLs in attributes and can't be meaningfully shortened.
                if is_html_only_line(line) {
                    continue;
                }

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
                    || ctx.line_info(line_number).is_some_and(|info| info.in_html_block)
                    || ctx.line_info(line_number).is_some_and(|info| info.in_html_comment)
                    || ctx.line_info(line_number).is_some_and(|info| info.in_esm_block)
                    || ctx.line_info(line_number).is_some_and(|info| info.in_jsx_expression)
                    || ctx.line_info(line_number).is_some_and(|info| info.in_jsx_block)
                    || ctx.line_info(line_number).is_some_and(|info| info.in_mdx_comment)
                    || ctx.line_info(line_number).is_some_and(|info| info.in_pymdown_block)
                {
                    continue;
                }

                // Check if this is a paragraph/regular text line
                // If paragraphs = false, skip lines that are NOT in special blocks
                // Blockquote content is treated as paragraph text, so it's not
                // included in the special blocks list here.
                if !effective_config.paragraphs {
                    let is_special_block = heading_lines_set.contains(&line_number)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_code_block)
                        || table_lines_set.contains(&line_number)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_html_block)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_html_comment)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_esm_block)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_jsx_expression)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_jsx_block)
                        || ctx.line_info(line_number).is_some_and(|info| info.in_mdx_comment)
                        || ctx
                            .line_info(line_number)
                            .is_some_and(super::super::lint_context::types::LineInfo::in_mkdocs_container);

                    // Skip regular paragraph text when paragraphs = false
                    if !is_special_block {
                        continue;
                    }
                }

                // Skip blockquote lines when blockquotes = false.
                // Also skip lazy continuation lines that belong to a blockquote
                // (lines without `>` prefix that follow a blockquote line).
                if !effective_config.blockquotes {
                    if ctx.lines[line_number - 1].blockquote.is_some() {
                        continue;
                    }
                    // Check for lazy continuation: scan backwards through
                    // non-blank lines to find if this paragraph started with
                    // a blockquote marker
                    if !line.trim().is_empty() {
                        let mut scan = line_number.saturating_sub(2);
                        loop {
                            if ctx.lines[scan].blockquote.is_some() {
                                // Found a blockquote ancestor — this is a lazy continuation
                                continue 'line_loop;
                            }
                            if lines[scan].trim().is_empty() || scan == 0 {
                                break;
                            }
                            scan -= 1;
                        }
                    }
                }

                // Skip lines that are only a URL, image ref, or link ref
                if self.should_ignore_line(line, lines, line_idx, ctx) {
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

            // In semantic-line-breaks mode, skip per-line checks —
            // all reflow is handled at the paragraph level with cascading splits
            if effective_config.reflow_mode == ReflowMode::SemanticLineBreaks {
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
            let paragraph_warnings = self.generate_paragraph_fixes(ctx, &effective_config, lines);
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
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());

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
        let table = crate::rule_config_serde::config_schema_table(&MD013Config::default())?;
        if table.is_empty() {
            None
        } else {
            Some((MD013Config::RULE_NAME.to_string(), toml::Value::Table(table)))
        }
    }

    fn config_aliases(&self) -> Option<std::collections::HashMap<String, String>> {
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("enable_reflow".to_string(), "reflow".to_string());
        aliases.insert("strict_sentences".to_string(), "require-sentence-capital".to_string());
        aliases.insert("strict-sentences".to_string(), "require-sentence-capital".to_string());
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
    fn is_blockquote_content_boundary(
        &self,
        content: &str,
        line_num: usize,
        ctx: &crate::lint_context::LintContext,
    ) -> bool {
        let trimmed = content.trim();

        trimmed.is_empty()
            || ctx.line_info(line_num).is_some_and(|info| {
                info.in_code_block
                    || info.in_front_matter
                    || info.in_html_block
                    || info.in_html_comment
                    || info.in_esm_block
                    || info.in_jsx_expression
                    || info.in_jsx_block
                    || info.in_mdx_comment
                    || info.in_mkdocstrings
                    || info.in_pymdown_block
                    || info.in_mkdocs_container()
                    || info.is_div_marker
            })
            || trimmed.starts_with('#')
            || trimmed.starts_with("```")
            || trimmed.starts_with("~~~")
            || trimmed.starts_with('>')
            || TableUtils::is_potential_table_row(content)
            || is_list_item(trimmed)
            || is_horizontal_rule(content)
            || (trimmed.starts_with('[') && content.contains("]:"))
            || is_template_directive_only(content)
            || is_standalone_attr_list(content)
            || is_snippet_block_delimiter(content)
            || is_github_alert_marker(trimmed)
            || is_html_only_line(content)
    }

    fn generate_blockquote_paragraph_fix(
        &self,
        ctx: &crate::lint_context::LintContext,
        config: &MD013Config,
        lines: &[&str],
        line_index: &LineIndex,
        start_idx: usize,
        line_ending: &str,
    ) -> (Option<LintWarning>, usize) {
        let Some(start_bq) = ctx.lines.get(start_idx).and_then(|line| line.blockquote.as_deref()) else {
            return (None, start_idx + 1);
        };
        let target_level = start_bq.nesting_level;

        let mut collected: Vec<CollectedBlockquoteLine> = Vec::new();
        let mut i = start_idx;

        while i < lines.len() {
            if !collected.is_empty() && has_hard_break(&collected[collected.len() - 1].data.content) {
                break;
            }

            let line_num = i + 1;
            if line_num > ctx.lines.len() {
                break;
            }

            if lines[i].trim().is_empty() {
                break;
            }

            let line_bq = ctx.lines[i].blockquote.as_deref();
            if let Some(bq) = line_bq {
                if bq.nesting_level != target_level {
                    break;
                }

                if self.is_blockquote_content_boundary(&bq.content, line_num, ctx) {
                    break;
                }

                collected.push(CollectedBlockquoteLine {
                    line_idx: i,
                    data: BlockquoteLineData::explicit(trim_preserving_hard_break(&bq.content), bq.prefix.clone()),
                });
                i += 1;
                continue;
            }

            let lazy_content = lines[i].trim_start();
            if self.is_blockquote_content_boundary(lazy_content, line_num, ctx) {
                break;
            }

            collected.push(CollectedBlockquoteLine {
                line_idx: i,
                data: BlockquoteLineData::lazy(trim_preserving_hard_break(lazy_content)),
            });
            i += 1;
        }

        if collected.is_empty() {
            return (None, start_idx + 1);
        }

        let next_idx = i;
        let paragraph_start = collected[0].line_idx;
        let end_line = collected[collected.len() - 1].line_idx;
        let line_data: Vec<BlockquoteLineData> = collected.iter().map(|l| l.data.clone()).collect();
        let paragraph_text = line_data
            .iter()
            .map(|d| d.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let contains_definition_list = line_data
            .iter()
            .any(|d| crate::utils::is_definition_list_item(&d.content));
        if contains_definition_list {
            return (None, next_idx);
        }

        let contains_snippets = line_data.iter().any(|d| is_snippet_block_delimiter(&d.content));
        if contains_snippets {
            return (None, next_idx);
        }

        let needs_reflow = match config.reflow_mode {
            ReflowMode::Normalize => {
                self.normalize_mode_needs_reflow(line_data.iter().map(|d| d.content.as_str()), config)
            }
            ReflowMode::SentencePerLine => {
                let sentences = split_into_sentences(&paragraph_text);
                sentences.len() > 1 || line_data.len() > 1
            }
            ReflowMode::SemanticLineBreaks => {
                let sentences = split_into_sentences(&paragraph_text);
                sentences.len() > 1
                    || line_data.len() > 1
                    || collected
                        .iter()
                        .any(|l| self.calculate_effective_length(lines[l.line_idx]) > config.line_length.get())
            }
            ReflowMode::Default => collected
                .iter()
                .any(|l| self.calculate_effective_length(lines[l.line_idx]) > config.line_length.get()),
        };

        if !needs_reflow {
            return (None, next_idx);
        }

        let fallback_prefix = start_bq.prefix.clone();
        let explicit_prefix = dominant_blockquote_prefix(&line_data, &fallback_prefix);
        let continuation_style = blockquote_continuation_style(&line_data);

        let reflow_line_length = if config.line_length.is_unlimited() {
            usize::MAX
        } else {
            config
                .line_length
                .get()
                .saturating_sub(self.calculate_string_length(&explicit_prefix))
                .max(1)
        };

        let reflow_options = crate::utils::text_reflow::ReflowOptions {
            line_length: reflow_line_length,
            break_on_sentences: true,
            preserve_breaks: false,
            sentence_per_line: config.reflow_mode == ReflowMode::SentencePerLine,
            semantic_line_breaks: config.reflow_mode == ReflowMode::SemanticLineBreaks,
            abbreviations: config.abbreviations_for_reflow(),
            length_mode: self.reflow_length_mode(),
            attr_lists: ctx.flavor.supports_attr_lists(),
            require_sentence_capital: config.require_sentence_capital,
            max_list_continuation_indent: if ctx.flavor.requires_strict_list_indent() {
                Some(4)
            } else {
                None
            },
        };

        let reflowed_with_style =
            reflow_blockquote_content(&line_data, &explicit_prefix, continuation_style, &reflow_options);

        if reflowed_with_style.is_empty() {
            return (None, next_idx);
        }

        let reflowed_text = reflowed_with_style.join(line_ending);

        let start_range = line_index.whole_line_range(paragraph_start + 1);
        let end_range = if end_line == lines.len() - 1 && !ctx.content.ends_with('\n') {
            line_index.line_text_range(end_line + 1, 1, lines[end_line].len() + 1)
        } else {
            line_index.whole_line_range(end_line + 1)
        };
        let byte_range = start_range.start..end_range.end;

        let replacement = if end_line < lines.len() - 1 || ctx.content.ends_with('\n') {
            format!("{reflowed_text}{line_ending}")
        } else {
            reflowed_text
        };

        let original_text = &ctx.content[byte_range.clone()];
        if original_text == replacement {
            return (None, next_idx);
        }

        let (warning_line, warning_end_line) = match config.reflow_mode {
            ReflowMode::Normalize => (paragraph_start + 1, end_line + 1),
            ReflowMode::SentencePerLine | ReflowMode::SemanticLineBreaks => (paragraph_start + 1, end_line + 1),
            ReflowMode::Default => {
                let violating_line = collected
                    .iter()
                    .find(|line| self.calculate_effective_length(lines[line.line_idx]) > config.line_length.get())
                    .map_or(paragraph_start + 1, |line| line.line_idx + 1);
                (violating_line, violating_line)
            }
        };

        let warning = LintWarning {
            rule_name: Some(self.name().to_string()),
            message: match config.reflow_mode {
                ReflowMode::Normalize => format!(
                    "Paragraph could be normalized to use line length of {} characters",
                    config.line_length.get()
                ),
                ReflowMode::SentencePerLine => {
                    let num_sentences = split_into_sentences(&paragraph_text).len();
                    if line_data.len() == 1 {
                        format!("Line contains {num_sentences} sentences (one sentence per line required)")
                    } else {
                        let num_lines = line_data.len();
                        format!(
                            "Paragraph should have one sentence per line (found {num_sentences} sentences across {num_lines} lines)"
                        )
                    }
                }
                ReflowMode::SemanticLineBreaks => {
                    let num_sentences = split_into_sentences(&paragraph_text).len();
                    format!("Paragraph should use semantic line breaks ({num_sentences} sentences)")
                }
                ReflowMode::Default => format!("Line length exceeds {} characters", config.line_length.get()),
            },
            line: warning_line,
            column: 1,
            end_line: warning_end_line,
            end_column: lines[warning_end_line.saturating_sub(1)].len() + 1,
            severity: Severity::Warning,
            fix: Some(crate::rule::Fix::new(byte_range, replacement)),
        };

        (Some(warning), next_idx)
    }

    /// Generate paragraph-based fixes
    fn generate_paragraph_fixes(
        &self,
        ctx: &crate::lint_context::LintContext,
        config: &MD013Config,
        lines: &[&str],
    ) -> Vec<LintWarning> {
        let mut warnings = Vec::new();
        let line_index = LineIndex::new(ctx.content);

        // Detect the content's line ending style to preserve it in replacements.
        // The LSP receives content from editors which may use CRLF (Windows).
        // Replacements must match the original line endings to avoid false positives.
        let line_ending = crate::utils::line_ending::detect_line_ending(ctx.content);

        let mut i = 0;
        while i < lines.len() {
            let line_num = i + 1;

            // Handle blockquote paragraphs with style-preserving reflow.
            // Skip blockquotes when blockquotes=false or paragraphs=false
            if line_num > 0 && line_num <= ctx.lines.len() && ctx.lines[line_num - 1].blockquote.is_some() {
                if !config.blockquotes || !config.paragraphs {
                    // Skip past all blockquote lines (explicit and lazy continuations).
                    // A lazy continuation is a non-blank line without `>` that follows
                    // a blockquote line and isn't a structural element.
                    let mut saw_explicit_bq = false;
                    while i < lines.len() && i < ctx.lines.len() {
                        if ctx.lines[i].blockquote.is_some() {
                            saw_explicit_bq = true;
                            i += 1;
                        } else if saw_explicit_bq
                            && !lines[i].trim().is_empty()
                            && !lines[i].trim_start().starts_with('#')
                            && !lines[i].trim_start().starts_with('>')
                        {
                            // Lazy continuation of preceding blockquote
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    continue;
                }
                let (warning, next_idx) =
                    self.generate_blockquote_paragraph_fix(ctx, config, lines, &line_index, i, line_ending);
                if let Some(warning) = warning {
                    warnings.push(warning);
                }
                i = next_idx;
                continue;
            }

            // Skip special structures (but NOT MkDocs containers - those get special handling)
            let should_skip_due_to_line_info = ctx.line_info(line_num).is_some_and(|info| {
                info.in_code_block
                    || info.in_front_matter
                    || info.in_html_block
                    || info.in_html_comment
                    || info.in_esm_block
                    || info.in_jsx_expression
                    || info.in_jsx_block
                    || info.in_mdx_comment
                    || info.in_mkdocstrings
                    || info.in_pymdown_block
            });

            // Skip link reference definitions but NOT footnote definitions.
            // Footnote definitions (`[^id]: prose`) contain reflowable text,
            // while link reference definitions (`[ref]: URL`) contain URLs
            // that cannot be shortened.
            let is_link_ref_def =
                lines[i].trim().starts_with('[') && !lines[i].trim().starts_with("[^") && lines[i].contains("]:");

            if should_skip_due_to_line_info
                || lines[i].trim().starts_with('#')
                || TableUtils::is_potential_table_row(lines[i])
                || lines[i].trim().is_empty()
                || is_horizontal_rule(lines[i])
                || is_template_directive_only(lines[i])
                || is_link_ref_def
                || ctx.line_info(line_num).is_some_and(|info| info.is_div_marker)
                || is_html_only_line(lines[i])
            {
                i += 1;
                continue;
            }

            // Handle footnote definitions: `[^id]: prose text that can be reflowed`
            // Supports multi-paragraph footnotes with code blocks, blockquotes,
            // tables, and lists preserved verbatim.
            // Validate structure: must start with `[^`, contain `]:`, and the ID
            // must not contain `[` or `]` (prevents false matches on nested brackets)
            if lines[i].trim().starts_with("[^") && lines[i].contains("]:") && {
                let after_caret = &lines[i].trim()[2..];
                after_caret
                    .find("]:")
                    .is_some_and(|pos| pos > 0 && !after_caret[..pos].contains(['[', ']']))
            } {
                let footnote_start = i;
                let line = lines[i];

                // Extract the prefix `[^id]:`
                let Some(colon_pos) = line.find("]:") else {
                    i += 1;
                    continue;
                };
                let prefix_end = colon_pos + 2;
                let prefix = &line[..prefix_end];

                // Content starts after `]: ` (with optional space)
                let content_start = if line[prefix_end..].starts_with(' ') {
                    prefix_end + 1
                } else {
                    prefix_end
                };
                let first_content = &line[content_start..];

                // CommonMark footnotes use 4-space continuation indent
                const FN_INDENT: usize = 4;

                // --- Line classification for footnote content ---
                #[derive(Debug, Clone)]
                enum FnLineType {
                    Content(String),
                    Verbatim(String, usize), // preserved text, original indent
                    Empty,
                }

                // Helper: compute visual indent (tabs = 4 spaces)
                let visual_indent = |s: &str| -> usize {
                    s.chars()
                        .take_while(|c| c.is_whitespace())
                        .map(|c| if c == '\t' { 4 } else { 1 })
                        .sum::<usize>()
                };

                // Helper: check if a trimmed line is a fence marker (homogeneous chars)
                let is_fence = |s: &str| -> bool {
                    let t = s.trim();
                    let fence_char = t.chars().next();
                    matches!(fence_char, Some('`') | Some('~'))
                        && t.chars().take_while(|&c| c == fence_char.unwrap()).count() >= 3
                };

                // Helper: check if a trimmed line is a setext underline
                let is_setext_underline = |s: &str| -> bool {
                    let t = s.trim();
                    !t.is_empty()
                        && (t.chars().all(|c| c == '=' || c == ' ') || t.chars().all(|c| c == '-' || c == ' '))
                        && t.contains(['=', '-'])
                };

                // Deferred body: `[^id]:\n    content` — first line has no content,
                // actual content starts on the next indented line
                let deferred_body = first_content.trim().is_empty();

                // Collect all lines belonging to this footnote definition
                let mut fn_lines: Vec<FnLineType> = Vec::new();
                if !deferred_body {
                    fn_lines.push(FnLineType::Content(first_content.to_string()));
                }
                let mut last_consumed = i;
                i += 1;

                // Strip only the footnote continuation indent, preserving
                // internal indentation (e.g., code block body indent)
                let strip_fn_indent = |s: &str| -> String {
                    let mut chars = s.chars();
                    let mut stripped = 0;
                    while stripped < FN_INDENT {
                        match chars.next() {
                            Some('\t') => stripped += 4,
                            Some(c) if c.is_whitespace() => stripped += 1,
                            _ => break,
                        }
                    }
                    chars.as_str().to_string()
                };

                let mut in_fenced_code = false;
                let mut consecutive_blanks = 0u32;

                while i < lines.len() {
                    let next = lines[i];
                    let next_trimmed = next.trim();

                    // Blank line handling
                    if next_trimmed.is_empty() {
                        consecutive_blanks += 1;
                        // 2+ consecutive blanks terminate the footnote
                        if consecutive_blanks >= 2 {
                            break;
                        }

                        // Inside a fenced code block, blank lines are part of the code
                        if in_fenced_code {
                            consecutive_blanks = 0; // Don't count blanks inside code blocks
                            fn_lines.push(FnLineType::Verbatim(String::new(), 0));
                            last_consumed = i;
                            i += 1;
                            continue;
                        }

                        // Peek ahead: if next non-blank line is indented >= FN_INDENT,
                        // this blank is an internal paragraph separator
                        if i + 1 < lines.len() {
                            let peek = lines[i + 1];
                            let peek_indent = visual_indent(peek);
                            if !peek.trim().is_empty() && peek_indent >= FN_INDENT {
                                fn_lines.push(FnLineType::Empty);
                                last_consumed = i;
                                i += 1;
                                continue;
                            }
                        }
                        // No valid continuation after blank — end of footnote
                        break;
                    }

                    consecutive_blanks = 0;
                    let indent = visual_indent(next);

                    // Not indented enough — end of footnote
                    if indent < FN_INDENT {
                        break;
                    }

                    // Inside a fenced code block: everything is verbatim until closing fence
                    if in_fenced_code {
                        fn_lines.push(FnLineType::Verbatim(strip_fn_indent(next), indent));
                        if is_fence(next_trimmed) {
                            in_fenced_code = false;
                        }
                        last_consumed = i;
                        i += 1;
                        continue;
                    }

                    // Fence opener — start verbatim code block
                    if is_fence(next_trimmed) {
                        in_fenced_code = true;
                        fn_lines.push(FnLineType::Verbatim(strip_fn_indent(next), indent));
                        last_consumed = i;
                        i += 1;
                        continue;
                    }

                    // Indented code block: indent >= FN_INDENT + 4 (= 8 spaces)
                    if indent >= FN_INDENT + 4 {
                        fn_lines.push(FnLineType::Verbatim(strip_fn_indent(next), indent));
                        last_consumed = i;
                        i += 1;
                        continue;
                    }

                    // Structural content that must be preserved verbatim
                    if next_trimmed.starts_with('#')
                        || is_list_item(next_trimmed)
                        || next_trimmed.starts_with('>')
                        || TableUtils::is_potential_table_row(next_trimmed)
                        || is_setext_underline(next_trimmed)
                        || is_horizontal_rule(next_trimmed)
                        || crate::utils::mkdocs_footnotes::is_footnote_definition(next_trimmed)
                    {
                        // Preserve verbatim: blockquotes, tables, lists, setext
                        // underlines, and horizontal rules inside the footnote
                        if next_trimmed.starts_with('>')
                            || TableUtils::is_potential_table_row(next_trimmed)
                            || is_list_item(next_trimmed)
                            || is_setext_underline(next_trimmed)
                            || is_horizontal_rule(next_trimmed)
                        {
                            fn_lines.push(FnLineType::Verbatim(strip_fn_indent(next), indent));
                            last_consumed = i;
                            i += 1;
                            continue;
                        }
                        // Headings, new footnote defs, link refs — end the footnote
                        break;
                    }

                    // Link reference definitions inside footnotes are not reflowable
                    if next_trimmed.starts_with('[')
                        && !next_trimmed.starts_with("[^")
                        && next_trimmed.contains("]:")
                        && LINK_REF_PATTERN.is_match(next_trimmed)
                    {
                        fn_lines.push(FnLineType::Verbatim(strip_fn_indent(next), indent));
                        last_consumed = i;
                        i += 1;
                        continue;
                    }

                    // HTML-only lines inside footnotes are not reflowable
                    if is_html_only_line(next_trimmed) {
                        fn_lines.push(FnLineType::Verbatim(strip_fn_indent(next), indent));
                        last_consumed = i;
                        i += 1;
                        continue;
                    }

                    // Regular prose content
                    fn_lines.push(FnLineType::Content(next_trimmed.to_string()));
                    last_consumed = i;
                    i += 1;
                }

                // Nothing collected or only empty lines
                if fn_lines.iter().all(|l| matches!(l, FnLineType::Empty)) || fn_lines.is_empty() {
                    continue;
                }

                // --- Group into blocks ---
                #[derive(Debug)]
                enum FnBlock {
                    Paragraph(Vec<String>),
                    Verbatim(Vec<(String, usize)>), // (content, indent) preserved as-is
                }

                let mut blocks: Vec<FnBlock> = Vec::new();
                let mut current_para: Vec<String> = Vec::new();
                let mut current_verbatim: Vec<(String, usize)> = Vec::new();

                for fl in &fn_lines {
                    match fl {
                        FnLineType::Content(s) => {
                            if !current_verbatim.is_empty() {
                                blocks.push(FnBlock::Verbatim(std::mem::take(&mut current_verbatim)));
                            }
                            current_para.push(s.clone());
                        }
                        FnLineType::Verbatim(s, indent) => {
                            if !current_para.is_empty() {
                                blocks.push(FnBlock::Paragraph(std::mem::take(&mut current_para)));
                            }
                            current_verbatim.push((s.clone(), *indent));
                        }
                        FnLineType::Empty => {
                            if !current_para.is_empty() {
                                blocks.push(FnBlock::Paragraph(std::mem::take(&mut current_para)));
                            }
                            if !current_verbatim.is_empty() {
                                blocks.push(FnBlock::Verbatim(std::mem::take(&mut current_verbatim)));
                            }
                        }
                    }
                }
                if !current_para.is_empty() {
                    blocks.push(FnBlock::Paragraph(current_para));
                }
                if !current_verbatim.is_empty() {
                    blocks.push(FnBlock::Verbatim(current_verbatim));
                }

                // --- Reflow paragraphs and reconstruct ---
                let prefix_display_width = prefix.chars().count() + 1; // +1 for space
                let reflow_line_length = if config.line_length.is_unlimited() {
                    usize::MAX
                } else {
                    config
                        .line_length
                        .get()
                        .saturating_sub(FN_INDENT.max(prefix_display_width))
                        .max(20)
                };
                let reflow_options = crate::utils::text_reflow::ReflowOptions {
                    line_length: reflow_line_length,
                    break_on_sentences: true,
                    preserve_breaks: false,
                    sentence_per_line: config.reflow_mode == ReflowMode::SentencePerLine,
                    semantic_line_breaks: config.reflow_mode == ReflowMode::SemanticLineBreaks,
                    abbreviations: config.abbreviations_for_reflow(),
                    length_mode: self.reflow_length_mode(),
                    attr_lists: ctx.flavor.supports_attr_lists(),
                    require_sentence_capital: config.require_sentence_capital,
                    max_list_continuation_indent: None,
                };

                let indent_str = " ".repeat(FN_INDENT);
                let mut result_lines: Vec<String> = Vec::new();
                let mut is_first_block = true;

                for block in &blocks {
                    match block {
                        FnBlock::Paragraph(para_lines) => {
                            let paragraph_text = para_lines.join(" ");
                            let paragraph_text = paragraph_text.trim();
                            if paragraph_text.is_empty() {
                                continue;
                            }

                            let reflowed = crate::utils::text_reflow::reflow_line(paragraph_text, &reflow_options);
                            if reflowed.is_empty() {
                                continue;
                            }

                            // Blank line separator between blocks
                            if !result_lines.is_empty() {
                                result_lines.push(String::new());
                            }

                            for (idx, rline) in reflowed.iter().enumerate() {
                                if is_first_block && idx == 0 {
                                    result_lines.push(format!("{prefix} {rline}"));
                                } else {
                                    result_lines.push(format!("{indent_str}{rline}"));
                                }
                            }
                            is_first_block = false;
                        }
                        FnBlock::Verbatim(verb_lines) => {
                            // Blank line separator between blocks
                            if !result_lines.is_empty() {
                                result_lines.push(String::new());
                            }

                            if is_first_block {
                                // Verbatim as first block in a deferred-body footnote
                                if deferred_body {
                                    result_lines.push(prefix.to_string());
                                }
                                is_first_block = false;
                            }
                            for (content, _orig_indent) in verb_lines {
                                result_lines.push(format!("{indent_str}{content}"));
                            }
                        }
                    }
                }

                // If nothing was produced, skip
                if result_lines.is_empty() {
                    continue;
                }

                let reflowed_text = result_lines.join(line_ending);

                // Calculate byte range using last_consumed
                let start_range = line_index.whole_line_range(footnote_start + 1);
                let end_range = if last_consumed == lines.len() - 1 && !ctx.content.ends_with('\n') {
                    line_index.line_text_range(last_consumed + 1, 1, lines[last_consumed].len() + 1)
                } else {
                    line_index.whole_line_range(last_consumed + 1)
                };
                let byte_range = start_range.start..end_range.end;

                let replacement = if last_consumed < lines.len() - 1 || ctx.content.ends_with('\n') {
                    format!("{reflowed_text}{line_ending}")
                } else {
                    reflowed_text
                };

                let original_text = &ctx.content[byte_range.clone()];
                let max_length = (footnote_start..=last_consumed)
                    .map(|idx| self.calculate_effective_length(lines[idx]))
                    .max()
                    .unwrap_or(0);
                let line_limit = if config.line_length.is_unlimited() {
                    usize::MAX
                } else {
                    config.line_length.get()
                };
                if original_text != replacement && max_length > line_limit {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        message: format!(
                            "Line length {} exceeds {} characters",
                            max_length,
                            config.line_length.get()
                        ),
                        line: footnote_start + 1,
                        column: 1,
                        end_line: last_consumed + 1,
                        end_column: lines[last_consumed].len() + 1,
                        severity: Severity::Warning,
                        fix: Some(crate::rule::Fix::new(byte_range, replacement)),
                    });
                }
                continue;
            }

            // Handle MkDocs container content (admonitions and tabs) with indent-preserving reflow
            if ctx
                .line_info(line_num)
                .is_some_and(super::super::lint_context::types::LineInfo::in_mkdocs_container)
            {
                // Skip admonition/tab marker lines — only reflow their indented content
                let current_line = lines[i];
                if mkdocs_admonitions::is_admonition_start(current_line) || mkdocs_tabs::is_tab_marker(current_line) {
                    i += 1;
                    continue;
                }

                let container_start = i;

                // Detect the actual indent level from the first content line
                // (supports nested admonitions with 8+ spaces)
                let first_line = lines[i];
                let base_indent_len = first_line.len() - first_line.trim_start().len();
                let base_indent: String = " ".repeat(base_indent_len);

                // Collect consecutive MkDocs container paragraph lines
                let mut container_lines: Vec<&str> = Vec::new();
                while i < lines.len() {
                    let current_line_num = i + 1;
                    let line_info = ctx.line_info(current_line_num);

                    // Stop if we leave the MkDocs container
                    if !line_info.is_some_and(super::super::lint_context::types::LineInfo::in_mkdocs_container) {
                        break;
                    }

                    let line = lines[i];

                    // Stop at paragraph boundaries within the container
                    if line.trim().is_empty() {
                        break;
                    }

                    // Skip list items, code blocks, headings, HTML-only lines within containers
                    if is_list_item(line.trim())
                        || line.trim().starts_with("```")
                        || line.trim().starts_with("~~~")
                        || line.trim().starts_with('#')
                        || is_html_only_line(line)
                    {
                        break;
                    }

                    container_lines.push(line);
                    i += 1;
                }

                if container_lines.is_empty() {
                    // Must advance i to avoid infinite loop when we encounter
                    // non-paragraph content (code block, list, heading, empty line)
                    // at the start of an MkDocs container
                    i += 1;
                    continue;
                }

                // Strip the base indent from each line and join for reflow
                let stripped_lines: Vec<&str> = container_lines
                    .iter()
                    .map(|line| {
                        if line.starts_with(&base_indent) {
                            &line[base_indent_len..]
                        } else {
                            line.trim_start()
                        }
                    })
                    .collect();
                let paragraph_text = stripped_lines.join(" ");

                // Check if reflow is needed
                let needs_reflow = match config.reflow_mode {
                    ReflowMode::Normalize => self.normalize_mode_needs_reflow(container_lines.iter().copied(), config),
                    ReflowMode::SentencePerLine => {
                        let sentences = split_into_sentences(&paragraph_text);
                        sentences.len() > 1 || container_lines.len() > 1
                    }
                    ReflowMode::SemanticLineBreaks => {
                        let sentences = split_into_sentences(&paragraph_text);
                        sentences.len() > 1
                            || container_lines.len() > 1
                            || container_lines
                                .iter()
                                .any(|line| self.calculate_effective_length(line) > config.line_length.get())
                    }
                    ReflowMode::Default => container_lines
                        .iter()
                        .any(|line| self.calculate_effective_length(line) > config.line_length.get()),
                };

                if !needs_reflow {
                    continue;
                }

                // Calculate byte range for this container paragraph
                let start_range = line_index.whole_line_range(container_start + 1);
                let end_line = container_start + container_lines.len() - 1;
                let end_range = if end_line == lines.len() - 1 && !ctx.content.ends_with('\n') {
                    line_index.line_text_range(end_line + 1, 1, lines[end_line].len() + 1)
                } else {
                    line_index.whole_line_range(end_line + 1)
                };
                let byte_range = start_range.start..end_range.end;

                // Reflow with adjusted line length (accounting for the 4-space indent)
                let reflow_line_length = if config.line_length.is_unlimited() {
                    usize::MAX
                } else {
                    config.line_length.get().saturating_sub(base_indent_len).max(1)
                };
                let reflow_options = crate::utils::text_reflow::ReflowOptions {
                    line_length: reflow_line_length,
                    break_on_sentences: true,
                    preserve_breaks: false,
                    sentence_per_line: config.reflow_mode == ReflowMode::SentencePerLine,
                    semantic_line_breaks: config.reflow_mode == ReflowMode::SemanticLineBreaks,
                    abbreviations: config.abbreviations_for_reflow(),
                    length_mode: self.reflow_length_mode(),
                    attr_lists: ctx.flavor.supports_attr_lists(),
                    require_sentence_capital: config.require_sentence_capital,
                    max_list_continuation_indent: if ctx.flavor.requires_strict_list_indent() {
                        Some(4)
                    } else {
                        None
                    },
                };
                let reflowed = crate::utils::text_reflow::reflow_line(&paragraph_text, &reflow_options);

                // Re-add the 4-space indent to each reflowed line
                let reflowed_with_indent: Vec<String> =
                    reflowed.iter().map(|line| format!("{base_indent}{line}")).collect();
                let reflowed_text = reflowed_with_indent.join(line_ending);

                // Preserve trailing newline
                let replacement = if end_line < lines.len() - 1 || ctx.content.ends_with('\n') {
                    format!("{reflowed_text}{line_ending}")
                } else {
                    reflowed_text
                };

                // Only generate a warning if the replacement is different
                let original_text = &ctx.content[byte_range.clone()];
                if original_text != replacement {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        message: format!(
                            "Line length {} exceeds {} characters (in MkDocs container)",
                            container_lines.iter().map(|l| l.len()).max().unwrap_or(0),
                            config.line_length.get()
                        ),
                        line: container_start + 1,
                        column: 1,
                        end_line: end_line + 1,
                        end_column: lines[end_line].len() + 1,
                        severity: Severity::Warning,
                        fix: Some(crate::rule::Fix::new(byte_range, replacement)),
                    });
                }
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

                // Checkbox ([ ]/[x]/[X]) is inline content, not part of the list marker.
                // Use the base bullet/number marker width for continuation recognition
                // so that continuation lines at 2+ spaces are collected for "- [ ] " items.
                let base_marker_len = if marker.contains("[ ] ") || marker.contains("[x] ") || marker.contains("[X] ") {
                    marker.find('[').unwrap_or(marker_len)
                } else {
                    marker_len
                };

                // MkDocs flavor requires at least 4 spaces for list continuation
                // after a blank line (multi-paragraph list items). For non-blank
                // continuation (lines directly following the marker line), use
                // the natural marker width so that 2-space indent is recognized.
                let item_indent = ctx.lines[i].indent;
                let min_continuation_indent = if ctx.flavor.requires_strict_list_indent() {
                    // Use 4-space relative indent from the list item's nesting level
                    item_indent + (base_marker_len - item_indent).max(4)
                } else {
                    marker_len
                };
                let content_continuation_indent = base_marker_len;

                // Track lines and their types (content, code block, fence, nested list)
                #[derive(Clone)]
                enum LineType {
                    Content(String),
                    CodeBlock(String, usize),         // content and original indent
                    SemanticLine(String), // Lines starting with NOTE:, WARNING:, etc that should stay separate
                    SnippetLine(String),  // MkDocs Snippets delimiters (-8<-) that must stay on their own line
                    DivMarker(String),    // Quarto/Pandoc div markers (::: opening or closing)
                    AdmonitionHeader(String, usize), // header text (e.g. "!!! note") and original indent
                    AdmonitionContent(String, usize), // body content text and original indent
                    Table(String, usize), // GFM table row, preserved verbatim with original indent
                    Empty,
                }

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
                            if !next_info.is_blank && next_info.indent >= min_continuation_indent {
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

                    // Valid continuation must be indented at least content_continuation_indent.
                    // For non-blank continuation, use marker_len (e.g. 2 for "- ").
                    // MkDocs strict 4-space requirement applies only after blank lines.
                    if indent >= content_continuation_indent {
                        let trimmed = line_info.content(ctx.content).trim();

                        // Check for MkDocs admonition lines inside list items BEFORE
                        // checking in_code_block. Lines inside code blocks within
                        // admonitions have both in_admonition and in_code_block set;
                        // admonition membership takes priority so the entire admonition
                        // structure (including embedded code blocks) is preserved.
                        if line_info.in_admonition {
                            let raw_content = line_info.content(ctx.content);
                            if mkdocs_admonitions::is_admonition_start(raw_content) {
                                let header_text = raw_content[indent..].trim_end().to_string();
                                list_item_lines.push(LineType::AdmonitionHeader(header_text, indent));
                            } else {
                                let body_text = raw_content[indent..].trim_end().to_string();
                                list_item_lines.push(LineType::AdmonitionContent(body_text, indent));
                            }
                            i += 1;
                            continue;
                        }

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

                        // Nested list items are always processed independently
                        // by the outer loop, so break when we encounter one.
                        // If a blank line was collected before this, uncollect it
                        // so the outer loop preserves the blank between parent and nested.
                        if is_list_item(trimmed) && indent >= marker_len {
                            if matches!(list_item_lines.last(), Some(LineType::Empty)) {
                                list_item_lines.pop();
                                i -= 1;
                            }
                            break;
                        }

                        // Normal continuation vs indented code block.
                        // Use min_continuation_indent for the threshold since
                        // code blocks start 4 spaces beyond the expected content
                        // level (which is min_continuation_indent for MkDocs).
                        if indent <= min_continuation_indent + 3 {
                            // Extract content (remove indentation and trailing whitespace)
                            // Preserve hard breaks (2 trailing spaces) while removing excessive whitespace
                            // See: https://github.com/rvben/rumdl/issues/76
                            let content = trim_preserving_hard_break(&line_info.content(ctx.content)[indent..]);

                            // Check if this is a div marker (::: opening or closing)
                            // These must be preserved on their own line, not merged into paragraphs
                            if line_info.is_div_marker {
                                list_item_lines.push(LineType::DivMarker(content));
                            }
                            // Check if this is a fence marker (opening or closing)
                            // These should be treated as code block lines, not paragraph content
                            else if is_fence_marker(&content) {
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
                            }
                            // Check if this is a GFM table row. Tables nested inside list
                            // items must be preserved verbatim — joining them with prose
                            // breaks the column structure.
                            //
                            // `is_potential_table_row` is intentionally permissive at the
                            // row level: any line with `|` and 2+ cells qualifies. To avoid
                            // misclassifying prose continuation lines that contain a literal
                            // pipe (e.g. "use grep | sort to ..."), require one of:
                            //   - the row is pipe-bordered (`| ... |`), the canonical form
                            //     for tables nested in lists; or
                            //   - the next line is a delimiter row (this is a header); or
                            //   - the previous classified line was already a Table (this is
                            //     a continuation row).
                            else if TableUtils::is_potential_table_row(&content) && {
                                let pipe_bordered = content.trim().starts_with('|') && content.trim().ends_with('|');
                                let next_is_delim = ctx
                                    .lines
                                    .get(i + 1)
                                    .is_some_and(|next| TableUtils::is_delimiter_row(next.content(ctx.content)));
                                let prev_was_table = matches!(list_item_lines.last(), Some(LineType::Table(..)));
                                pipe_bordered || next_is_delim || prev_was_table
                            } {
                                list_item_lines.push(LineType::Table(content, indent));
                            } else {
                                list_item_lines.push(LineType::Content(content));
                            }
                            i += 1;
                        } else {
                            // indent >= min_continuation_indent + 4: indented code block
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

                // Determine the output continuation indent.
                // Normalize/Default modes canonicalize to min_continuation_indent
                // (fixing over-indented continuation). Semantic/SentencePerLine
                // modes preserve the user's actual indent since they only fix
                // line breaking, not indentation.
                let indent_size = match config.reflow_mode {
                    ReflowMode::SemanticLineBreaks | ReflowMode::SentencePerLine => {
                        // Find indent of the first plain text continuation line,
                        // skipping the marker line (index 0), nested list items,
                        // code blocks, and blank lines.
                        list_item_lines
                            .iter()
                            .enumerate()
                            .skip(1)
                            .find_map(|(k, lt)| {
                                if matches!(lt, LineType::Content(_)) {
                                    Some(ctx.lines[list_start + k].indent)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(min_continuation_indent)
                    }
                    _ => min_continuation_indent,
                };
                // For checkbox items in mkdocs flavor, enforce minimum indent so
                // continuation lines use the structural list indent (4), not the
                // content-aligned indent (6) which Python-Markdown doesn't support
                let has_checkbox = base_marker_len < marker_len;
                let indent_size = if has_checkbox && ctx.flavor.requires_strict_list_indent() {
                    indent_size.max(min_continuation_indent)
                } else {
                    indent_size
                };
                let expected_indent = " ".repeat(indent_size);

                // Split list_item_lines into blocks (paragraphs, code blocks, nested lists, semantic lines, and HTML blocks)
                let mut builder = BlockBuilder::new();
                for line in &list_item_lines {
                    match line {
                        LineType::Empty => builder.feed_blank_line(),
                        LineType::Content(content) => builder.feed_content(content),
                        LineType::CodeBlock(content, indent) => builder.feed_code_line(content, *indent),
                        LineType::SemanticLine(content) => builder.feed_semantic_line(content),
                        LineType::SnippetLine(content) => builder.feed_snippet_line(content),
                        LineType::DivMarker(content) => builder.feed_div_marker(content),
                        LineType::AdmonitionHeader(header_text, indent) => {
                            builder.feed_admonition_header(header_text, *indent)
                        }
                        LineType::AdmonitionContent(content, indent) => {
                            builder.feed_admonition_content(content, *indent)
                        }
                        LineType::Table(content, indent) => builder.feed_table_line(content, *indent),
                    }
                }
                let blocks = builder.finalize();

                // Helper: check if a line (raw source or stripped content) is exempt
                // from line-length checks. Link reference definitions are always exempt;
                // standalone link/image lines are exempt when strict mode is off.
                // Also checks content after stripping list markers, since list item
                // continuation lines may contain link ref defs.
                let is_exempt_line = |raw_line: &str| -> bool {
                    let trimmed = raw_line.trim();
                    // Link reference definitions: always exempt
                    if trimmed.starts_with('[') && trimmed.contains("]:") && LINK_REF_PATTERN.is_match(trimmed) {
                        return true;
                    }
                    // Also check after stripping list markers (for list item content)
                    if is_list_item(trimmed) {
                        let (_, content) = extract_list_marker_and_content(trimmed);
                        let content_trimmed = content.trim();
                        if content_trimmed.starts_with('[')
                            && content_trimmed.contains("]:")
                            && LINK_REF_PATTERN.is_match(content_trimmed)
                        {
                            return true;
                        }
                    }
                    // Standalone link/image lines: exempt when not strict
                    if !config.strict && is_standalone_link_or_image_line(raw_line) {
                        return true;
                    }
                    // HTML-only lines: exempt when not strict
                    if !config.strict && is_html_only_line(raw_line) {
                        return true;
                    }
                    false
                };

                // Check if reflowing is needed (only for content paragraphs, not code blocks or nested lists)
                // Exclude link reference definitions and standalone link lines from content
                // so they don't pollute combined_content or trigger false reflow.
                let content_lines: Vec<String> = list_item_lines
                    .iter()
                    .filter_map(|line| {
                        if let LineType::Content(s) = line {
                            if is_exempt_line(s) {
                                return None;
                            }
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                // Check if we need to reflow this list item
                // We check the combined content to see if it exceeds length limits
                let combined_content = content_lines.join(" ").trim().to_string();

                // Helper to check if we should reflow in normalize mode
                let should_normalize = || {
                    // Don't normalize if the list item only contains nested lists, code blocks, or semantic lines
                    // DO normalize if it has plain text content that spans multiple lines
                    let has_code_blocks = blocks.iter().any(|b| matches!(b, Block::Code { .. }));
                    let has_semantic_lines = blocks.iter().any(|b| matches!(b, Block::SemanticLine(_)));
                    let has_snippet_lines = blocks.iter().any(|b| matches!(b, Block::SnippetLine(_)));
                    let has_div_markers = blocks.iter().any(|b| matches!(b, Block::DivMarker(_)));
                    let has_admonitions = blocks.iter().any(|b| matches!(b, Block::Admonition { .. }));
                    let has_tables = blocks.iter().any(|b| matches!(b, Block::Table { .. }));
                    let has_paragraphs = blocks.iter().any(|b| matches!(b, Block::Paragraph(_)));

                    // If we have structural blocks but no paragraphs, don't normalize
                    if (has_code_blocks
                        || has_semantic_lines
                        || has_snippet_lines
                        || has_div_markers
                        || has_admonitions
                        || has_tables)
                        && !has_paragraphs
                    {
                        return false;
                    }

                    // If we have paragraphs, check if they span multiple lines or there are multiple blocks
                    if has_paragraphs {
                        // Count only paragraphs that contain at least one non-exempt line.
                        // Paragraphs consisting entirely of link ref defs or standalone links
                        // should not trigger normalization.
                        let paragraph_count = blocks
                            .iter()
                            .filter(|b| {
                                if let Block::Paragraph(para_lines) = b {
                                    !para_lines.iter().all(|line| is_exempt_line(line))
                                } else {
                                    false
                                }
                            })
                            .count();
                        if paragraph_count > 1 {
                            // Multiple non-exempt paragraph blocks should be normalized
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
                        // 1. Any non-exempt paragraph, when joined, exceeds the limit, OR
                        // 2. Any admonition content line exceeds the limit, OR
                        // 3. The list item should be normalized (has multi-line plain text)
                        let any_paragraph_exceeds = blocks.iter().any(|block| match block {
                            Block::Paragraph(para_lines) => {
                                if para_lines.iter().all(|line| is_exempt_line(line)) {
                                    return false;
                                }
                                let joined = para_lines.join(" ");
                                let with_marker = format!("{}{}", " ".repeat(indent_size), joined.trim());
                                self.calculate_effective_length(&with_marker) > config.line_length.get()
                            }
                            Block::Admonition {
                                content_lines,
                                header_indent,
                                ..
                            } => content_lines.iter().any(|(content, indent)| {
                                if content.is_empty() {
                                    return false;
                                }
                                let with_indent = format!("{}{}", " ".repeat(*indent.max(header_indent)), content);
                                self.calculate_effective_length(&with_indent) > config.line_length.get()
                            }),
                            _ => false,
                        });
                        if any_paragraph_exceeds {
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
                    ReflowMode::SemanticLineBreaks => {
                        let sentences = split_into_sentences(&combined_content);
                        sentences.len() > 1
                            || (list_start..i).any(|line_idx| {
                                let line = lines[line_idx];
                                let trimmed = line.trim();
                                if trimmed.is_empty() || is_exempt_line(line) {
                                    return false;
                                }
                                self.calculate_effective_length(line) > config.line_length.get()
                            })
                    }
                    ReflowMode::Default => {
                        // In default mode, only reflow if any individual non-exempt line exceeds limit
                        (list_start..i).any(|line_idx| {
                            let line = lines[line_idx];
                            let trimmed = line.trim();
                            // Skip blank lines and exempt lines
                            if trimmed.is_empty() || is_exempt_line(line) {
                                return false;
                            }
                            self.calculate_effective_length(line) > config.line_length.get()
                        })
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
                        semantic_line_breaks: config.reflow_mode == ReflowMode::SemanticLineBreaks,
                        abbreviations: config.abbreviations_for_reflow(),
                        length_mode: self.reflow_length_mode(),
                        attr_lists: ctx.flavor.supports_attr_lists(),
                        require_sentence_capital: config.require_sentence_capital,
                        max_list_continuation_indent: if ctx.flavor.requires_strict_list_indent() {
                            Some(4)
                        } else {
                            None
                        },
                    };

                    let mut result: Vec<String> = Vec::new();
                    let mut is_first_block = true;

                    for (block_idx, block) in blocks.iter().enumerate() {
                        match block {
                            Block::Paragraph(para_lines) => {
                                // If every line in this paragraph is exempt (link ref defs,
                                // standalone links), preserve the paragraph verbatim instead
                                // of reflowing it. Reflowing would corrupt link ref defs.
                                let all_exempt = para_lines.iter().all(|line| is_exempt_line(line));

                                if all_exempt {
                                    for (idx, line) in para_lines.iter().enumerate() {
                                        if is_first_block && idx == 0 {
                                            result.push(format!("{marker}{line}"));
                                            is_first_block = false;
                                        } else {
                                            result.push(format!("{expected_indent}{line}"));
                                        }
                                    }
                                } else {
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
                                }

                                // Add blank line after paragraph block if there's a next block.
                                // Check if next block is a code block that doesn't want a preceding blank.
                                // Also don't add blank lines before snippet lines (they should stay tight).
                                // Only add if not already ending with one (avoids double blanks).
                                if block_idx < blocks.len() - 1 {
                                    let next_block = &blocks[block_idx + 1];
                                    let should_add_blank = match next_block {
                                        Block::Code {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::Table {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::SnippetLine(_) | Block::DivMarker(_) => false,
                                        _ => true, // For all other blocks, add blank line
                                    };
                                    if should_add_blank && result.last().is_none_or(|s: &String| !s.is_empty()) {
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
                            Block::SemanticLine(content) => {
                                // Preserve semantic lines (NOTE:, WARNING:, etc.) as-is on their own line.
                                // Only add blank before if not already ending with one.
                                if !is_first_block && result.last().is_none_or(|s: &String| !s.is_empty()) {
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

                                // Add blank line after semantic line if there's a next block.
                                // Only add if not already ending with one.
                                if block_idx < blocks.len() - 1 {
                                    let next_block = &blocks[block_idx + 1];
                                    let should_add_blank = match next_block {
                                        Block::Code {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::Table {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::SnippetLine(_) | Block::DivMarker(_) => false,
                                        _ => true, // For all other blocks, add blank line
                                    };
                                    if should_add_blank && result.last().is_none_or(|s: &String| !s.is_empty()) {
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
                            Block::DivMarker(content) => {
                                // Preserve div markers (::: opening or closing) as-is on their own line
                                if is_first_block {
                                    result.push(format!("{marker}{content}"));
                                    is_first_block = false;
                                } else {
                                    result.push(format!("{expected_indent}{content}"));
                                }
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

                                // Add blank line after HTML block if there's a next block.
                                // Only add if not already ending with one (avoids double blanks
                                // when the HTML block itself contained a trailing blank line).
                                if block_idx < blocks.len() - 1 {
                                    let next_block = &blocks[block_idx + 1];
                                    let should_add_blank = match next_block {
                                        Block::Code {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::Html {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::Table {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::SnippetLine(_) | Block::DivMarker(_) => false,
                                        _ => true, // For all other blocks, add blank line
                                    };
                                    if should_add_blank && result.last().is_none_or(|s: &String| !s.is_empty()) {
                                        result.push(String::new());
                                    }
                                }
                            }
                            Block::Table {
                                lines: table_lines,
                                has_preceding_blank: _,
                            } => {
                                // Preserve table rows verbatim with their original indentation.
                                // Reflowing rows would corrupt column alignment and inject `|`
                                // characters mid-paragraph (issue #590).
                                // The leading blank line is emitted by the previous block.
                                for (idx, (content, orig_indent)) in table_lines.iter().enumerate() {
                                    if is_first_block && idx == 0 {
                                        // First line of first block gets the list marker
                                        result.push(format!(
                                            "{marker}{}",
                                            " ".repeat(orig_indent.saturating_sub(marker_len)) + content
                                        ));
                                        is_first_block = false;
                                    } else {
                                        result.push(format!("{}{}", " ".repeat(*orig_indent), content));
                                    }
                                }

                                // Add blank line after table block if there's a next block.
                                if block_idx < blocks.len() - 1 {
                                    let next_block = &blocks[block_idx + 1];
                                    let should_add_blank = match next_block {
                                        Block::Code {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::Html {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::Table {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::SnippetLine(_) | Block::DivMarker(_) => false,
                                        _ => true,
                                    };
                                    if should_add_blank && result.last().is_none_or(|s: &String| !s.is_empty()) {
                                        result.push(String::new());
                                    }
                                }
                            }
                            Block::Admonition {
                                header,
                                header_indent,
                                content_lines: admon_lines,
                            } => {
                                // Reconstruct admonition block with header at original indent
                                // and body content reflowed to fit within the line length limit

                                // Add blank line before admonition if not first block
                                if !is_first_block && result.last().is_none_or(|s: &String| !s.is_empty()) {
                                    result.push(String::new());
                                }

                                // Output the header at its original indent
                                let header_indent_str = " ".repeat(*header_indent);
                                if is_first_block {
                                    result.push(format!(
                                        "{marker}{}",
                                        " ".repeat(header_indent.saturating_sub(marker_len)) + header
                                    ));
                                    is_first_block = false;
                                } else {
                                    result.push(format!("{header_indent_str}{header}"));
                                }

                                // Derive body indent from the first non-empty content line's
                                // stored indent, falling back to header_indent + 4 for
                                // empty-body admonitions
                                let body_indent = admon_lines
                                    .iter()
                                    .find(|(content, _)| !content.is_empty())
                                    .map_or(header_indent + 4, |(_, indent)| *indent);
                                let body_indent_str = " ".repeat(body_indent);

                                // Segment body content into code blocks (verbatim) and
                                // text paragraphs (reflowable), separated by blank lines.
                                // Code lines store (content, orig_indent) to reconstruct
                                // internal indentation relative to body_indent.
                                enum AdmonSegment {
                                    Text(Vec<String>),
                                    Code(Vec<(String, usize)>),
                                }

                                let mut segments: Vec<AdmonSegment> = Vec::new();
                                let mut current_text: Vec<String> = Vec::new();
                                let mut current_code: Vec<(String, usize)> = Vec::new();
                                let mut in_admon_code = false;
                                // Track the opening fence character so closing fences
                                // must match (backticks close backticks, tildes close tildes)
                                let mut fence_char: char = '`';

                                // Opening fences: ``` or ~~~ followed by optional info string
                                let get_opening_fence = |s: &str| -> Option<(char, usize)> {
                                    let t = s.trim_start();
                                    if t.starts_with("```") {
                                        Some(('`', t.bytes().take_while(|&b| b == b'`').count()))
                                    } else if t.starts_with("~~~") {
                                        Some(('~', t.bytes().take_while(|&b| b == b'~').count()))
                                    } else {
                                        None
                                    }
                                };
                                // Closing fences: ONLY fence chars + optional trailing spaces
                                let get_closing_fence = |s: &str| -> Option<(char, usize)> {
                                    let t = s.trim();
                                    if t.starts_with("```") && t.bytes().all(|b| b == b'`') {
                                        Some(('`', t.len()))
                                    } else if t.starts_with("~~~") && t.bytes().all(|b| b == b'~') {
                                        Some(('~', t.len()))
                                    } else {
                                        None
                                    }
                                };
                                let mut fence_len: usize = 3;

                                for (content, orig_indent) in admon_lines {
                                    if in_admon_code {
                                        // Closing fence must use the same character, be
                                        // at least as long, and have no info string
                                        if let Some((ch, len)) = get_closing_fence(content)
                                            && ch == fence_char
                                            && len >= fence_len
                                        {
                                            current_code.push((content.clone(), *orig_indent));
                                            in_admon_code = false;
                                            segments.push(AdmonSegment::Code(std::mem::take(&mut current_code)));
                                            continue;
                                        }
                                        current_code.push((content.clone(), *orig_indent));
                                    } else if let Some((ch, len)) = get_opening_fence(content) {
                                        if !current_text.is_empty() {
                                            segments.push(AdmonSegment::Text(std::mem::take(&mut current_text)));
                                        }
                                        in_admon_code = true;
                                        fence_char = ch;
                                        fence_len = len;
                                        current_code.push((content.clone(), *orig_indent));
                                    } else if content.is_empty() {
                                        if !current_text.is_empty() {
                                            segments.push(AdmonSegment::Text(std::mem::take(&mut current_text)));
                                        }
                                    } else {
                                        current_text.push(content.clone());
                                    }
                                }
                                if in_admon_code && !current_code.is_empty() {
                                    segments.push(AdmonSegment::Code(std::mem::take(&mut current_code)));
                                }
                                if !current_text.is_empty() {
                                    segments.push(AdmonSegment::Text(std::mem::take(&mut current_text)));
                                }

                                // Build reflow options once for all text segments
                                let admon_reflow_length = if config.line_length.is_unlimited() {
                                    usize::MAX
                                } else {
                                    config.line_length.get().saturating_sub(body_indent).max(1)
                                };

                                let admon_reflow_options = crate::utils::text_reflow::ReflowOptions {
                                    line_length: admon_reflow_length,
                                    break_on_sentences: true,
                                    preserve_breaks: false,
                                    sentence_per_line: config.reflow_mode == ReflowMode::SentencePerLine,
                                    semantic_line_breaks: config.reflow_mode == ReflowMode::SemanticLineBreaks,
                                    abbreviations: config.abbreviations_for_reflow(),
                                    length_mode: self.reflow_length_mode(),
                                    attr_lists: ctx.flavor.supports_attr_lists(),
                                    require_sentence_capital: config.require_sentence_capital,
                                    max_list_continuation_indent: if ctx.flavor.requires_strict_list_indent() {
                                        Some(4)
                                    } else {
                                        None
                                    },
                                };

                                // Output each segment
                                for segment in &segments {
                                    // Blank line before each segment (after the header or previous segment)
                                    result.push(String::new());

                                    match segment {
                                        AdmonSegment::Code(lines) => {
                                            for (line, orig_indent) in lines {
                                                if line.is_empty() {
                                                    // Preserve blank lines inside code blocks
                                                    result.push(String::new());
                                                } else {
                                                    // Reconstruct with body_indent + any extra
                                                    // indentation the line had beyond body_indent
                                                    let extra = orig_indent.saturating_sub(body_indent);
                                                    let indent_str = " ".repeat(body_indent + extra);
                                                    result.push(format!("{indent_str}{line}"));
                                                }
                                            }
                                        }
                                        AdmonSegment::Text(lines) => {
                                            let paragraph_text = lines.join(" ").trim().to_string();
                                            if paragraph_text.is_empty() {
                                                continue;
                                            }
                                            let reflowed = crate::utils::text_reflow::reflow_line(
                                                &paragraph_text,
                                                &admon_reflow_options,
                                            );
                                            for line in &reflowed {
                                                result.push(format!("{body_indent_str}{line}"));
                                            }
                                        }
                                    }
                                }

                                // Add blank line after admonition if there's a next block
                                if block_idx < blocks.len() - 1 {
                                    let next_block = &blocks[block_idx + 1];
                                    let should_add_blank = match next_block {
                                        Block::Code {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::Table {
                                            has_preceding_blank, ..
                                        } => *has_preceding_blank,
                                        Block::SnippetLine(_) | Block::DivMarker(_) => false,
                                        _ => true,
                                    };
                                    if should_add_blank && result.last().is_none_or(|s: &String| !s.is_empty()) {
                                        result.push(String::new());
                                    }
                                }
                            }
                        }
                    }

                    let reflowed_text = result.join(line_ending);

                    // Preserve trailing newline
                    let replacement = if end_line < lines.len() - 1 || ctx.content.ends_with('\n') {
                        format!("{reflowed_text}{line_ending}")
                    } else {
                        reflowed_text
                    };

                    // Get the original text to compare
                    let original_text = &ctx.content[byte_range.clone()];

                    // Physical-line-length scan, shared by the Normalize-mode gate and its
                    // message. The list-item reflow preserves code blocks, HTML blocks,
                    // admonition headers, fence markers, semantic markers, and snippet/div
                    // markers verbatim; only paragraph content and admonition bodies are
                    // restructured. Only those lines drive the length warning, so that
                    // preserved-but-overlong content does not keep the paragraph-level
                    // warning alive when the reflow would not fix that line.
                    let should_count_for_length = |line_idx: usize| -> bool {
                        let line = lines[line_idx];
                        let trimmed = line.trim();
                        if trimmed.is_empty() || is_exempt_line(line) {
                            return false;
                        }
                        let info = &ctx.lines[line_idx];
                        if info.in_code_block || info.in_html_block {
                            return false;
                        }
                        if info.in_admonition && mkdocs_admonitions::is_admonition_start(line) {
                            return false;
                        }
                        if is_fence_marker(line) || is_semantic_line(line) {
                            return false;
                        }
                        if is_snippet_block_delimiter(line) {
                            return false;
                        }
                        if line.trim_start().starts_with(":::") {
                            return false;
                        }
                        true
                    };
                    let max_physical_length = (list_start..i)
                        .filter(|&idx| should_count_for_length(idx))
                        .map(|idx| self.calculate_effective_length(lines[idx]))
                        .max()
                        .unwrap_or(0);
                    // `line-length = 0` means "no limit" — never emit a length warning in
                    // that configuration, even when the reflow would restructure content.
                    let any_paragraph_line_over =
                        !config.line_length.is_unlimited() && max_physical_length > config.line_length.get();

                    // Only generate a warning if the replacement is different from the original.
                    // For Normalize mode, additionally require that at least one physical
                    // content line exceeds the limit — otherwise the reflow is a cosmetic
                    // change (continuation-indent normalization or joining/splitting
                    // already-fitting multi-line paragraphs) and should not be reported as
                    // a length violation. SentencePerLine and SemanticLineBreaks modes warn
                    // on sentence structure, not length, so they ignore this gate.
                    let gate_ok = match config.reflow_mode {
                        ReflowMode::Normalize => original_text != replacement && any_paragraph_line_over,
                        _ => original_text != replacement,
                    };
                    if gate_ok {
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
                            ReflowMode::SemanticLineBreaks => {
                                let num_sentences = split_into_sentences(&combined_content).len();
                                format!("Paragraph should use semantic line breaks ({num_sentences} sentences)")
                            }
                            ReflowMode::Normalize => {
                                // Report the physical line length of the longest over-limit
                                // content line. The gate above guarantees at least one.
                                format!(
                                    "Line length {} exceeds {} characters",
                                    max_physical_length,
                                    config.line_length.get()
                                )
                            }
                            ReflowMode::Default => {
                                // Report the actual longest non-exempt line, not the combined content
                                let max_length = (list_start..i)
                                    .filter(|&line_idx| {
                                        let line = lines[line_idx];
                                        let trimmed = line.trim();
                                        !trimmed.is_empty() && !is_exempt_line(line)
                                    })
                                    .map(|line_idx| self.calculate_effective_length(lines[line_idx]))
                                    .max()
                                    .unwrap_or(0);
                                format!(
                                    "Line length {} exceeds {} characters",
                                    max_length,
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
                            fix: Some(crate::rule::Fix::new(byte_range, replacement)),
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
                    || ctx.line_info(next_line_num).is_some_and(|info| info.in_jsx_block)
                    || ctx.line_info(next_line_num).is_some_and(|info| info.in_mdx_comment)
                    || ctx
                        .line_info(next_line_num)
                        .is_some_and(super::super::lint_context::types::LineInfo::in_mkdocs_container)
                    || (next_line_num > 0
                        && next_line_num <= ctx.lines.len()
                        && ctx.lines[next_line_num - 1].blockquote.is_some())
                    || next_trimmed.starts_with('#')
                    || TableUtils::is_potential_table_row(next_line)
                    || is_list_item(next_trimmed)
                    || is_horizontal_rule(next_line)
                    || (next_trimmed.starts_with('[') && next_line.contains("]:"))
                    || is_template_directive_only(next_line)
                    || is_standalone_attr_list(next_line)
                    || is_snippet_block_delimiter(next_line)
                    || ctx.line_info(next_line_num).is_some_and(|info| info.is_div_marker)
                    || is_html_only_line(next_line)
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

            // Compute the common leading indent of all non-empty paragraph lines,
            // but only when those lines are structurally inside a list block.
            // Indented continuation lines that follow a nested list arrive here
            // with their structural indentation intact (e.g. 2 spaces for a
            // top-level list item). Stripping the indent before reflow and
            // re-applying it afterward prevents the fixer from moving those
            // lines to column 0.
            //
            // The list-block guard is essential: top-level paragraphs that happen
            // to start with spaces (insignificant in Markdown) must NOT have those
            // spaces preserved or injected by the fixer.
            let common_indent: String = if ctx.is_in_list_block(paragraph_start + 1) {
                let min_len = paragraph_lines
                    .iter()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.len() - l.trim_start().len())
                    .min()
                    .unwrap_or(0);
                paragraph_lines
                    .iter()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| l[..min_len].to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };

            // Combine paragraph lines into a single string for processing.
            // This must be done BEFORE the needs_reflow check for sentence-per-line mode.
            let paragraph_text = if common_indent.is_empty() {
                paragraph_lines.join(" ")
            } else {
                paragraph_lines
                    .iter()
                    .map(|l| {
                        if l.starts_with(common_indent.as_str()) {
                            &l[common_indent.len()..]
                        } else {
                            l.trim_start()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            };

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
                ReflowMode::Normalize => self.normalize_mode_needs_reflow(paragraph_lines.iter().copied(), config),
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
                            // Only join if it fits within line-length.
                            // paragraph_text has the common indent stripped, so add it
                            // back to get the true output length before comparing.
                            let effective_length =
                                self.calculate_effective_length(&paragraph_text) + common_indent.len();
                            effective_length <= config.line_length.get()
                        }
                    } else {
                        false
                    }
                }
                ReflowMode::SemanticLineBreaks => {
                    let sentences = split_into_sentences(&paragraph_text);
                    // Reflow if multiple sentences, multiple lines, or any line exceeds limit
                    sentences.len() > 1
                        || paragraph_lines.len() > 1
                        || paragraph_lines
                            .iter()
                            .any(|line| self.calculate_effective_length(line) > config.line_length.get())
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
                    semantic_line_breaks: config.reflow_mode == ReflowMode::SemanticLineBreaks,
                    abbreviations: config.abbreviations_for_reflow(),
                    length_mode: self.reflow_length_mode(),
                    attr_lists: ctx.flavor.supports_attr_lists(),
                    require_sentence_capital: config.require_sentence_capital,
                    max_list_continuation_indent: if ctx.flavor.requires_strict_list_indent() {
                        Some(4)
                    } else {
                        None
                    },
                };
                let mut reflowed = crate::utils::text_reflow::reflow_line(&paragraph_text, &reflow_options);

                // Re-apply the common indent to each non-empty reflowed line so
                // that the replacement preserves the original structural indentation.
                if !common_indent.is_empty() {
                    for line in &mut reflowed {
                        if !line.is_empty() {
                            *line = format!("{common_indent}{line}");
                        }
                    }
                }

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

                let reflowed_text = reflowed.join(line_ending);

                // Preserve trailing newline if the original paragraph had one
                let replacement = if end_line < lines.len() - 1 || ctx.content.ends_with('\n') {
                    format!("{reflowed_text}{line_ending}")
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
                        ReflowMode::SentencePerLine | ReflowMode::SemanticLineBreaks => {
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
                            ReflowMode::SemanticLineBreaks => {
                                let num_sentences = split_into_sentences(&paragraph_text).len();
                                format!(
                                    "Paragraph should use semantic line breaks ({num_sentences} sentences)"
                                )
                            },
                            ReflowMode::Default => format!("Line length exceeds {} characters", config.line_length.get()),
                        },
                        line: warning_line,
                        column: 1,
                        end_line: warning_end_line,
                        end_column: lines[warning_end_line.saturating_sub(1)].len() + 1,
                        severity: Severity::Warning,
                        fix: Some(crate::rule::Fix::new(byte_range, replacement)),
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

    /// Calculate effective line length
    ///
    /// Returns the actual display length of the line using the configured length mode.
    fn calculate_effective_length(&self, line: &str) -> usize {
        self.calculate_string_length(line)
    }

    /// Calculate line length with inline link/image URLs removed.
    ///
    /// For each inline link `[text](url)` or image `![alt](url)` on the line,
    /// computes the "savings" from removing the URL portion (keeping only `[text]`
    /// or `![alt]`). Returns `effective_length - total_savings`.
    ///
    /// Handles nested constructs (e.g., `[![img](url)](url)`) by only counting the
    /// outermost construct to avoid double-counting.
    fn calculate_text_only_length(
        &self,
        effective_length: usize,
        line_number: usize,
        ctx: &crate::lint_context::LintContext,
    ) -> usize {
        let line_range = ctx.line_index.line_content_range(line_number);
        let line_byte_end = line_range.end;

        // Collect inline links/images on this line: (byte_offset, byte_end, text_only_display_len)
        let mut constructs: Vec<(usize, usize, usize)> = Vec::new();

        // Binary search: links are sorted by byte_offset, so link.line is non-decreasing
        let link_start = ctx.links.partition_point(|l| l.line < line_number);
        for link in &ctx.links[link_start..] {
            if link.line != line_number {
                break;
            }
            if link.is_reference {
                continue;
            }
            if !matches!(link.link_type, LinkType::Inline) {
                continue;
            }
            if link.byte_end > line_byte_end {
                continue;
            }
            let text_only_len = 2 + self.calculate_string_length(&link.text);
            constructs.push((link.byte_offset, link.byte_end, text_only_len));
        }

        let img_start = ctx.images.partition_point(|i| i.line < line_number);
        for image in &ctx.images[img_start..] {
            if image.line != line_number {
                break;
            }
            if image.is_reference {
                continue;
            }
            if !matches!(image.link_type, LinkType::Inline) {
                continue;
            }
            if image.byte_end > line_byte_end {
                continue;
            }
            let text_only_len = 3 + self.calculate_string_length(&image.alt_text);
            constructs.push((image.byte_offset, image.byte_end, text_only_len));
        }

        if constructs.is_empty() {
            return effective_length;
        }

        // Sort by byte offset to handle overlapping/nested constructs
        constructs.sort_by_key(|&(start, _, _)| start);

        let mut total_savings: usize = 0;
        let mut last_end: usize = 0;

        for (start, end, text_only_len) in &constructs {
            // Skip constructs nested inside a previously counted one
            if *start < last_end {
                continue;
            }
            // Full construct length in configured length mode
            let full_source = &ctx.content[*start..*end];
            let full_len = self.calculate_string_length(full_source);
            total_savings += full_len.saturating_sub(*text_only_len);
            last_end = *end;
        }

        effective_length.saturating_sub(total_savings)
    }
}
