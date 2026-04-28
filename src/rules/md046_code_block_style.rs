use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::calculate_indentation_width_default;
use crate::utils::mkdocs_admonitions;
use crate::utils::mkdocs_tabs;
use crate::utils::range_utils::calculate_line_range;
use toml;

mod md046_config;
pub use md046_config::CodeBlockStyle;
use md046_config::MD046Config;

/// Pre-computed context arrays for indented code block detection.
struct IndentContext<'a> {
    in_list_context: &'a [bool],
    in_tab_context: &'a [bool],
    in_admonition_context: &'a [bool],
    /// Lines belonging to a non-code container whose body can legitimately be
    /// indented by 4+ spaces or contain verbatim fence markers: HTML/MDX
    /// comments, raw HTML blocks, JSX blocks, mkdocstrings blocks, footnote
    /// definitions, and blockquotes.
    ///
    /// These lines are excluded from `detect_style`'s style tally, from
    /// `is_indented_code_block_with_context`, and from
    /// `categorize_indented_blocks`'s fence rewriting — keeping detection in
    /// lockstep with the warning-side skip list used in `check`.
    in_comment_or_html: &'a [bool],
    /// Per-line content column of the most recent list item this line
    /// belongs to (in list continuation), or None if not in list context.
    ///
    /// CommonMark places an indented code block within a list item only when
    /// the line's indent is at least `baseline + 4`. Without this, every
    /// continuation line gets the conservative "skip in list context" treatment
    /// — silently turning real list-internal code blocks into fmt no-ops.
    /// With this, the rule recognizes them, and the fence converter can emit
    /// fences at `baseline` spaces so the block stays attached to the bullet.
    list_item_baseline: &'a [Option<usize>],
}

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
        calculate_indentation_width_default(line) < 4
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

    /// Per-line content column of the most recent list item this line
    /// belongs to (in list continuation), or None.
    ///
    /// Mirrors the iteration in `precompute_block_continuation_context` but
    /// captures the parsed list item's `content_column` from `LineInfo`.
    /// `is_indented_code_block_with_context` consults this so list-internal
    /// indented blocks are recognized iff their indent crosses
    /// `baseline + 4` — the CommonMark threshold for an indented code block
    /// inside a list item. The fix loop reuses the baseline to anchor the
    /// generated fences at the list-item content column.
    fn precompute_list_item_baseline(
        &self,
        ctx: &crate::lint_context::LintContext,
        lines: &[&str],
    ) -> Vec<Option<usize>> {
        let mut baselines = vec![None; lines.len()];
        let mut last_baseline: Option<usize> = None;
        let mut last_list_item_line: Option<usize> = None;
        let mut blank_line_count = 0usize;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();

            // List item line — read the parsed content column directly.
            if let Some(item) = ctx.line_info(i + 1).and_then(|li| li.list_item.as_ref()) {
                last_baseline = Some(item.content_column);
                last_list_item_line = Some(i);
                blank_line_count = 0;
                baselines[i] = last_baseline;
                continue;
            }

            // Blank line within continuation — propagate baseline.
            if line.trim().is_empty() {
                if last_baseline.is_some() {
                    blank_line_count += 1;
                    baselines[i] = last_baseline;
                }
                continue;
            }

            // Non-empty unindented content. Headings/HRs always end the list;
            // otherwise mirror the >5-line / >1-blank heuristic from
            // `precompute_block_continuation_context`.
            if indent_len == 0 {
                if trimmed.starts_with('#') || trimmed.starts_with("---") || trimmed.starts_with("***") {
                    last_baseline = None;
                    last_list_item_line = None;
                } else if let Some(list_line) = last_list_item_line
                    && (i - list_line > 5 || blank_line_count > 1)
                {
                    last_baseline = None;
                    last_list_item_line = None;
                }
                blank_line_count = 0;
                continue;
            }

            // Indented continuation — keep the baseline.
            if last_baseline.is_some() {
                baselines[i] = last_baseline;
                blank_line_count = 0;
            }
        }

        baselines
    }

    /// Check if a line is an indented code block using pre-computed context arrays
    fn is_indented_code_block_with_context(
        &self,
        lines: &[&str],
        i: usize,
        is_mkdocs: bool,
        ctx: &IndentContext,
    ) -> bool {
        if i >= lines.len() {
            return false;
        }

        let line = lines[i];

        // Check if indented by at least 4 columns (accounting for tab expansion)
        let indent = calculate_indentation_width_default(line);
        if indent < 4 {
            return false;
        }

        // List/footnote continuation: only treat as a code block when the
        // indent crosses the list-item content baseline + 4. Without a
        // baseline (e.g. footnote definition continuation), keep the
        // conservative skip — those containers don't expose a column we can
        // anchor a fence to.
        if ctx.in_list_context[i] {
            let crosses_baseline = ctx
                .list_item_baseline
                .get(i)
                .copied()
                .flatten()
                .is_some_and(|base| indent >= base + 4);
            if !crosses_baseline {
                return false;
            }
        }

        // Skip if this is MkDocs tab content (pre-computed)
        if is_mkdocs && ctx.in_tab_context[i] {
            return false;
        }

        // Skip if this is MkDocs admonition content (pre-computed)
        // Admonitions are supported in MkDocs and other extended Markdown processors
        if is_mkdocs && ctx.in_admonition_context[i] {
            return false;
        }

        // Skip if inside an HTML/MDX comment, raw HTML block, JSX block,
        // mkdocstrings block, footnote definition, or blockquote. These
        // containers can legitimately hold 4+ space indented text that is
        // not a code block. Counting them would desync style detection from
        // the warning-side skip list in `check`.
        if ctx.in_comment_or_html.get(i).copied().unwrap_or(false) {
            return false;
        }

        // Check if preceded by a blank line (typical for code blocks)
        // OR if the previous line is also an indented code block (continuation).
        // Mirror the list-baseline check on the previous line so a list-internal
        // code block that spans multiple lines is recognized as continuous.
        let has_blank_line_before = i == 0 || lines[i - 1].trim().is_empty();
        let prev_is_indented_code = i > 0
            && {
                let prev_indent = calculate_indentation_width_default(lines[i - 1]);
                if prev_indent < 4 {
                    false
                } else if ctx.in_list_context[i - 1] {
                    ctx.list_item_baseline
                        .get(i - 1)
                        .copied()
                        .flatten()
                        .is_some_and(|base| prev_indent >= base + 4)
                } else {
                    true
                }
            }
            && !(is_mkdocs && ctx.in_tab_context[i - 1])
            && !(is_mkdocs && ctx.in_admonition_context[i - 1])
            && !ctx.in_comment_or_html.get(i - 1).copied().unwrap_or(false);

        // If no blank line before and previous line is not indented code,
        // it's likely list continuation, not a code block
        if !has_blank_line_before && !prev_is_indented_code {
            return false;
        }

        true
    }

    /// Pre-compute which lines sit inside a non-code container whose body may
    /// legitimately be indented by 4+ spaces without being an indented code
    /// block: HTML comments, raw HTML blocks, JSX blocks, MDX comments,
    /// mkdocstrings blocks, footnote definitions, and blockquotes.
    ///
    /// This mirrors the skip list used in `check` when emitting indented
    /// code-block warnings, keeping style detection and warning emission in
    /// lockstep.
    fn precompute_comment_or_html_context(ctx: &crate::lint_context::LintContext, line_count: usize) -> Vec<bool> {
        (0..line_count)
            .map(|i| {
                ctx.line_info(i + 1).is_some_and(|info| {
                    info.in_html_comment
                        || info.in_mdx_comment
                        || info.in_html_block
                        || info.in_jsx_block
                        || info.in_mkdocstrings
                        || info.in_footnote_definition
                        || info.blockquote.is_some()
                })
            })
            .collect()
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
                } else if !line.trim().is_empty() && calculate_indentation_width_default(line) < 4 {
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
            let line_indent = calculate_indentation_width_default(line);

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
        ictx: &IndentContext<'_>,
    ) -> (Vec<bool>, Vec<bool>) {
        let mut is_misplaced = vec![false; lines.len()];
        let mut contains_fences = vec![false; lines.len()];

        // Find contiguous indented blocks and categorize them
        let mut i = 0;
        while i < lines.len() {
            // Find the start of an indented block
            if !self.is_indented_code_block_with_context(lines, i, is_mkdocs, ictx) {
                i += 1;
                continue;
            }

            // Found start of an indented block - collect all contiguous lines
            let block_start = i;
            let mut block_end = i;

            while block_end < lines.len() && self.is_indented_code_block_with_context(lines, block_end, is_mkdocs, ictx)
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

    fn check_unclosed_code_blocks(&self, ctx: &crate::lint_context::LintContext) -> Vec<LintWarning> {
        let mut warnings = Vec::new();
        let lines = ctx.raw_lines();

        // Check if any fenced block has a markdown/md language tag
        let has_markdown_doc_block = ctx.code_block_details.iter().any(|d| {
            if !d.is_fenced {
                return false;
            }
            let lang = d.info_string.to_lowercase();
            lang.starts_with("markdown") || lang.starts_with("md")
        });

        // Skip unclosed block detection if document contains markdown documentation blocks
        // (they have nested fence examples that pulldown-cmark misparses)
        if has_markdown_doc_block {
            return warnings;
        }

        for detail in &ctx.code_block_details {
            if !detail.is_fenced {
                continue;
            }

            // Only check blocks that extend to EOF
            if detail.end != ctx.content.len() {
                continue;
            }

            // Find the line index for this block's start
            let opening_line_idx = match ctx.line_offsets.binary_search(&detail.start) {
                Ok(idx) => idx,
                Err(idx) => idx.saturating_sub(1),
            };

            // Determine fence marker from the actual line content
            let line = lines.get(opening_line_idx).unwrap_or(&"");
            let trimmed = line.trim();
            let fence_marker = if let Some(pos) = trimmed.find("```") {
                let count = trimmed[pos..].chars().take_while(|&c| c == '`').count();
                "`".repeat(count)
            } else if let Some(pos) = trimmed.find("~~~") {
                let count = trimmed[pos..].chars().take_while(|&c| c == '~').count();
                "~".repeat(count)
            } else {
                "```".to_string()
            };

            // Check if the last non-empty line is a valid closing fence
            let last_non_empty_line = lines.iter().rev().find(|l| !l.trim().is_empty()).unwrap_or(&"");
            let last_trimmed = last_non_empty_line.trim();
            let fence_char = fence_marker.chars().next().unwrap_or('`');

            let has_closing_fence = if fence_char == '`' {
                last_trimmed.starts_with("```") && {
                    let fence_len = last_trimmed.chars().take_while(|&c| c == '`').count();
                    last_trimmed[fence_len..].trim().is_empty()
                }
            } else {
                last_trimmed.starts_with("~~~") && {
                    let fence_len = last_trimmed.chars().take_while(|&c| c == '~').count();
                    last_trimmed[fence_len..].trim().is_empty()
                }
            };

            if !has_closing_fence {
                // Skip if inside HTML comment
                if ctx
                    .lines
                    .get(opening_line_idx)
                    .is_some_and(|info| info.in_html_comment || info.in_mdx_comment)
                {
                    continue;
                }

                let (start_line, start_col, end_line, end_col) = calculate_line_range(opening_line_idx + 1, line);

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
        }

        warnings
    }

    fn detect_style(&self, lines: &[&str], is_mkdocs: bool, ictx: &IndentContext) -> Option<CodeBlockStyle> {
        if lines.is_empty() {
            return None;
        }

        let mut fenced_count = 0;
        let mut indented_count = 0;

        // Count all code block occurrences (prevalence-based approach).
        //
        // Both counts must ignore fence markers and indented text that live
        // inside a non-code container (HTML/MDX comments, raw HTML/JSX
        // blocks, mkdocstrings, footnote definitions, blockquotes) so that
        // the detected style stays in lockstep with the warning-side skip
        // list in `check`. Without this, a document that contains a single
        // real code block plus a fake fence or indented paragraph nested in
        // a comment is wrongly classified and the real block gets flagged.
        let mut in_fenced = false;
        let mut prev_was_indented = false;

        for (i, line) in lines.iter().enumerate() {
            let in_container = ictx.in_comment_or_html.get(i).copied().unwrap_or(false);

            if self.is_fenced_code_block_start(line) {
                if in_container {
                    // Fence marker inside a container — not a real fence,
                    // don't flip state or count it.
                    prev_was_indented = false;
                    continue;
                }
                if !in_fenced {
                    // Opening fence
                    fenced_count += 1;
                    in_fenced = true;
                } else {
                    // Closing fence
                    in_fenced = false;
                }
                prev_was_indented = false;
            } else if !in_fenced && self.is_indented_code_block_with_context(lines, i, is_mkdocs, ictx) {
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
            None
        } else if fenced_count > 0 && indented_count == 0 {
            Some(CodeBlockStyle::Fenced)
        } else if fenced_count == 0 && indented_count > 0 {
            Some(CodeBlockStyle::Indented)
        } else if fenced_count >= indented_count {
            Some(CodeBlockStyle::Fenced)
        } else {
            Some(CodeBlockStyle::Indented)
        }
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
        let unclosed_warnings = self.check_unclosed_code_blocks(ctx);

        // If we found unclosed blocks, return those warnings first
        if !unclosed_warnings.is_empty() {
            return Ok(unclosed_warnings);
        }

        // Check for code block style consistency
        let lines = ctx.raw_lines();
        let mut warnings = Vec::new();

        let is_mkdocs = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;

        // Determine the target style
        let target_style = match self.config.style {
            CodeBlockStyle::Consistent => {
                let in_list_context = self.precompute_block_continuation_context(lines);
                let list_item_baseline = self.precompute_list_item_baseline(ctx, lines);
                let in_comment_or_html = Self::precompute_comment_or_html_context(ctx, lines.len());
                let in_tab_context = if is_mkdocs {
                    self.precompute_mkdocs_tab_context(lines)
                } else {
                    vec![false; lines.len()]
                };
                let in_admonition_context = if is_mkdocs {
                    self.precompute_mkdocs_admonition_context(lines)
                } else {
                    vec![false; lines.len()]
                };
                let ictx = IndentContext {
                    in_list_context: &in_list_context,
                    in_tab_context: &in_tab_context,
                    in_admonition_context: &in_admonition_context,
                    in_comment_or_html: &in_comment_or_html,
                    list_item_baseline: &list_item_baseline,
                };
                self.detect_style(lines, is_mkdocs, &ictx)
                    .unwrap_or(CodeBlockStyle::Fenced)
            }
            _ => self.config.style,
        };

        // Iterate code_block_details directly (O(k) where k is number of blocks)
        let mut reported_indented_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();

        for detail in &ctx.code_block_details {
            if detail.start >= ctx.content.len() || detail.end > ctx.content.len() {
                continue;
            }

            let start_line_idx = match ctx.line_offsets.binary_search(&detail.start) {
                Ok(idx) => idx,
                Err(idx) => idx.saturating_sub(1),
            };

            if detail.is_fenced {
                if target_style == CodeBlockStyle::Indented {
                    let line = lines.get(start_line_idx).unwrap_or(&"");

                    if ctx
                        .lines
                        .get(start_line_idx)
                        .is_some_and(|info| info.in_html_comment || info.in_mdx_comment || info.in_footnote_definition)
                    {
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
                        fix: None,
                    });
                }
            } else {
                // Indented code block
                if target_style == CodeBlockStyle::Fenced && !reported_indented_lines.contains(&start_line_idx) {
                    let line = lines.get(start_line_idx).unwrap_or(&"");

                    // Skip blocks in contexts that aren't real indented code blocks
                    if ctx.lines.get(start_line_idx).is_some_and(|info| {
                        info.in_html_comment
                            || info.in_mdx_comment
                            || info.in_html_block
                            || info.in_jsx_block
                            || info.in_mkdocstrings
                            || info.in_footnote_definition
                            || info.blockquote.is_some()
                    }) {
                        continue;
                    }

                    // Use pre-computed LineInfo for MkDocs container context
                    if is_mkdocs
                        && ctx
                            .lines
                            .get(start_line_idx)
                            .is_some_and(|info| info.in_admonition || info.in_content_tab)
                    {
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
                        fix: None,
                    });
                }
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

        let lines = ctx.raw_lines();

        // Determine target style
        let is_mkdocs = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;

        let in_comment_or_html = Self::precompute_comment_or_html_context(ctx, lines.len());

        // Pre-compute list, tab, and admonition contexts once
        let in_list_context = self.precompute_block_continuation_context(lines);
        let list_item_baseline = self.precompute_list_item_baseline(ctx, lines);
        let in_tab_context = if is_mkdocs {
            self.precompute_mkdocs_tab_context(lines)
        } else {
            vec![false; lines.len()]
        };
        let in_admonition_context = if is_mkdocs {
            self.precompute_mkdocs_admonition_context(lines)
        } else {
            vec![false; lines.len()]
        };

        let ictx = IndentContext {
            in_list_context: &in_list_context,
            in_tab_context: &in_tab_context,
            in_admonition_context: &in_admonition_context,
            in_comment_or_html: &in_comment_or_html,
            list_item_baseline: &list_item_baseline,
        };

        let target_style = match self.config.style {
            CodeBlockStyle::Consistent => self
                .detect_style(lines, is_mkdocs, &ictx)
                .unwrap_or(CodeBlockStyle::Fenced),
            _ => self.config.style,
        };

        // Categorize indented blocks:
        // - misplaced_fence_lines: complete fenced blocks that were over-indented (safe to dedent)
        // - unsafe_fence_lines: contain fence markers but aren't complete (skip fixing to avoid broken output)
        let (misplaced_fence_lines, unsafe_fence_lines) = self.categorize_indented_blocks(lines, is_mkdocs, &ictx);

        let mut result = String::with_capacity(content.len());
        let mut in_fenced_block = false;
        // Tracks the opening fence: (fence_char, opener_length).
        // Per CommonMark spec, the closing fence must use the same character and have
        // at least as many characters as the opener, with no info string.
        let mut fenced_fence_opener: Option<(char, usize)> = None;
        let mut in_indented_block = false;
        // Indent string emitted on the opening fence of the current
        // indented→fenced conversion (e.g. "  " for an indented block inside
        // a `- ` list item, "" at top level). Reused on close so the closing
        // fence sits at the same column as the opener.
        let mut current_block_fence_indent = String::new();

        // Track which code block opening lines are disabled by inline config
        let mut current_block_disabled = false;

        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            let trimmed = line.trim_start();

            // Handle fenced code blocks
            // Per CommonMark: fence must have 0-3 spaces of indentation
            if !in_fenced_block
                && Self::has_valid_fence_indent(line)
                && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
            {
                // Check if inline config disables this rule for the opening fence
                current_block_disabled = ctx.inline_config().is_rule_disabled(self.name(), line_num);
                in_fenced_block = true;
                let fence_char = if trimmed.starts_with("```") { '`' } else { '~' };
                let opener_len = trimmed.chars().take_while(|&c| c == fence_char).count();
                fenced_fence_opener = Some((fence_char, opener_len));

                if current_block_disabled {
                    // Inline config disables this rule — preserve original
                    result.push_str(line);
                    result.push('\n');
                } else if target_style == CodeBlockStyle::Indented {
                    // Skip the opening fence
                    in_indented_block = true;
                } else {
                    // Keep the fenced block
                    result.push_str(line);
                    result.push('\n');
                }
            } else if in_fenced_block && fenced_fence_opener.is_some() {
                let (fence_char, opener_len) = fenced_fence_opener.unwrap();
                // Per CommonMark: closing fence uses the same character, has at least as
                // many characters as the opener, and has no info string (only optional trailing spaces).
                let closer_len = trimmed.chars().take_while(|&c| c == fence_char).count();
                let after_closer = &trimmed[closer_len..];
                let is_closer = closer_len >= opener_len && after_closer.trim().is_empty() && closer_len > 0;
                if is_closer {
                    in_fenced_block = false;
                    fenced_fence_opener = None;
                    in_indented_block = false;

                    if current_block_disabled {
                        result.push_str(line);
                        result.push('\n');
                    } else if target_style == CodeBlockStyle::Indented {
                        // Skip the closing fence
                    } else {
                        // Keep the fenced block
                        result.push_str(line);
                        result.push('\n');
                    }
                    current_block_disabled = false;
                } else if current_block_disabled {
                    // Inline config disables this rule — preserve original
                    result.push_str(line);
                    result.push('\n');
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
            } else if self.is_indented_code_block_with_context(lines, i, is_mkdocs, &ictx) {
                // This is an indented code block

                // Respect inline disable comments
                if ctx.inline_config().is_rule_disabled(self.name(), line_num) {
                    result.push_str(line);
                    result.push('\n');
                    continue;
                }

                // Check if we need to start a new fenced block
                let prev_line_is_indented =
                    i > 0 && self.is_indented_code_block_with_context(lines, i - 1, is_mkdocs, &ictx);

                if target_style == CodeBlockStyle::Fenced {
                    // Anchor fences at the list-item content baseline when
                    // converting a list-internal indented block (e.g. column
                    // 2 for `- `), so the new fenced block stays attached
                    // to the bullet. Top-level indented blocks have no
                    // baseline → fences sit at column 0.
                    let baseline = list_item_baseline.get(i).copied().flatten().unwrap_or(0);
                    // Per CommonMark, the indented-code prefix is exactly 4
                    // spaces past the surrounding container's content
                    // column. Strip those 4 spaces (not all leading
                    // whitespace) so any internal indentation past that
                    // point is preserved verbatim in the fenced body.
                    let body = line.strip_prefix("    ").unwrap_or(line);

                    // Check if this line is part of a misplaced fenced block
                    // (pre-computed block-level analysis, not per-line)
                    if misplaced_fence_lines[i] {
                        // Just remove the indentation - this is a complete misplaced fenced block
                        result.push_str(line.trim_start());
                        result.push('\n');
                    } else if unsafe_fence_lines[i] {
                        // This block contains fence markers but isn't a complete fenced block
                        // Wrapping would create invalid nested fences - keep as-is (don't fix)
                        result.push_str(line);
                        result.push('\n');
                    } else if !prev_line_is_indented && !in_indented_block {
                        // Start of a new indented block that should be fenced
                        current_block_fence_indent = " ".repeat(baseline);
                        result.push_str(&current_block_fence_indent);
                        result.push_str("```\n");
                        result.push_str(body);
                        result.push('\n');
                        in_indented_block = true;
                    } else {
                        // Inside an indented block
                        result.push_str(body);
                        result.push('\n');
                    }

                    // Check if this is the end of the indented block
                    let next_line_is_indented =
                        i < lines.len() - 1 && self.is_indented_code_block_with_context(lines, i + 1, is_mkdocs, &ictx);
                    // Don't close if this is an unsafe block (kept as-is)
                    if !next_line_is_indented
                        && in_indented_block
                        && !misplaced_fence_lines[i]
                        && !unsafe_fence_lines[i]
                    {
                        result.push_str(&current_block_fence_indent);
                        result.push_str("```\n");
                        in_indented_block = false;
                        current_block_fence_indent.clear();
                    }
                } else {
                    // Keep indented block as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else {
                // Regular line
                if in_indented_block && target_style == CodeBlockStyle::Fenced {
                    result.push_str(&current_block_fence_indent);
                    result.push_str("```\n");
                    in_indented_block = false;
                    current_block_fence_indent.clear();
                }

                result.push_str(line);
                result.push('\n');
            }
        }

        // Close any remaining blocks
        if in_indented_block && target_style == CodeBlockStyle::Fenced {
            result.push_str(&current_block_fence_indent);
            result.push_str("```\n");
        }

        // Close any unclosed fenced blocks.
        // Only close if check() also confirms this block is unclosed. The line-by-line
        // fence scanner in fix() can disagree with pulldown-cmark on block boundaries
        // (e.g., markdown documentation blocks with nested fence examples), so we use
        // check_unclosed_code_blocks() as the authoritative source of truth.
        if let Some((fence_char, opener_len)) = fenced_fence_opener
            && in_fenced_block
        {
            let has_unclosed_violation = !self.check_unclosed_code_blocks(ctx).is_empty();
            if has_unclosed_violation {
                let closer: String = std::iter::repeat_n(fence_char, opener_len).collect();
                result.push_str(&closer);
                result.push('\n');
            }
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

    /// Test helper: detect_style with automatic context computation.
    ///
    /// The container context (HTML/MDX comments, HTML/JSX blocks,
    /// mkdocstrings, footnote definitions, blockquotes) is not populated by
    /// this helper — callers that need to exercise those paths should go
    /// through the full `rule.check(&ctx)` entry point so the real LineInfo
    /// is computed from a `LintContext`.
    fn detect_style_from_content(rule: &MD046CodeBlockStyle, content: &str, is_mkdocs: bool) -> Option<CodeBlockStyle> {
        let lines: Vec<&str> = content.lines().collect();
        let in_list_context = rule.precompute_block_continuation_context(&lines);
        let in_tab_context = if is_mkdocs {
            rule.precompute_mkdocs_tab_context(&lines)
        } else {
            vec![false; lines.len()]
        };
        let in_admonition_context = if is_mkdocs {
            rule.precompute_mkdocs_admonition_context(&lines)
        } else {
            vec![false; lines.len()]
        };
        let in_comment_or_html = vec![false; lines.len()];
        // List baseline is None for every line: this helper preserves the
        // pre-baseline behavior where any list-context line is conservatively
        // skipped. Tests that need list-internal indented code blocks
        // recognized must drive the rule through `check`/`fix` with a real
        // `LintContext`.
        let list_item_baseline: Vec<Option<usize>> = vec![None; lines.len()];
        let ictx = IndentContext {
            in_list_context: &in_list_context,
            in_tab_context: &in_tab_context,
            in_admonition_context: &in_admonition_context,
            in_comment_or_html: &in_comment_or_html,
            list_item_baseline: &list_item_baseline,
        };
        rule.detect_style(&lines, is_mkdocs, &ictx)
    }

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
        let style = detect_style_from_content(&rule, content, false);

        assert_eq!(style, Some(CodeBlockStyle::Fenced));
    }

    #[test]
    fn test_detect_style_indented() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "Text\n\n    code\n\nMore";
        let style = detect_style_from_content(&rule, content, false);

        assert_eq!(style, Some(CodeBlockStyle::Indented));
    }

    #[test]
    fn test_detect_style_none() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "No code blocks here";
        let style = detect_style_from_content(&rule, content, false);

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
        let style = detect_style_from_content(&rule, content, true);
        assert_eq!(style, None); // No code blocks detected

        // In standard mode, it would detect indented code blocks
        let style = detect_style_from_content(&rule, content, false);
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
    fn test_consistent_style_indented_html_comment() {
        // Under the default `Consistent` style, indented content inside an
        // HTML comment must not contribute to the document's code-block style
        // tally. Otherwise a single fenced block alongside an indented HTML
        // comment flips the detected style to `Indented`, emitting a spurious
        // "Use indented code blocks" warning against the only real code block.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# MD046 false-positive reproduction\n\
                       \n\
                       <!--\n    \
                       This is just an indented comment, not a code block.\n\
                       \n    \
                       A second line is required to trigger the false-positive.\n\
                       \n    \
                       Actually, three lines are required.\n\
                       -->\n\
                       \n\
                       ```md\n\
                       This should be fine, since it's the only code block and therefore consistent.\n\
                       ```\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "A single fenced block and an indented HTML comment must produce no MD046 warnings",
        );
    }

    #[test]
    fn test_consistent_style_indented_html_block() {
        // Indented content inside a raw HTML block (e.g. a `<div>` tag pair)
        // must not count as an indented code block when `detect_style` picks
        // the document's predominant style.
        //
        // Per CommonMark, a type-6 HTML block is terminated by a blank line,
        // so the content here is kept contiguous to remain inside the block.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       <div class=\"note\">\n    \
                       line one of indented html content\n    \
                       line two of indented html content\n    \
                       line three of indented html content\n\
                       </div>\n\
                       \n\
                       ```md\n\
                       real fenced block\n\
                       ```\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Indented content inside a raw HTML block must not influence MD046 style detection",
        );
    }

    #[test]
    fn test_consistent_style_fake_fence_inside_html_comment() {
        // Fence markers inside an HTML comment must not contribute to the
        // fenced count during style detection. Otherwise a document whose
        // only real code block is indented gets flagged "Use fenced code
        // blocks" under `Consistent` because the verbatim ``` inside the
        // comment ties the count.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Title\n\
                       \n\
                       <!--\n\
                       ```\n\
                       fake fence inside comment\n\
                       ```\n\
                       -->\n\
                       \n    \
                       real indented code block line 1\n    \
                       real indented code block line 2\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Fence markers inside an HTML comment must not influence MD046 style detection",
        );
    }

    #[test]
    fn test_consistent_style_indented_footnote_definition() {
        // Footnote-definition continuation lines are commonly indented by 4+
        // spaces. They must not be counted as indented code blocks during
        // style detection under `Consistent`.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       Reference to a footnote[^note].\n\
                       \n\
                       [^note]: First line of the footnote.\n    \
                       Second indented continuation line.\n    \
                       Third indented continuation line.\n    \
                       Fourth indented continuation line.\n\
                       \n\
                       ```md\n\
                       real fenced block\n\
                       ```\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Footnote-definition continuation content must not influence MD046 style detection",
        );
    }

    #[test]
    fn test_consistent_style_indented_blockquote() {
        // Indented content inside a blockquote (`>     foo`) must not be
        // counted as an indented code block by `detect_style`. The check-side
        // skip list already excludes `blockquote.is_some()` for indented
        // warnings, so detection must match to keep `Consistent` stable.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       >     line one of quoted indented content\n\
                       >\n\
                       >     line two of quoted indented content\n\
                       >\n\
                       >     line three of quoted indented content\n\
                       \n\
                       ```md\n\
                       real fenced block\n\
                       ```\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Indented content inside a blockquote must not influence MD046 style detection",
        );
    }

    #[test]
    fn test_consistent_style_genuine_indented_block_detected_as_indented() {
        // A top-level indented code block that is not inside any container
        // must still count toward the Indented tally under `Consistent` style.
        // This guards against over-filtering: the `in_comment_or_html` skip
        // must not suppress real indented code blocks.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       Some prose.\n\
                       \n    \
                       real indented code line 1\n    \
                       real indented code line 2\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only one indented block exists; Consistent must detect it as Indented and
        // produce no warnings (the detected style matches the only real block).
        assert_eq!(
            result,
            vec![],
            "A genuine top-level indented block must be detected as Indented style under Consistent",
        );
    }

    #[test]
    fn test_consistent_style_skipped_lines_dont_override_real_block() {
        // Two indented-but-skipped regions (inside HTML comments) plus one
        // genuine indented code block and no fenced blocks: the skipped lines
        // must be excluded from the tally, leaving indented_count=1, fenced_count=0,
        // so Consistent still selects Indented and emits no warnings.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       <!--\n    \
                       skipped indented comment line 1\n    \
                       skipped indented comment line 2\n\
                       -->\n\
                       \n\
                       <!--\n    \
                       second skipped region\n    \
                       also skipped\n\
                       -->\n\
                       \n    \
                       real indented code line\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Skipped container lines must not outweigh the single real indented block",
        );
    }

    #[test]
    fn test_consistent_style_fenced_wins_over_skipped_indented() {
        // One real fenced block plus two indented-but-skipped regions: after
        // filtering the skipped lines the tally is fenced=1, indented=0, so
        // Consistent selects Fenced and emits no warnings.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       <!--\n    \
                       skipped indented region one\n    \
                       more of region one\n\
                       -->\n\
                       \n\
                       <!--\n    \
                       skipped indented region two\n    \
                       more of region two\n\
                       -->\n\
                       \n\
                       ```md\n\
                       real fenced block\n\
                       ```\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Fenced block must win when all indented lines are inside skipped containers",
        );
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
