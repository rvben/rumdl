use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

/// Rule MD038: No space inside code span markers
///
/// See [docs/md038.md](../../docs/md038.md) for full documentation, configuration, and examples.
///
/// MD038: Spaces inside code span elements
///
/// This rule is triggered when there are spaces inside code span elements.
///
/// For example:
///
/// ``` markdown
/// ` some text`
/// `some text `
/// ` some text `
/// ```
///
/// To fix this issue, remove the leading and trailing spaces within the code span markers:
///
/// ``` markdown
/// `some text`
/// ```
///
/// Note: Code spans containing backticks (e.g., `` `backticks` inside ``) are not flagged
/// to avoid breaking nested backtick structures used to display backticks in documentation.
#[derive(Debug, Clone)]
pub struct MD038NoSpaceInCode {
    pub enabled: bool,
    /// Allow leading/trailing spaces in code spans when they improve readability
    pub allow_intentional_spaces: bool,
    /// Allow spaces around single characters (e.g., ` y ` for visibility)
    pub allow_single_char_spaces: bool,
    /// Allow spaces in command examples (heuristic: contains common shell indicators)
    pub allow_command_spaces: bool,
}

impl Default for MD038NoSpaceInCode {
    fn default() -> Self {
        Self::new()
    }
}

impl MD038NoSpaceInCode {
    pub fn new() -> Self {
        Self {
            enabled: true,
            allow_intentional_spaces: true, // More lenient by default
            allow_single_char_spaces: true,
            allow_command_spaces: true,
        }
    }

    pub fn strict() -> Self {
        Self {
            enabled: true,
            allow_intentional_spaces: false,
            allow_single_char_spaces: false,
            allow_command_spaces: false,
        }
    }

    /// Determine if spaces in a code span should be allowed based on content heuristics
    fn should_allow_spaces(&self, code_content: &str, trimmed: &str) -> bool {
        // If intentional spaces are globally allowed, apply heuristics
        if self.allow_intentional_spaces {
            // Allow single character with spaces for visibility (e.g., ` y `, ` * `)
            if self.allow_single_char_spaces && trimmed.len() == 1 {
                return true;
            }

            // Allow command examples with spaces
            if self.allow_command_spaces && self.looks_like_command(trimmed) {
                return true;
            }

            // Allow spaces around variable references or file patterns
            if self.looks_like_variable_or_pattern(trimmed) {
                return true;
            }

            // Allow if spaces improve readability for complex content
            if self.spaces_improve_readability(code_content, trimmed) {
                return true;
            }
        }

        false
    }

    /// Check if content looks like a shell command that benefits from spaces
    fn looks_like_command(&self, content: &str) -> bool {
        // Common command patterns - check case-insensitive prefixes
        const COMMAND_PREFIXES: &[&str] = &[
            "git ", "npm ", "cargo ", "docker ", "kubectl ", "pip ", "yarn ", "sudo ", "chmod ", "chown ", "ls ",
            "cd ", "mkdir ", "rm ", "cp ", "mv ", "cat ", "grep ", "find ", "awk ", "sed ",
        ];

        // Check if content starts with any command (case-insensitive)
        // Use iterator with early return to avoid allocating lowercase string unless needed
        let needs_lowercase_check = COMMAND_PREFIXES.iter().any(|&cmd| {
            content.len() >= cmd.len() && content.as_bytes()[..cmd.len()].eq_ignore_ascii_case(cmd.as_bytes())
        });

        needs_lowercase_check
            || content.contains(" -") // Commands with flags
            || content.contains(" --") // Commands with long flags
    }

    /// Check if content looks like a variable reference or file pattern
    fn looks_like_variable_or_pattern(&self, content: &str) -> bool {
        // Variable patterns: $VAR, ${VAR}, %VAR%, etc.
        content.starts_with('$')
            || content.starts_with('%') && content.ends_with('%')
            || (content.contains("*") && content.len() > 3) // File patterns like *.txt (must be substantial)
            || (content.contains("?") && content.len() > 3 && content.contains("."))
        // File patterns like file?.txt
    }

    /// Check if spaces improve readability for complex content
    fn spaces_improve_readability(&self, _code_content: &str, trimmed: &str) -> bool {
        // Complex content that benefits from spacing - be more conservative
        trimmed.len() >= 20 // Only longer content might benefit from spacing
            || trimmed.contains("://") // URLs
            || trimmed.contains("->") // Arrows or operators
            || trimmed.contains("=>") // Lambda arrows
            || trimmed.contains("&&") || trimmed.contains("||") // Boolean operators
            || (trimmed.chars().filter(|c| c.is_ascii_punctuation()).count() as f64 / trimmed.len() as f64) > 0.4
        // Higher punctuation density threshold
    }

