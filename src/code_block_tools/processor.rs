//! Main processor for code block linting and formatting.
//!
//! This module coordinates language resolution, tool lookup, execution,
//! and result collection for processing code blocks in markdown files.

#[cfg(test)]
use super::config::LanguageToolConfig;
use super::config::{CodeBlockToolsConfig, NormalizeLanguage, OnError, OnMissing};
use super::executor::{ExecutorError, ToolExecutor, ToolOutput};
use super::linguist::LinguistResolver;
use super::registry::ToolRegistry;
use crate::rule::{LintWarning, Severity};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Special built-in tool name for rumdl's own markdown linting.
/// When this tool is configured for markdown blocks, the processor skips
/// external execution since it's handled by embedded markdown linting.
pub const RUMDL_BUILTIN_TOOL: &str = "rumdl";

/// Check if a language is markdown (handles common variations).
fn is_markdown_language(lang: &str) -> bool {
    matches!(lang.to_lowercase().as_str(), "markdown" | "md")
}

/// Information about a fenced code block for processing.
#[derive(Debug, Clone)]
pub struct FencedCodeBlockInfo {
    /// 0-indexed line number where opening fence starts.
    pub start_line: usize,
    /// 0-indexed line number where closing fence ends.
    pub end_line: usize,
    /// Byte offset where code content starts (after opening fence line).
    pub content_start: usize,
    /// Byte offset where code content ends (before closing fence line).
    pub content_end: usize,
    /// Language tag extracted from info string (first token).
    pub language: String,
    /// Full info string from the fence.
    pub info_string: String,
    /// The fence character used (` or ~).
    pub fence_char: char,
    /// Length of the fence (3 or more).
    pub fence_length: usize,
    /// Leading whitespace on the fence line.
    pub indent: usize,
    /// Exact leading whitespace prefix from the fence line.
    pub indent_prefix: String,
}

/// A diagnostic message from an external tool.
#[derive(Debug, Clone)]
pub struct CodeBlockDiagnostic {
    /// Line number in the original markdown file (1-indexed).
    pub file_line: usize,
    /// Column number (1-indexed, if available).
    pub column: Option<usize>,
    /// Message from the tool.
    pub message: String,
    /// Severity (error, warning, info).
    pub severity: DiagnosticSeverity,
    /// Name of the tool that produced this.
    pub tool: String,
    /// Line where the code block starts (1-indexed, for context).
    pub code_block_start: usize,
}

/// Severity level for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

impl CodeBlockDiagnostic {
    /// Convert to a LintWarning for integration with rumdl's warning system.
    pub fn to_lint_warning(&self) -> LintWarning {
        let severity = match self.severity {
            DiagnosticSeverity::Error => Severity::Error,
            DiagnosticSeverity::Warning => Severity::Warning,
            DiagnosticSeverity::Info => Severity::Info,
        };

        LintWarning {
            message: self.message.clone(),
            line: self.file_line,
            column: self.column.unwrap_or(1),
            end_line: self.file_line,
            end_column: self.column.unwrap_or(1),
            severity,
            fix: None, // External tool diagnostics don't provide fixes
            rule_name: Some(self.tool.clone()),
        }
    }
}

/// Error during code block processing.
#[derive(Debug, Clone)]
pub enum ProcessorError {
    /// Tool execution failed.
    ToolError(ExecutorError),
    /// No tools configured for language.
    NoToolsConfigured { language: String },
    /// Tool binary not found.
    ToolBinaryNotFound { tool: String, language: String },
    /// Processing was aborted due to on_error = fail.
    Aborted { message: String },
}

impl std::fmt::Display for ProcessorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolError(e) => write!(f, "{e}"),
            Self::NoToolsConfigured { language } => {
                write!(f, "No tools configured for language '{language}'")
            }
            Self::ToolBinaryNotFound { tool, language } => {
                write!(f, "Tool '{tool}' binary not found for language '{language}'")
            }
            Self::Aborted { message } => write!(f, "Processing aborted: {message}"),
        }
    }
}

impl std::error::Error for ProcessorError {}

impl From<ExecutorError> for ProcessorError {
    fn from(e: ExecutorError) -> Self {
        Self::ToolError(e)
    }
}

/// Result of processing a single code block.
#[derive(Debug)]
pub struct CodeBlockResult {
    /// Diagnostics from linting.
    pub diagnostics: Vec<CodeBlockDiagnostic>,
    /// Formatted content (if formatting was requested and succeeded).
    pub formatted_content: Option<String>,
    /// Whether the code block was modified.
    pub was_modified: bool,
}

/// Result of formatting code blocks in a document.
#[derive(Debug)]
pub struct FormatOutput {
    /// The formatted content (may be partially formatted if errors occurred).
    pub content: String,
    /// Whether any errors occurred during formatting.
    pub had_errors: bool,
    /// Error messages for blocks that couldn't be formatted.
    pub error_messages: Vec<String>,
}

