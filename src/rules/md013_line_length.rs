/// Rule MD013: Line length
///
/// See [docs/md013.md](../../docs/md013.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::range_utils::LineIndex;
use crate::utils::range_utils::calculate_excess_range;
use crate::utils::regex_cache::{
    IMAGE_REF_PATTERN, INLINE_LINK_REGEX as MARKDOWN_LINK_PATTERN, LINK_REF_PATTERN, URL_IN_TEXT, URL_PATTERN,
};
use crate::utils::table_utils::TableUtils;
use crate::utils::text_reflow::split_into_sentences;
use toml;

pub mod md013_config;
use md013_config::{MD013Config, ReflowMode};

#[derive(Clone, Default)]
pub struct MD013LineLength {
    pub(crate) config: MD013Config,
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
        if ctx.is_in_code_block(current_line + 1) && !trimmed.is_empty() && !line.contains(' ') && !line.contains('\t')
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

        // Fast early return using should_skip
        // But don't skip if we're in reflow mode with Normalize or SentencePerLine
        if self.should_skip(ctx)
            && !(self.config.reflow
                && (self.config.reflow_mode == ReflowMode::Normalize
                    || self.config.reflow_mode == ReflowMode::SentencePerLine))
        {
            return Ok(Vec::new());
        }

        // Direct implementation without DocumentStructure
        let mut warnings = Vec::new();

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

        // Pre-filter lines that could be problematic to avoid processing all lines
        let mut candidate_lines = Vec::new();
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            // Quick length check first
            if line_info.content.len() > effective_config.line_length {
                candidate_lines.push(line_idx);
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
            ctx.lines.iter().map(|l| l.content.as_str()).collect()
        } else {
            content.lines().collect()
        };

        // Create a quick lookup set for heading lines (only if needed)
        let heading_lines_set: std::collections::HashSet<usize> = if !effective_config.headings {
            ctx.lines
                .iter()
                .enumerate()
                .filter(|(_, line)| line.heading.is_some())
                .map(|(idx, _)| idx + 1)
                .collect()
        } else {
            std::collections::HashSet::new()
        };

        // Use TableUtils to find all table blocks (only if needed)
        let table_lines_set: std::collections::HashSet<usize> = if !effective_config.tables {
            let table_blocks = TableUtils::find_table_blocks(content, ctx);
            let mut table_lines = std::collections::HashSet::new();
            for table in &table_blocks {
                table_lines.insert(table.header_line + 1);
                table_lines.insert(table.delimiter_line + 1);
                for &line in &table.content_lines {
                    table_lines.insert(line + 1);
                }
            }
            table_lines
        } else {
            std::collections::HashSet::new()
        };

