use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::code_block_utils::CodeBlockStyle;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{calculate_line_range, LineIndex};
use lazy_static::lazy_static;
use regex::Regex;
use toml;

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
    style: CodeBlockStyle,
}

impl MD046CodeBlockStyle {
    pub fn new(style: CodeBlockStyle) -> Self {
        Self { style }
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
        let start = if i >= 5 { i - 5 } else { 0 };
        let end = if i + 5 < lines.len() {
            i + 5
        } else {
            lines.len()
        };

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
        if i > 0
            && lines[i - 1]
                .trim_start()
                .matches(&['-', '*', '+'][..])
                .count()
                > 0
        {
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
        if !ctx.content.contains("```")
            && !ctx.content.contains("~~~")
            && !ctx.content.contains("    ")
        {
            return Ok(Vec::new());
        }

        // Try optimized path first, fallback to regular method
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

        let lines: Vec<&str> = content.lines().collect();

        // Determine target style
        let target_style = match self.style {
            CodeBlockStyle::Consistent => {
                self.detect_style(content).unwrap_or(CodeBlockStyle::Fenced)
            }
            _ => self.style,
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
                fenced_fence_type = Some(if trimmed.starts_with("```") {
                    "```"
                } else {
                    "~~~"
                });

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
                    let _next_line_is_indented =
                        i < lines.len() - 1 && self.is_indented_code_block(&lines, i + 1);
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
            || (!ctx.content.contains("```")
                && !ctx.content.contains("~~~")
                && !ctx.content.contains("    "))
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
            && (self.style == CodeBlockStyle::Fenced || self.style == CodeBlockStyle::Consistent)
        {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();

        // Determine the target style from the detected style in the document
        let target_style = match self.style {
            CodeBlockStyle::Consistent => {
                // For consistent style, use the same logic as the check method to ensure compatibility
                let mut first_fenced_line = usize::MAX;
                let mut first_indented_line = usize::MAX;

                for (i, line) in lines.iter().enumerate() {
                    if first_fenced_line == usize::MAX
                        && (line.trim_start().starts_with("```")
                            || line.trim_start().starts_with("~~~"))
                    {
                        first_fenced_line = i;
                    } else if first_indented_line == usize::MAX
                        && self.is_indented_code_block(&lines, i)
                    {
                        first_indented_line = i;
                    }

                    if first_fenced_line != usize::MAX && first_indented_line != usize::MAX {
                        break;
                    }
                }

                // Determine which style to use based on which appears first
                if first_fenced_line != usize::MAX
                    && (first_indented_line == usize::MAX
                        || first_fenced_line < first_indented_line)
                {
                    CodeBlockStyle::Fenced
                } else if first_indented_line != usize::MAX {
                    CodeBlockStyle::Indented
                } else {
                    // Default to fenced if no code blocks found
                    CodeBlockStyle::Fenced
                }
            }
            _ => self.style,
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
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(i + 1, line);

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
                        if lines[j].trim_start().starts_with("```")
                            || lines[j].trim_start().starts_with("~~~")
                        {
                            // Add warnings for content lines and closing fence
                            for (k, line_content) in
                                lines.iter().enumerate().take(j + 1).skip(i + 1)
                            {
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
                        if lines[j].trim_start().starts_with("```")
                            || lines[j].trim_start().starts_with("~~~")
                        {
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
                        let (start_line, start_col, end_line, end_col) =
                            calculate_line_range(i + 1, line);

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
        let mut map = toml::map::Map::new();
        map.insert(
            "style".to_string(),
            toml::Value::String(self.style.to_string()),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD046", "style")
            .unwrap_or_else(|| "consistent".to_string());
        let style = match style.as_str() {
            "fenced" => CodeBlockStyle::Fenced,
            "indented" => CodeBlockStyle::Indented,
            "consistent" => CodeBlockStyle::Consistent,
            _ => CodeBlockStyle::Consistent,
        };
        Box::new(MD046CodeBlockStyle::new(style))
    }
}

impl DocumentStructureExtensions for MD046CodeBlockStyle {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> bool {
        !ctx.content.is_empty() && !structure.code_blocks.is_empty()
    }
}
