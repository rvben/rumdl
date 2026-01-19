use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::code_block_utils::CodeBlockStyle;
use crate::utils::element_cache::ElementCache;
use crate::utils::mkdocs_admonitions;
use crate::utils::mkdocs_footnotes;
use crate::utils::mkdocs_tabs;
use crate::utils::range_utils::calculate_line_range;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use toml;

mod md046_config;
use md046_config::MD046Config;

/// Rule MD046: Code block style
///
/// See [docs/md046.md](../../docs/md046.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when code blocks do not use a consistent style (either fenced or indented).
#[derive(Clone)]
pub struct MD046CodeBlockStyle {
    config: MD046Config,
}

impl MD046CodeBlockStyle {
    pub fn new(style: CodeBlockStyle) -> Self {
        Self {
            config: MD046Config { style },
        }
    }

    pub fn from_config_struct(config: MD046Config) -> Self {
        Self { config }
    }

    /// Check if line has valid fence indentation per CommonMark spec (0-3 spaces)
    ///
    /// Per CommonMark 0.31.2: "An opening code fence may be indented 0-3 spaces."
    /// 4+ spaces of indentation makes it an indented code block instead.
    fn has_valid_fence_indent(line: &str) -> bool {
        ElementCache::calculate_indentation_width_default(line) < 4
    }

    /// Check if a line is a valid fenced code block start per CommonMark spec
    ///
    /// Per CommonMark 0.31.2: "A code fence is a sequence of at least three consecutive
    /// backtick characters (`) or tilde characters (~). An opening code fence may be
    /// indented 0-3 spaces."
    ///
    /// This means 4+ spaces of indentation makes it an indented code block instead,
    /// where the fence characters become literal content.
    fn is_fenced_code_block_start(&self, line: &str) -> bool {
        if !Self::has_valid_fence_indent(line) {
            return false;
        }

        let trimmed = line.trim_start();
        trimmed.starts_with("```") || trimmed.starts_with("~~~")
    }

    fn is_list_item(&self, line: &str) -> bool {
        let trimmed = line.trim_start();
        (trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ "))
            || (trimmed.len() > 2
                && trimmed.chars().next().unwrap().is_numeric()
                && (trimmed.contains(". ") || trimmed.contains(") ")))
    }

    /// Check if a line is a footnote definition according to CommonMark footnote extension spec
    ///
    /// # Specification Compliance
    /// Based on commonmark-hs footnote extension and GitHub's implementation:
    /// - Format: `[^label]: content`
    /// - Labels cannot be empty or whitespace-only
    /// - Labels cannot contain line breaks (unlike regular link references)
    /// - Labels typically contain alphanumerics, hyphens, underscores (though some parsers are more permissive)
    ///
    /// # Examples
    /// Valid:
    /// - `[^1]: Footnote text`
    /// - `[^foo-bar]: Content`
    /// - `[^test_123]: More content`
    ///
    /// Invalid:
    /// - `[^]: No label`
    /// - `[^ ]: Whitespace only`
    /// - `[^]]: Extra bracket`
    fn is_footnote_definition(&self, line: &str) -> bool {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("[^") || trimmed.len() < 5 {
            return false;
        }

        if let Some(close_bracket_pos) = trimmed.find("]:")
            && close_bracket_pos > 2
        {
            let label = &trimmed[2..close_bracket_pos];

            if label.trim().is_empty() {
                return false;
            }

            // Per spec: labels cannot contain line breaks (check for \r since \n can't appear in a single line)
            if label.contains('\r') {
                return false;
            }

            // Validate characters per GitHub's behavior: alphanumeric, hyphens, underscores only
            if label.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                return true;
            }
        }

        false
    }

    /// Pre-compute which lines are in block continuation context (lists, footnotes) with a single forward pass
    ///
    /// # Specification-Based Context Tracking
    /// This function implements CommonMark-style block continuation semantics:
    ///
    /// ## List Items
    /// - List items can contain multiple paragraphs and blocks
    /// - Content continues if indented appropriately
    /// - Context ends at structural boundaries (headings, horizontal rules) or column-0 paragraphs
    ///
    /// ## Footnotes
    /// Per commonmark-hs footnote extension and GitHub's implementation:
    /// - Footnote content continues as long as it's indented
    /// - Blank lines within footnotes don't terminate them (if next content is indented)
    /// - Non-indented content terminates the footnote
    /// - Similar to list items but can span more content
    ///
    /// # Performance
    /// O(n) single forward pass, replacing O(n²) backward scanning
    ///
    /// # Returns
    /// Boolean vector where `true` indicates the line is part of a list/footnote continuation
    fn precompute_block_continuation_context(&self, lines: &[&str]) -> Vec<bool> {
        let mut in_continuation_context = vec![false; lines.len()];
        let mut last_list_item_line: Option<usize> = None;
        let mut last_footnote_line: Option<usize> = None;
        let mut blank_line_count = 0;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();

            // Check if this is a list item
            if self.is_list_item(line) {
                last_list_item_line = Some(i);
                last_footnote_line = None; // List item ends any footnote context
                blank_line_count = 0;
                in_continuation_context[i] = true;
                continue;
            }

            // Check if this is a footnote definition
            if self.is_footnote_definition(line) {
                last_footnote_line = Some(i);
                last_list_item_line = None; // Footnote ends any list context
                blank_line_count = 0;
                in_continuation_context[i] = true;
                continue;
            }

            // Handle empty lines
            if line.trim().is_empty() {
                // Blank lines within continuations are allowed
                if last_list_item_line.is_some() || last_footnote_line.is_some() {
                    blank_line_count += 1;
                    in_continuation_context[i] = true;

                    // Per spec: multiple consecutive blank lines might terminate context
                    // GitHub allows multiple blank lines within footnotes if next content is indented
                    // We'll check on the next non-blank line
                }
                continue;
            }

            // Non-empty line - check for structural breaks or continuation
            if indent_len == 0 && !trimmed.is_empty() {
                // Content at column 0 (not indented)

                // Headings definitely end all contexts
                if trimmed.starts_with('#') {
                    last_list_item_line = None;
                    last_footnote_line = None;
                    blank_line_count = 0;
                    continue;
                }

                // Horizontal rules end all contexts
                if trimmed.starts_with("---") || trimmed.starts_with("***") {
                    last_list_item_line = None;
                    last_footnote_line = None;
                    blank_line_count = 0;
                    continue;
                }

                // Non-indented paragraph/content terminates contexts
                // But be conservative: allow some distance for lists
                if let Some(list_line) = last_list_item_line
                    && (i - list_line > 5 || blank_line_count > 1)
                {
                    last_list_item_line = None;
                }

                // For footnotes, non-indented content always terminates
                if last_footnote_line.is_some() {
                    last_footnote_line = None;
                }

                blank_line_count = 0;

                // If no active context, this is a regular line
                if last_list_item_line.is_none() && last_footnote_line.is_some() {
                    last_footnote_line = None;
                }
                continue;
            }

            // Indented content - part of continuation if we have active context
            if indent_len > 0 && (last_list_item_line.is_some() || last_footnote_line.is_some()) {
                in_continuation_context[i] = true;
                blank_line_count = 0;
            }
        }