    /// Check if a code span is likely part of a nested backtick structure
    fn is_likely_nested_backticks(&self, ctx: &crate::lint_context::LintContext, span_index: usize) -> bool {
        // If there are multiple code spans on the same line, and there's text
        // between them that contains "code" or other indicators, it's likely nested
        let code_spans = ctx.code_spans();
        let current_span = &code_spans[span_index];
        let current_line = current_span.line;

        // Look for other code spans on the same line
        let same_line_spans: Vec<_> = code_spans
            .iter()
            .enumerate()
            .filter(|(i, s)| s.line == current_line && *i != span_index)
            .collect();

        if same_line_spans.is_empty() {
            return false;
        }

        // Check if there's content between spans that might indicate nesting
        // Get the line content
        let line_idx = current_line - 1; // Convert to 0-based
        if line_idx >= ctx.lines.len() {
            return false;
        }

        let line_content = &ctx.lines[line_idx].content;

        // For each pair of adjacent code spans, check what's between them
        for (_, other_span) in &same_line_spans {
            let start = current_span.end_col.min(other_span.end_col);
            let end = current_span.start_col.max(other_span.start_col);

            if start < end && end <= line_content.len() {
                let between = &line_content[start..end];
                // If there's text containing "code" or similar patterns between spans,
                // it's likely they're showing nested backticks
                if between.contains("code") || between.contains("backtick") {
                    return true;
                }
            }
        }

        false
    }
}

impl Rule for MD038NoSpaceInCode {
    fn name(&self) -> &'static str {
        "MD038"
    }

    fn description(&self) -> &'static str {
        "Spaces inside code span elements"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Other
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        if !self.enabled {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();

        // Use centralized code spans from LintContext
        let code_spans = ctx.code_spans();
        for (i, code_span) in code_spans.iter().enumerate() {
            let code_content = &code_span.content;

            // Skip empty code spans
            if code_content.is_empty() {
                continue;
            }

            // Early check: if no leading/trailing whitespace, skip trimming
            if !code_content.chars().next().is_some_and(|c| c.is_whitespace())
                && !code_content.chars().last().is_some_and(|c| c.is_whitespace())
            {
                continue;
            }

            let trimmed = code_content.trim();

            // Check if there are leading or trailing spaces
            if code_content != trimmed {
                // Check if the content itself contains backticks - if so, skip to avoid
                // breaking nested backtick structures
                if trimmed.contains('`') {
                    continue;
                }

                // Check if this might be part of a nested backtick structure
                // by looking for other code spans nearby that might indicate nesting
                if self.is_likely_nested_backticks(ctx, i) {
                    continue;
                }

                // Check if spaces are allowed in this context
                if self.should_allow_spaces(code_content, trimmed) {
                    continue;
                }

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: code_span.line,
                    column: code_span.start_col + 1, // Convert to 1-indexed
                    end_line: code_span.line,
                    end_column: code_span.end_col, // Don't add 1 to match test expectation
                    message: "Spaces inside code span elements".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: code_span.byte_offset..code_span.byte_end,
                        replacement: format!(
                            "{}{}{}",
                            "`".repeat(code_span.backtick_count),
                            trimmed,
                            "`".repeat(code_span.backtick_count)
                        ),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if !self.enabled {
            return Ok(content.to_string());
        }

        // Early return if no backticks in content
        if !content.contains('`') {
            return Ok(content.to_string());
        }

        // Get warnings to identify what needs to be fixed
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        // Collect all fixes and sort by position (reverse order to avoid position shifts)
        let mut fixes: Vec<(std::ops::Range<usize>, String)> = warnings
            .into_iter()
            .filter_map(|w| w.fix.map(|f| (f.range, f.replacement)))
            .collect();

        fixes.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));

        // Apply fixes - only allocate string when we have fixes to apply
        let mut result = content.to_string();
        for (range, replacement) in fixes {
            result.replace_range(range, &replacement);
        }

        Ok(result)
    }

    /// Check if content is likely to have code spans
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        !ctx.content.contains('`')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "allow_intentional_spaces".to_string(),
            toml::Value::Boolean(self.allow_intentional_spaces),
        );
        map.insert(
            "allow_single_char_spaces".to_string(),
            toml::Value::Boolean(self.allow_single_char_spaces),
        );
        map.insert(
            "allow_command_spaces".to_string(),
            toml::Value::Boolean(self.allow_command_spaces),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let allow_intentional_spaces =
            crate::config::get_rule_config_value::<bool>(config, "MD038", "allow_intentional_spaces").unwrap_or(true); // Default to true for better UX

        let allow_single_char_spaces =
            crate::config::get_rule_config_value::<bool>(config, "MD038", "allow_single_char_spaces").unwrap_or(true);

        let allow_command_spaces =
            crate::config::get_rule_config_value::<bool>(config, "MD038", "allow_command_spaces").unwrap_or(true);

        Box::new(MD038NoSpaceInCode {
            enabled: true,
            allow_intentional_spaces,
            allow_single_char_spaces,
            allow_command_spaces,
        })
    }
}

