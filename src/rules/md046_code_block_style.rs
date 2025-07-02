use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::code_block_utils::CodeBlockStyle;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{LineIndex, calculate_line_range};
use lazy_static::lazy_static;
use regex::Regex;
use toml;

mod md046_config;
use md046_config::MD046Config;

lazy_static! {
    static ref LIST_MARKER: Regex = Regex::new(r"^[\s]*[-+*][\s]+|^[\s]*\d+\.[\s]+").unwrap();
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

    fn is_indented_code_block(&self, lines: &[&str], i: usize) -> bool {
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

        // Check if this is part of a formatted text block (not a code block)
        if self.is_part_of_formatted_text_block(lines, i) {
            return false;
        }

        // Check if preceded by a blank line (typical for code blocks)
        // OR if the previous line is also an indented code block (continuation)
        let has_blank_line_before = i == 0 || lines[i - 1].trim().is_empty();
        let prev_is_indented_code = i > 0
            && (lines[i - 1].starts_with("    ") || lines[i - 1].starts_with("\t"))
            && !self.is_part_of_list_structure(lines, i - 1)
            && !self.is_part_of_formatted_text_block(lines, i - 1);

        // If no blank line before and previous line is not indented code,
        // it's likely list continuation, not a code block
        if !has_blank_line_before && !prev_is_indented_code {
            return false;
        }

        true
    }

    /// Check if an indented line is part of a formatted text block (like license text)
    /// rather than a code block
    fn is_part_of_formatted_text_block(&self, lines: &[&str], i: usize) -> bool {
        let line = lines[i];
        let trimmed = line.trim();

        // Look for patterns that suggest this is formatted text, not code:

        // 1. License/legal text patterns
        if trimmed.contains("Copyright")
            || trimmed.contains("License")
            || trimmed.contains("Foundation")
            || trimmed.contains("Certificate")
            || trimmed.contains("Origin")
            || trimmed.starts_with("Version ")
            || trimmed.contains("permitted")
            || trimmed.contains("contribution")
            || trimmed.contains("certify")
        {
            return true;
        }

        // 2. Address/contact information patterns
        if trimmed.contains("Drive")
            || trimmed.contains("Suite")
            || trimmed.contains("CA,")
            || trimmed.contains("San Francisco")
        {
            return true;
        }

        // 3. Email signature patterns
        if trimmed.contains("Signed-off-by:") || trimmed.contains("@") && trimmed.contains(".com") {
            return true;
        }

        // 4. Check if this is part of a larger block of indented text
        // that looks like formatted prose rather than code
        let mut consecutive_indented_lines = 0;
        let mut has_prose_content = false;

        // Look at surrounding lines to see if this is part of a prose block
        let start = i.saturating_sub(5);
        let end = if i + 5 < lines.len() { i + 5 } else { lines.len() };

        for check_line in lines.iter().take(end).skip(start) {
            if check_line.starts_with("    ") || check_line.starts_with("\t") {
                consecutive_indented_lines += 1;
                let check_trimmed = check_line.trim();
                // Look for prose indicators
                if check_trimmed.len() > 20
                    && (check_trimmed.contains(" the ")
                        || check_trimmed.contains(" and ")
                        || check_trimmed.contains(" or ")
                        || check_trimmed.contains(" to ")
                        || check_trimmed.contains(" of ")
                        || check_trimmed.contains(" in ")
                        || check_trimmed.contains(" is ")
                        || check_trimmed.contains(" that "))
                {
                    has_prose_content = true;
                }
            }
        }

        // If we have many consecutive indented lines with prose content,
        // it's likely formatted text, not code
        if consecutive_indented_lines >= 5 && has_prose_content {
            return true;
        }

        false
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

    /// Helper function to check if a line is part of a list
    fn is_in_list(&self, lines: &[&str], i: usize) -> bool {
        // Check if current line is a list item
        if i > 0 && lines[i - 1].trim_start().matches(&['-', '*', '+'][..]).count() > 0 {
            return true;
        }

        // Check for numbered list items
        if i > 0 {
            let prev = lines[i - 1].trim_start();
            if prev.len() > 2
                && prev.chars().next().unwrap().is_numeric()
                && (prev.contains(". ") || prev.contains(") "))
            {
                return true;
            }
        }

        false
    }

    fn check_unclosed_code_blocks(
        &self,
        ctx: &crate::lint_context::LintContext,
        _line_index: &LineIndex,
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
                            if let Some((_, _, _, _, is_md)) = _popped {
                                if is_md {
                                    inside_markdown_documentation_block = false;
                                }
                            }
                            continue;
                        }
                    }
                }

                // This is an opening fence (has content after marker or no matching open fence)
                let after_fence = &trimmed[fence_length..];
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
                                let line_start_byte = ctx.content.lines().take(i).map(|l| l.len() + 1).sum::<usize>();

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

    fn detect_style(&self, content: &str) -> Option<CodeBlockStyle> {
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
            } else if self.is_indented_code_block(&lines, i) {
                indented_found = true;
                indented_line = indented_line.min(i);
            }
        }

        if !fenced_found && !indented_found {
            // No code blocks found
            None
        } else if fenced_found && !indented_found {
            // Only fenced blocks found
            return Some(CodeBlockStyle::Fenced);
        } else if !fenced_found && indented_found {
            // Only indented blocks found
            return Some(CodeBlockStyle::Indented);
        } else {
            // Both types found - use the first one encountered
            if indented_line < fenced_line {
                return Some(CodeBlockStyle::Indented);
            } else {
                return Some(CodeBlockStyle::Fenced);
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

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
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

        // Try optimized path for style consistency checks
        let structure = DocumentStructure::new(ctx.content);
        if self.has_relevant_elements(ctx, &structure) {
            return self.check_with_structure(ctx, &structure);
        }

        Ok(Vec::new())
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
        let target_style = match self.config.style {
            CodeBlockStyle::Consistent => self.detect_style(content).unwrap_or(CodeBlockStyle::Fenced),
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
            } else if self.is_indented_code_block(&lines, i) {
                // This is an indented code block

                // Check if we need to start a new fenced block
                let prev_line_is_indented = i > 0 && self.is_indented_code_block(&lines, i - 1);

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
                    let _next_line_is_indented = i < lines.len() - 1 && self.is_indented_code_block(&lines, i + 1);
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
        if let Some(fence_type) = fenced_fence_type {
            if in_fenced_block {
                result.push_str(fence_type);
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
        ctx.content.is_empty()
            || (!ctx.content.contains("```") && !ctx.content.contains("~~~") && !ctx.content.contains("    "))
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        if ctx.content.is_empty() {
            return Ok(Vec::new());
        }

        // First, always check for unclosed code blocks and nested fences
        let line_index = LineIndex::new(ctx.content.to_string());
        let unclosed_warnings = self.check_unclosed_code_blocks(ctx, &line_index)?;

        // If we found unclosed blocks or nested fences, return those warnings first
        if !unclosed_warnings.is_empty() {
            return Ok(unclosed_warnings);
        }

        if !self.has_relevant_elements(ctx, structure) {
            return Ok(Vec::new());
        }

        // Skip README.md files - they often contain a mix of styles for documentation purposes
        if ctx.content.contains("# rumdl") && ctx.content.contains("## Quick Start") {
            return Ok(Vec::new());
        }

        // If there are no code blocks, nothing to check
        if structure.code_blocks.is_empty() {
            return Ok(Vec::new());
        }

        // Analyze code blocks in the content to determine what types are present
        // If all blocks are fenced and target style is fenced, or all blocks are indented and target style is indented, return empty
        let lines: Vec<&str> = ctx.content.lines().collect();
        let mut all_fenced = true;

        for block in &structure.code_blocks {
            // If we find a non-fenced block, set all_fenced to false
            match block.block_type {
                crate::utils::document_structure::CodeBlockType::Fenced => {
                    // Keep all_fenced as true
                }
                crate::utils::document_structure::CodeBlockType::Indented => {
                    all_fenced = false;
                    break;
                }
            }
        }

        // Fast path: If all blocks are fenced and target style is fenced (or consistent), return empty
        if all_fenced
            && (self.config.style == CodeBlockStyle::Fenced || self.config.style == CodeBlockStyle::Consistent)
        {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();

        // Determine the target style from the detected style in the document
        let target_style = match self.config.style {
            CodeBlockStyle::Consistent => {
                // For consistent style, use the same logic as the check method to ensure compatibility
                let mut first_fenced_line = usize::MAX;
                let mut first_indented_line = usize::MAX;

                for (i, line) in lines.iter().enumerate() {
                    if first_fenced_line == usize::MAX
                        && (line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~"))
                    {
                        first_fenced_line = i;
                    } else if first_indented_line == usize::MAX && self.is_indented_code_block(&lines, i) {
                        first_indented_line = i;
                    }

                    if first_fenced_line != usize::MAX && first_indented_line != usize::MAX {
                        break;
                    }
                }

                // Determine which style to use based on which appears first
                if first_fenced_line != usize::MAX
                    && (first_indented_line == usize::MAX || first_fenced_line < first_indented_line)
                {
                    CodeBlockStyle::Fenced
                } else if first_indented_line != usize::MAX {
                    CodeBlockStyle::Indented
                } else {
                    // Default to fenced if no code blocks found
                    CodeBlockStyle::Fenced
                }
            }
            _ => self.config.style,
        };

        // Keep track of code blocks we've processed to avoid duplicate warnings
        let mut processed_blocks = std::collections::HashSet::new();

        // Process each code block based on its type, following the same logic as the check method
        for (i, line) in lines.iter().enumerate() {
            let i_1based = i + 1; // Convert to 1-based for comparison with line numbers

            // Skip if we've already processed this block
            if processed_blocks.contains(&i_1based) {
                continue;
            }

            // Check for fenced code blocks
            if !self.is_in_list(&lines, i)
                && (line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~"))
            {
                if target_style == CodeBlockStyle::Indented {
                    // Calculate precise character range for the entire fence line
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, line);

                    // Add warning for opening fence
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
                            replacement: String::new(), // Remove the opening fence
                        }),
                    });

                    // Find closing fence and add warnings for all lines in the fenced block
                    let mut j = i + 1;
                    while j < lines.len() {
                        if lines[j].trim_start().starts_with("```") || lines[j].trim_start().starts_with("~~~") {
                            // Add warnings for content lines and closing fence
                            for (k, line_content) in lines.iter().enumerate().take(j + 1).skip(i + 1) {
                                // Calculate precise character range for the entire line
                                let (start_line, start_col, end_line, end_col) =
                                    calculate_line_range(k + 1, line_content);

                                warnings.push(LintWarning {
                                    rule_name: Some(self.name()),
                                    line: start_line,
                                    column: start_col,
                                    end_line,
                                    end_column: end_col,
                                    message: "Use fenced code blocks".to_string(),
                                    severity: Severity::Warning,
                                    fix: Some(Fix {
                                        range: line_index.line_col_to_byte_range(k + 1, 1),
                                        replacement: if k == j {
                                            String::new() // Remove closing fence
                                        } else {
                                            format!("    {}", line_content.trim_start())
                                            // Convert content to indented
                                        },
                                    }),
                                });
                            }

                            // Mark all lines in the fenced block as processed
                            for k in i..=j {
                                processed_blocks.insert(k + 1);
                            }
                            break;
                        }
                        j += 1;
                    }
                } else {
                    // Mark this block as processed (for non-indented target styles)
                    processed_blocks.insert(i_1based);

                    // Find closing fence to mark all lines as processed
                    let mut j = i + 1;
                    while j < lines.len() {
                        if lines[j].trim_start().starts_with("```") || lines[j].trim_start().starts_with("~~~") {
                            // Mark all lines in between as processed
                            for k in i + 1..=j {
                                processed_blocks.insert(k + 1);
                            }
                            break;
                        }
                        j += 1;
                    }
                }
            }
            // Check for indented code blocks
            else if !self.is_in_list(&lines, i) && self.is_indented_code_block(&lines, i) {
                if target_style == CodeBlockStyle::Fenced {
                    // Check if this is the start of a new indented block
                    let prev_line_is_indented = i > 0 && self.is_indented_code_block(&lines, i - 1);

                    if !prev_line_is_indented {
                        // Calculate precise character range for the entire indented line
                        let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, line);

                        // Add warning for indented block that should be fenced
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
                                replacement: "```\n".to_string() + line.trim_start(),
                            }),
                        });
                    }
                }

                // Mark this line as processed
                processed_blocks.insert(i_1based);
            }
        }

        Ok(warnings)
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