        in_continuation_context
    }

    /// Check if a line is an indented code block using pre-computed context arrays
    fn is_indented_code_block_with_context(
        &self,
        lines: &[&str],
        i: usize,
        is_mkdocs: bool,
        in_list_context: &[bool],
        in_tab_context: &[bool],
        in_admonition_context: &[bool],
    ) -> bool {
        if i >= lines.len() {
            return false;
        }

        let line = lines[i];

        // Check if indented by at least 4 columns (accounting for tab expansion)
        let indent = ElementCache::calculate_indentation_width_default(line);
        if indent < 4 {
            return false;
        }

        // Check if this is part of a list structure (pre-computed)
        if in_list_context[i] {
            return false;
        }

        // Skip if this is MkDocs tab content (pre-computed)
        if is_mkdocs && in_tab_context[i] {
            return false;
        }

        // Skip if this is MkDocs admonition content (pre-computed)
        // Admonitions are supported in MkDocs and other extended Markdown processors
        if is_mkdocs && in_admonition_context[i] {
            return false;
        }

        // Check if preceded by a blank line (typical for code blocks)
        // OR if the previous line is also an indented code block (continuation)
        let has_blank_line_before = i == 0 || lines[i - 1].trim().is_empty();
        let prev_is_indented_code = i > 0
            && ElementCache::calculate_indentation_width_default(lines[i - 1]) >= 4
            && !in_list_context[i - 1]
            && !(is_mkdocs && in_tab_context[i - 1])
            && !(is_mkdocs && in_admonition_context[i - 1]);

        // If no blank line before and previous line is not indented code,
        // it's likely list continuation, not a code block
        if !has_blank_line_before && !prev_is_indented_code {
            return false;
        }

        true
    }

    /// Pre-compute which lines are in MkDocs tab context with a single forward pass
    fn precompute_mkdocs_tab_context(&self, lines: &[&str]) -> Vec<bool> {
        let mut in_tab_context = vec![false; lines.len()];
        let mut current_tab_indent: Option<usize> = None;

        for (i, line) in lines.iter().enumerate() {
            // Check if this is a tab marker
            if mkdocs_tabs::is_tab_marker(line) {
                let tab_indent = mkdocs_tabs::get_tab_indent(line).unwrap_or(0);
                current_tab_indent = Some(tab_indent);
                in_tab_context[i] = true;
                continue;
            }

            // If we have a current tab, check if this line is tab content
            if let Some(tab_indent) = current_tab_indent {
                if mkdocs_tabs::is_tab_content(line, tab_indent) {
                    in_tab_context[i] = true;
                } else if !line.trim().is_empty() && ElementCache::calculate_indentation_width_default(line) < 4 {
                    // Non-indented, non-empty line ends tab context
                    current_tab_indent = None;
                } else {
                    // Empty or indented line maintains tab context
                    in_tab_context[i] = true;
                }
            }
        }

        in_tab_context
    }

    /// Pre-compute which lines are in MkDocs admonition context with a single forward pass
    ///
    /// MkDocs admonitions use `!!!` or `???` markers followed by a type, and their content
    /// is indented by 4 spaces. This function marks all admonition markers and their
    /// indented content as being in an admonition context, preventing them from being
    /// incorrectly flagged as indented code blocks.
    ///
    /// Supports nested admonitions by maintaining a stack of active admonition contexts.
    fn precompute_mkdocs_admonition_context(&self, lines: &[&str]) -> Vec<bool> {
        let mut in_admonition_context = vec![false; lines.len()];
        // Stack of active admonition indentation levels (supports nesting)
        let mut admonition_stack: Vec<usize> = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let line_indent = ElementCache::calculate_indentation_width_default(line);

            // Check if this is an admonition marker
            if mkdocs_admonitions::is_admonition_start(line) {
                let adm_indent = mkdocs_admonitions::get_admonition_indent(line).unwrap_or(0);

                // Pop any admonitions that this one is not nested within
                while let Some(&top_indent) = admonition_stack.last() {
                    // New admonition must be indented more than parent to be nested
                    if adm_indent <= top_indent {
                        admonition_stack.pop();
                    } else {
                        break;
                    }
                }

                // Push this admonition onto the stack
                admonition_stack.push(adm_indent);
                in_admonition_context[i] = true;
                continue;
            }

            // Handle empty lines - they're valid within admonitions
            if line.trim().is_empty() {
                if !admonition_stack.is_empty() {
                    in_admonition_context[i] = true;
                }
                continue;
            }

            // For non-empty lines, check if we're still in any admonition context
            // Pop admonitions where the content indent requirement is not met
            while let Some(&top_indent) = admonition_stack.last() {
                // Content must be indented at least 4 spaces from the admonition marker
                if line_indent >= top_indent + 4 {
                    // This line is valid content for the top admonition (or one below)
                    break;
                } else {
                    // Not indented enough for this admonition - pop it
                    admonition_stack.pop();
                }
            }

            // If we're still in any admonition context, mark this line
            if !admonition_stack.is_empty() {
                in_admonition_context[i] = true;
            }
        }

        in_admonition_context
    }

    /// Categorize indented blocks for fix behavior
    ///
    /// Returns two vectors:
    /// - `is_misplaced`: Lines that are part of a complete misplaced fenced block (dedent only)
    /// - `contains_fences`: Lines that contain fence markers but aren't a complete block (skip fixing)
    ///
    /// A misplaced fenced block is a contiguous indented block that:
    /// 1. Starts with a valid fence opener (``` or ~~~)
    /// 2. Ends with a matching fence closer
    ///
    /// An unsafe block contains fence markers but isn't complete - wrapping would create invalid markdown.
    fn categorize_indented_blocks(
        &self,
        lines: &[&str],
        is_mkdocs: bool,
        in_list_context: &[bool],
        in_tab_context: &[bool],
        in_admonition_context: &[bool],
    ) -> (Vec<bool>, Vec<bool>) {
        let mut is_misplaced = vec![false; lines.len()];
        let mut contains_fences = vec![false; lines.len()];

        // Find contiguous indented blocks and categorize them
        let mut i = 0;
        while i < lines.len() {
            // Find the start of an indented block
            if !self.is_indented_code_block_with_context(
                lines,
                i,
                is_mkdocs,
                in_list_context,
                in_tab_context,
                in_admonition_context,
            ) {
                i += 1;
                continue;
            }

            // Found start of an indented block - collect all contiguous lines
            let block_start = i;
            let mut block_end = i;

            while block_end < lines.len()
                && self.is_indented_code_block_with_context(
                    lines,
                    block_end,
                    is_mkdocs,
                    in_list_context,
                    in_tab_context,
                    in_admonition_context,
                )
            {
                block_end += 1;
            }

            // Now we have an indented block from block_start to block_end (exclusive)
            if block_end > block_start {
                let first_line = lines[block_start].trim_start();
                let last_line = lines[block_end - 1].trim_start();

                // Check if first line is a fence opener
                let is_backtick_fence = first_line.starts_with("```");
                let is_tilde_fence = first_line.starts_with("~~~");

                if is_backtick_fence || is_tilde_fence {
                    let fence_char = if is_backtick_fence { '`' } else { '~' };
                    let opener_len = first_line.chars().take_while(|&c| c == fence_char).count();

                    // Check if last line is a matching fence closer
                    let closer_fence_len = last_line.chars().take_while(|&c| c == fence_char).count();
                    let after_closer = &last_line[closer_fence_len..];

                    if closer_fence_len >= opener_len && after_closer.trim().is_empty() {
                        // Complete misplaced fenced block - safe to dedent
                        is_misplaced[block_start..block_end].fill(true);
                    } else {
                        // Incomplete fenced block - unsafe to wrap (would create nested fences)
                        contains_fences[block_start..block_end].fill(true);
                    }
                } else {
                    // Check if ANY line in the block contains fence markers
                    // If so, wrapping would create invalid markdown
                    let has_fence_markers = (block_start..block_end).any(|j| {
                        let trimmed = lines[j].trim_start();
                        trimmed.starts_with("```") || trimmed.starts_with("~~~")
                    });

                    if has_fence_markers {
                        contains_fences[block_start..block_end].fill(true);
                    }
                }
            }

            i = block_end;
        }

        (is_misplaced, contains_fences)
    }

    fn check_unclosed_code_blocks(
        &self,
        ctx: &crate::lint_context::LintContext,
    ) -> Result<Vec<LintWarning>, LintError> {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = ctx.content.lines().collect();

        // Use pulldown-cmark to detect fenced code blocks - this handles list-indented fences correctly
        let options = Options::all();
        let parser = Parser::new_ext(ctx.content, options).into_offset_iter();

        // Track code blocks: (start_byte, end_byte, fence_marker, line_idx, is_fenced, is_markdown_doc)
        let mut code_blocks: Vec<(usize, usize, String, usize, bool, bool)> = Vec::new();
        let mut current_block_start: Option<(usize, String, usize, bool)> = None;

        for (event, range) in parser {
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                    // Find the line index for this byte offset
                    let line_idx = ctx
                        .line_offsets
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|&(_, offset)| *offset <= range.start)
                        .map(|(idx, _)| idx)
                        .unwrap_or(0);

                    // Determine fence marker from the actual line content
                    let line = lines.get(line_idx).unwrap_or(&"");
                    let trimmed = line.trim();

                    // Find the fence marker - could be at start of line or after list marker
                    let fence_marker = if let Some(pos) = trimmed.find("```") {
                        let count = trimmed[pos..].chars().take_while(|&c| c == '`').count();
                        "`".repeat(count)
                    } else if let Some(pos) = trimmed.find("~~~") {
                        let count = trimmed[pos..].chars().take_while(|&c| c == '~').count();
                        "~".repeat(count)
                    } else {
                        "```".to_string()
                    };

                    // Check if this is a markdown documentation block
                    let lang_info = info.to_string().to_lowercase();
                    let is_markdown_doc = lang_info.starts_with("markdown") || lang_info.starts_with("md");

                    current_block_start = Some((range.start, fence_marker, line_idx, is_markdown_doc));
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some((start, fence_marker, line_idx, is_markdown_doc)) = current_block_start.take() {
                        code_blocks.push((start, range.end, fence_marker, line_idx, true, is_markdown_doc));
                    }
                }
                _ => {}
            }
        }

        // Check if any block is a markdown documentation block - if so, skip all
        // unclosed block detection since markdown docs often contain fence examples
        // that pulldown-cmark misparses
        let has_markdown_doc_block = code_blocks.iter().any(|(_, _, _, _, _, is_md)| *is_md);

        // Handle unclosed code block - pulldown-cmark extends unclosed blocks to EOF
        // and still emits End event, so we need to check if block ends at EOF without closing fence
        // Skip if document contains markdown documentation blocks (they have nested fence examples)
        if !has_markdown_doc_block {
            for (block_start, block_end, fence_marker, opening_line_idx, is_fenced, _is_md) in &code_blocks {
                if !is_fenced {
                    continue;
                }

                // Only check blocks that extend to EOF
                if *block_end != ctx.content.len() {
                    continue;
                }

                // Check if the last NON-EMPTY line of content is a valid closing fence
                // (skip trailing empty lines)
                let last_non_empty_line = lines.iter().rev().find(|l| !l.trim().is_empty()).unwrap_or(&"");
                let trimmed = last_non_empty_line.trim();
                let fence_char = fence_marker.chars().next().unwrap_or('`');

                // Check if it's a closing fence (just fence chars, no content after)
                let has_closing_fence = if fence_char == '`' {
                    trimmed.starts_with("```") && {
                        let fence_len = trimmed.chars().take_while(|&c| c == '`').count();
                        trimmed[fence_len..].trim().is_empty()
                    }
                } else {
                    trimmed.starts_with("~~~") && {
                        let fence_len = trimmed.chars().take_while(|&c| c == '~').count();
                        trimmed[fence_len..].trim().is_empty()
                    }
                };

                if !has_closing_fence {
                    let line = lines.get(*opening_line_idx).unwrap_or(&"");
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(*opening_line_idx + 1, line);

                    // Skip if inside HTML comment
                    if let Some(line_info) = ctx.lines.get(*opening_line_idx)
                        && line_info.in_html_comment
                    {
                        continue;
                    }

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: format!("Code block opened with '{fence_marker}' but never closed"),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: (ctx.content.len()..ctx.content.len()),
                            replacement: format!("\n{fence_marker}"),
                        }),
                    });
                }

                let _ = block_start; // Suppress unused warning
            }
        }

        // Also check for truly unclosed blocks (pulldown-cmark saw Start but no End)
        // Skip if document contains markdown documentation blocks
        if !has_markdown_doc_block && let Some((_start, fence_marker, line_idx, _is_md)) = current_block_start {
            let line = lines.get(line_idx).unwrap_or(&"");
            let (start_line, start_col, end_line, end_col) = calculate_line_range(line_idx + 1, line);

            // Skip if inside HTML comment
            if let Some(line_info) = ctx.lines.get(line_idx)
                && line_info.in_html_comment
            {
                return Ok(warnings);
            }

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!("Code block opened with '{fence_marker}' but never closed"),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: (ctx.content.len()..ctx.content.len()),
                    replacement: format!("\n{fence_marker}"),
                }),
            });
        }

        // Check for nested fence issues (same fence char with >= length inside a block)
        // This uses a separate pass with manual parsing, but only for fences that
        // pulldown-cmark recognized as valid code blocks
        // Skip entirely if document has markdown documentation blocks
        if has_markdown_doc_block {
            return Ok(warnings);
        }

        for (block_start, block_end, fence_marker, opening_line_idx, is_fenced, is_markdown_doc) in &code_blocks {
            if !is_fenced {
                continue;
            }

            // Skip nested fence detection for markdown documentation blocks
            if *is_markdown_doc {
                continue;
            }

            let opening_line = lines.get(*opening_line_idx).unwrap_or(&"");

            let fence_char = fence_marker.chars().next().unwrap_or('`');
            let fence_length = fence_marker.len();

            // Check lines within this code block for potential nested fences
            for (i, line) in lines.iter().enumerate() {
                let line_start = ctx.line_offsets.get(i).copied().unwrap_or(0);
                let line_end = ctx.line_offsets.get(i + 1).copied().unwrap_or(ctx.content.len());

                // Skip if line is not inside this code block (excluding opening/closing lines)
                if line_start <= *block_start || line_end >= *block_end {
                    continue;
                }

                // Skip lines inside HTML comments
                if let Some(line_info) = ctx.lines.get(i)
                    && line_info.in_html_comment
                {
                    continue;
                }

                let trimmed = line.trim();

                // Check if this looks like a fence with same char and >= length
                if (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
                    && trimmed.starts_with(&fence_char.to_string())
                {
                    let inner_fence_length = trimmed.chars().take_while(|&c| c == fence_char).count();
                    let after_fence = &trimmed[inner_fence_length..];

                    // Only flag if same char, >= length, and has language (opening fence pattern)
                    if inner_fence_length >= fence_length
                        && !after_fence.trim().is_empty()
                        && !after_fence.contains('`')
                    {
                        // Check if it looks like a valid language identifier
                        let identifier = after_fence.trim();
                        let looks_like_language =
                            identifier.chars().next().is_some_and(|c| c.is_alphabetic() || c == '#')
                                && identifier.len() <= 30
                                && identifier.chars().all(|c| c.is_alphanumeric() || "-_+#. ".contains(c));

                        if looks_like_language {
                            let (start_line, start_col, end_line, end_col) =
                                calculate_line_range(*opening_line_idx + 1, opening_line);

                            let line_start_byte = ctx.line_index.get_line_start_byte(i + 1).unwrap_or(0);

                            warnings.push(LintWarning {
                                rule_name: Some(self.name().to_string()),
                                line: start_line,
                                column: start_col,
                                end_line,
                                end_column: end_col,
                                message: format!(
                                    "Code block '{fence_marker}' should be closed before starting new one at line {}",
                                    i + 1
                                ),
                                severity: Severity::Warning,
                                fix: Some(Fix {
                                    range: (line_start_byte..line_start_byte),
                                    replacement: format!("{fence_marker}\n\n"),
                                }),
                            });

                            break; // Only report first nested issue per block
                        }
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn detect_style(&self, content: &str, is_mkdocs: bool) -> Option<CodeBlockStyle> {
        // Empty content has no style
        if content.is_empty() {
            return None;
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut fenced_count = 0;
        let mut indented_count = 0;

        // Pre-compute list, tab, and admonition contexts for efficiency
        let in_list_context = self.precompute_block_continuation_context(&lines);
        let in_tab_context = if is_mkdocs {
            self.precompute_mkdocs_tab_context(&lines)
        } else {
            vec![false; lines.len()]
        };
        let in_admonition_context = if is_mkdocs {
            self.precompute_mkdocs_admonition_context(&lines)
        } else {
            vec![false; lines.len()]
        };

        // Count all code block occurrences (prevalence-based approach)
        let mut in_fenced = false;
        let mut prev_was_indented = false;

        for (i, line) in lines.iter().enumerate() {
            if self.is_fenced_code_block_start(line) {
                if !in_fenced {
                    // Opening fence
                    fenced_count += 1;
                    in_fenced = true;
                } else {
                    // Closing fence
                    in_fenced = false;
                }
            } else if !in_fenced
                && self.is_indented_code_block_with_context(
                    &lines,
                    i,
                    is_mkdocs,
                    &in_list_context,
                    &in_tab_context,
                    &in_admonition_context,
                )
            {
                // Count each continuous indented block once
                if !prev_was_indented {
                    indented_count += 1;
                }
                prev_was_indented = true;
            } else {
                prev_was_indented = false;
            }
        }

        if fenced_count == 0 && indented_count == 0 {
            // No code blocks found
            None
        } else if fenced_count > 0 && indented_count == 0 {
            // Only fenced blocks found
            Some(CodeBlockStyle::Fenced)
        } else if fenced_count == 0 && indented_count > 0 {
            // Only indented blocks found
            Some(CodeBlockStyle::Indented)
        } else {
            // Both types found - use most prevalent
            // In case of tie, prefer fenced (more common, widely supported)
            if fenced_count >= indented_count {
                Some(CodeBlockStyle::Fenced)
            } else {
                Some(CodeBlockStyle::Indented)
            }
        }
    }
}

#[inline]
fn line_idx_from_offset(line_offsets: &[usize], offset: usize) -> usize {
    match line_offsets.binary_search(&offset) {
        Ok(idx) => idx,
        Err(idx) => idx.saturating_sub(1),
    }
}

impl Rule for MD046CodeBlockStyle {
    fn name(&self) -> &'static str {
        "MD046"
    }

    fn description(&self) -> &'static str {
        "Code blocks should use a consistent style"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early return for empty content
        if ctx.content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for code blocks before processing
        if !ctx.content.contains("```")
            && !ctx.content.contains("~~~")
            && !ctx.content.contains("    ")
            && !ctx.content.contains('\t')
        {
            return Ok(Vec::new());
        }

        // First, always check for unclosed code blocks
        let unclosed_warnings = self.check_unclosed_code_blocks(ctx)?;

        // If we found unclosed blocks, return those warnings first
        if !unclosed_warnings.is_empty() {
            return Ok(unclosed_warnings);
        }

        // Check for code block style consistency
        let lines: Vec<&str> = ctx.content.lines().collect();
        let mut warnings = Vec::new();

        // Check if we're in MkDocs mode
        let is_mkdocs = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;

        // Determine the target style from the detected style in the document
        let target_style = match self.config.style {
            CodeBlockStyle::Consistent => self
                .detect_style(ctx.content, is_mkdocs)
                .unwrap_or(CodeBlockStyle::Fenced),
            _ => self.config.style,
        };

        // Pre-compute tab and admonition contexts for MkDocs filtering
        let in_tab_context = if is_mkdocs {
            self.precompute_mkdocs_tab_context(&lines)
        } else {
            vec![false; lines.len()]
        };
        let in_admonition_context = if is_mkdocs {
            self.precompute_mkdocs_admonition_context(&lines)
        } else {
            vec![false; lines.len()]
        };

        // Parse code blocks using pulldown-cmark to get the actual block kind
        // (Fenced vs Indented) - this is crucial for correct detection
        let mut in_fenced_block = vec![false; lines.len()];
        let mut reported_indented_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();

        let options = Options::all();
        let parser = Parser::new_ext(ctx.content, options).into_offset_iter();

        for (event, range) in parser {
            let start = range.start;
            let end = range.end;

            if start >= ctx.content.len() || end > ctx.content.len() {
                continue;
            }

            // Find the line index for this block's start
            let start_line_idx = line_idx_from_offset(&ctx.line_offsets, start);

            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) => {
                    // Mark all lines in this fenced block
                    for (line_idx, line_info) in ctx.lines.iter().enumerate() {
                        if line_info.byte_offset >= start && line_info.byte_offset < end {
                            in_fenced_block[line_idx] = true;
                        }
                    }

                    // Flag fenced blocks when we want indented style
                    if target_style == CodeBlockStyle::Indented {
                        let line = lines.get(start_line_idx).unwrap_or(&"");

                        // Skip if inside HTML comment
                        if ctx.lines.get(start_line_idx).is_some_and(|info| info.in_html_comment) {
                            continue;
                        }

                        let (start_line, start_col, end_line, end_col) = calculate_line_range(start_line_idx + 1, line);
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "Use indented code blocks".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: ctx.line_index.line_col_to_byte_range(start_line_idx + 1, 1),
                                replacement: String::new(),
                            }),
                        });
                    }
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                    // This is an indented code block (per pulldown-cmark's CommonMark parsing)
                    // This includes 4-space indented fences which are invalid per CommonMark
                    // Flag when we want fenced style
                    if target_style == CodeBlockStyle::Fenced && !reported_indented_lines.contains(&start_line_idx) {
                        let line = lines.get(start_line_idx).unwrap_or(&"");

                        // Skip if inside HTML comment, mkdocstrings, or blockquote
                        // Indented content inside blockquotes is NOT an indented code block
                        if ctx.lines.get(start_line_idx).is_some_and(|info| {
                            info.in_html_comment || info.in_mkdocstrings || info.blockquote.is_some()
                        }) {
                            continue;
                        }

                        // Skip if inside a footnote definition
                        if mkdocs_footnotes::is_within_footnote_definition(ctx.content, start) {
                            continue;
                        }

                        // Skip if inside MkDocs tab content
                        if is_mkdocs && in_tab_context.get(start_line_idx).copied().unwrap_or(false) {
                            continue;
                        }

                        // Skip if inside MkDocs admonition content
                        if is_mkdocs && in_admonition_context.get(start_line_idx).copied().unwrap_or(false) {
                            continue;
                        }

                        reported_indented_lines.insert(start_line_idx);

                        let (start_line, start_col, end_line, end_col) = calculate_line_range(start_line_idx + 1, line);
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "Use fenced code blocks".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: ctx.line_index.line_col_to_byte_range(start_line_idx + 1, 1),
                                replacement: format!("```\n{}", line.trim_start()),
                            }),
                        });
                    }
                }
                _ => {}
            }
        }

        // Sort warnings by line number for consistent output
        warnings.sort_by_key(|w| (w.line, w.column));

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if content.is_empty() {
            return Ok(String::new());
        }

        // First check if we have nested fence issues that need special handling
        let unclosed_warnings = self.check_unclosed_code_blocks(ctx)?;

        // If we have nested fence warnings, apply those fixes first
        if !unclosed_warnings.is_empty() {
            // Check if any warnings are about nested fences (not just unclosed blocks)
            for warning in &unclosed_warnings {
                if warning
                    .message
                    .contains("should be closed before starting new one at line")
                {
                    // Apply the nested fence fix
                    if let Some(fix) = &warning.fix {
                        let mut result = String::new();
                        result.push_str(&content[..fix.range.start]);
                        result.push_str(&fix.replacement);
                        result.push_str(&content[fix.range.start..]);
                        return Ok(result);
                    }
                }
            }
        }

        let lines: Vec<&str> = content.lines().collect();

        // Determine target style
        let is_mkdocs = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;
        let target_style = match self.config.style {
            CodeBlockStyle::Consistent => self.detect_style(content, is_mkdocs).unwrap_or(CodeBlockStyle::Fenced),
            _ => self.config.style,
        };

        // Pre-compute list, tab, and admonition contexts for efficiency
        let in_list_context = self.precompute_block_continuation_context(&lines);
        let in_tab_context = if is_mkdocs {
            self.precompute_mkdocs_tab_context(&lines)
        } else {
            vec![false; lines.len()]
        };
        let in_admonition_context = if is_mkdocs {
            self.precompute_mkdocs_admonition_context(&lines)
        } else {
            vec![false; lines.len()]
        };

        // Categorize indented blocks:
        // - misplaced_fence_lines: complete fenced blocks that were over-indented (safe to dedent)
        // - unsafe_fence_lines: contain fence markers but aren't complete (skip fixing to avoid broken output)
        let (misplaced_fence_lines, unsafe_fence_lines) = self.categorize_indented_blocks(
            &lines,
            is_mkdocs,
            &in_list_context,
            &in_tab_context,
            &in_admonition_context,
        );

        let mut result = String::with_capacity(content.len());
        let mut in_fenced_block = false;
        let mut fenced_fence_type = None;
        let mut in_indented_block = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            // Handle fenced code blocks
            // Per CommonMark: fence must have 0-3 spaces of indentation
            if !in_fenced_block
                && Self::has_valid_fence_indent(line)
                && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
            {
                in_fenced_block = true;
                fenced_fence_type = Some(if trimmed.starts_with("```") { "```" } else { "~~~" });

                if target_style == CodeBlockStyle::Indented {
                    // Skip the opening fence
                    in_indented_block = true;
                } else {
                    // Keep the fenced block
                    result.push_str(line);
                    result.push('\n');
                }
            } else if in_fenced_block && fenced_fence_type.is_some() {
                let fence = fenced_fence_type.unwrap();
                if trimmed.starts_with(fence) {
                    in_fenced_block = false;
                    fenced_fence_type = None;
                    in_indented_block = false;

                    if target_style == CodeBlockStyle::Indented {
                        // Skip the closing fence
                    } else {
                        // Keep the fenced block
                        result.push_str(line);
                        result.push('\n');
                    }
                } else if target_style == CodeBlockStyle::Indented {
                    // Convert content inside fenced block to indented
                    // IMPORTANT: Preserve the original line content (including internal indentation)
                    // Don't use trimmed, as that would strip internal code indentation
                    result.push_str("    ");
                    result.push_str(line);
                    result.push('\n');
                } else {
                    // Keep fenced block content as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else if self.is_indented_code_block_with_context(
                &lines,
                i,
                is_mkdocs,
                &in_list_context,
                &in_tab_context,
                &in_admonition_context,
            ) {
                // This is an indented code block

                // Check if we need to start a new fenced block
                let prev_line_is_indented = i > 0
                    && self.is_indented_code_block_with_context(
                        &lines,
                        i - 1,
                        is_mkdocs,
                        &in_list_context,
                        &in_tab_context,
                        &in_admonition_context,
                    );

                if target_style == CodeBlockStyle::Fenced {
                    let trimmed_content = line.trim_start();

                    // Check if this line is part of a misplaced fenced block
                    // (pre-computed block-level analysis, not per-line)
                    if misplaced_fence_lines[i] {
                        // Just remove the indentation - this is a complete misplaced fenced block
                        result.push_str(trimmed_content);
                        result.push('\n');
                    } else if unsafe_fence_lines[i] {
                        // This block contains fence markers but isn't a complete fenced block
                        // Wrapping would create invalid nested fences - keep as-is (don't fix)
                        result.push_str(line);
                        result.push('\n');
                    } else if !prev_line_is_indented && !in_indented_block {
                        // Start of a new indented block that should be fenced
                        result.push_str("```\n");
                        result.push_str(trimmed_content);
                        result.push('\n');
                        in_indented_block = true;
                    } else {
                        // Inside an indented block
                        result.push_str(trimmed_content);
                        result.push('\n');
                    }

                    // Check if this is the end of the indented block
                    let next_line_is_indented = i < lines.len() - 1
                        && self.is_indented_code_block_with_context(
                            &lines,
                            i + 1,
                            is_mkdocs,
                            &in_list_context,
                            &in_tab_context,
                            &in_admonition_context,
                        );
                    // Don't close if this is an unsafe block (kept as-is)
                    if !next_line_is_indented
                        && in_indented_block
                        && !misplaced_fence_lines[i]
                        && !unsafe_fence_lines[i]
                    {
                        result.push_str("```\n");
                        in_indented_block = false;
                    }
                } else {
                    // Keep indented block as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else {
                // Regular line
                if in_indented_block && target_style == CodeBlockStyle::Fenced {
                    result.push_str("```\n");
                    in_indented_block = false;
                }

                result.push_str(line);
                result.push('\n');
            }
        }

        // Close any remaining blocks
        if in_indented_block && target_style == CodeBlockStyle::Fenced {
            result.push_str("```\n");
        }

        // Close any unclosed fenced blocks
        if let Some(fence_type) = fenced_fence_type
            && in_fenced_block
        {
            result.push_str(fence_type);
            result.push('\n');
        }

        // Remove trailing newline if original didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or unlikely to contain code blocks
        // Note: indented code blocks use 4 spaces, can't optimize that easily
        ctx.content.is_empty() || (!ctx.likely_has_code() && !ctx.has_char('~') && !ctx.content.contains("    "))
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD046Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_fenced_code_block_detection() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        assert!(rule.is_fenced_code_block_start("```"));
        assert!(rule.is_fenced_code_block_start("```rust"));
        assert!(rule.is_fenced_code_block_start("~~~"));
        assert!(rule.is_fenced_code_block_start("~~~python"));
        assert!(rule.is_fenced_code_block_start("  ```"));
        assert!(!rule.is_fenced_code_block_start("``"));
        assert!(!rule.is_fenced_code_block_start("~~"));
        assert!(!rule.is_fenced_code_block_start("Regular text"));
    }

    #[test]
    fn test_consistent_style_with_fenced_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "```\ncode\n```\n\nMore text\n\n```\nmore code\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // All blocks are fenced, so consistent style should be OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_consistent_style_with_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "Text\n\n    code\n    more code\n\nMore text\n\n    another block";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // All blocks are indented, so consistent style should be OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_consistent_style_mixed() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "```\nfenced code\n```\n\nText\n\n    indented code\n\nMore";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Mixed styles should be flagged
        assert!(!result.is_empty());
    }

    #[test]
    fn test_fenced_style_with_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "Text\n\n    indented code\n    more code\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Indented blocks should be flagged when fenced style is required
        assert!(!result.is_empty());
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_fenced_style_with_tab_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "Text\n\n\ttab indented code\n\tmore code\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Tab-indented blocks should also be flagged when fenced style is required
        assert!(!result.is_empty());
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_fenced_style_with_mixed_whitespace_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        // 2 spaces + tab = 4 columns due to tab expansion (tab goes to column 4)
        let content = "Text\n\n  \tmixed indent code\n  \tmore code\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Mixed whitespace indented blocks should also be flagged
        assert!(
            !result.is_empty(),
            "Mixed whitespace (2 spaces + tab) should be detected as indented code"
        );
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_fenced_style_with_one_space_tab_indent() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        // 1 space + tab = 4 columns (tab expands to next tab stop at column 4)
        let content = "Text\n\n \ttab after one space\n \tmore code\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "1 space + tab should be detected as indented code");
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_indented_style_with_fenced_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "Text\n\n```\nfenced code\n```\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Fenced blocks should be flagged when indented style is required
        assert!(!result.is_empty());
        assert!(result[0].message.contains("Use indented code blocks"));
    }

    #[test]
    fn test_unclosed_code_block() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode without closing fence";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("never closed"));
    }

    #[test]
    fn test_nested_code_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\nouter\n```\n\ninner text\n\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // This should parse as two separate code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_indented_to_fenced() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "Text\n\n    code line 1\n    code line 2\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("```\ncode line 1\ncode line 2\n```"));
    }

    #[test]
    fn test_fix_fenced_to_indented() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "Text\n\n```\ncode line 1\ncode line 2\n```\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("    code line 1\n    code line 2"));
        assert!(!fixed.contains("```"));
    }

    #[test]
    fn test_fix_fenced_to_indented_preserves_internal_indentation() {
        // Issue #270: When converting fenced code to indented, internal indentation must be preserved
        // HTML templates, Python, etc. rely on proper indentation
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = r#"# Test

```html
<!doctype html>
<html>
  <head>
    <title>Test</title>
  </head>
</html>
```
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The internal indentation (2 spaces for <head>, 4 for <title>) must be preserved
        // Each line gets 4 spaces prepended for the indented code block
        assert!(
            fixed.contains("      <head>"),
            "Expected 6 spaces before <head> (4 for code block + 2 original), got:\n{fixed}"
        );
        assert!(
            fixed.contains("        <title>"),
            "Expected 8 spaces before <title> (4 for code block + 4 original), got:\n{fixed}"
        );
        assert!(!fixed.contains("```"), "Fenced markers should be removed");
    }

    #[test]
    fn test_fix_fenced_to_indented_preserves_python_indentation() {
        // Issue #270: Python is indentation-sensitive - must preserve internal structure
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = r#"# Python Example

```python
def greet(name):
    if name:
        print(f"Hello, {name}!")
    else:
        print("Hello, World!")
```
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Python indentation must be preserved exactly
        assert!(
            fixed.contains("    def greet(name):"),
            "Function def should have 4 spaces (code block indent)"
        );
        assert!(
            fixed.contains("        if name:"),
            "if statement should have 8 spaces (4 code + 4 Python)"
        );
        assert!(
            fixed.contains("            print"),
            "print should have 12 spaces (4 code + 8 Python)"
        );
    }

    #[test]
    fn test_fix_fenced_to_indented_preserves_yaml_indentation() {
        // Issue #270: YAML is also indentation-sensitive
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = r#"# Config

```yaml
server:
  host: localhost
  port: 8080
  ssl:
    enabled: true
    cert: /path/to/cert
```
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("    server:"), "Root key should have 4 spaces");
        assert!(fixed.contains("      host:"), "First level should have 6 spaces");
        assert!(fixed.contains("      ssl:"), "ssl key should have 6 spaces");
        assert!(fixed.contains("        enabled:"), "Nested ssl should have 8 spaces");
    }

    #[test]
    fn test_fix_fenced_to_indented_preserves_empty_lines() {
        // Empty lines within code blocks should also get the 4-space prefix
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "```\nline1\n\nline2\n```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The fixed content should have proper structure
        assert!(fixed.contains("    line1"), "line1 should be indented");
        assert!(fixed.contains("    line2"), "line2 should be indented");
        // Empty line between them is preserved (may or may not have spaces)
    }

    #[test]
    fn test_fix_fenced_to_indented_multiple_blocks() {
        // Multiple fenced blocks should all preserve their indentation
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = r#"# Doc

```python
def foo():
    pass
```

Text between.

```yaml
key:
  value: 1
```
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("    def foo():"), "Python def should be indented");
        assert!(fixed.contains("        pass"), "Python body should have 8 spaces");
        assert!(fixed.contains("    key:"), "YAML root should have 4 spaces");
        assert!(fixed.contains("      value:"), "YAML nested should have 6 spaces");
        assert!(!fixed.contains("```"), "No fence markers should remain");
    }

    #[test]
    fn test_fix_unclosed_block() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode without closing";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should add closing fence
        assert!(fixed.ends_with("```"));
    }

    #[test]
    fn test_code_block_in_list() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "- List item\n    code in list\n    more code\n- Next item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Code in lists should not be flagged
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_detect_style_fenced() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "```\ncode\n```";
        let style = rule.detect_style(content, false);

        assert_eq!(style, Some(CodeBlockStyle::Fenced));
    }

    #[test]
    fn test_detect_style_indented() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "Text\n\n    code\n\nMore";
        let style = rule.detect_style(content, false);

        assert_eq!(style, Some(CodeBlockStyle::Indented));
    }

    #[test]
    fn test_detect_style_none() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "No code blocks here";
        let style = rule.detect_style(content, false);

        assert_eq!(style, None);
    }

    #[test]
    fn test_tilde_fence() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "~~~\ncode\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Tilde fences should be accepted as fenced blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_language_specification() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```rust\nfn main() {}\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_default_config() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let (name, _config) = rule.default_config_section().unwrap();
        assert_eq!(name, "MD046");
    }

    #[test]
    fn test_markdown_documentation_block() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```markdown\n# Example\n\n```\ncode\n```\n\nText\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Nested code blocks in markdown documentation should be allowed
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_preserve_trailing_newline() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode\n```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, content);
    }

    #[test]
    fn test_mkdocs_tabs_not_flagged_as_indented_code() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

