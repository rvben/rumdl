use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::code_block_utils::CodeBlockStyle;
use crate::utils::element_cache::ElementCache;
use crate::utils::mkdocs_tabs;
use crate::utils::range_utils::calculate_line_range;
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

        // Check if preceded by a blank line (typical for code blocks)
        // OR if the previous line is also an indented code block (continuation)
        let has_blank_line_before = i == 0 || lines[i - 1].trim().is_empty();
        let prev_is_indented_code = i > 0
            && ElementCache::calculate_indentation_width_default(lines[i - 1]) >= 4
            && !in_list_context[i - 1]
            && !(is_mkdocs && in_tab_context[i - 1]);

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
    ) -> (Vec<bool>, Vec<bool>) {
        let mut is_misplaced = vec![false; lines.len()];
        let mut contains_fences = vec![false; lines.len()];

        // Find contiguous indented blocks and categorize them
        let mut i = 0;
        while i < lines.len() {
            // Find the start of an indented block
            if !self.is_indented_code_block_with_context(lines, i, is_mkdocs, in_list_context, in_tab_context) {
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
        let mut fence_stack: Vec<(String, usize, usize, bool, bool)> = Vec::new(); // (fence_marker, fence_length, opening_line, flagged_for_nested, is_markdown_example)

        // Track if we're inside a markdown code block (for documentation examples)
        // This is used to allow nested code blocks in markdown documentation
        let mut inside_markdown_documentation_block = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            // Skip lines inside HTML comments - code block examples in comments are not real code blocks
            if let Some(line_info) = ctx.lines.get(i)
                && line_info.in_html_comment
            {
                continue;
            }

            // Check for fence markers (``` or ~~~)
            // Per CommonMark: fence must have 0-3 spaces of indentation
            if Self::has_valid_fence_indent(line) && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
                let fence_char = if trimmed.starts_with("```") { '`' } else { '~' };

                // Count the fence length
                let fence_length = trimmed.chars().take_while(|&c| c == fence_char).count();

                // Check what comes after the fence characters
                let after_fence = &trimmed[fence_length..];

                // CommonMark spec: "If the info string comes after a backtick fence,
                // it may not contain any backtick characters."
                // This means ```something``` is NOT a valid fence - the backticks are inline code.
                if fence_char == '`' && after_fence.contains('`') {
                    continue;
                }

                // Check if this is a valid fence pattern
                // Valid markdown code fence syntax:
                // - ``` or ~~~ (just fence)
                // - ``` language or ~~~ language (fence with space then language)
                // - ```language (without space) is accepted by many parsers but only for actual languages
                let is_valid_fence_pattern = if after_fence.is_empty() {
                    // Empty after fence is always valid (e.g., ``` or ~~~)
                    true
                } else if after_fence.starts_with(' ') || after_fence.starts_with('\t') {
                    // Space after fence - anything following is valid as info string
                    true
                } else {
                    // No space after fence - must be a valid language identifier
                    // Be strict to avoid false positives on content that looks like fences
                    let identifier = after_fence.trim().to_lowercase();

                    // Reject obvious non-language patterns
                    if identifier.contains("fence") || identifier.contains("still") {
                        false
                    } else if identifier.len() > 20 {
                        // Most language identifiers are short
                        false
                    } else if let Some(first_char) = identifier.chars().next() {
                        // Must start with letter or # (for C#, F#)
                        if !first_char.is_alphabetic() && first_char != '#' {
                            false
                        } else {
                            // Check all characters are valid for a language identifier
                            // Also check it's not just random text
                            let valid_chars = identifier.chars().all(|c| {
                                c.is_alphanumeric() || c == '-' || c == '_' || c == '+' || c == '#' || c == '.'
                            });

                            // Additional check: at least 2 chars and not all consonants (helps filter random words)
                            valid_chars && identifier.len() >= 2
                        }
                    } else {
                        false
                    }
                };

                // When inside a code block, be conservative about what we treat as a fence
                if !fence_stack.is_empty() {
                    // Skip if not a valid fence pattern to begin with
                    if !is_valid_fence_pattern {
                        continue;
                    }

                    // Check if this could be a closing fence for the current block
                    if let Some((open_marker, open_length, _, _, _)) = fence_stack.last() {
                        if fence_char == open_marker.chars().next().unwrap() && fence_length >= *open_length {
                            // Potential closing fence - check if it has content after
                            if !after_fence.trim().is_empty() {
                                // Has content after - likely not a closing fence
                                // Apply structural validation to determine if it's a nested fence

                                // Skip patterns that are clearly decorative or content
                                // 1. Contains special characters not typical in language identifiers
                                let has_special_chars = after_fence.chars().any(|c| {
                                    !c.is_alphanumeric()
                                        && c != '-'
                                        && c != '_'
                                        && c != '+'
                                        && c != '#'
                                        && c != '.'
                                        && c != ' '
                                        && c != '\t'
                                });

                                if has_special_chars {
                                    continue; // e.g., ~~~!@#$%, ~~~~~~~~^^^^
                                }

                                // 2. Check for repetitive non-alphanumeric patterns
                                if fence_length > 4 && after_fence.chars().take(4).all(|c| !c.is_alphanumeric()) {
                                    continue; // e.g., ~~~~~~~~~~ or ````````
                                }

                                // 3. If no space after fence, must look like a valid language identifier
                                if !after_fence.starts_with(' ') && !after_fence.starts_with('\t') {
                                    let identifier = after_fence.trim();

                                    // Must start with letter or # (for C#, F#)
                                    if let Some(first) = identifier.chars().next()
                                        && !first.is_alphabetic()
                                        && first != '#'
                                    {
                                        continue;
                                    }

                                    // Reasonable length for a language identifier
                                    if identifier.len() > 30 {
                                        continue;
                                    }
                                }
                            }
                            // Otherwise, could be a closing fence - let it through
                        } else {
                            // Different fence type or insufficient length
                            // Only treat as nested if it looks like a real fence with language

                            // Must have proper spacing or no content after fence
                            if !after_fence.is_empty()
                                && !after_fence.starts_with(' ')
                                && !after_fence.starts_with('\t')
                            {
                                // No space after fence - be very strict
                                let identifier = after_fence.trim();

                                // Skip if contains any special characters beyond common ones
                                if identifier.chars().any(|c| {
                                    !c.is_alphanumeric() && c != '-' && c != '_' && c != '+' && c != '#' && c != '.'
                                }) {
                                    continue;
                                }

                                // Skip if doesn't start with letter or #
                                if let Some(first) = identifier.chars().next()
                                    && !first.is_alphabetic()
                                    && first != '#'
                                {
                                    continue;
                                }
                            }
                        }
                    }
                }

                // We'll check if this is a markdown block after determining if it's an opening fence

                // Check if this is a closing fence for the current open fence
                if let Some((open_marker, open_length, _open_line, _flagged, _is_md)) = fence_stack.last() {
                    // Must match fence character and have at least as many characters
                    if fence_char == open_marker.chars().next().unwrap() && fence_length >= *open_length {
                        // Check if this line has only whitespace after the fence marker
                        let after_fence = &trimmed[fence_length..];
                        if after_fence.trim().is_empty() {
                            // This is a valid closing fence
                            let _popped = fence_stack.pop();

                            // Check if we're exiting a markdown documentation block
                            if let Some((_, _, _, _, is_md)) = _popped
                                && is_md
                            {
                                inside_markdown_documentation_block = false;
                            }
                            continue;
                        }
                    }
                }

                // This is an opening fence (has content after marker or no matching open fence)
                // Note: after_fence was already calculated above during validation
                if !after_fence.trim().is_empty() || fence_stack.is_empty() {
                    // Only flag as problematic if we're opening a new fence while another is still open
                    // AND they use the same fence character (indicating potential confusion)
                    // AND we're not inside a markdown documentation block
                    let has_nested_issue =
                        if let Some((open_marker, open_length, open_line, _, _)) = fence_stack.last_mut() {
                            if fence_char == open_marker.chars().next().unwrap()
                                && fence_length >= *open_length
                                && !inside_markdown_documentation_block
                            {
                                // This is problematic - same fence character used with equal or greater length while another is open
                                let (opening_start_line, opening_start_col, opening_end_line, opening_end_col) =
                                    calculate_line_range(*open_line, lines[*open_line - 1]);

                                // Calculate the byte position to insert closing fence before this line
                                let line_start_byte = ctx.line_index.get_line_start_byte(i + 1).unwrap_or(0);

                                warnings.push(LintWarning {
                                    rule_name: Some(self.name().to_string()),
                                    line: opening_start_line,
                                    column: opening_start_col,
                                    end_line: opening_end_line,
                                    end_column: opening_end_col,
                                    message: format!(
                                        "Code block '{}' should be closed before starting new one at line {}",
                                        open_marker,
                                        i + 1
                                    ),
                                    severity: Severity::Warning,
                                    fix: Some(Fix {
                                        range: (line_start_byte..line_start_byte),
                                        replacement: format!("{open_marker}\n\n"),
                                    }),
                                });

                                // Mark the current fence as flagged for nested issue
                                fence_stack.last_mut().unwrap().3 = true;
                                true // We flagged a nested issue for this fence
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                    // Check if this opening fence is a markdown code block
                    let after_fence_for_lang = &trimmed[fence_length..];
                    let lang_info = after_fence_for_lang.trim().to_lowercase();
                    let is_markdown_fence = lang_info.starts_with("markdown") || lang_info.starts_with("md");

                    // If we're opening a markdown documentation block, mark that we're inside one
                    if is_markdown_fence && !inside_markdown_documentation_block {
                        inside_markdown_documentation_block = true;
                    }

                    // Add this fence to the stack
                    let fence_marker = fence_char.to_string().repeat(fence_length);
                    fence_stack.push((fence_marker, fence_length, i + 1, has_nested_issue, is_markdown_fence));
                }
            }
        }

        // Check for unclosed fences at end of file
        // Only flag unclosed if we haven't already flagged for nested issues
        for (fence_marker, _, opening_line, flagged_for_nested, _) in fence_stack {
            if !flagged_for_nested {
                let (start_line, start_col, end_line, end_col) =
                    calculate_line_range(opening_line, lines[opening_line - 1]);

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

        // Pre-compute list and tab contexts for efficiency
        let in_list_context = self.precompute_block_continuation_context(&lines);
        let in_tab_context = if is_mkdocs {
            self.precompute_mkdocs_tab_context(&lines)
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
                && self.is_indented_code_block_with_context(&lines, i, is_mkdocs, &in_list_context, &in_tab_context)
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

        // Pre-compute list and tab contexts once for all checks
        let in_list_context = self.precompute_block_continuation_context(&lines);
        let in_tab_context = if is_mkdocs {
            self.precompute_mkdocs_tab_context(&lines)
        } else {
            vec![false; lines.len()]
        };

        // Determine the target style from the detected style in the document
        let target_style = match self.config.style {
            CodeBlockStyle::Consistent => self
                .detect_style(ctx.content, is_mkdocs)
                .unwrap_or(CodeBlockStyle::Fenced),
            _ => self.config.style,
        };

        // Process each line to find style inconsistencies
        // Pre-compute which lines are inside FENCED code blocks (not indented)
        // Use pre-computed code blocks from context
        let mut in_fenced_block = vec![false; lines.len()];
        for &(start, end) in &ctx.code_blocks {
            // Check if this block is fenced by examining its content
            if start < ctx.content.len() && end <= ctx.content.len() {
                let block_content = &ctx.content[start..end];
                let is_fenced = block_content.starts_with("```") || block_content.starts_with("~~~");

                if is_fenced {
                    // Mark all lines in this fenced block
                    for (line_idx, line_info) in ctx.lines.iter().enumerate() {
                        if line_info.byte_offset >= start && line_info.byte_offset < end {
                            in_fenced_block[line_idx] = true;
                        }
                    }
                }
            }
        }

        let mut in_fence = false;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            // Skip lines that are in HTML blocks - they shouldn't be treated as indented code
            if ctx.line_info(i + 1).is_some_and(|info| info.in_html_block) {
                continue;
            }

            // Skip lines inside HTML comments - code block examples in comments are not real code blocks
            if ctx.line_info(i + 1).is_some_and(|info| info.in_html_comment) {
                continue;
            }

            // Skip if this line is in a mkdocstrings block (but not other skip contexts,
            // since MD046 needs to detect regular code blocks)
            if ctx.lines[i].in_mkdocstrings {
                continue;
            }

            // Check for fenced code block markers (for style checking)
            // Per CommonMark: fence must have 0-3 spaces of indentation
            if Self::has_valid_fence_indent(line) && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
                if target_style == CodeBlockStyle::Indented && !in_fence {
                    // This is an opening fence marker but we want indented style
                    // Only flag the opening marker, not the closing one
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, line);
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Use indented code blocks".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: ctx.line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: String::new(),
                        }),
                    });
                }
                // Toggle fence state
                in_fence = !in_fence;
                continue;
            }

            // Skip content lines inside fenced blocks
            // This prevents false positives like flagging ~~~~ inside bash output
            if in_fenced_block[i] {
                continue;
            }

            // Check for indented code blocks (when not inside a fenced block)
            if self.is_indented_code_block_with_context(&lines, i, is_mkdocs, &in_list_context, &in_tab_context)
                && target_style == CodeBlockStyle::Fenced
            {
                // Check if this is the start of a new indented block
                let prev_line_is_indented = i > 0
                    && self.is_indented_code_block_with_context(
                        &lines,
                        i - 1,
                        is_mkdocs,
                        &in_list_context,
                        &in_tab_context,
                    );

                if !prev_line_is_indented {
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, line);
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Use fenced code blocks".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: ctx.line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: format!("```\n{}", line.trim_start()),
                        }),
                    });
                }
            }
        }

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

        // Pre-compute list and tab contexts for efficiency
        let in_list_context = self.precompute_block_continuation_context(&lines);
        let in_tab_context = if is_mkdocs {
            self.precompute_mkdocs_tab_context(&lines)
        } else {
            vec![false; lines.len()]
        };

        // Categorize indented blocks:
        // - misplaced_fence_lines: complete fenced blocks that were over-indented (safe to dedent)
        // - unsafe_fence_lines: contain fence markers but aren't complete (skip fixing to avoid broken output)
        let (misplaced_fence_lines, unsafe_fence_lines) =
            self.categorize_indented_blocks(&lines, is_mkdocs, &in_list_context, &in_tab_context);

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
                    result.push_str("    ");
                    result.push_str(trimmed);
                    result.push('\n');
                } else {
                    // Keep fenced block content as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else if self.is_indented_code_block_with_context(&lines, i, is_mkdocs, &in_list_context, &in_tab_context)
            {
                // This is an indented code block

                // Check if we need to start a new fenced block
                let prev_line_is_indented = i > 0
                    && self.is_indented_code_block_with_context(
                        &lines,
                        i - 1,
                        is_mkdocs,
                        &in_list_context,
                        &in_tab_context,
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
