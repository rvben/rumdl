//! Main processor for code block linting and formatting.
//!
//! This module coordinates language resolution, tool lookup, execution,
//! and result collection for processing code blocks in markdown files.

use super::config::{CodeBlockToolsConfig, NormalizeLanguage, OnError};
use super::executor::{ExecutorError, ToolExecutor, ToolOutput};
use super::linguist::LinguistResolver;
use super::registry::ToolRegistry;
use crate::rule::{LintWarning, Severity};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

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

/// Main processor for code block tools.
pub struct CodeBlockToolProcessor<'a> {
    config: &'a CodeBlockToolsConfig,
    linguist: LinguistResolver,
    registry: ToolRegistry,
    executor: ToolExecutor,
}

impl<'a> CodeBlockToolProcessor<'a> {
    /// Create a new processor with the given configuration.
    pub fn new(config: &'a CodeBlockToolsConfig) -> Self {
        Self {
            config,
            linguist: LinguistResolver::new(),
            registry: ToolRegistry::new(config.tools.clone()),
            executor: ToolExecutor::new(config.timeout),
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
        match self.config.normalize_language {
            NormalizeLanguage::Linguist => self.linguist.resolve(language),
            NormalizeLanguage::Exact => language.to_lowercase(),
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
                Some(lc) => &lc.lint,
                None => continue, // No config for this language
            };

            if lint_tools.is_empty() {
                continue;
            }

            // Extract code block content
            let code_content = if block.content_start < block.content_end && block.content_end <= content.len() {
                &content[block.content_start..block.content_end]
            } else {
                continue;
            };

            // Run each lint tool
            for tool_id in lint_tools {
                let tool_def = match self.registry.get(tool_id) {
                    Some(t) => t,
                    None => {
                        log::warn!("Unknown tool '{tool_id}' configured for language '{canonical_lang}'");
                        continue;
                    }
                };

                match self.executor.lint(tool_def, code_content, Some(self.config.timeout)) {
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
    /// Returns the modified content with formatted code blocks.
    pub fn format(&self, content: &str) -> Result<String, ProcessorError> {
        let blocks = self.extract_code_blocks(content);

        if blocks.is_empty() {
            return Ok(content.to_string());
        }

        // Process blocks in reverse order to maintain byte offsets
        let mut result = content.to_string();

        for block in blocks.into_iter().rev() {
            if block.language.is_empty() {
                continue;
            }

            let canonical_lang = self.resolve_language(&block.language);

            // Get format tools for this language
            let format_tools = match self.config.languages.get(&canonical_lang) {
                Some(lc) => &lc.format,
                None => continue,
            };

            if format_tools.is_empty() {
                continue;
            }

            // Extract code block content
            if block.content_start >= block.content_end || block.content_end > result.len() {
                continue;
            }
            let code_content = result[block.content_start..block.content_end].to_string();

            // Run format tools (use first successful one)
            let mut formatted = code_content.clone();
            for tool_id in format_tools {
                let tool_def = match self.registry.get(tool_id) {
                    Some(t) => t,
                    None => {
                        log::warn!("Unknown tool '{tool_id}' configured for language '{canonical_lang}'");
                        continue;
                    }
                };

                match self.executor.format(tool_def, &formatted, Some(self.config.timeout)) {
                    Ok(output) => {
                        // Ensure trailing newline matches original
                        formatted = output;
                        if code_content.ends_with('\n') && !formatted.ends_with('\n') {
                            formatted.push('\n');
                        } else if !code_content.ends_with('\n') && formatted.ends_with('\n') {
                            formatted.pop();
                        }
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

            // Replace content if changed
            if formatted != code_content {
                result.replace_range(block.content_start..block.content_end, &formatted);
            }
        }

        Ok(result)
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

            // Try pattern: "In - line N: message" (shellcheck style)
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
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 3 {
            // parts[0] = filename, parts[1] = line, parts[2] = col or message
            if let Ok(line_num) = parts[1].trim().parse::<usize>() {
                let (column, message) = if parts.len() >= 4 {
                    // Has column
                    let col = parts[2].trim().parse::<usize>().ok();
                    (col, parts[3].trim().to_string())
                } else {
                    // No column
                    (None, parts[2].trim().to_string())
                };

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
                    format!("{sev_part} {msg_part}")
                };
                let severity = self.infer_severity(&message);
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
                let message = after_line[colon_pos + 1..].trim().to_string();
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

    /// Infer severity from message content.
    fn infer_severity(&self, message: &str) -> DiagnosticSeverity {
        let lower = message.to_lowercase();
        if lower.contains("error") || lower.starts_with("e") && lower.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
        {
            DiagnosticSeverity::Error
        } else if lower.contains("warning") || lower.contains("warn") {
            DiagnosticSeverity::Warning
        } else {
            DiagnosticSeverity::Info
        }
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
    fn test_infer_severity() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config);

        assert_eq!(
            processor.infer_severity("E501 line too long"),
            DiagnosticSeverity::Error
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
        assert_eq!(result.unwrap(), content);
    }
}