=== "Python"

    This is tab content
    Not an indented code block

    ```python
    def hello():
        print("Hello")
    ```

=== "JavaScript"

    More tab content here
    Also not an indented code block"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag tab content as indented code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_mkdocs_tabs_with_actual_indented_code() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

=== "Tab 1"

    This is tab content

Regular text

    This is an actual indented code block
    Should be flagged"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag the actual indented code block but not the tab content
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_mkdocs_tabs_detect_style() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = r#"=== "Tab 1"

    Content in tab
    More content

=== "Tab 2"

    Content in second tab"#;

        // In MkDocs mode, tab content should not be detected as indented code blocks
        let style = rule.detect_style(content, true);
        assert_eq!(style, None); // No code blocks detected

        // In standard mode, it would detect indented code blocks
        let style = rule.detect_style(content, false);
        assert_eq!(style, Some(CodeBlockStyle::Indented));
    }

    #[test]
    fn test_mkdocs_nested_tabs() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

=== "Outer Tab"

    Some content

    === "Nested Tab"

        Nested tab content
        Should not be flagged"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Nested tabs should not be flagged
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_mkdocs_admonitions_not_flagged_as_indented_code() {
        // Issue #269: MkDocs admonitions have indented bodies that should NOT be
        // treated as indented code blocks when style = "fenced"
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

!!! note
    This is normal admonition content, not a code block.
    It spans multiple lines.

??? warning "Collapsible Warning"
    This is also admonition content.

???+ tip "Expanded Tip"
    And this one too.

Regular text outside admonitions."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Admonition content should not be flagged
        assert_eq!(
            result.len(),
            0,
            "Admonition content in MkDocs mode should not trigger MD046"
        );
    }

    #[test]
    fn test_mkdocs_admonition_with_actual_indented_code() {
        // After an admonition ends, regular indented code blocks SHOULD be flagged
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

!!! note
    This is admonition content.

Regular text ends the admonition.

    This is actual indented code (should be flagged)"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Should only flag the actual indented code block
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_admonition_in_standard_mode_flagged() {
        // In standard Markdown mode, admonitions are not recognized, so the
        // indented content should be flagged as indented code
        // Note: A blank line is required before indented code blocks per CommonMark
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

!!! note

    This looks like code in standard mode.

Regular text."#;

        // In Standard mode, admonitions are not recognized
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The indented content should be flagged in standard mode
        assert_eq!(
            result.len(),
            1,
            "Admonition content in Standard mode should be flagged as indented code"
        );
    }

    #[test]
    fn test_mkdocs_admonition_with_fenced_code_inside() {
        // Issue #269: Admonitions can contain fenced code blocks - must handle correctly
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

!!! note "Code Example"
    Here's some code:

    ```python
    def hello():
        print("world")
    ```

    More text after code.

Regular text."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag anything - the fenced block inside admonition is valid
        assert_eq!(result.len(), 0, "Fenced code blocks inside admonitions should be valid");
    }

    #[test]
    fn test_mkdocs_nested_admonitions() {
        // Nested admonitions are valid MkDocs syntax
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

!!! note "Outer"
    Outer content.

    !!! warning "Inner"
        Inner content.
        More inner content.

    Back to outer.

Regular text."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Nested admonitions should not trigger MD046
        assert_eq!(result.len(), 0, "Nested admonitions should not be flagged");
    }

    #[test]
    fn test_mkdocs_admonition_fix_does_not_wrap() {
        // The fix function should not wrap admonition content in fences
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"!!! note
    Content that should stay as admonition content.
    Not be wrapped in code fences.
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Fix should not add fence markers to admonition content
        assert!(
            !fixed.contains("```\n    Content"),
            "Admonition content should not be wrapped in fences"
        );
        assert_eq!(fixed, content, "Content should remain unchanged");
    }

    #[test]
    fn test_mkdocs_empty_admonition() {
        // Empty admonitions (marker only) should not cause issues
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"!!! note

Regular paragraph after empty admonition.

    This IS an indented code block (after blank + non-indented line)."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // The indented code block after the paragraph should be flagged
        assert_eq!(result.len(), 1, "Indented code after admonition ends should be flagged");
    }

    #[test]
    fn test_mkdocs_indented_admonition() {
        // Admonitions can themselves be indented (e.g., inside list items)
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"- List item

    !!! note
        Indented admonition content.
        More content.

- Next item"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Admonition inside list should not be flagged
        assert_eq!(
            result.len(),
            0,
            "Indented admonitions (e.g., in lists) should not be flagged"
        );
    }

    #[test]
    fn test_footnote_indented_paragraphs_not_flagged() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Test Document with Footnotes

This is some text with a footnote[^1].

Here's some code:

```bash
echo "fenced code block"
```

More text with another footnote[^2].

[^1]: Really interesting footnote text.

    Even more interesting second paragraph.

[^2]: Another footnote.

    With a second paragraph too.

    And even a third paragraph!"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Indented paragraphs in footnotes should not be flagged as code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_footnote_definition_detection() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        // Valid footnote definitions (per CommonMark footnote extension spec)
        // Reference: https://github.com/jgm/commonmark-hs/blob/master/commonmark-extensions/test/footnotes.md
        assert!(rule.is_footnote_definition("[^1]: Footnote text"));
        assert!(rule.is_footnote_definition("[^foo]: Footnote text"));
        assert!(rule.is_footnote_definition("[^long-name]: Footnote text"));
        assert!(rule.is_footnote_definition("[^test_123]: Mixed chars"));
        assert!(rule.is_footnote_definition("    [^1]: Indented footnote"));
        assert!(rule.is_footnote_definition("[^a]: Minimal valid footnote"));
        assert!(rule.is_footnote_definition("[^123]: Numeric label"));
        assert!(rule.is_footnote_definition("[^_]: Single underscore"));
        assert!(rule.is_footnote_definition("[^-]: Single hyphen"));

        // Invalid: empty or whitespace-only labels (spec violation)
        assert!(!rule.is_footnote_definition("[^]: No label"));
        assert!(!rule.is_footnote_definition("[^ ]: Whitespace only"));
        assert!(!rule.is_footnote_definition("[^  ]: Multiple spaces"));
        assert!(!rule.is_footnote_definition("[^\t]: Tab only"));

        // Invalid: malformed syntax
        assert!(!rule.is_footnote_definition("[^]]: Extra bracket"));
        assert!(!rule.is_footnote_definition("Regular text [^1]:"));
        assert!(!rule.is_footnote_definition("[1]: Not a footnote"));
        assert!(!rule.is_footnote_definition("[^")); // Too short
        assert!(!rule.is_footnote_definition("[^1:")); // Missing closing bracket
        assert!(!rule.is_footnote_definition("^1]: Missing opening bracket"));

        // Invalid: disallowed characters in label
        assert!(!rule.is_footnote_definition("[^test.name]: Period"));
        assert!(!rule.is_footnote_definition("[^test name]: Space in label"));
        assert!(!rule.is_footnote_definition("[^test@name]: Special char"));
        assert!(!rule.is_footnote_definition("[^test/name]: Slash"));
        assert!(!rule.is_footnote_definition("[^test\\name]: Backslash"));

        // Edge case: line breaks not allowed in labels
        // (This is a string test, actual multiline would need different testing)
        assert!(!rule.is_footnote_definition("[^test\r]: Carriage return"));
    }

    #[test]
    fn test_footnote_with_blank_lines() {
        // Spec requirement: blank lines within footnotes don't terminate them
        // if next content is indented (matches GitHub's implementation)
        // Reference: commonmark-hs footnote extension behavior
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

Text with footnote[^1].

[^1]: First paragraph.

    Second paragraph after blank line.

    Third paragraph after another blank line.

Regular text at column 0 ends the footnote."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The indented paragraphs in the footnote should not be flagged as code blocks
        assert_eq!(
            result.len(),
            0,
            "Indented content within footnotes should not trigger MD046"
        );
    }

    #[test]
    fn test_footnote_multiple_consecutive_blank_lines() {
        // Edge case: multiple consecutive blank lines within a footnote
        // Should still work if next content is indented
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"Text[^1].

[^1]: First paragraph.



    Content after three blank lines (still part of footnote).

Not indented, so footnote ends here."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The indented content should not be flagged
        assert_eq!(
            result.len(),
            0,
            "Multiple blank lines shouldn't break footnote continuation"
        );
    }

    #[test]
    fn test_footnote_terminated_by_non_indented_content() {
        // Spec requirement: non-indented content always terminates the footnote
        // Reference: commonmark-hs footnote extension
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"[^1]: Footnote content.

    More indented content in footnote.

This paragraph is not indented, so footnote ends.

    This should be flagged as indented code block."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The last indented block should be flagged (it's after the footnote ended)
        assert_eq!(
            result.len(),
            1,
            "Indented code after footnote termination should be flagged"
        );
        assert!(
            result[0].message.contains("Use fenced code blocks"),
            "Expected MD046 warning for indented code block"
        );
        assert!(result[0].line >= 7, "Warning should be on the indented code block line");
    }

    #[test]
    fn test_footnote_terminated_by_structural_elements() {
        // Spec requirement: headings and horizontal rules terminate footnotes
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"[^1]: Footnote content.

    More content.

## Heading terminates footnote

    This indented content should be flagged.

---

    This should also be flagged (after horizontal rule)."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Both indented blocks after structural elements should be flagged
        assert_eq!(
            result.len(),
            2,
            "Both indented blocks after termination should be flagged"
        );
    }

    #[test]
    fn test_footnote_with_code_block_inside() {
        // Spec behavior: footnotes can contain fenced code blocks
        // The fenced code must be properly indented within the footnote
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"Text[^1].

[^1]: Footnote with code:

    ```python
    def hello():
        print("world")
    ```

    More footnote text after code."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should have no warnings - the fenced code block is valid
        assert_eq!(result.len(), 0, "Fenced code blocks within footnotes should be allowed");
    }

    #[test]
    fn test_footnote_with_8_space_indented_code() {
        // Edge case: code blocks within footnotes need 8 spaces (4 for footnote + 4 for code)
        // This should NOT be flagged as it's properly nested indented code
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"Text[^1].

[^1]: Footnote with nested code.

        code block
        more code"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The 8-space indented code is valid within footnote
        assert_eq!(
            result.len(),
            0,
            "8-space indented code within footnotes represents nested code blocks"
        );
    }

    #[test]
    fn test_multiple_footnotes() {
        // Spec behavior: each footnote definition starts a new block context
        // Previous footnote ends when new footnote begins
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"Text[^1] and more[^2].

[^1]: First footnote.

    Continuation of first.

[^2]: Second footnote starts here, ending the first.

    Continuation of second."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // All indented content is part of footnotes
        assert_eq!(
            result.len(),
            0,
            "Multiple footnotes should each maintain their continuation context"
        );
    }

    #[test]
    fn test_list_item_ends_footnote_context() {
        // Spec behavior: list items and footnotes are mutually exclusive contexts
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"[^1]: Footnote.

    Content in footnote.

- List item starts here (ends footnote context).

    This indented content is part of the list, not the footnote."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // List continuation should not be flagged
        assert_eq!(
            result.len(),
            0,
            "List items should end footnote context and start their own"
        );
    }

    #[test]
    fn test_footnote_vs_actual_indented_code() {
        // Critical test: verify we can still detect actual indented code blocks outside footnotes
        // This ensures the fix doesn't cause false negatives
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Heading

Text with footnote[^1].

[^1]: Footnote content.

    Part of footnote (should not be flagged).

Regular paragraph ends footnote context.

    This is actual indented code (MUST be flagged)
    Should be detected as code block"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag the indented code after the regular paragraph
        assert_eq!(
            result.len(),
            1,
            "Must still detect indented code blocks outside footnotes"
        );
        assert!(
            result[0].message.contains("Use fenced code blocks"),
            "Expected MD046 warning for indented code"
        );
        assert!(
            result[0].line >= 11,
            "Warning should be on the actual indented code line"
        );
    }

    #[test]
    fn test_spec_compliant_label_characters() {
        // Spec requirement: labels must contain only alphanumerics, hyphens, underscores
        // Reference: commonmark-hs footnote extension
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        // Valid according to spec
        assert!(rule.is_footnote_definition("[^test]: text"));
        assert!(rule.is_footnote_definition("[^TEST]: text"));
        assert!(rule.is_footnote_definition("[^test-name]: text"));
        assert!(rule.is_footnote_definition("[^test_name]: text"));
        assert!(rule.is_footnote_definition("[^test123]: text"));
        assert!(rule.is_footnote_definition("[^123]: text"));
        assert!(rule.is_footnote_definition("[^a1b2c3]: text"));

        // Invalid characters (spec violations)
        assert!(!rule.is_footnote_definition("[^test.name]: text")); // Period
        assert!(!rule.is_footnote_definition("[^test name]: text")); // Space
        assert!(!rule.is_footnote_definition("[^test@name]: text")); // At sign
        assert!(!rule.is_footnote_definition("[^test#name]: text")); // Hash
        assert!(!rule.is_footnote_definition("[^test$name]: text")); // Dollar
        assert!(!rule.is_footnote_definition("[^test%name]: text")); // Percent
    }

    #[test]
    fn test_code_block_inside_html_comment() {
        // Regression test: code blocks inside HTML comments should not be flagged
        // Found in denoland/deno test fixture during sanity testing
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

Some text.

<!--
Example code block in comment:

```typescript
console.log("Hello");
```

More comment text.
-->

More content."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Code blocks inside HTML comments should not be flagged as unclosed"
        );
    }

    #[test]
    fn test_unclosed_fence_inside_html_comment() {
        // Even an unclosed fence inside an HTML comment should be ignored
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

<!--
Example with intentionally unclosed fence:

```
code without closing
-->

More content."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Unclosed fences inside HTML comments should be ignored"
        );
    }

    #[test]
    fn test_multiline_html_comment_with_indented_code() {
        // Indented code inside HTML comments should also be ignored
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

<!--
Example:

    indented code
    more code

End of comment.
-->

Regular text."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Indented code inside HTML comments should not be flagged"
        );
    }

    #[test]
    fn test_code_block_after_html_comment() {
        // Code blocks after HTML comments should still be detected
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

<!-- comment -->

Text before.

    indented code should be flagged

More text."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            1,
            "Code blocks after HTML comments should still be detected"
        );
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_four_space_indented_fence_is_not_valid_fence() {
        // Per CommonMark 0.31.2: "An opening code fence may be indented 0-3 spaces."
        // 4+ spaces means it's NOT a valid fence opener - it becomes an indented code block
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        // Valid fences (0-3 spaces)
        assert!(rule.is_fenced_code_block_start("```"));
        assert!(rule.is_fenced_code_block_start(" ```"));
        assert!(rule.is_fenced_code_block_start("  ```"));
        assert!(rule.is_fenced_code_block_start("   ```"));

        // Invalid fences (4+ spaces) - these are indented code blocks instead
        assert!(!rule.is_fenced_code_block_start("    ```"));
        assert!(!rule.is_fenced_code_block_start("     ```"));
        assert!(!rule.is_fenced_code_block_start("        ```"));

        // Tab counts as 4 spaces per CommonMark
        assert!(!rule.is_fenced_code_block_start("\t```"));
    }

    #[test]
    fn test_issue_237_indented_fenced_block_detected_as_indented() {
        // Issue #237: User has fenced code block indented by 4 spaces
        // Per CommonMark, this should be detected as an INDENTED code block
        // because 4+ spaces of indentation makes the fence invalid
        //
        // Reference: https://github.com/rvben/rumdl/issues/237
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        // This is the exact test case from issue #237
        let content = r#"## Test

    ```js
    var foo = "hello";
    ```
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag this as an indented code block that should use fenced style
        assert_eq!(
            result.len(),
            1,
            "4-space indented fence should be detected as indented code block"
        );
        assert!(
            result[0].message.contains("Use fenced code blocks"),
            "Expected 'Use fenced code blocks' message"
        );
    }

    #[test]
    fn test_issue_276_indented_code_in_list() {
        // Issue #276: Indented code blocks inside lists should be detected
        // Reference: https://github.com/rvben/rumdl/issues/276
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"1. First item
2. Second item with code:

        # This is a code block in a list
        print("Hello, world!")

4. Third item"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag the indented code block inside the list
        assert!(
            !result.is_empty(),
            "Indented code block inside list should be flagged when style=fenced"
        );
        assert!(
            result[0].message.contains("Use fenced code blocks"),
            "Expected 'Use fenced code blocks' message"
        );
    }

    #[test]
    fn test_three_space_indented_fence_is_valid() {
        // 3 spaces is the maximum allowed per CommonMark - should be recognized as fenced
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"## Test

   ```js
   var foo = "hello";
   ```
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // 3-space indent is valid for fenced blocks - should pass
        assert_eq!(
            result.len(),
            0,
            "3-space indented fence should be recognized as valid fenced code block"
        );
    }

    #[test]
    fn test_indented_style_with_deeply_indented_fenced() {
        // When style=indented, a 4-space indented "fenced" block should still be detected
        // as an indented code block (which is what we want!)
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);

        let content = r#"Text

    ```js
    var foo = "hello";
    ```