/// Main processor for code block tools.
pub struct CodeBlockToolProcessor<'a> {
    config: &'a CodeBlockToolsConfig,
    linguist: LinguistResolver,
    registry: ToolRegistry,
    executor: ToolExecutor,
    user_aliases: std::collections::HashMap<String, String>,
}

impl<'a> CodeBlockToolProcessor<'a> {
    /// Create a new processor with the given configuration.
    pub fn new(config: &'a CodeBlockToolsConfig) -> Self {
        let user_aliases = config
            .language_aliases
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v.to_lowercase()))
            .collect();
        Self {
            config,
            linguist: LinguistResolver::new(),
            registry: ToolRegistry::new(config.tools.clone()),
            executor: ToolExecutor::new(config.timeout),
            user_aliases,
        }
    }

    /// Extract all fenced code blocks from content.
    pub fn extract_code_blocks(&self, content: &str) -> Vec<FencedCodeBlockInfo> {
        let mut blocks = Vec::new();
        let mut current_block: Option<FencedCodeBlockBuilder> = None;

        let options = Options::all();
        let parser = Parser::new_ext(content, options).into_offset_iter();

        let lines: Vec<&str> = content.lines().collect();

        for (event, range) in parser {
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                    let info_string = info.to_string();
                    let language = info_string.split_whitespace().next().unwrap_or("").to_string();

                    // Find start line
                    let start_line = content[..range.start].chars().filter(|&c| c == '\n').count();

                    // Find content start (after opening fence line)
                    let content_start = content[range.start..]
                        .find('\n')
                        .map(|i| range.start + i + 1)
                        .unwrap_or(content.len());

                    // Detect fence character and length from the line
                    let fence_line = lines.get(start_line).unwrap_or(&"");
                    let trimmed = fence_line.trim_start();
                    let indent = fence_line.len() - trimmed.len();
                    let indent_prefix = fence_line.get(..indent).unwrap_or("").to_string();
                    let (fence_char, fence_length) = if trimmed.starts_with('~') {
                        ('~', trimmed.chars().take_while(|&c| c == '~').count())
                    } else {
                        ('`', trimmed.chars().take_while(|&c| c == '`').count())
                    };

                    current_block = Some(FencedCodeBlockBuilder {
                        start_line,
                        content_start,
                        language,
                        info_string,
                        fence_char,
                        fence_length,
                        indent,
                        indent_prefix,
                    });
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(builder) = current_block.take() {
                        // Find end line
                        let end_line = content[..range.end].chars().filter(|&c| c == '\n').count();

                        // Find content end (before closing fence line)
                        let search_start = builder.content_start.min(range.end);
                        let content_end = if search_start < range.end {
                            content[search_start..range.end]
                                .rfind('\n')
                                .map(|i| search_start + i)
                                .unwrap_or(search_start)
                        } else {
                            search_start
                        };

                        if content_end >= builder.content_start {
                            blocks.push(FencedCodeBlockInfo {
                                start_line: builder.start_line,
                                end_line,
                                content_start: builder.content_start,
                                content_end,
                                language: builder.language,
                                info_string: builder.info_string,
                                fence_char: builder.fence_char,
                                fence_length: builder.fence_length,
                                indent: builder.indent,
                                indent_prefix: builder.indent_prefix,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        blocks
    }

    /// Resolve a language tag to its canonical name.
    fn resolve_language(&self, language: &str) -> String {
        let lower = language.to_lowercase();
        if let Some(mapped) = self.user_aliases.get(&lower) {
            return mapped.clone();
        }
        match self.config.normalize_language {
            NormalizeLanguage::Linguist => self.linguist.resolve(&lower),
            NormalizeLanguage::Exact => lower,
        }
    }

    /// Get the effective on_error setting for a language.
    fn get_on_error(&self, language: &str) -> OnError {
        self.config
            .languages
            .get(language)
            .and_then(|lc| lc.on_error)
            .unwrap_or(self.config.on_error)
    }

    /// Strip the fence indentation prefix from each line of a code block.
    fn strip_indent_from_block(&self, content: &str, indent_prefix: &str) -> String {
        if indent_prefix.is_empty() {
            return content.to_string();
        }

        let mut out = String::with_capacity(content.len());
        for line in content.split_inclusive('\n') {
            if let Some(stripped) = line.strip_prefix(indent_prefix) {
                out.push_str(stripped);
            } else {
                out.push_str(line);
            }
        }
        out
    }

    /// Re-apply the fence indentation prefix to each line of a code block.
    fn apply_indent_to_block(&self, content: &str, indent_prefix: &str) -> String {
        if indent_prefix.is_empty() {
            return content.to_string();
        }
        if content.is_empty() {
            return String::new();
        }

        let mut out = String::with_capacity(content.len() + indent_prefix.len());
        for line in content.split_inclusive('\n') {
            if line == "\n" {
                out.push_str(line);
            } else {
                out.push_str(indent_prefix);
                out.push_str(line);
            }
        }
        out
    }

    /// Lint all code blocks in the content.
    ///
    /// Returns diagnostics from all configured linters.
    pub fn lint(&self, content: &str) -> Result<Vec<CodeBlockDiagnostic>, ProcessorError> {
        let mut all_diagnostics = Vec::new();
        let blocks = self.extract_code_blocks(content);

        for block in blocks {
            if block.language.is_empty() {
                continue; // Skip blocks without language tag
            }

            let canonical_lang = self.resolve_language(&block.language);

            // Get lint tools for this language
            let lint_tools = match self.config.languages.get(&canonical_lang) {
                Some(lc) if !lc.lint.is_empty() => &lc.lint,
                _ => {
                    // No tools configured for this language in lint mode
                    match self.config.on_missing_language_definition {
                        OnMissing::Ignore => continue,
                        OnMissing::Fail => {
                            all_diagnostics.push(CodeBlockDiagnostic {
                                file_line: block.start_line + 1,
                                column: None,
                                message: format!("No lint tools configured for language '{canonical_lang}'"),
                                severity: DiagnosticSeverity::Error,
                                tool: "code-block-tools".to_string(),
                                code_block_start: block.start_line + 1,
                            });
                            continue;
                        }
                        OnMissing::FailFast => {
                            return Err(ProcessorError::NoToolsConfigured {
                                language: canonical_lang,
                            });
                        }
                    }
                }
            };

            // Extract code block content
            let code_content_raw = if block.content_start < block.content_end && block.content_end <= content.len() {
                &content[block.content_start..block.content_end]
            } else {
                continue;
            };
            let code_content = self.strip_indent_from_block(code_content_raw, &block.indent_prefix);

            // Run each lint tool
            for tool_id in lint_tools {
                // Skip built-in "rumdl" tool for markdown - handled separately by embedded markdown linting
                if tool_id == RUMDL_BUILTIN_TOOL && is_markdown_language(&canonical_lang) {
                    continue;
                }

                let tool_def = match self.registry.get(tool_id) {
                    Some(t) => t,
                    None => {
                        log::warn!("Unknown tool '{tool_id}' configured for language '{canonical_lang}'");
                        continue;
                    }
                };

                // Check if tool binary exists before running
                let tool_name = tool_def.command.first().map(String::as_str).unwrap_or("");
                if !tool_name.is_empty() && !self.executor.is_tool_available(tool_name) {
                    match self.config.on_missing_tool_binary {
                        OnMissing::Ignore => {
                            log::debug!("Tool binary '{tool_name}' not found, skipping");
                            continue;
                        }
                        OnMissing::Fail => {
                            all_diagnostics.push(CodeBlockDiagnostic {
                                file_line: block.start_line + 1,
                                column: None,
                                message: format!("Tool binary '{tool_name}' not found in PATH"),
                                severity: DiagnosticSeverity::Error,
                                tool: "code-block-tools".to_string(),
                                code_block_start: block.start_line + 1,
                            });
                            continue;
                        }
                        OnMissing::FailFast => {
                            return Err(ProcessorError::ToolBinaryNotFound {
                                tool: tool_name.to_string(),
                                language: canonical_lang.clone(),
                            });
                        }
                    }
                }

                match self.executor.lint(tool_def, &code_content, Some(self.config.timeout)) {
                    Ok(output) => {
                        // Parse tool output into diagnostics
                        let diagnostics = self.parse_tool_output(
                            &output,
                            tool_id,
                            block.start_line + 1, // Convert to 1-indexed
                        );
                        all_diagnostics.extend(diagnostics);
                    }
                    Err(e) => {
                        let on_error = self.get_on_error(&canonical_lang);
                        match on_error {
                            OnError::Fail => return Err(e.into()),
                            OnError::Warn => {
                                log::warn!("Tool '{tool_id}' failed: {e}");
                            }
                            OnError::Skip => {
                                // Silently skip
                            }
                        }
                    }
                }
            }
        }

        Ok(all_diagnostics)
    }

    /// Format all code blocks in the content.
    ///
    /// Returns the modified content with formatted code blocks and any errors that occurred.
    /// With `on-missing-*` = `fail`, errors are collected but formatting continues.
    /// With `on-missing-*` = `fail-fast`, returns Err immediately on first error.
    pub fn format(&self, content: &str) -> Result<FormatOutput, ProcessorError> {
        let blocks = self.extract_code_blocks(content);

        if blocks.is_empty() {
            return Ok(FormatOutput {
                content: content.to_string(),
                had_errors: false,
                error_messages: Vec::new(),
            });
        }

        // Process blocks in reverse order to maintain byte offsets
        let mut result = content.to_string();
        let mut error_messages: Vec<String> = Vec::new();

        for block in blocks.into_iter().rev() {
            if block.language.is_empty() {
                continue;
            }

            let canonical_lang = self.resolve_language(&block.language);

            // Get format tools for this language
            let format_tools = match self.config.languages.get(&canonical_lang) {
                Some(lc) if !lc.format.is_empty() => &lc.format,
                _ => {
                    // No tools configured for this language in format mode
                    match self.config.on_missing_language_definition {
                        OnMissing::Ignore => continue,
                        OnMissing::Fail => {
                            error_messages.push(format!(
                                "No format tools configured for language '{canonical_lang}' at line {}",
                                block.start_line + 1
                            ));
                            continue;
                        }
                        OnMissing::FailFast => {
                            return Err(ProcessorError::NoToolsConfigured {
                                language: canonical_lang,
                            });
                        }
                    }
                }
            };

            // Extract code block content
            if block.content_start >= block.content_end || block.content_end > result.len() {
                continue;
            }
            let code_content_raw = result[block.content_start..block.content_end].to_string();
            let code_content = self.strip_indent_from_block(&code_content_raw, &block.indent_prefix);

            // Run format tools (use first successful one)
            let mut formatted = code_content.clone();
            let mut tool_ran = false;
            for tool_id in format_tools {
                // Skip built-in "rumdl" tool for markdown - handled separately by embedded markdown formatting
                if tool_id == RUMDL_BUILTIN_TOOL && is_markdown_language(&canonical_lang) {
                    continue;
                }

                let tool_def = match self.registry.get(tool_id) {
                    Some(t) => t,
                    None => {
                        log::warn!("Unknown tool '{tool_id}' configured for language '{canonical_lang}'");
                        continue;
                    }
                };

                // Check if tool binary exists before running
                let tool_name = tool_def.command.first().map(String::as_str).unwrap_or("");
                if !tool_name.is_empty() && !self.executor.is_tool_available(tool_name) {
                    match self.config.on_missing_tool_binary {
                        OnMissing::Ignore => {
                            log::debug!("Tool binary '{tool_name}' not found, skipping");
                            continue;
                        }
                        OnMissing::Fail => {
                            error_messages.push(format!(
                                "Tool binary '{tool_name}' not found in PATH for language '{canonical_lang}' at line {}",
                                block.start_line + 1
                            ));
                            continue;
                        }
                        OnMissing::FailFast => {
                            return Err(ProcessorError::ToolBinaryNotFound {
                                tool: tool_name.to_string(),
                                language: canonical_lang.clone(),
                            });
                        }
                    }
                }

                match self.executor.format(tool_def, &formatted, Some(self.config.timeout)) {
                    Ok(output) => {
                        // Ensure trailing newline matches original (unindented)
                        formatted = output;
                        if code_content.ends_with('\n') && !formatted.ends_with('\n') {
                            formatted.push('\n');
                        } else if !code_content.ends_with('\n') && formatted.ends_with('\n') {
                            formatted.pop();
                        }
                        tool_ran = true;
                        break; // Use first successful formatter
                    }
                    Err(e) => {
                        let on_error = self.get_on_error(&canonical_lang);
                        match on_error {
                            OnError::Fail => return Err(e.into()),
                            OnError::Warn => {
                                log::warn!("Formatter '{tool_id}' failed: {e}");
                            }
                            OnError::Skip => {}
                        }
                    }
                }
            }

            // Replace content if changed and a tool actually ran
            if tool_ran && formatted != code_content {
                let reindented = self.apply_indent_to_block(&formatted, &block.indent_prefix);
                if reindented != code_content_raw {
                    result.replace_range(block.content_start..block.content_end, &reindented);
                }
            }
        }

        Ok(FormatOutput {
            content: result,
            had_errors: !error_messages.is_empty(),
            error_messages,
        })
    }

    /// Parse tool output into diagnostics.
    ///
    /// This is a basic parser that handles common output formats.
    /// Tools vary widely in their output format, so this is best-effort.
    fn parse_tool_output(
        &self,
        output: &ToolOutput,
        tool_id: &str,
        code_block_start_line: usize,
    ) -> Vec<CodeBlockDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut shellcheck_line: Option<usize> = None;

        // Combine stdout and stderr for parsing
        let stdout = &output.stdout;
        let stderr = &output.stderr;
        let combined = format!("{stdout}\n{stderr}");

        // Look for common line:column:message patterns
        // Examples:
        // - ruff: "_.py:1:1: E501 Line too long"
        // - shellcheck: "In - line 1: ..."
        // - eslint: "1:10 error Description"

        for line in combined.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(line_num) = self.parse_shellcheck_header(line) {
                shellcheck_line = Some(line_num);
                continue;
            }

            if let Some(line_num) = shellcheck_line
                && let Some(diag) = self.parse_shellcheck_message(line, tool_id, code_block_start_line, line_num)
            {
                diagnostics.push(diag);
                continue;
            }

            // Try pattern: "file:line:col: message" or "file:line: message"
            if let Some(diag) = self.parse_standard_format(line, tool_id, code_block_start_line) {
                diagnostics.push(diag);
                continue;
            }

            // Try pattern: "line:col message" (eslint style)
            if let Some(diag) = self.parse_eslint_format(line, tool_id, code_block_start_line) {
                diagnostics.push(diag);
                continue;
            }

            // Try single-line shellcheck format fallback
            if let Some(diag) = self.parse_shellcheck_format(line, tool_id, code_block_start_line) {
                diagnostics.push(diag);
            }
        }

        // If no diagnostics parsed but tool failed, create a generic one
        if diagnostics.is_empty() && !output.success {
            let message = if !output.stderr.is_empty() {
                output.stderr.lines().next().unwrap_or("Tool failed").to_string()
            } else if !output.stdout.is_empty() {
                output.stdout.lines().next().unwrap_or("Tool failed").to_string()
            } else {
                let exit_code = output.exit_code;
                format!("Tool exited with code {exit_code}")
            };

            diagnostics.push(CodeBlockDiagnostic {
                file_line: code_block_start_line,
                column: None,
                message,
                severity: DiagnosticSeverity::Error,
                tool: tool_id.to_string(),
                code_block_start: code_block_start_line,
            });
        }

        diagnostics
    }

    /// Parse standard "file:line:col: message" format.
    fn parse_standard_format(
        &self,
        line: &str,
        tool_id: &str,
        code_block_start_line: usize,
    ) -> Option<CodeBlockDiagnostic> {
        // Match patterns like "file.py:1:10: E501 message"
        let mut parts = line.rsplitn(4, ':');
        let message = parts.next()?.trim().to_string();
        let part1 = parts.next()?.trim().to_string();
        let part2 = parts.next()?.trim().to_string();
        let part3 = parts.next().map(|s| s.trim().to_string());

        let (line_part, col_part) = if part3.is_some() {
            (part2, Some(part1))
        } else {
            (part1, None)
        };

        if let Ok(line_num) = line_part.parse::<usize>() {
            let column = col_part.and_then(|s| s.parse::<usize>().ok());
            let message = Self::strip_fixable_markers(&message);
            if !message.is_empty() {
                let severity = self.infer_severity(&message);
                return Some(CodeBlockDiagnostic {
                    file_line: code_block_start_line + line_num,
                    column,
                    message,
                    severity,
                    tool: tool_id.to_string(),
                    code_block_start: code_block_start_line,
                });
            }
        }
        None
    }

    /// Parse eslint-style "line:col severity message" format.
    fn parse_eslint_format(
        &self,
        line: &str,
        tool_id: &str,
        code_block_start_line: usize,
    ) -> Option<CodeBlockDiagnostic> {
        // Match "1:10 error Message"
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        if parts.len() >= 2 {
            let loc_parts: Vec<&str> = parts[0].split(':').collect();
            if loc_parts.len() == 2
                && let (Ok(line_num), Ok(col)) = (loc_parts[0].parse::<usize>(), loc_parts[1].parse::<usize>())
            {
                let (sev_part, msg_part) = if parts.len() >= 3 {
                    (parts[1], parts[2])
                } else {
                    (parts[1], "")
                };
                let message = if msg_part.is_empty() {
                    sev_part.to_string()
                } else {
                    msg_part.to_string()
                };
                let message = Self::strip_fixable_markers(&message);
                let severity = match sev_part.to_lowercase().as_str() {
                    "error" => DiagnosticSeverity::Error,
                    "warning" | "warn" => DiagnosticSeverity::Warning,
                    "info" => DiagnosticSeverity::Info,
                    _ => self.infer_severity(&message),
                };
                return Some(CodeBlockDiagnostic {
                    file_line: code_block_start_line + line_num,
                    column: Some(col),
                    message,
                    severity,
                    tool: tool_id.to_string(),
                    code_block_start: code_block_start_line,
                });
            }
        }
        None
    }

    /// Parse shellcheck-style "In - line N: message" format.
    fn parse_shellcheck_format(
        &self,
        line: &str,
        tool_id: &str,
        code_block_start_line: usize,
    ) -> Option<CodeBlockDiagnostic> {
        // Match "In - line 5:" pattern
        if line.starts_with("In ")
            && line.contains(" line ")
            && let Some(line_start) = line.find(" line ")
        {
            let after_line = &line[line_start + 6..];
            if let Some(colon_pos) = after_line.find(':')
                && let Ok(line_num) = after_line[..colon_pos].trim().parse::<usize>()
            {
                let message = Self::strip_fixable_markers(after_line[colon_pos + 1..].trim());
                if !message.is_empty() {
                    let severity = self.infer_severity(&message);
                    return Some(CodeBlockDiagnostic {
                        file_line: code_block_start_line + line_num,
                        column: None,
                        message,
                        severity,
                        tool: tool_id.to_string(),
                        code_block_start: code_block_start_line,
                    });
                }
            }
        }
        None
    }

    /// Parse shellcheck header line to capture line number context.
    fn parse_shellcheck_header(&self, line: &str) -> Option<usize> {
        if line.starts_with("In ")
            && line.contains(" line ")
            && let Some(line_start) = line.find(" line ")
        {
            let after_line = &line[line_start + 6..];
            if let Some(colon_pos) = after_line.find(':') {
                return after_line[..colon_pos].trim().parse::<usize>().ok();
            }
        }
        None
    }

    /// Parse shellcheck message line containing SCXXXX codes.
    fn parse_shellcheck_message(
        &self,
        line: &str,
        tool_id: &str,
        code_block_start_line: usize,
        line_num: usize,
    ) -> Option<CodeBlockDiagnostic> {
        let sc_pos = line.find("SC")?;
        let after_sc = &line[sc_pos + 2..];
        let code_len = after_sc.chars().take_while(|c| c.is_ascii_digit()).count();
        if code_len == 0 {
            return None;
        }
        let after_code = &after_sc[code_len..];
        let sev_start = after_code.find('(')? + 1;
        let sev_end = after_code[sev_start..].find(')')? + sev_start;
        let sev = after_code[sev_start..sev_end].trim().to_lowercase();
        let message_start = after_code.find("):")? + 2;
        let message = Self::strip_fixable_markers(after_code[message_start..].trim());
        if message.is_empty() {
            return None;
        }

        let severity = match sev.as_str() {
            "error" => DiagnosticSeverity::Error,
            "warning" | "warn" => DiagnosticSeverity::Warning,
            "info" | "style" => DiagnosticSeverity::Info,
            _ => self.infer_severity(&message),
        };

        Some(CodeBlockDiagnostic {
            file_line: code_block_start_line + line_num,
            column: None,
            message,
            severity,
            tool: tool_id.to_string(),
            code_block_start: code_block_start_line,
        })
    }

    /// Infer severity from message content.
    fn infer_severity(&self, message: &str) -> DiagnosticSeverity {
        let lower = message.to_lowercase();
        if lower.contains("error")
            || lower.starts_with("e") && lower.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
            || lower.starts_with("f") && lower.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
        {
            DiagnosticSeverity::Error
        } else if lower.contains("warning")
            || lower.contains("warn")
            || lower.starts_with("w") && lower.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
        {
            DiagnosticSeverity::Warning
        } else {
            DiagnosticSeverity::Info
        }
    }

    /// Strip "fixable" markers from external tool messages.
    ///
    /// External tools like ruff show `[*]` to indicate fixable issues, but in rumdl's
    /// context these markers can be misleading - the lint tool's fix capability may
    /// differ from what our configured formatter can fix. We strip these markers
    /// to avoid making promises we can't keep.
    fn strip_fixable_markers(message: &str) -> String {
        message
            .replace(" [*]", "")
            .replace("[*] ", "")
            .replace("[*]", "")
            .replace(" (fixable)", "")
            .replace("(fixable) ", "")
            .replace("(fixable)", "")
            .replace(" [fix available]", "")
            .replace("[fix available] ", "")
            .replace("[fix available]", "")
            .replace(" [autofix]", "")
            .replace("[autofix] ", "")
            .replace("[autofix]", "")
            .trim()
            .to_string()
    }
}

/// Builder for FencedCodeBlockInfo during parsing.
struct FencedCodeBlockBuilder {
    start_line: usize,
    content_start: usize,
    language: String,
    info_string: String,
    fence_char: char,
    fence_length: usize,
    indent: usize,
    indent_prefix: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> CodeBlockToolsConfig {
        CodeBlockToolsConfig::default()
    }

    #[test]
    fn test_extract_code_blocks() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        let content = r#"# Example

```python
def hello():
    print("Hello")
```

Some text

```rust
fn main() {}
```
"#;

        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 2);

        assert_eq!(blocks[0].language, "python");
        assert_eq!(blocks[0].fence_char, '`');
        assert_eq!(blocks[0].fence_length, 3);
        assert_eq!(blocks[0].start_line, 2);
        assert_eq!(blocks[0].indent, 0);
        assert_eq!(blocks[0].indent_prefix, "");

        assert_eq!(blocks[1].language, "rust");
        assert_eq!(blocks[1].fence_char, '`');
        assert_eq!(blocks[1].fence_length, 3);
    }

    #[test]
    fn test_extract_code_blocks_with_info_string() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```python title=\"example.py\"\ncode\n```";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, "python");
        assert_eq!(blocks[0].info_string, "python title=\"example.py\"");
    }

    #[test]
    fn test_extract_code_blocks_tilde_fence() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "~~~bash\necho hello\n~~~";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, "bash");
        assert_eq!(blocks[0].fence_char, '~');
        assert_eq!(blocks[0].fence_length, 3);
        assert_eq!(blocks[0].indent_prefix, "");
    }

    #[test]
    fn test_extract_code_blocks_with_indent_prefix() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "  - item\n    ```python\n    print('hi')\n    ```";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].indent_prefix, "    ");
    }

    #[test]
    fn test_extract_code_blocks_no_language() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```\nplain code\n```";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, "");
    }

    #[test]
    fn test_resolve_language_linguist() {
        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Linguist;
        let processor = CodeBlockToolProcessor::new(&config);

        assert_eq!(processor.resolve_language("py"), "python");
        assert_eq!(processor.resolve_language("bash"), "shell");
        assert_eq!(processor.resolve_language("js"), "javascript");
    }

    #[test]
    fn test_resolve_language_exact() {
        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        let processor = CodeBlockToolProcessor::new(&config);

        assert_eq!(processor.resolve_language("py"), "py");
        assert_eq!(processor.resolve_language("BASH"), "bash");
    }

    #[test]
    fn test_resolve_language_user_alias_override() {
        let mut config = default_config();
        config.language_aliases.insert("py".to_string(), "python".to_string());
        config.normalize_language = NormalizeLanguage::Exact;
        let processor = CodeBlockToolProcessor::new(&config);

        assert_eq!(processor.resolve_language("PY"), "python");
    }

    #[test]
    fn test_indent_strip_and_reapply_roundtrip() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        let raw = "    def hello():\n        print('hi')";
        let stripped = processor.strip_indent_from_block(raw, "    ");
        assert_eq!(stripped, "def hello():\n    print('hi')");

        let reapplied = processor.apply_indent_to_block(&stripped, "    ");
        assert_eq!(reapplied, raw);
    }

    #[test]
    fn test_infer_severity() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        assert_eq!(
            processor.infer_severity("E501 line too long"),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            processor.infer_severity("W291 trailing whitespace"),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            processor.infer_severity("error: something failed"),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            processor.infer_severity("warning: unused variable"),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            processor.infer_severity("note: consider using"),
            DiagnosticSeverity::Info
        );
    }

    #[test]
    fn test_parse_standard_format_windows_path() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        let output = ToolOutput {
            stdout: "C:\\path\\file.py:2:5: E123 message".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "ruff:check", 10);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].file_line, 12);
        assert_eq!(diags[0].column, Some(5));
        assert_eq!(diags[0].message, "E123 message");
    }

    #[test]
    fn test_parse_eslint_severity() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        let output = ToolOutput {
            stdout: "1:2 error Unexpected token".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "eslint", 5);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].file_line, 6);
        assert_eq!(diags[0].column, Some(2));
        assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
        assert_eq!(diags[0].message, "Unexpected token");
    }

    #[test]
    fn test_parse_shellcheck_multiline() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        let output = ToolOutput {
            stdout: "In - line 3:\necho $var\n ^-- SC2086 (info): Double quote to prevent globbing".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "shellcheck", 10);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].file_line, 13);
        assert_eq!(diags[0].severity, DiagnosticSeverity::Info);
        assert_eq!(diags[0].message, "Double quote to prevent globbing");
    }

    #[test]
    fn test_lint_no_config() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```python\nprint('hello')\n```";
        let result = processor.lint(content);

        // Should succeed with no diagnostics (no tools configured)
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_format_no_config() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```python\nprint('hello')\n```";
        let result = processor.format(content);

        // Should succeed with unchanged content (no tools configured)
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.content, content);
        assert!(!output.had_errors);
        assert!(output.error_messages.is_empty());
    }

    #[test]
    fn test_lint_on_missing_language_definition_fail() {
        let mut config = default_config();
        config.on_missing_language_definition = OnMissing::Fail;
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```python\nprint('hello')\n```\n\n```javascript\nconsole.log('hi');\n```";
        let result = processor.lint(content);

        // Should succeed but return diagnostics for both missing language definitions
        assert!(result.is_ok());
        let diagnostics = result.unwrap();
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("No lint tools configured"));
        assert!(diagnostics[0].message.contains("python"));
        assert!(diagnostics[1].message.contains("javascript"));
    }

    #[test]
    fn test_lint_on_missing_language_definition_fail_fast() {
        let mut config = default_config();
        config.on_missing_language_definition = OnMissing::FailFast;
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```python\nprint('hello')\n```\n\n```javascript\nconsole.log('hi');\n```";
        let result = processor.lint(content);

        // Should fail immediately on first missing language
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProcessorError::NoToolsConfigured { .. }));
    }

    #[test]
    fn test_format_on_missing_language_definition_fail() {
        let mut config = default_config();
        config.on_missing_language_definition = OnMissing::Fail;
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```python\nprint('hello')\n```";
        let result = processor.format(content);

        // Should succeed but report errors
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.content, content); // Content unchanged
        assert!(output.had_errors);
        assert!(!output.error_messages.is_empty());
        assert!(output.error_messages[0].contains("No format tools configured"));
    }

    #[test]
    fn test_format_on_missing_language_definition_fail_fast() {
        let mut config = default_config();
        config.on_missing_language_definition = OnMissing::FailFast;
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```python\nprint('hello')\n```";
        let result = processor.format(content);

        // Should fail immediately
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProcessorError::NoToolsConfigured { .. }));
    }

    #[test]
    fn test_lint_on_missing_tool_binary_fail() {
        use super::super::config::{LanguageToolConfig, ToolDefinition};

        let mut config = default_config();
        config.on_missing_tool_binary = OnMissing::Fail;

        // Configure a tool with a non-existent binary
        let lang_config = LanguageToolConfig {
            lint: vec!["nonexistent-linter".to_string()],
            ..Default::default()
        };
        config.languages.insert("python".to_string(), lang_config);

        let tool_def = ToolDefinition {
            command: vec!["nonexistent-binary-xyz123".to_string()],
            ..Default::default()
        };
        config.tools.insert("nonexistent-linter".to_string(), tool_def);

        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```python\nprint('hello')\n```";
        let result = processor.lint(content);

        // Should succeed but return diagnostic for missing binary
        assert!(result.is_ok());
        let diagnostics = result.unwrap();
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("not found in PATH"));
    }

    #[test]
    fn test_lint_on_missing_tool_binary_fail_fast() {
        use super::super::config::{LanguageToolConfig, ToolDefinition};

        let mut config = default_config();
        config.on_missing_tool_binary = OnMissing::FailFast;

        // Configure a tool with a non-existent binary
        let lang_config = LanguageToolConfig {
            lint: vec!["nonexistent-linter".to_string()],
            ..Default::default()
        };
        config.languages.insert("python".to_string(), lang_config);

        let tool_def = ToolDefinition {
            command: vec!["nonexistent-binary-xyz123".to_string()],
            ..Default::default()
        };
        config.tools.insert("nonexistent-linter".to_string(), tool_def);

        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```python\nprint('hello')\n```";
        let result = processor.lint(content);

        // Should fail immediately
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProcessorError::ToolBinaryNotFound { .. }));
    }

    #[test]
    fn test_format_on_missing_tool_binary_fail() {
        use super::super::config::{LanguageToolConfig, ToolDefinition};

        let mut config = default_config();
        config.on_missing_tool_binary = OnMissing::Fail;

        // Configure a tool with a non-existent binary
        let lang_config = LanguageToolConfig {
            format: vec!["nonexistent-formatter".to_string()],
            ..Default::default()
        };
        config.languages.insert("python".to_string(), lang_config);

        let tool_def = ToolDefinition {
            command: vec!["nonexistent-binary-xyz123".to_string()],
            ..Default::default()
        };
        config.tools.insert("nonexistent-formatter".to_string(), tool_def);

        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```python\nprint('hello')\n```";
        let result = processor.format(content);

        // Should succeed but report errors
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.content, content); // Content unchanged
        assert!(output.had_errors);
        assert!(!output.error_messages.is_empty());
        assert!(output.error_messages[0].contains("not found in PATH"));
    }

    #[test]
    fn test_format_on_missing_tool_binary_fail_fast() {
        use super::super::config::{LanguageToolConfig, ToolDefinition};

        let mut config = default_config();
        config.on_missing_tool_binary = OnMissing::FailFast;

        // Configure a tool with a non-existent binary
        let lang_config = LanguageToolConfig {
            format: vec!["nonexistent-formatter".to_string()],
            ..Default::default()
        };
        config.languages.insert("python".to_string(), lang_config);

        let tool_def = ToolDefinition {
            command: vec!["nonexistent-binary-xyz123".to_string()],
            ..Default::default()
        };
        config.tools.insert("nonexistent-formatter".to_string(), tool_def);

        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```python\nprint('hello')\n```";
        let result = processor.format(content);

        // Should fail immediately
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProcessorError::ToolBinaryNotFound { .. }));
    }

    #[test]
    fn test_lint_rumdl_builtin_skipped_for_markdown() {
        // Configure the built-in "rumdl" tool for markdown
        // The processor should skip it (handled by embedded markdown linting)
        let mut config = default_config();
        config.languages.insert(
            "markdown".to_string(),
            LanguageToolConfig {
                lint: vec![RUMDL_BUILTIN_TOOL.to_string()],
                format: vec![],
                on_error: None,
            },
        );
        config.on_missing_language_definition = OnMissing::Fail;
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```markdown\n# Hello\n```";
        let result = processor.lint(content);

        // Should succeed with no diagnostics - "rumdl" tool is skipped, not treated as unknown
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_format_rumdl_builtin_skipped_for_markdown() {
        // Configure the built-in "rumdl" tool for markdown
        let mut config = default_config();
        config.languages.insert(
            "markdown".to_string(),
            LanguageToolConfig {
                lint: vec![],
                format: vec![RUMDL_BUILTIN_TOOL.to_string()],
                on_error: None,
            },
        );
        let processor = CodeBlockToolProcessor::new(&config);

        let content = "```markdown\n# Hello\n```";
        let result = processor.format(content);

        // Should succeed with unchanged content - "rumdl" tool is skipped
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.content, content);
        assert!(!output.had_errors);
    }

    #[test]
    fn test_is_markdown_language() {
        // Test the helper function
        assert!(is_markdown_language("markdown"));
        assert!(is_markdown_language("Markdown"));
        assert!(is_markdown_language("MARKDOWN"));
        assert!(is_markdown_language("md"));
        assert!(is_markdown_language("MD"));
        assert!(!is_markdown_language("python"));
        assert!(!is_markdown_language("rust"));
        assert!(!is_markdown_language(""));
    }
}