        // Only process candidate lines that were pre-filtered
        // Skip line length checks entirely in sentence-per-line mode
        if effective_config.reflow_mode != ReflowMode::SentencePerLine {
            for &line_idx in &candidate_lines {
                let line_number = line_idx + 1;
                let line = lines[line_idx];

                // Calculate effective length excluding unbreakable URLs
                let effective_length = self.calculate_effective_length(line);

                // Use single line length limit for all content
                let line_limit = effective_config.line_length;

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
                        || (!effective_config.code_blocks && ctx.is_in_code_block(line_number))
                        || (!effective_config.tables && table_lines_set.contains(&line_number))
                        || ctx.lines[line_number - 1].blockquote.is_some()
                        || ctx.is_in_html_block(line_number)
                    {
                        continue;
                    }

                    // Skip lines that are only a URL, image ref, or link ref
                    if self.should_ignore_line(line, &lines, line_idx, ctx) {
                        continue;
                    }
                }

                // Don't provide fix for individual lines when reflow is enabled
                // Paragraph-based fixes will be handled separately
                let fix = None;

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
        // Skip if content is empty
        if ctx.content.is_empty() {
            return true;
        }

        // For sentence-per-line or normalize mode, never skip based on line length
        if self.config.reflow
            && (self.config.reflow_mode == ReflowMode::SentencePerLine
                || self.config.reflow_mode == ReflowMode::Normalize)
        {
            return false;
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
    /// Generate paragraph-based fixes
    fn generate_paragraph_fixes(
        &self,
        ctx: &crate::lint_context::LintContext,
        config: &MD013Config,
        lines: &[&str],
    ) -> Vec<LintWarning> {
        let mut warnings = Vec::new();
        let line_index = LineIndex::new(ctx.content.to_string());

        let mut i = 0;
        while i < lines.len() {
            let line_num = i + 1;

            // Skip special structures
            if ctx.is_in_code_block(line_num)
                || ctx.is_in_front_matter(line_num)
                || ctx.is_in_html_block(line_num)
                || (line_num > 0 && line_num <= ctx.lines.len() && ctx.lines[line_num - 1].blockquote.is_some())
                || lines[i].trim().starts_with('#')
                || TableUtils::is_potential_table_row(lines[i])
                || lines[i].trim().is_empty()
                || is_horizontal_rule(lines[i].trim())
            {
                i += 1;
                continue;
            }

            // Check if this is a list item - handle it specially
            let trimmed = lines[i].trim();
            if is_list_item(trimmed) {
                // Collect the entire list item including continuation lines
                let list_start = i;
                let (marker, first_content) = extract_list_marker_and_content(lines[i]);
                let marker_len = marker.len();

                // Track lines and their types (content, code block, fence)
                #[derive(Clone)]
                enum LineType {
                    Content(String),
                    CodeBlock(String, usize), // content and original indent
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
                        let trimmed = line_info.content.trim();

                        // Use pre-computed in_code_block from ctx
                        if line_info.in_code_block {
                            list_item_lines.push(LineType::CodeBlock(line_info.content[indent..].to_string(), indent));
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
                            let has_nested_content = list_item_lines
                                .iter()
                                .any(|line| matches!(line, LineType::Content(c) if is_list_item(c.trim())));

                            if !has_blank_before && !has_nested_content {
                                // Single-paragraph context with no prior nested items: starts a new item
                                // End parent collection; nested list will be processed next
                                break;
                            }
                            // else: multi-paragraph context or continuation of nested list, keep collecting
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
                            let content = trim_preserving_hard_break(&line_info.content[indent..]);
                            list_item_lines.push(LineType::Content(content));
                            i += 1;
                        } else {
                            // indent >= marker_len + 4: indented code block
                            list_item_lines.push(LineType::CodeBlock(line_info.content[indent..].to_string(), indent));
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

                // Split list_item_lines into blocks (paragraphs and code blocks)
                #[derive(Clone)]
                enum Block {
                    Paragraph(Vec<String>),
                    CodeBlock(Vec<(String, usize)>), // (content, indent) pairs
                }

                let mut blocks: Vec<Block> = Vec::new();
                let mut current_paragraph: Vec<String> = Vec::new();
                let mut current_code_block: Vec<(String, usize)> = Vec::new();
                let mut in_code = false;

                for line in &list_item_lines {
                    match line {
                        LineType::Empty => {
                            if in_code {
                                current_code_block.push((String::new(), 0));
                            } else if !current_paragraph.is_empty() {
                                blocks.push(Block::Paragraph(current_paragraph.clone()));
                                current_paragraph.clear();
                            }
                        }
                        LineType::Content(content) => {
                            if in_code {
                                // Switching from code to content
                                blocks.push(Block::CodeBlock(current_code_block.clone()));
                                current_code_block.clear();
                                in_code = false;
                            }
                            current_paragraph.push(content.clone());
                        }
                        LineType::CodeBlock(content, indent) => {
                            if !in_code {
                                // Switching from content to code
                                if !current_paragraph.is_empty() {
                                    blocks.push(Block::Paragraph(current_paragraph.clone()));
                                    current_paragraph.clear();
                                }
                                in_code = true;
                            }
                            current_code_block.push((content.clone(), *indent));
                        }
                    }
                }

                // Push remaining block
                if in_code && !current_code_block.is_empty() {
                    blocks.push(Block::CodeBlock(current_code_block));
                } else if !current_paragraph.is_empty() {
                    blocks.push(Block::Paragraph(current_paragraph));
                }

                // Check if reflowing is needed (only for content paragraphs, not code blocks)
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

                let needs_reflow = self.calculate_effective_length(&full_line) > config.line_length
                    || (config.reflow_mode == ReflowMode::Normalize && content_lines.len() > 1)
                    || (config.reflow_mode == ReflowMode::SentencePerLine && {
                        // Check if list item has multiple sentences
                        let sentences = split_into_sentences(&combined_content);
                        sentences.len() > 1
                    });

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
                    let reflow_options = crate::utils::text_reflow::ReflowOptions {
                        line_length: config.line_length - indent_size,
                        break_on_sentences: true,
                        preserve_breaks: false,
                        sentence_per_line: config.reflow_mode == ReflowMode::SentencePerLine,
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
                                    // Check if this segment ends with a hard break
                                    let has_hard_break = segment.last().is_some_and(|line| line.ends_with("  "));

                                    // Join and reflow the segment (removing the hard break marker for processing)
                                    let segment_for_reflow: Vec<String> = segment
                                        .iter()
                                        .map(|line| {
                                            // Strip hard break marker (2 spaces) for reflow processing
                                            if line.ends_with("  ") {
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
                                        if has_hard_break && let Some(last_line) = result.last_mut() {
                                            last_line.push_str("  ");
                                        }
                                    }
                                }

                                // Add blank line after paragraph block if there's a next block
                                if block_idx < blocks.len() - 1 {
                                    result.push(String::new());
                                }
                            }
                            Block::CodeBlock(code_lines) => {
                                // Preserve code blocks as-is with original indentation
                                if !is_first_block {
                                    result.push(String::new());
                                }

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
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: if config.reflow_mode == ReflowMode::SentencePerLine {
                                "Line contains multiple sentences (one sentence per line expected)".to_string()
                            } else {
                                format!("Line length exceeds {} characters", config.line_length)
                            },
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
                    || ctx.is_in_code_block(next_line_num)
                    || ctx.is_in_front_matter(next_line_num)
                    || ctx.is_in_html_block(next_line_num)
                    || (next_line_num > 0
                        && next_line_num <= ctx.lines.len()
                        && ctx.lines[next_line_num - 1].blockquote.is_some())
                    || next_trimmed.starts_with('#')
                    || TableUtils::is_potential_table_row(next_line)
                    || is_list_item(next_trimmed)
                    || is_horizontal_rule(next_trimmed)
                    || (next_trimmed.starts_with('[') && next_line.contains("]:"))
                {
                    break;
                }

                // Check if the previous line ends with a hard break (2+ spaces)
                if i > 0 && lines[i - 1].ends_with("  ") {
                    // Don't include lines after hard breaks in the same paragraph
                    break;
                }

                paragraph_lines.push(next_line);
                i += 1;
            }

            // Check if this paragraph needs reflowing
            let needs_reflow = match config.reflow_mode {
                ReflowMode::Normalize => {
                    // In normalize mode, reflow multi-line paragraphs
                    paragraph_lines.len() > 1
                }
                ReflowMode::SentencePerLine => {
                    // In sentence-per-line mode, check if any line has multiple sentences
                    paragraph_lines.iter().any(|line| {
                        // Count sentences in this line
                        let sentences = split_into_sentences(line);
                        sentences.len() > 1
                    })
                }
                ReflowMode::Default => {
                    // In default mode, only reflow if lines exceed limit
                    paragraph_lines
                        .iter()
                        .any(|line| self.calculate_effective_length(line) > config.line_length)
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

                // Combine paragraph lines into a single string for reflowing
                let paragraph_text = paragraph_lines.join(" ");

                // Check if the paragraph ends with a hard break
                let has_hard_break = paragraph_lines.last().is_some_and(|l| l.ends_with("  "));

                // Reflow the paragraph
                let reflow_options = crate::utils::text_reflow::ReflowOptions {
                    line_length: config.line_length,
                    break_on_sentences: true,
                    preserve_breaks: false,
                    sentence_per_line: config.reflow_mode == ReflowMode::SentencePerLine,
                };
                let mut reflowed = crate::utils::text_reflow::reflow_line(&paragraph_text, &reflow_options);

                // If the original paragraph ended with a hard break, preserve it
                if has_hard_break && !reflowed.is_empty() {
                    let last_idx = reflowed.len() - 1;
                    if !reflowed[last_idx].ends_with("  ") {
                        reflowed[last_idx].push_str("  ");
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
                    // In sentence-per-line mode, report lines with multiple sentences
                    let (warning_line, warning_end_line) = match config.reflow_mode {
                        ReflowMode::Normalize => (paragraph_start + 1, end_line + 1),
                        ReflowMode::SentencePerLine => {
                            // Find the first line with multiple sentences
                            let mut violating_line = paragraph_start;
                            for (idx, line) in paragraph_lines.iter().enumerate() {
                                let sentences = split_into_sentences(line);
                                if sentences.len() > 1 {
                                    violating_line = paragraph_start + idx;
                                    break;
                                }
                            }
                            (violating_line + 1, violating_line + 1)
                        }
                        ReflowMode::Default => {
                            // Find the first line that exceeds the limit
                            let mut violating_line = paragraph_start;
                            for (idx, line) in paragraph_lines.iter().enumerate() {
                                if self.calculate_effective_length(line) > config.line_length {
                                    violating_line = paragraph_start + idx;
                                    break;
                                }
                            }
                            (violating_line + 1, violating_line + 1)
                        }
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: match config.reflow_mode {
                            ReflowMode::Normalize => format!(
                                "Paragraph could be normalized to use line length of {} characters",
                                config.line_length
                            ),
                            ReflowMode::SentencePerLine => {
                                "Line contains multiple sentences (one sentence per line expected)".to_string()
                            }
                            ReflowMode::Default => format!("Line length exceeds {} characters", config.line_length),
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

    /// Calculate effective line length excluding unbreakable URLs
    fn calculate_effective_length(&self, line: &str) -> usize {
        if self.config.strict {
            // In strict mode, count everything
            return line.chars().count();
        }

        // Quick byte-level check: if line doesn't contain "http" or "[", it can't have URLs or markdown links
        let bytes = line.as_bytes();
        if !bytes.contains(&b'h') && !bytes.contains(&b'[') {
            return line.chars().count();
        }

        // More precise check for URLs and links
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

/// Extract list marker and content from a list item
/// Trim trailing whitespace while preserving hard breaks (exactly 2 trailing spaces)
/// Hard breaks in Markdown are indicated by 2 trailing spaces before a newline
fn trim_preserving_hard_break(s: &str) -> String {
    // Strip trailing \r from CRLF line endings first to handle Windows files
    let s = s.strip_suffix('\r').unwrap_or(s);

    // Check if there are at least 2 trailing spaces (potential hard break)
    if s.ends_with("  ") {
        // Find the position where non-space content ends
        let content_end = s.trim_end().len();
        if content_end == 0 {
            // String is all whitespace
            return String::new();
        }
        // Preserve exactly 2 trailing spaces for hard break
        format!("{}  ", &s[..content_end])
    } else {
        // No hard break, just trim all trailing whitespace
        s.trim_end().to_string()
    }
}

/// Split paragraph lines into segments at hard break boundaries.
/// Each segment is a group of lines that can be reflowed together.
/// Lines with hard breaks (ending with 2+ spaces) form segment boundaries.
///
/// Example:
///   Input:  ["Line 1", "Line 2  ", "Line 3", "Line 4"]
///   Output: [["Line 1", "Line 2  "], ["Line 3", "Line 4"]]
///
/// The first segment includes "Line 2  " which has a hard break at the end.
/// The second segment starts after the hard break.
fn split_into_segments(para_lines: &[String]) -> Vec<Vec<String>> {
    let mut segments: Vec<Vec<String>> = Vec::new();
    let mut current_segment: Vec<String> = Vec::new();

    for line in para_lines {
        current_segment.push(line.clone());

        // If this line has a hard break, end the current segment
        if line.ends_with("  ") {
            segments.push(current_segment.clone());
            current_segment.clear();
        }
    }

    // Add any remaining lines as the final segment
    if !current_segment.is_empty() {
        segments.push(current_segment);
    }

    segments
}

fn extract_list_marker_and_content(line: &str) -> (String, String) {
    // First, find the leading indentation
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let trimmed = &line[indent_len..];

    // Handle bullet lists
    // Trim trailing whitespace while preserving hard breaks
    if let Some(rest) = trimmed.strip_prefix("- ") {
        return (format!("{indent}- "), trim_preserving_hard_break(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        return (format!("{indent}* "), trim_preserving_hard_break(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("+ ") {
        return (format!("{indent}+ "), trim_preserving_hard_break(rest));
    }

    // Handle numbered lists on trimmed content
    let mut chars = trimmed.chars();
    let mut marker_content = String::new();

    while let Some(c) = chars.next() {
        marker_content.push(c);
        if c == '.' {
            // Check if next char is a space
            if let Some(next) = chars.next()
                && next == ' '
            {
                marker_content.push(next);
                // Trim trailing whitespace while preserving hard breaks
                let content = trim_preserving_hard_break(chars.as_str());
                return (format!("{indent}{marker_content}"), content);
            }
            break;
        }
    }

    // Fallback - shouldn't happen if is_list_item was correct
    (String::new(), line.to_string())
}

// Helper functions
fn is_horizontal_rule(line: &str) -> bool {
    if line.len() < 3 {
        return false;
    }
    // Check if line consists only of -, _, or * characters (at least 3)
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return false;
    }
    let first_char = chars[0];
    if first_char != '-' && first_char != '_' && first_char != '*' {
        return false;
    }
    // All characters should be the same (allowing spaces between)
    for c in &chars {
        if *c != first_char && *c != ' ' {
            return false;
        }
    }
    // Must have at least 3 of the marker character
    chars.iter().filter(|c| **c == first_char).count() >= 3
}

fn is_numbered_list_item(line: &str) -> bool {
    let mut chars = line.chars();
    // Must start with a digit
    if !chars.next().is_some_and(|c| c.is_numeric()) {
        return false;
    }
    // Can have more digits
    while let Some(c) = chars.next() {
        if c == '.' {
            // After period, must have a space or be end of line
            return chars.next().is_none_or(|c| c == ' ');
        }
        if !c.is_numeric() {
            return false;
        }
    }
    false
}

fn is_list_item(line: &str) -> bool {
    // Bullet lists
    if (line.starts_with('-') || line.starts_with('*') || line.starts_with('+'))
        && line.len() > 1
        && line.chars().nth(1) == Some(' ')
    {
        return true;
    }
    // Numbered lists
    is_numbered_list_item(line)
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
    fn test_content_detection() {
        let rule = MD013LineLength::default();

        // Use a line longer than default line_length (80) to ensure it's not skipped
        let long_line = "a".repeat(100);
        let ctx = LintContext::new(&long_line, crate::config::MarkdownFlavor::Standard);
        assert!(!rule.should_skip(&ctx)); // Should not skip processing when there's long content

        let empty_ctx = LintContext::new("", crate::config::MarkdownFlavor::Standard);
        assert!(rule.should_skip(&empty_ctx)); // Should skip processing when content is empty
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
        // Default mode should fix paragraphs that contain lines exceeding limit
        // The paragraph-based approach treats consecutive lines as a unit
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
        assert_eq!(warnings.len(), 1, "Should flag the paragraph with long line");
        // The warning reports the line that violates in default mode
        assert_eq!(warnings[0].line, 2, "Should flag line 2 that exceeds limit");

        let fixed = rule.fix(&ctx).unwrap();
        // The paragraph gets reflowed as a unit
        assert!(
            fixed.contains("Short line. This is"),
            "Should combine and reflow the paragraph"
        );
        assert!(
            fixed.contains("wrapping. Another short"),
            "Should include all paragraph content"
        );
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

    #[test]
    fn test_sentence_per_line_detection() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::SentencePerLine,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config.clone());

        // Test detection of multiple sentences
        let content = "This is sentence one. This is sentence two. And sentence three!";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        // Debug: check if should_skip returns false
        assert!(!rule.should_skip(&ctx), "Should not skip for sentence-per-line mode");

        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should detect multiple sentences on one line");
        assert_eq!(
            result[0].message,
            "Line contains multiple sentences (one sentence per line expected)"
        );
    }

    #[test]
    fn test_sentence_per_line_fix() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::SentencePerLine,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "First sentence. Second sentence.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should detect violation");
        assert!(result[0].fix.is_some(), "Should provide a fix");

        let fix = result[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement.trim(), "First sentence.\nSecond sentence.");
    }

    #[test]
    fn test_sentence_per_line_abbreviations() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::SentencePerLine,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Should NOT trigger on abbreviations
        let content = "Mr. Smith met Dr. Jones at 3:00 PM.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should not detect abbreviations as sentence boundaries"
        );
    }

    #[test]
    fn test_sentence_per_line_with_markdown() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::SentencePerLine,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "# Heading\n\nSentence with **bold**. Another with [link](url).";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should detect multiple sentences with markdown");
        assert_eq!(result[0].line, 3); // Third line has the violation
    }

    #[test]
    fn test_sentence_per_line_questions_exclamations() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::SentencePerLine,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "Is this a question? Yes it is! And a statement.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should detect sentences with ? and !");

        let fix = result[0].fix.as_ref().unwrap();
        let lines: Vec<&str> = fix.replacement.trim().lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Is this a question?");
        assert_eq!(lines[1], "Yes it is!");
        assert_eq!(lines[2], "And a statement.");
    }

    #[test]
    fn test_sentence_per_line_in_lists() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::SentencePerLine,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "- List item one. With two sentences.\n- Another item.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should detect sentences in list items");
        // The fix should preserve list formatting
        let fix = result[0].fix.as_ref().unwrap();
        assert!(fix.replacement.starts_with("- "), "Should preserve list marker");
    }

    #[test]
    fn test_multi_paragraph_list_item_with_3_space_indent() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            line_length: 999999,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "1. First paragraph\n   continuation line.\n\n   Second paragraph\n   more content.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should detect multi-line paragraphs in list item");
        let fix = result[0].fix.as_ref().unwrap();

        // Should preserve paragraph structure, not collapse everything
        assert!(
            fix.replacement.contains("\n\n"),
            "Should preserve blank line between paragraphs"
        );
        assert!(fix.replacement.starts_with("1. "), "Should preserve list marker");
    }

    #[test]
    fn test_multi_paragraph_list_item_with_4_space_indent() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            line_length: 999999,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // User's example from issue #76 - uses 4 spaces for continuation
        let content = "1. It **generated an application template**. There's a lot of files and\n    configurations required to build a native installer, above and\n    beyond the code of your actual application.\n\n    If you're not happy with the template provided by Briefcase, you can\n    provide your own.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert!(
            !result.is_empty(),
            "Should detect multi-line paragraphs in list item with 4-space indent"
        );
        let fix = result[0].fix.as_ref().unwrap();

        // Should preserve paragraph structure
        assert!(
            fix.replacement.contains("\n\n"),
            "Should preserve blank line between paragraphs"
        );
        assert!(fix.replacement.starts_with("1. "), "Should preserve list marker");

        // Both paragraphs should be reflowed but kept separate
        let lines: Vec<&str> = fix.replacement.split('\n').collect();
        let blank_line_idx = lines.iter().position(|l| l.trim().is_empty());
        assert!(blank_line_idx.is_some(), "Should have blank line separating paragraphs");
    }

    #[test]
    fn test_multi_paragraph_bullet_list_item() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            line_length: 999999,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "- First paragraph\n  continuation.\n\n  Second paragraph\n  more text.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should detect multi-line paragraphs in bullet list");
        let fix = result[0].fix.as_ref().unwrap();

        assert!(
            fix.replacement.contains("\n\n"),
            "Should preserve blank line between paragraphs"
        );
        assert!(fix.replacement.starts_with("- "), "Should preserve bullet marker");
    }

    #[test]
    fn test_code_block_in_list_item_five_spaces() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            line_length: 80,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // 5 spaces = code block indentation (marker_len=3 + 4 = 7, but we have 5 which is marker_len+2, still valid continuation but >= marker_len+4 would be code)
        // For "1. " marker (3 chars), 3+4=7 spaces would be code block
        let content = "1. First paragraph with some text that should be reflowed.\n\n       code_block()\n       more_code()\n\n   Second paragraph.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        if !result.is_empty() {
            let fix = result[0].fix.as_ref().unwrap();
            // Code block lines should NOT be reflowed - they should be preserved with original indentation
            assert!(
                fix.replacement.contains("       code_block()"),
                "Code block should be preserved: {}",
                fix.replacement
            );
            assert!(
                fix.replacement.contains("       more_code()"),
                "Code block should be preserved: {}",
                fix.replacement
            );
        }
    }

    #[test]
    fn test_fenced_code_block_in_list_item() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            line_length: 80,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "1. First paragraph with some text.\n\n   ```rust\n   fn foo() {}\n   let x = 1;\n   ```\n\n   Second paragraph.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        if !result.is_empty() {
            let fix = result[0].fix.as_ref().unwrap();
            // Fenced code block should be preserved
            assert!(
                fix.replacement.contains("```rust"),
                "Should preserve fence: {}",
                fix.replacement
            );
            assert!(
                fix.replacement.contains("fn foo() {}"),
                "Should preserve code: {}",
                fix.replacement
            );
            assert!(
                fix.replacement.contains("```"),
                "Should preserve closing fence: {}",
                fix.replacement
            );
        }
    }

