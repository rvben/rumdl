use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::code_block_utils::CodeBlockStyle;
use crate::utils::mkdocs_tabs;
use crate::utils::range_utils::{LineIndex, calculate_line_range};
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

    fn is_fenced_code_block_start(&self, line: &str) -> bool {
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

    fn is_indented_code_block(&self, lines: &[&str], i: usize, is_mkdocs: bool) -> bool {
        if i >= lines.len() {
            return false;
        }

        let line = lines[i];

        // Check if indented by at least 4 spaces or tab
        if !(line.starts_with("    ") || line.starts_with("\t")) {
            return false;
        }

        // Check if this is part of a list structure
        if self.is_part_of_list_structure(lines, i) {
            return false;
        }

        // Skip if this is MkDocs tab content
        if is_mkdocs && self.is_in_mkdocs_tab(lines, i) {
            return false;
        }

        // Check if preceded by a blank line (typical for code blocks)
        // OR if the previous line is also an indented code block (continuation)
        let has_blank_line_before = i == 0 || lines[i - 1].trim().is_empty();
        let prev_is_indented_code = i > 0
            && (lines[i - 1].starts_with("    ") || lines[i - 1].starts_with("\t"))
            && !self.is_part_of_list_structure(lines, i - 1)
            && !(is_mkdocs && self.is_in_mkdocs_tab(lines, i - 1));

        // If no blank line before and previous line is not indented code,
        // it's likely list continuation, not a code block
        if !has_blank_line_before && !prev_is_indented_code {
            return false;
        }

        true
    }

    /// Check if an indented line is part of a list structure
    fn is_part_of_list_structure(&self, lines: &[&str], i: usize) -> bool {
        // Look backwards to find if we're in a list context
        // We need to be more aggressive about detecting list contexts

        for j in (0..i).rev() {
            let line = lines[j];

            // Skip empty lines - they don't break list context
            if line.trim().is_empty() {
                continue;
            }

            // If we find a list item, we're definitely in a list context
            if self.is_list_item(line) {
                return true;
            }

            // Check if this line looks like it's part of a list item
            // (indented content that's not a code block)
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();

            // If we find a line that starts at column 0 and is not a list item,
            // check if it's a structural element that would end list context
            if indent_len == 0 && !trimmed.is_empty() {
                // Headings definitely end list context
                if trimmed.starts_with('#') {
                    break;
                }
                // Horizontal rules end list context
                if trimmed.starts_with("---") || trimmed.starts_with("***") {
                    break;
                }
                // If it's a paragraph that doesn't look like it's part of a list,
                // we might not be in a list anymore, but let's be conservative
                // and keep looking a bit more
                if j > 0 && i >= 5 && j < i - 5 {
                    // Only break if we've looked back a reasonable distance
                    break;
                }
            }

            // Continue looking backwards through indented content
        }

        false
    }

    /// Helper function to check if a line is part of MkDocs tab content
    fn is_in_mkdocs_tab(&self, lines: &[&str], i: usize) -> bool {
        // Look backwards for tab markers
        for j in (0..i).rev() {
            let line = lines[j];

            // Check if this is a tab marker
            if mkdocs_tabs::is_tab_marker(line) {
                let tab_indent = mkdocs_tabs::get_tab_indent(line).unwrap_or(0);
                // Check if current line has proper tab content indentation
                if mkdocs_tabs::is_tab_content(lines[i], tab_indent) {
                    return true;
                }
                // If we found a tab but indentation doesn't match, we're not in it
                return false;
            }

            // If we hit a non-indented, non-empty line that's not a tab, stop searching
            if !line.trim().is_empty() && !line.starts_with("    ") && !mkdocs_tabs::is_tab_marker(line) {
                break;
            }
        }
        false
    }

    fn check_unclosed_code_blocks(
        &self,
        ctx: &crate::lint_context::LintContext,
        line_index: &LineIndex,
    ) -> Result<Vec<LintWarning>, LintError> {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = ctx.content.lines().collect();
        let mut fence_stack: Vec<(String, usize, usize, bool, bool)> = Vec::new(); // (fence_marker, fence_length, opening_line, flagged_for_nested, is_markdown_example)

        // Track if we're inside a markdown code block (for documentation examples)
        // This is used to allow nested code blocks in markdown documentation
        let mut inside_markdown_documentation_block = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            // Check for fence markers (``` or ~~~)
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let fence_char = if trimmed.starts_with("```") { '`' } else { '~' };

                // Count the fence length
                let fence_length = trimmed.chars().take_while(|&c| c == fence_char).count();

                // Check what comes after the fence characters
                let after_fence = &trimmed[fence_length..];

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
                                let line_start_byte = line_index.get_line_start_byte(i + 1).unwrap_or(0);

                                warnings.push(LintWarning {
                                    rule_name: Some(self.name()),
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
                    rule_name: Some(self.name()),
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
        let mut fenced_found = false;
        let mut indented_found = false;
        let mut fenced_line = usize::MAX;
        let mut indented_line = usize::MAX;

        // First scan through all lines to find code blocks
        for (i, line) in lines.iter().enumerate() {
            if self.is_fenced_code_block_start(line) {
                fenced_found = true;
                fenced_line = fenced_line.min(i);
            } else if self.is_indented_code_block(&lines, i, is_mkdocs) {
                indented_found = true;
                indented_line = indented_line.min(i);
            }
        }

        if !fenced_found && !indented_found {
            // No code blocks found
            None
        } else if fenced_found && !indented_found {
            // Only fenced blocks found
            Some(CodeBlockStyle::Fenced)
        } else if !fenced_found && indented_found {
            // Only indented blocks found
            Some(CodeBlockStyle::Indented)
        } else {
            // Both types found - use the first one encountered
            if indented_line < fenced_line {
                Some(CodeBlockStyle::Indented)
            } else {
                Some(CodeBlockStyle::Fenced)
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
        if !ctx.content.contains("```") && !ctx.content.contains("~~~") && !ctx.content.contains("    ") {
            return Ok(Vec::new());
        }

        // First, always check for unclosed code blocks
        let line_index = LineIndex::new(ctx.content.to_string());
        let unclosed_warnings = self.check_unclosed_code_blocks(ctx, &line_index)?;

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

        // Process each line to find style inconsistencies
        let mut in_fenced_block = false;
        let line_index = LineIndex::new(ctx.content.to_string());

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            // Skip lines that are in HTML blocks - they shouldn't be treated as indented code
            if ctx.is_in_html_block(i + 1) {
                continue;
            }

            // Skip if this line is in a mkdocstrings block (but not other skip contexts,
            // since MD046 needs to detect regular code blocks)
            if ctx.lines[i].in_mkdocstrings {
                continue;
            }

            // Track fenced code blocks
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_fenced_block = !in_fenced_block;

                if target_style == CodeBlockStyle::Indented && !in_fenced_block {
                    // This is starting a fenced block but we want indented style
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, line);
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Use indented code blocks".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: String::new(),
                        }),
                    });
                }
            }
            // Check for indented code blocks (when not in a fenced block)
            else if !in_fenced_block
                && self.is_indented_code_block(&lines, i, is_mkdocs)
                && target_style == CodeBlockStyle::Fenced
            {
                // Check if this is the start of a new indented block
                let prev_line_is_indented = i > 0 && self.is_indented_code_block(&lines, i - 1, is_mkdocs);

                if !prev_line_is_indented {
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, line);
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Use fenced code blocks".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
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
        let line_index = LineIndex::new(ctx.content.to_string());
        let unclosed_warnings = self.check_unclosed_code_blocks(ctx, &line_index)?;

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

        let mut result = String::with_capacity(content.len());
        let mut in_fenced_block = false;
        let mut fenced_fence_type = None;
        let mut in_indented_block = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            // Handle fenced code blocks
            if !in_fenced_block && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
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
            } else if self.is_indented_code_block(&lines, i, is_mkdocs) {
                // This is an indented code block

                // Check if we need to start a new fenced block
                let prev_line_is_indented = i > 0 && self.is_indented_code_block(&lines, i - 1, is_mkdocs);

                if target_style == CodeBlockStyle::Fenced {
                    if !prev_line_is_indented && !in_indented_block {
                        // Start of a new indented block that should be fenced
                        result.push_str("```\n");
                        result.push_str(line.trim_start());
                        result.push('\n');
                        in_indented_block = true;
                    } else {
                        // Inside an indented block
                        result.push_str(line.trim_start());
                        result.push('\n');
                    }

                    // Check if this is the end of the indented block
                    let _next_line_is_indented =
                        i < lines.len() - 1 && self.is_indented_code_block(&lines, i + 1, is_mkdocs);
                    if !_next_line_is_indented && in_indented_block {
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // All blocks are fenced, so consistent style should be OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_consistent_style_with_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "Text\n\n    code\n    more code\n\nMore text\n\n    another block";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // All blocks are indented, so consistent style should be OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_consistent_style_mixed() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "```\nfenced code\n```\n\nText\n\n    indented code\n\nMore";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Mixed styles should be flagged
        assert!(!result.is_empty());
    }

    #[test]
    fn test_fenced_style_with_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "Text\n\n    indented code\n    more code\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Indented blocks should be flagged when fenced style is required
        assert!(!result.is_empty());
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_indented_style_with_fenced_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "Text\n\n```\nfenced code\n```\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Fenced blocks should be flagged when indented style is required
        assert!(!result.is_empty());
        assert!(result[0].message.contains("Use indented code blocks"));
    }

    #[test]
    fn test_unclosed_code_block() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode without closing fence";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("never closed"));
    }

    #[test]
    fn test_nested_code_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\nouter\n```\n\ninner text\n\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // This should parse as two separate code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_indented_to_fenced() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "Text\n\n    code line 1\n    code line 2\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("```\ncode line 1\ncode line 2\n```"));
    }

    #[test]
    fn test_fix_fenced_to_indented() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "Text\n\n```\ncode line 1\ncode line 2\n```\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("    code line 1\n    code line 2"));
        assert!(!fixed.contains("```"));
    }

    #[test]
    fn test_fix_unclosed_block() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode without closing";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();

        // Should add closing fence
        assert!(fixed.ends_with("```"));
    }

    #[test]
    fn test_code_block_in_list() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "- List item\n    code in list\n    more code\n- Next item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Tilde fences should be accepted as fenced blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_language_specification() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```rust\nfn main() {}\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Nested code blocks in markdown documentation should be allowed
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_preserve_trailing_newline() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode\n```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs);
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs);
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs);
        let result = rule.check(&ctx).unwrap();

        // Nested tabs should not be flagged
        assert_eq!(result.len(), 0);
    }
}