More text
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // When target style is "indented", 4-space indented content is correct
        // The fence markers become literal content in the indented code block
        assert_eq!(
            result.len(),
            0,
            "4-space indented content should be valid when style=indented"
        );
    }

    #[test]
    fn test_fix_misplaced_fenced_block() {
        // Issue #237: When a fenced code block is accidentally indented 4+ spaces,
        // the fix should just remove the indentation, not wrap in more fences
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"## Test

    ```js
    var foo = "hello";
    ```
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The fix should just remove the 4-space indentation
        let expected = r#"## Test

```js
var foo = "hello";
```
"#;

        assert_eq!(fixed, expected, "Fix should remove indentation, not add more fences");
    }

    #[test]
    fn test_fix_regular_indented_block() {
        // Regular indented code blocks (without fence markers) should still be
        // wrapped in fences when converted
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"Text

    var foo = "hello";
    console.log(foo);

More text
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should wrap in fences
        assert!(fixed.contains("```\nvar foo"), "Should add opening fence");
        assert!(fixed.contains("console.log(foo);\n```"), "Should add closing fence");
    }

    #[test]
    fn test_fix_indented_block_with_fence_like_content() {
        // If an indented block contains fence-like content but doesn't form a
        // complete fenced block, we should NOT autofix it because wrapping would
        // create invalid nested fences. The block is left unchanged.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"Text

    some code
    ```not a fence opener
    more code
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Block should be left unchanged to avoid creating invalid nested fences
        assert!(fixed.contains("    some code"), "Unsafe block should be left unchanged");
        assert!(!fixed.contains("```\nsome code"), "Should NOT wrap unsafe block");
    }

    #[test]
    fn test_fix_mixed_indented_and_misplaced_blocks() {
        // Mixed blocks: regular indented code followed by misplaced fenced block
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"Text

    regular indented code

More text

    ```python
    print("hello")
    ```
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // First block should be wrapped
        assert!(
            fixed.contains("```\nregular indented code\n```"),
            "First block should be wrapped in fences"
        );

        // Second block should be dedented (not wrapped)
        assert!(
            fixed.contains("\n```python\nprint(\"hello\")\n```"),
            "Second block should be dedented, not double-wrapped"
        );
        // Should NOT have nested fences
        assert!(
            !fixed.contains("```\n```python"),
            "Should not have nested fence openers"
        );
    }
}