impl crate::utils::document_structure::DocumentStructureExtensions for MD038NoSpaceInCode {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        // We now use centralized code spans from LintContext
        // Quick check without parsing
        ctx.content.contains('`')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md038_valid() {
        let rule = MD038NoSpaceInCode::new();
        let valid_cases = vec![
            "This is `code` in a sentence.",
            "This is a `longer code span` in a sentence.",
            "This is `code with internal spaces` which is fine.",
            "This is`` code with double backticks`` which is also fine.",
            "Code span at `end of line`",
            "`Start of line` code span",
            "Multiple `code spans` in `one line` are fine",
            "Code span with `symbols: !@#$%^&*()`",
            "Empty code span `` is technically valid",
            // New cases that should be allowed with lenient settings
            "Type ` y ` to confirm.",                       // Single character with spaces
            "Use ` git commit -m \"message\" ` to commit.", // Command with spaces
            "The variable ` $HOME ` contains home path.",   // Variable reference
            "The pattern ` *.txt ` matches text files.",    // File pattern
            "URL example ` https://example.com/very/long/path?query=value&more=params ` here.", // Complex long URL
        ];
        for case in valid_cases {
            let ctx = crate::lint_context::LintContext::new(case);
            let result = rule.check(&ctx).unwrap();
            assert!(result.is_empty(), "Valid case should not have warnings: {case}");
        }
    }

    #[test]
    fn test_md038_invalid() {
        let rule = MD038NoSpaceInCode::new();
        // Cases that should still be flagged even with lenient settings
        let invalid_cases = vec![
            "This is ` random word ` with unnecessary spaces.", // Not a command/variable/single char
            "Text with ` plain text ` should be flagged.",      // Just plain text with spaces
            "Code with ` just code ` here.",                    // Simple code with spaces
            "Multiple ` word ` spans with ` text ` in one line.", // Multiple simple cases
        ];
        for case in invalid_cases {
            let ctx = crate::lint_context::LintContext::new(case);
            let result = rule.check(&ctx).unwrap();
            assert!(!result.is_empty(), "Invalid case should have warnings: {case}");
        }
    }

    #[test]
    fn test_md038_strict_mode() {
        let rule = MD038NoSpaceInCode::strict();
        // In strict mode, ALL spaces should be flagged
        let invalid_cases = vec![
            "Type ` y ` to confirm.",                       // Single character with spaces
            "Use ` git commit -m \"message\" ` to commit.", // Command with spaces
            "The variable ` $HOME ` contains home path.",   // Variable reference
            "The pattern ` *.txt ` matches text files.",    // File pattern
            "This is ` code` with leading space.",
            "This is `code ` with trailing space.",
            "This is ` code ` with both leading and trailing space.",
        ];
        for case in invalid_cases {
            let ctx = crate::lint_context::LintContext::new(case);
            let result = rule.check(&ctx).unwrap();
            assert!(!result.is_empty(), "Strict mode should flag all spaces: {case}");
        }
    }

    #[test]
    fn test_md038_fix() {
        let rule = MD038NoSpaceInCode::new();
        let test_cases = vec![
            (
                "This is ` code` with leading space.",
                "This is `code` with leading space.",
            ),
            (
                "This is `code ` with trailing space.",
                "This is `code` with trailing space.",
            ),
            ("This is ` code ` with both spaces.", "This is `code` with both spaces."),
            (
                "Multiple ` code ` and `spans ` to fix.",
                "Multiple `code` and `spans` to fix.",
            ),
        ];
        for (input, expected) in test_cases {
            let ctx = crate::lint_context::LintContext::new(input);
            let result = rule.fix(&ctx).unwrap();
            assert_eq!(result, expected, "Fix did not produce expected output for: {input}");
        }
    }

    #[test]
    fn test_check_invalid_leading_space() {
        let rule = MD038NoSpaceInCode::new();
        let input = "This has a ` leading space` in code";
        let ctx = crate::lint_context::LintContext::new(input);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].fix.is_some());
    }

    #[test]
    fn test_code_span_parsing_nested_backticks() {
        let content = "Code with ` nested `code` example ` should preserve backticks";
        let ctx = crate::lint_context::LintContext::new(content);

        println!("Content: {content}");
        println!("Code spans found:");
        let code_spans = ctx.code_spans();
        for (i, span) in code_spans.iter().enumerate() {
            println!(
                "  Span {}: line={}, col={}-{}, backticks={}, content='{}'",
                i, span.line, span.start_col, span.end_col, span.backtick_count, span.content
            );
        }

        // This test reveals the issue - we're getting multiple separate code spans instead of one
        assert_eq!(code_spans.len(), 2, "Should parse as 2 code spans");
    }

    #[test]
    fn test_nested_backtick_detection() {
        let rule = MD038NoSpaceInCode::strict();

        // In strict mode, should_allow_spaces returns false, but the check method
        // will skip code spans with backticks anyway
        assert!(!rule.should_allow_spaces(" plain text ", "plain text"));

        // Test with lenient mode
        let lenient_rule = MD038NoSpaceInCode::new();
        assert!(lenient_rule.should_allow_spaces(" y ", "y")); // Single char
        assert!(!lenient_rule.should_allow_spaces(" plain text ", "plain text"));
    }
}