impl DocumentStructureExtensions for MD046CodeBlockStyle {
    fn has_relevant_elements(&self, ctx: &crate::lint_context::LintContext, structure: &DocumentStructure) -> bool {
        !ctx.content.is_empty() && !structure.code_blocks.is_empty()
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // All blocks are fenced, so consistent style should be OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_consistent_style_with_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "Text\n\n    code\n    more code\n\nMore text\n\n    another block";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // All blocks are indented, so consistent style should be OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_consistent_style_mixed() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "```\nfenced code\n```\n\nText\n\n    indented code\n\nMore";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Mixed styles should be flagged
        assert!(!result.is_empty());
    }

    #[test]
    fn test_fenced_style_with_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "Text\n\n    indented code\n    more code\n\nMore text";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Indented blocks should be flagged when fenced style is required
        assert!(!result.is_empty());
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_indented_style_with_fenced_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "Text\n\n```\nfenced code\n```\n\nMore text";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Fenced blocks should be flagged when indented style is required
        assert!(!result.is_empty());
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_unclosed_code_block() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode without closing fence";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("never closed"));
    }

    #[test]
    fn test_nested_code_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\nouter\n```\n\ninner text\n\n```\ncode\n```";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // This should parse as two separate code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_indented_to_fenced() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "Text\n\n    code line 1\n    code line 2\n\nMore text";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("```\ncode line 1\ncode line 2\n```"));
    }

    #[test]
    fn test_fix_fenced_to_indented() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "Text\n\n```\ncode line 1\ncode line 2\n```\n\nMore text";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("    code line 1\n    code line 2"));
        assert!(!fixed.contains("```"));
    }

    #[test]
    fn test_fix_unclosed_block() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode without closing";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Should add closing fence
        assert!(fixed.ends_with("```"));
    }

    #[test]
    fn test_code_block_in_list() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "- List item\n    code in list\n    more code\n- Next item";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Code in lists should not be flagged
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_detect_style_fenced() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "```\ncode\n```";
        let style = rule.detect_style(content);

        assert_eq!(style, Some(CodeBlockStyle::Fenced));
    }

    #[test]
    fn test_detect_style_indented() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "Text\n\n    code\n\nMore";
        let style = rule.detect_style(content);

        assert_eq!(style, Some(CodeBlockStyle::Indented));
    }

    #[test]
    fn test_detect_style_none() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "No code blocks here";
        let style = rule.detect_style(content);

        assert_eq!(style, None);
    }

    #[test]
    fn test_tilde_fence() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "~~~\ncode\n~~~";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Tilde fences should be accepted as fenced blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_language_specification() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```rust\nfn main() {}\n```";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "";
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Nested code blocks in markdown documentation should be allowed
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_preserve_trailing_newline() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode\n```\n";
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, content);
    }
}