    #[test]
    fn test_mixed_indentation_3_and_4_spaces() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            line_length: 999999,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // First continuation has 3 spaces, second has 4 - both should be accepted
        let content = "1. Text\n   3 space continuation\n    4 space continuation";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should detect multi-line list item");
        let fix = result[0].fix.as_ref().unwrap();
        // Should reflow all content together
        assert!(
            fix.replacement.contains("3 space continuation"),
            "Should include 3-space line: {}",
            fix.replacement
        );
        assert!(
            fix.replacement.contains("4 space continuation"),
            "Should include 4-space line: {}",
            fix.replacement
        );
    }

    #[test]
    fn test_nested_list_in_multi_paragraph_item() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            line_length: 999999,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        let content = "1. First paragraph.\n\n   - Nested item\n     continuation\n\n   Second paragraph.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Nested lists at continuation indent should be INCLUDED in parent item
        assert!(!result.is_empty(), "Should detect and reflow parent item");
        if let Some(fix) = result[0].fix.as_ref() {
            // The nested list should be preserved in the output
            assert!(
                fix.replacement.contains("- Nested"),
                "Should preserve nested list: {}",
                fix.replacement
            );
            assert!(
                fix.replacement.contains("Second paragraph"),
                "Should include content after nested list: {}",
                fix.replacement
            );
        }
    }

    #[test]
    fn test_nested_fence_markers_different_types() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            line_length: 80,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Nested fences with different markers (backticks inside tildes)
        let content = "1. Example with nested fences:\n\n   ~~~markdown\n   This shows ```python\n   code = True\n   ```\n   ~~~\n\n   Text after.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        if !result.is_empty() {
            let fix = result[0].fix.as_ref().unwrap();
            // Inner fence should NOT close outer fence (different markers)
            assert!(
                fix.replacement.contains("```python"),
                "Should preserve inner fence: {}",
                fix.replacement
            );
            assert!(
                fix.replacement.contains("~~~"),
                "Should preserve outer fence: {}",
                fix.replacement
            );
            // All lines should remain as code
            assert!(
                fix.replacement.contains("code = True"),
                "Should preserve code: {}",
                fix.replacement
            );
        }
    }

    #[test]
    fn test_nested_fence_markers_same_type() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            line_length: 80,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Nested backticks - inner must have different length or won't work
        let content =
            "1. Example:\n\n   ````markdown\n   Shows ```python in code\n   ```\n   text here\n   ````\n\n   After.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        if !result.is_empty() {
            let fix = result[0].fix.as_ref().unwrap();
            // 4 backticks opened, 3 backticks shouldn't close it
            assert!(
                fix.replacement.contains("```python"),
                "Should preserve inner fence: {}",
                fix.replacement
            );
            assert!(
                fix.replacement.contains("````"),
                "Should preserve outer fence: {}",
                fix.replacement
            );
            assert!(
                fix.replacement.contains("text here"),
                "Should keep text as code: {}",
                fix.replacement
            );
        }
    }

    #[test]
    fn test_sibling_list_item_breaks_parent() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            line_length: 999999,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Sibling list item (at indent 0, before parent marker at 3)
        let content = "1. First item\n   continuation.\n2. Second item";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should process first item only, second item breaks it
        if !result.is_empty() {
            let fix = result[0].fix.as_ref().unwrap();
            // Should only include first item
            assert!(fix.replacement.starts_with("1. "), "Should start with first marker");
            assert!(fix.replacement.contains("continuation"), "Should include continuation");
            // Should NOT include second item (it's outside the byte range)
        }
    }

    #[test]
    fn test_nested_list_at_continuation_indent_preserved() {
        let config = MD013Config {
            reflow: true,
            reflow_mode: ReflowMode::Normalize,
            line_length: 999999,
            ..Default::default()
        };
        let rule = MD013LineLength::from_config_struct(config);

        // Nested list at exactly continuation indent (3 spaces for "1. ")
        let content = "1. Parent paragraph\n   with continuation.\n\n   - Nested at 3 spaces\n   - Another nested\n\n   After nested.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        if !result.is_empty() {
            let fix = result[0].fix.as_ref().unwrap();
            // All nested content should be preserved
            assert!(
                fix.replacement.contains("- Nested"),
                "Should include first nested item: {}",
                fix.replacement
            );
            assert!(
                fix.replacement.contains("- Another"),
                "Should include second nested item: {}",
                fix.replacement
            );
            assert!(
                fix.replacement.contains("After nested"),
                "Should include content after nested list: {}",
                fix.replacement
            );
        }
    }
}
