//! Main processor for code block linting and formatting.
//!
//! This module coordinates language resolution, tool lookup, execution,
//! and result collection for processing code blocks in markdown files.

#[cfg(test)]
use super::config::LanguageToolConfig;
use super::config::{CodeBlockToolsConfig, NormalizeLanguage, OnError, OnMissing, ToolDefinition};
use super::executor::{ExecutorError, ToolExecutor, ToolOutput};
use super::linguist::LinguistResolver;
use super::registry::ToolRegistry;
use crate::config::MarkdownFlavor;
use crate::rule::{LintWarning, Severity};
use crate::utils::rumdl_parser_options;
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

/// Special built-in tool name for rumdl's own markdown linting.
/// When this tool is configured for markdown blocks, the processor skips
/// external execution since it's handled by embedded markdown linting.
pub const RUMDL_BUILTIN_TOOL: &str = "rumdl";

/// Check if a language is markdown (handles common variations).
fn is_markdown_language(lang: &str) -> bool {
    matches!(lang.to_lowercase().as_str(), "markdown" | "md")
}

/// Strip ANSI escape sequences from tool output.
///
/// Many tools output colored text (e.g. `\x1b[1;31mError\x1b[0m`), which prevents
/// structured parsers from matching patterns like `file:line:col: message`.
fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // Consume until we hit an ASCII letter (the terminator)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
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
    /// Tool execution failed with code block location context.
    ToolErrorAt {
        error: ExecutorError,
        line: usize,
        language: String,
    },
    /// No tools configured for language.
    NoToolsConfigured { language: String, line: usize },
    /// Tool binary not found.
    ToolBinaryNotFound {
        tool: String,
        language: String,
        line: usize,
    },
    /// Processing was aborted due to on_error = fail.
    Aborted { message: String },
}

impl std::fmt::Display for ProcessorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolError(e) => write!(f, "{e}"),
            Self::ToolErrorAt { error, line, language } => {
                write!(f, "line {line} ({language}): {error}")
            }
            Self::NoToolsConfigured { language, line } => {
                write!(f, "line {line} ({language}): no tools configured")
            }
            Self::ToolBinaryNotFound { tool, language, line } => {
                write!(f, "line {line} ({language}): tool '{tool}' not found in PATH")
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
/// Context in which a tool is being used.
#[derive(Copy, Clone)]
enum ToolContext {
    Lint,
    Format,
}

pub struct CodeBlockToolProcessor<'a> {
    config: &'a CodeBlockToolsConfig,
    flavor: MarkdownFlavor,
    linguist: LinguistResolver,
    registry: ToolRegistry,
    executor: ToolExecutor,
    user_aliases: std::collections::HashMap<String, String>,
}

impl<'a> CodeBlockToolProcessor<'a> {
    /// Create a new processor with the given configuration and markdown flavor.
    pub fn new(config: &'a CodeBlockToolsConfig, flavor: MarkdownFlavor) -> Self {
        let user_aliases = config
            .language_aliases
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v.to_lowercase()))
            .collect();
        Self {
            config,
            flavor,
            linguist: LinguistResolver::new(),
            registry: ToolRegistry::new(config.tools.clone()),
            executor: ToolExecutor::new(config.timeout),
            user_aliases,
        }
    }

    /// Resolve a tool ID with context awareness.
    ///
    /// When a bare tool name (e.g., "tombi") is used in a specific context
    /// (lint or format), try the context-specific variant first (e.g., "tombi:format"),
    /// then common alternatives (e.g., "tombi:check"), before falling back to the bare name.
    fn resolve_tool<'b>(&'b self, tool_id: &str, context: ToolContext) -> Option<&'b ToolDefinition> {
        // If the tool ID already has a colon suffix, use it directly
        if tool_id.contains(':') {
            return self.registry.get(tool_id);
        }

        // Try context-specific variants first
        let suffixes = match context {
            ToolContext::Format => &["format", "fmt", "fix", "reformat"][..],
            ToolContext::Lint => &["lint", "check"][..],
        };

        for suffix in suffixes {
            let qualified = format!("{tool_id}:{suffix}");
            if let Some(def) = self.registry.get(&qualified) {
                return Some(def);
            }
        }

        // Fall back to bare name
        self.registry.get(tool_id)
    }

    /// Quick check whether any configured language might appear in fenced code blocks.
    /// Scans for `` ```lang `` or `` ~~~lang `` patterns without full parsing.
    fn has_potential_matching_blocks(&self, content: &str, lint_mode: bool) -> bool {
        // Collect languages that have tools configured for the requested mode
        let configured_langs: Vec<&str> = self
            .config
            .languages
            .iter()
            .filter(|(_, lc)| {
                lc.enabled
                    && if lint_mode {
                        !lc.lint.is_empty()
                    } else {
                        !lc.format.is_empty()
                    }
            })
            .map(|(lang, _)| lang.as_str())
            .collect();

        if configured_langs.is_empty() {
            return false;
        }

        // Scan content line-by-line for fence openers matching configured languages
        for line in content.lines() {
            let trimmed = line.trim_start();
            let after_fence = if let Some(rest) = trimmed.strip_prefix("```") {
                rest
            } else if let Some(rest) = trimmed.strip_prefix("~~~") {
                rest
            } else {
                continue;
            };

            let lang = after_fence.split_whitespace().next().unwrap_or("");
            if lang.is_empty() {
                continue;
            }
            // Check both the raw language and the canonical (normalized) form
            let canonical = self.resolve_language(lang);
            if configured_langs.contains(&canonical.as_str()) {
                return true;
            }
        }

        false
    }

    /// Extract all fenced code blocks from content.
    pub fn extract_code_blocks(&self, content: &str) -> Vec<FencedCodeBlockInfo> {
        let mut blocks = Vec::new();
        let mut current_block: Option<FencedCodeBlockBuilder> = None;

        let options = rumdl_parser_options();
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
                        .map_or(content.len(), |i| range.start + i + 1);

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
                                .map_or(search_start, |i| search_start + i)
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

        // For MkDocs flavor, also extract code blocks inside admonitions and tabs
        if self.flavor == MarkdownFlavor::MkDocs {
            let mkdocs_blocks = self.extract_mkdocs_code_blocks(content);
            for mb in mkdocs_blocks {
                // Deduplicate: only add if no existing block starts at the same line
                if !blocks.iter().any(|b| b.start_line == mb.start_line) {
                    blocks.push(mb);
                }
            }
            blocks.sort_by_key(|b| b.start_line);
        }

        blocks
    }

    /// Extract fenced code blocks that are inside MkDocs admonitions or tabs.
    ///
    /// pulldown_cmark doesn't parse MkDocs-specific constructs, so indented
    /// code blocks inside `!!!`/`???` admonitions or `===` tabs are missed.
    /// This method manually scans for them.
    fn extract_mkdocs_code_blocks(&self, content: &str) -> Vec<FencedCodeBlockInfo> {
        use crate::utils::mkdocs_admonitions;
        use crate::utils::mkdocs_tabs;

        let mut blocks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Track current MkDocs context indent level
        // We only need to know if we're inside any MkDocs block, so a simple stack suffices.
        let mut context_indent_stack: Vec<usize> = Vec::new();

        // Track fence state inside MkDocs context
        let mut in_fence = false;
        let mut fence_start_line: usize = 0;
        let mut fence_content_start: usize = 0;
        let mut fence_char: char = '`';
        let mut fence_length: usize = 0;
        let mut fence_indent: usize = 0;
        let mut fence_indent_prefix = String::new();
        let mut fence_language = String::new();
        let mut fence_info_string = String::new();

        // Compute byte offsets via pointer arithmetic.
        // `content.lines()` returns slices into the original string,
        // so each line's pointer offset from `content` gives its byte position.
        // This correctly handles \n, \r\n, and empty lines.
        let content_start_ptr = content.as_ptr() as usize;
        let line_offsets: Vec<usize> = lines
            .iter()
            .map(|line| line.as_ptr() as usize - content_start_ptr)
            .collect();

        for (i, line) in lines.iter().enumerate() {
            let line_indent = crate::utils::mkdocs_common::get_line_indent(line);
            let is_admonition = mkdocs_admonitions::is_admonition_start(line);
            let is_tab = mkdocs_tabs::is_tab_marker(line);

            // Pop contexts when the current line is not indented enough to be content.
            // This runs for ALL lines (including new admonition/tab starts) to clean
            // up stale entries before potentially pushing a new context.
            if !line.trim().is_empty() {
                while let Some(&ctx_indent) = context_indent_stack.last() {
                    if line_indent < ctx_indent + 4 {
                        context_indent_stack.pop();
                        if in_fence {
                            in_fence = false;
                        }
                    } else {
                        break;
                    }
                }
            }

            // Check for admonition start — push new context
            if is_admonition && let Some(indent) = mkdocs_admonitions::get_admonition_indent(line) {
                context_indent_stack.push(indent);
                continue;
            }

            // Check for tab marker — push new context
            if is_tab && let Some(indent) = mkdocs_tabs::get_tab_indent(line) {
                context_indent_stack.push(indent);
                continue;
            }

            // Only look for fences inside a MkDocs context
            if context_indent_stack.is_empty() {
                continue;
            }

            let trimmed = line.trim_start();
            let leading_spaces = line.len() - trimmed.len();

            if !in_fence {
                // Check for fence opening
                let (fc, fl) = if trimmed.starts_with("```") {
                    ('`', trimmed.chars().take_while(|&c| c == '`').count())
                } else if trimmed.starts_with("~~~") {
                    ('~', trimmed.chars().take_while(|&c| c == '~').count())
                } else {
                    continue;
                };

                if fl >= 3 {
                    in_fence = true;
                    fence_start_line = i;
                    fence_char = fc;
                    fence_length = fl;
                    fence_indent = leading_spaces;
                    fence_indent_prefix = line.get(..leading_spaces).unwrap_or("").to_string();

                    let after_fence = &trimmed[fl..];
                    fence_info_string = after_fence.trim().to_string();
                    fence_language = fence_info_string.split_whitespace().next().unwrap_or("").to_string();

                    // Content starts at the next line's byte offset
                    fence_content_start = line_offsets.get(i + 1).copied().unwrap_or(content.len());
                }
            } else {
                // Check for fence closing
                let is_closing = if fence_char == '`' {
                    trimmed.starts_with("```")
                        && trimmed.chars().take_while(|&c| c == '`').count() >= fence_length
                        && trimmed.trim_start_matches('`').trim().is_empty()
                } else {
                    trimmed.starts_with("~~~")
                        && trimmed.chars().take_while(|&c| c == '~').count() >= fence_length
                        && trimmed.trim_start_matches('~').trim().is_empty()
                };

                if is_closing {
                    let content_end = line_offsets.get(i).copied().unwrap_or(content.len());

                    if content_end >= fence_content_start {
                        blocks.push(FencedCodeBlockInfo {
                            start_line: fence_start_line,
                            end_line: i,
                            content_start: fence_content_start,
                            content_end,
                            language: fence_language.clone(),
                            info_string: fence_info_string.clone(),
                            fence_char,
                            fence_length,
                            indent: fence_indent,
                            indent_prefix: fence_indent_prefix.clone(),
                        });
                    }

                    in_fence = false;
                }
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
        // Skip the expensive parse when no tools could possibly produce output.
        // With on_missing=Ignore (default) and no languages with lint tools configured,
        // every block would be skipped, so the parse is wasted work.
        if self.config.on_missing_language_definition == OnMissing::Ignore
            && !self
                .config
                .languages
                .values()
                .any(|lc| lc.enabled && !lc.lint.is_empty())
        {
            return Ok(Vec::new());
        }

        // Quick content check: skip parsing if no configured language appears in the content.
        // This avoids the expensive pulldown-cmark parse when there are no matching code blocks.
        if self.config.on_missing_language_definition == OnMissing::Ignore
            && !self.has_potential_matching_blocks(content, true)
        {
            return Ok(Vec::new());
        }

        let mut all_diagnostics = Vec::new();
        let blocks = self.extract_code_blocks(content);

        for block in blocks {
            if block.language.is_empty() {
                continue; // Skip blocks without language tag
            }

            let canonical_lang = self.resolve_language(&block.language);

            // Get lint tools for this language
            let lang_config = self.config.languages.get(&canonical_lang);

            // If language is explicitly configured with enabled=false, skip silently
            if let Some(lc) = lang_config
                && !lc.enabled
            {
                continue;
            }

            let lint_tools = match lang_config {
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
                                line: block.start_line + 1,
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

                let Some(tool_def) = self.resolve_tool(tool_id, ToolContext::Lint) else {
                    log::warn!("Unknown tool '{tool_id}' configured for language '{canonical_lang}'");
                    continue;
                };

                // Check if tool binary exists before running
                let tool_name = tool_def.command.first().map_or("", String::as_str);
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
                                line: block.start_line + 1,
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
        let no_output = FormatOutput {
            content: content.to_string(),
            had_errors: false,
            error_messages: Vec::new(),
        };

        // Skip the expensive parse when no tools could produce output
        if self.config.on_missing_language_definition == OnMissing::Ignore
            && !self
                .config
                .languages
                .values()
                .any(|lc| lc.enabled && !lc.format.is_empty())
        {
            return Ok(no_output);
        }

        // Quick content check: skip parsing if no configured language appears in the content
        if self.config.on_missing_language_definition == OnMissing::Ignore
            && !self.has_potential_matching_blocks(content, false)
        {
            return Ok(no_output);
        }

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
            let lang_config = self.config.languages.get(&canonical_lang);

            // If language is explicitly configured with enabled=false, skip silently
            if let Some(lc) = lang_config
                && !lc.enabled
            {
                continue;
            }

            let format_tools = match lang_config {
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
                                line: block.start_line + 1,
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

                let Some(tool_def) = self.resolve_tool(tool_id, ToolContext::Format) else {
                    log::warn!("Unknown tool '{tool_id}' configured for language '{canonical_lang}'");
                    continue;
                };

                // Check if tool binary exists before running
                let tool_name = tool_def.command.first().map_or("", String::as_str);
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
                                line: block.start_line + 1,
                            });
                        }
                    }
                }

                match self.executor.format(tool_def, &formatted, Some(self.config.timeout)) {
                    Ok(output) => {
                        // Guard against formatters that produce empty output for non-empty input.
                        // This prevents data loss from misconfigured tools (e.g., a lint tool
                        // used as a formatter that validates but doesn't output content).
                        if output.trim().is_empty() && !formatted.trim().is_empty() {
                            log::warn!("Formatter '{tool_id}' produced empty output for non-empty input, skipping");
                            continue;
                        }

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
                            OnError::Fail => {
                                return Err(ProcessorError::ToolErrorAt {
                                    error: e,
                                    line: block.start_line + 1,
                                    language: canonical_lang,
                                });
                            }
                            OnError::Warn => {
                                error_messages.push(format!("line {} ({}): {e}", block.start_line + 1, canonical_lang));
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

        // Strip ANSI escape codes and combine stdout + stderr for parsing
        let stdout_clean = strip_ansi_codes(&output.stdout);
        let stderr_clean = strip_ansi_codes(&output.stderr);
        let combined = format!("{stdout_clean}\n{stderr_clean}");

        // State for multi-line "Error: msg" / "at line N column M" pattern
        let mut pending_error: Option<(String, DiagnosticSeverity)> = None;

        for line in combined.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Resolve pending "Error: msg" from previous line
            if let Some((ref msg, severity)) = pending_error {
                if let Some((line_num, col)) = Self::parse_at_line_column(line) {
                    diagnostics.push(CodeBlockDiagnostic {
                        file_line: code_block_start_line + line_num,
                        column: Some(col),
                        message: msg.clone(),
                        severity,
                        tool: tool_id.to_string(),
                        code_block_start: code_block_start_line,
                    });
                    pending_error = None;
                    continue;
                }
                // No position info found; emit error without line mapping
                diagnostics.push(CodeBlockDiagnostic {
                    file_line: code_block_start_line,
                    column: None,
                    message: msg.clone(),
                    severity,
                    tool: tool_id.to_string(),
                    code_block_start: code_block_start_line,
                });
                pending_error = None;
                // Fall through to parse current line
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
                continue;
            }

            // Try multi-line "Error: msg" / "Warning: msg" pattern
            if let Some(error_info) = Self::parse_error_line(line) {
                pending_error = Some(error_info);
            }
        }

        // Flush any remaining pending error
        if let Some((msg, severity)) = pending_error {
            diagnostics.push(CodeBlockDiagnostic {
                file_line: code_block_start_line,
                column: None,
                message: msg,
                severity,
                tool: tool_id.to_string(),
                code_block_start: code_block_start_line,
            });
        }

        // If no diagnostics parsed but tool failed, use combined output as fallback
        if diagnostics.is_empty() && !output.success {
            let lines: Vec<&str> = combined.lines().map(str::trim).filter(|l| !l.is_empty()).collect();

            if lines.is_empty() {
                let exit_code = output.exit_code;
                diagnostics.push(CodeBlockDiagnostic {
                    file_line: code_block_start_line,
                    column: None,
                    message: format!("Tool exited with code {exit_code}"),
                    severity: DiagnosticSeverity::Error,
                    tool: tool_id.to_string(),
                    code_block_start: code_block_start_line,
                });
            } else {
                for line_text in lines {
                    diagnostics.push(CodeBlockDiagnostic {
                        file_line: code_block_start_line,
                        column: None,
                        message: line_text.to_string(),
                        severity: DiagnosticSeverity::Error,
                        tool: tool_id.to_string(),
                        code_block_start: code_block_start_line,
                    });
                }
            }
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
        let code_len = after_sc.chars().take_while(char::is_ascii_digit).count();
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

    /// Parse "Error: <message>" or "Warning: <message>" lines.
    ///
    /// Used for tools like tombi that output multi-line diagnostics where the
    /// error message and position are on separate lines. Only matches capitalized
    /// prefixes to avoid conflicting with lowercase `error:` in less structured output.
    fn parse_error_line(line: &str) -> Option<(String, DiagnosticSeverity)> {
        let (msg, severity) = if let Some(msg) = line.strip_prefix("Error:") {
            (msg, DiagnosticSeverity::Error)
        } else if let Some(msg) = line.strip_prefix("Warning:") {
            (msg, DiagnosticSeverity::Warning)
        } else {
            return None;
        };
        let msg = msg.trim();
        if msg.is_empty() {
            return None;
        }
        Some((msg.to_string(), severity))
    }

    /// Parse "at line N column M" position lines (case-insensitive).
    ///
    /// Returns (line_number, column_number) if the pattern matches.
    fn parse_at_line_column(line: &str) -> Option<(usize, usize)> {
        let lower = line.to_lowercase();
        let rest = lower.strip_prefix("at line ")?;
        let mut parts = rest.split_whitespace();
        let line_num: usize = parts.next()?.parse().ok()?;
        if parts.next()? != "column" {
            return None;
        }
        let col: usize = parts.next()?.parse().ok()?;
        Some((line_num, col))
    }

    /// Infer severity from message content.
    fn infer_severity(&self, message: &str) -> DiagnosticSeverity {
        let lower = message.to_lowercase();
        if lower.contains("error")
            || lower.starts_with('e') && lower.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
            || lower.starts_with('f') && lower.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
        {
            DiagnosticSeverity::Error
        } else if lower.contains("warning")
            || lower.contains("warn")
            || lower.starts_with('w') && lower.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
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
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```python title=\"example.py\"\ncode\n```";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, "python");
        assert_eq!(blocks[0].info_string, "python title=\"example.py\"");
    }

    #[test]
    fn test_extract_code_blocks_tilde_fence() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "  - item\n    ```python\n    print('hi')\n    ```";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].indent_prefix, "    ");
    }

    #[test]
    fn test_extract_code_blocks_no_language() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```\nplain code\n```";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, "");
    }

    #[test]
    fn test_resolve_language_linguist() {
        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Linguist;
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        assert_eq!(processor.resolve_language("py"), "python");
        assert_eq!(processor.resolve_language("bash"), "shell");
        assert_eq!(processor.resolve_language("js"), "javascript");
    }

    #[test]
    fn test_resolve_language_exact() {
        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        assert_eq!(processor.resolve_language("py"), "py");
        assert_eq!(processor.resolve_language("BASH"), "bash");
    }

    #[test]
    fn test_resolve_language_user_alias_override() {
        let mut config = default_config();
        config.language_aliases.insert("py".to_string(), "python".to_string());
        config.normalize_language = NormalizeLanguage::Exact;
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        assert_eq!(processor.resolve_language("PY"), "python");
    }

    #[test]
    fn test_indent_strip_and_reapply_roundtrip() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let raw = "    def hello():\n        print('hi')";
        let stripped = processor.strip_indent_from_block(raw, "    ");
        assert_eq!(stripped, "def hello():\n    print('hi')");

        let reapplied = processor.apply_indent_to_block(&stripped, "    ");
        assert_eq!(reapplied, raw);
    }

    #[test]
    fn test_infer_severity() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```python\nprint('hello')\n```";
        let result = processor.lint(content);

        // Should succeed with no diagnostics (no tools configured)
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_format_no_config() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
                ..Default::default()
            },
        );
        config.on_missing_language_definition = OnMissing::Fail;
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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
                format: vec![RUMDL_BUILTIN_TOOL.to_string()],
                ..Default::default()
            },
        );
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

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

    // Issue #423: MkDocs admonition code block detection

    #[test]
    fn test_extract_mkdocs_admonition_code_block() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        let content = "!!! note\n    Some text\n\n    ```python\n    def hello():\n        pass\n    ```\n";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 1, "Should detect code block inside MkDocs admonition");
        assert_eq!(blocks[0].language, "python");
    }

    #[test]
    fn test_extract_mkdocs_tab_code_block() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        let content = "=== \"Python\"\n\n    ```python\n    print(\"hello\")\n    ```\n";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 1, "Should detect code block inside MkDocs tab");
        assert_eq!(blocks[0].language, "python");
    }

    #[test]
    fn test_standard_flavor_ignores_admonition_indented_content() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // With standard flavor, pulldown_cmark parses this differently;
        // our MkDocs extraction should NOT run
        let content = "!!! note\n    Some text\n\n    ```python\n    def hello():\n        pass\n    ```\n";
        let blocks = processor.extract_code_blocks(content);

        // Standard flavor relies on pulldown_cmark only, which may or may not detect
        // indented fenced blocks. The key assertion is that we don't double-detect.
        // With standard flavor, the MkDocs extraction path is skipped entirely.
        for (i, b) in blocks.iter().enumerate() {
            for (j, b2) in blocks.iter().enumerate() {
                if i != j {
                    assert_ne!(b.start_line, b2.start_line, "No duplicate blocks should exist");
                }
            }
        }
    }

    #[test]
    fn test_mkdocs_top_level_blocks_alongside_admonition() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        let content =
            "```rust\nfn main() {}\n```\n\n!!! note\n    Some text\n\n    ```python\n    print(\"hello\")\n    ```\n";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(
            blocks.len(),
            2,
            "Should detect both top-level and admonition code blocks"
        );
        assert_eq!(blocks[0].language, "rust");
        assert_eq!(blocks[1].language, "python");
    }

    #[test]
    fn test_mkdocs_nested_admonition_code_block() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        let content = "\
!!! note
    Some text

    !!! warning
        Nested content

        ```python
        x = 1
        ```
";
        let blocks = processor.extract_code_blocks(content);
        assert_eq!(blocks.len(), 1, "Should detect code block inside nested admonition");
        assert_eq!(blocks[0].language, "python");
    }

    #[test]
    fn test_mkdocs_consecutive_admonitions_no_stale_context() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        // Two consecutive admonitions at the same indent level.
        // The first has no code block, the second does.
        let content = "\
!!! note
    First admonition content

!!! warning
    Second admonition content

    ```python
    y = 2
    ```
";
        let blocks = processor.extract_code_blocks(content);
        assert_eq!(blocks.len(), 1, "Should detect code block in second admonition only");
        assert_eq!(blocks[0].language, "python");
    }

    #[test]
    fn test_mkdocs_crlf_line_endings() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        // Use \r\n line endings
        let content = "!!! note\r\n    Some text\r\n\r\n    ```python\r\n    x = 1\r\n    ```\r\n";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 1, "Should detect code block with CRLF line endings");
        assert_eq!(blocks[0].language, "python");

        // Verify byte offsets point to valid content
        let extracted = &content[blocks[0].content_start..blocks[0].content_end];
        assert!(
            extracted.contains("x = 1"),
            "Extracted content should contain code. Got: {extracted:?}"
        );
    }

    #[test]
    fn test_mkdocs_unclosed_fence_in_admonition() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        // Unclosed fence should not produce a block
        let content = "!!! note\n    ```python\n    x = 1\n    no closing fence\n";
        let blocks = processor.extract_code_blocks(content);
        assert_eq!(blocks.len(), 0, "Unclosed fence should not produce a block");
    }

    #[test]
    fn test_mkdocs_tilde_fence_in_admonition() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        let content = "!!! note\n    ~~~ruby\n    puts 'hi'\n    ~~~\n";
        let blocks = processor.extract_code_blocks(content);
        assert_eq!(blocks.len(), 1, "Should detect tilde-fenced code block");
        assert_eq!(blocks[0].language, "ruby");
    }

    #[test]
    fn test_mkdocs_empty_lines_in_code_block() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        // Code block with empty lines inside — verifies byte offsets are correct
        // across empty lines (the previous find("") approach would break here)
        let content = "!!! note\n    ```python\n    x = 1\n\n    y = 2\n    ```\n";
        let blocks = processor.extract_code_blocks(content);
        assert_eq!(blocks.len(), 1);

        let extracted = &content[blocks[0].content_start..blocks[0].content_end];
        assert!(
            extracted.contains("x = 1") && extracted.contains("y = 2"),
            "Extracted content should span across the empty line. Got: {extracted:?}"
        );
    }

    #[test]
    fn test_mkdocs_content_byte_offsets_lf() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        let content = "!!! note\n    ```python\n    print('hi')\n    ```\n";
        let blocks = processor.extract_code_blocks(content);
        assert_eq!(blocks.len(), 1);

        // Verify the extracted content is exactly the code body
        let extracted = &content[blocks[0].content_start..blocks[0].content_end];
        assert_eq!(extracted, "    print('hi')\n", "Content offsets should be exact for LF");
    }

    #[test]
    fn test_mkdocs_content_byte_offsets_crlf() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        let content = "!!! note\r\n    ```python\r\n    print('hi')\r\n    ```\r\n";
        let blocks = processor.extract_code_blocks(content);
        assert_eq!(blocks.len(), 1);

        let extracted = &content[blocks[0].content_start..blocks[0].content_end];
        assert_eq!(
            extracted, "    print('hi')\r\n",
            "Content offsets should be exact for CRLF"
        );
    }

    #[test]
    fn test_lint_enabled_false_skips_language_in_strict_mode() {
        // With on-missing-language-definition = "fail", a language configured
        // with enabled=false should be silently skipped (no error).
        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        config.on_missing_language_definition = OnMissing::Fail;

        // Python has tools, plaintext is disabled
        config.languages.insert(
            "python".to_string(),
            LanguageToolConfig {
                lint: vec!["ruff:check".to_string()],
                ..Default::default()
            },
        );
        config.languages.insert(
            "plaintext".to_string(),
            LanguageToolConfig {
                enabled: false,
                ..Default::default()
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```plaintext\nsome text\n```";
        let result = processor.lint(content);

        // No error for plaintext: enabled=false satisfies strict mode
        assert!(result.is_ok());
        let diagnostics = result.unwrap();
        assert!(
            diagnostics.is_empty(),
            "Expected no diagnostics for disabled language, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_format_enabled_false_skips_language_in_strict_mode() {
        // Same test but for format mode
        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        config.on_missing_language_definition = OnMissing::Fail;

        config.languages.insert(
            "plaintext".to_string(),
            LanguageToolConfig {
                enabled: false,
                ..Default::default()
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```plaintext\nsome text\n```";
        let result = processor.format(content);

        // No error for plaintext: enabled=false satisfies strict mode
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.had_errors, "Expected no errors for disabled language");
        assert!(
            output.error_messages.is_empty(),
            "Expected no error messages, got: {:?}",
            output.error_messages
        );
    }

    #[test]
    fn test_enabled_false_default_true_preserved() {
        // Verify that when enabled is not set, it defaults to true (existing behavior)
        let mut config = default_config();
        config.on_missing_language_definition = OnMissing::Fail;

        // Configure python without explicitly setting enabled
        config.languages.insert(
            "python".to_string(),
            LanguageToolConfig {
                lint: vec!["ruff:check".to_string()],
                ..Default::default()
            },
        );

        let lang_config = config.languages.get("python").unwrap();
        assert!(lang_config.enabled, "enabled should default to true");
    }

    #[test]
    fn test_enabled_false_with_fail_fast_no_error() {
        // Even with fail-fast, enabled=false should skip silently
        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        config.on_missing_language_definition = OnMissing::FailFast;

        config.languages.insert(
            "unknown".to_string(),
            LanguageToolConfig {
                enabled: false,
                ..Default::default()
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```unknown\nsome content\n```";
        let result = processor.lint(content);

        // Should not return an error: enabled=false takes precedence over fail-fast
        assert!(result.is_ok(), "Expected Ok but got Err: {result:?}");
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_enabled_false_format_with_fail_fast_no_error() {
        // Same for format mode
        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        config.on_missing_language_definition = OnMissing::FailFast;

        config.languages.insert(
            "unknown".to_string(),
            LanguageToolConfig {
                enabled: false,
                ..Default::default()
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```unknown\nsome content\n```";
        let result = processor.format(content);

        assert!(result.is_ok(), "Expected Ok but got Err: {result:?}");
        let output = result.unwrap();
        assert!(!output.had_errors);
    }

    #[test]
    fn test_enabled_false_with_tools_still_skips() {
        // If enabled=false but tools are listed, the language should still be skipped
        let mut config = default_config();
        config.on_missing_language_definition = OnMissing::Fail;

        config.languages.insert(
            "python".to_string(),
            LanguageToolConfig {
                enabled: false,
                lint: vec!["ruff:check".to_string()],
                format: vec!["ruff:format".to_string()],
                on_error: None,
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```python\nprint('hello')\n```";

        // Lint should skip
        let lint_result = processor.lint(content);
        assert!(lint_result.is_ok());
        assert!(lint_result.unwrap().is_empty());

        // Format should skip
        let format_result = processor.format(content);
        assert!(format_result.is_ok());
        let output = format_result.unwrap();
        assert!(!output.had_errors);
        assert_eq!(output.content, content, "Content should be unchanged");
    }

    #[test]
    fn test_enabled_true_without_tools_triggers_strict_mode() {
        // A language configured with enabled=true (default) but no tools
        // should still trigger strict mode errors
        let mut config = default_config();
        config.on_missing_language_definition = OnMissing::Fail;

        config.languages.insert(
            "python".to_string(),
            LanguageToolConfig {
                // enabled defaults to true, no tools
                ..Default::default()
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```python\nprint('hello')\n```";
        let result = processor.lint(content);

        // Should report an error because enabled=true but no lint tools configured
        assert!(result.is_ok());
        let diagnostics = result.unwrap();
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("No lint tools configured"));
    }

    #[test]
    fn test_mixed_enabled_and_disabled_languages() {
        // Multiple languages: one disabled, one unconfigured
        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        config.on_missing_language_definition = OnMissing::Fail;

        config.languages.insert(
            "plaintext".to_string(),
            LanguageToolConfig {
                enabled: false,
                ..Default::default()
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "\
```plaintext
some text
```

```javascript
console.log('hi');
```
";

        let result = processor.lint(content);
        assert!(result.is_ok());
        let diagnostics = result.unwrap();

        // plaintext: skipped (enabled=false), no error
        // javascript: not configured at all, should trigger strict mode error
        assert_eq!(diagnostics.len(), 1, "Expected 1 diagnostic, got: {diagnostics:?}");
        assert!(
            diagnostics[0].message.contains("javascript"),
            "Error should be about javascript, got: {}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_generic_fallback_includes_all_stderr_lines() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // Use output that won't be parsed by any structured format parser
        let output = ToolOutput {
            stdout: String::new(),
            stderr: "Parse error at position 42\nUnexpected token '::'\n3 errors found".to_string(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tombi", 5);
        assert_eq!(diags.len(), 3, "Expected one diagnostic per non-empty stderr line");
        assert_eq!(diags[0].message, "Parse error at position 42");
        assert_eq!(diags[1].message, "Unexpected token '::'");
        assert_eq!(diags[2].message, "3 errors found");
        assert!(diags.iter().all(|d| d.tool == "tombi"));
        assert!(diags.iter().all(|d| d.file_line == 5));
    }

    #[test]
    fn test_generic_fallback_includes_all_stdout_lines_when_stderr_empty() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let output = ToolOutput {
            stdout: "Line 1 error\nLine 2 detail\nLine 3 summary".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "some-tool", 10);
        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].message, "Line 1 error");
        assert_eq!(diags[1].message, "Line 2 detail");
        assert_eq!(diags[2].message, "Line 3 summary");
    }

    #[test]
    fn test_generic_fallback_skips_blank_lines() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let output = ToolOutput {
            stdout: String::new(),
            stderr: "error: bad input\n\n  \n\ndetail: see above\n".to_string(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 1);
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].message, "error: bad input");
        assert_eq!(diags[1].message, "detail: see above");
    }

    #[test]
    fn test_generic_fallback_exit_code_when_no_output() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let output = ToolOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 42,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 1);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "Tool exited with code 42");
    }

    #[test]
    fn test_generic_fallback_not_triggered_on_success() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let output = ToolOutput {
            stdout: "some informational output".to_string(),
            stderr: String::new(),
            exit_code: 0,
            success: true,
        };

        let diags = processor.parse_tool_output(&output, "tool", 1);
        assert!(
            diags.is_empty(),
            "Successful tool runs should produce no fallback diagnostics"
        );
    }

    #[test]
    fn test_ansi_codes_stripped_before_parsing() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // ruff-style output with ANSI color codes wrapping the message
        let output = ToolOutput {
            stdout: "\x1b[1m_.py\x1b[0m:\x1b[33m1\x1b[0m:\x1b[33m1\x1b[0m: \x1b[31mE501\x1b[0m Line too long"
                .to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "ruff:check", 5);
        assert_eq!(diags.len(), 1, "ANSI-colored output should still be parsed");
        assert_eq!(diags[0].message, "E501 Line too long");
        assert_eq!(diags[0].file_line, 6); // 5 + 1
    }

    #[test]
    fn test_tombi_multiline_error_format() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // Simulates tombi output (ANSI codes stripped for clarity)
        let output = ToolOutput {
            stdout: "[test]\ntest: \"test\"\nError: invalid key\n    at line 2 column 1\nError: expected key\n    at line 2 column 1\nError: expected '='\n    at line 2 column 1\nError: expected value\n    at line 2 column 1".to_string(),
            stderr: "1 file failed to be formatted".to_string(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tombi", 7);
        assert_eq!(
            diags.len(),
            4,
            "Expected 4 diagnostics from tombi errors, got {diags:?}"
        );
        assert_eq!(diags[0].message, "invalid key");
        assert_eq!(diags[0].file_line, 9); // 7 + 2
        assert_eq!(diags[0].column, Some(1));
        assert_eq!(diags[1].message, "expected key");
        assert_eq!(diags[1].file_line, 9);
        assert_eq!(diags[2].message, "expected '='");
        assert_eq!(diags[3].message, "expected value");
        assert!(diags.iter().all(|d| d.tool == "tombi"));
    }

    #[test]
    fn test_tombi_with_ansi_codes() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // Real tombi output with ANSI escape codes
        let output = ToolOutput {
            stdout: "[test]\ntest: \"test\"\n\x1b[1;31m  Error\x1b[0m: \x1b[1minvalid key\x1b[0m\n    \x1b[90mat line 2 column 1\x1b[0m\n\x1b[1;31m  Error\x1b[0m: \x1b[1mexpected '='\x1b[0m\n    \x1b[90mat line 2 column 1\x1b[0m".to_string(),
            stderr: "1 file failed to be formatted".to_string(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tombi", 7);
        assert_eq!(
            diags.len(),
            2,
            "Expected 2 diagnostics from ANSI-colored tombi output, got {diags:?}"
        );
        assert_eq!(diags[0].message, "invalid key");
        assert_eq!(diags[0].file_line, 9);
        assert_eq!(diags[1].message, "expected '='");
        assert_eq!(diags[1].file_line, 9);
    }

    #[test]
    fn test_fallback_combines_stdout_and_stderr() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // Tool puts some errors on stdout, summary on stderr
        let output = ToolOutput {
            stdout: "problem found in input".to_string(),
            stderr: "1 file failed".to_string(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 1);
        assert_eq!(diags.len(), 2, "Fallback should include both stdout and stderr");
        assert_eq!(diags[0].message, "problem found in input");
        assert_eq!(diags[1].message, "1 file failed");
    }

    #[test]
    fn test_error_line_without_position_info() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // Error: line not followed by "at line N column M"
        let output = ToolOutput {
            stdout: "Error: something went wrong\nsome unrelated line".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 5);
        // "Error: something went wrong" → parsed by error-line parser (no position)
        // "some unrelated line" → no parser matches, but diagnostics not empty → no fallback
        assert!(!diags.is_empty());
        assert_eq!(diags[0].message, "something went wrong");
        assert_eq!(diags[0].file_line, 5); // No line offset, uses code_block_start
    }

    #[test]
    fn test_warning_line_with_position() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let output = ToolOutput {
            stdout: "Warning: deprecated syntax\n    at line 3 column 5".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 10);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "deprecated syntax");
        assert_eq!(diags[0].file_line, 13); // 10 + 3
        assert_eq!(diags[0].column, Some(5));
        assert!(matches!(diags[0].severity, DiagnosticSeverity::Warning));
    }

    #[test]
    fn test_strip_ansi_codes() {
        assert_eq!(strip_ansi_codes("hello"), "hello");
        assert_eq!(strip_ansi_codes("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(
            strip_ansi_codes("\x1b[1;31m  Error\x1b[0m: \x1b[1mmsg\x1b[0m"),
            "  Error: msg"
        );
        assert_eq!(strip_ansi_codes("no codes here"), "no codes here");
        assert_eq!(strip_ansi_codes(""), "");
        assert_eq!(
            strip_ansi_codes("\x1b[90mat line 2 column 1\x1b[0m"),
            "at line 2 column 1"
        );
    }

    #[test]
    fn test_parse_at_line_column() {
        assert_eq!(
            CodeBlockToolProcessor::parse_at_line_column("at line 2 column 1"),
            Some((2, 1))
        );
        assert_eq!(
            CodeBlockToolProcessor::parse_at_line_column("at line 10 column 15"),
            Some((10, 15))
        );
        assert_eq!(
            CodeBlockToolProcessor::parse_at_line_column("At Line 5 Column 3"),
            Some((5, 3))
        );
        assert_eq!(
            CodeBlockToolProcessor::parse_at_line_column("not a position line"),
            None
        );
        assert_eq!(
            CodeBlockToolProcessor::parse_at_line_column("at line abc column 1"),
            None
        );
    }

    #[test]
    fn test_parse_error_line() {
        let (msg, sev) = CodeBlockToolProcessor::parse_error_line("Error: invalid key").unwrap();
        assert_eq!(msg, "invalid key");
        assert!(matches!(sev, DiagnosticSeverity::Error));

        let (msg, sev) = CodeBlockToolProcessor::parse_error_line("Warning: deprecated").unwrap();
        assert_eq!(msg, "deprecated");
        assert!(matches!(sev, DiagnosticSeverity::Warning));

        // Lowercase should NOT match (avoids conflict with unstructured tool output)
        assert!(CodeBlockToolProcessor::parse_error_line("error: bad input").is_none());
        assert!(CodeBlockToolProcessor::parse_error_line("warning: minor issue").is_none());

        // Empty message after prefix should not match
        assert!(CodeBlockToolProcessor::parse_error_line("Error:").is_none());
        assert!(CodeBlockToolProcessor::parse_error_line("Error:   ").is_none());

        // Not an error line
        assert!(CodeBlockToolProcessor::parse_error_line("some random text").is_none());
    }

    #[test]
    fn test_consecutive_error_lines_without_position() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // Two Error: lines in a row — first should flush without position,
        // second gets position from "at line"
        let output = ToolOutput {
            stdout: "Error: first problem\nError: second problem\n    at line 3 column 1".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 5);
        assert_eq!(diags.len(), 2, "Expected 2 diagnostics, got {diags:?}");
        // First error flushed without position when second Error: was encountered
        assert_eq!(diags[0].message, "first problem");
        assert_eq!(diags[0].file_line, 5); // No line mapping
        assert_eq!(diags[0].column, None);
        // Second error resolved with position
        assert_eq!(diags[1].message, "second problem");
        assert_eq!(diags[1].file_line, 8); // 5 + 3
        assert_eq!(diags[1].column, Some(1));
    }

    #[test]
    fn test_error_line_at_end_of_output() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // Error: as the very last line — flushed by post-loop code
        let output = ToolOutput {
            stdout: "Error: trailing error".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 5);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "trailing error");
        assert_eq!(diags[0].file_line, 5); // No position info available
        assert_eq!(diags[0].column, None);
    }

    #[test]
    fn test_blank_lines_between_error_and_position() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // Blank lines between Error: and "at line" should be transparently skipped
        let output = ToolOutput {
            stdout: "Error: spaced out\n\n\n    at line 4 column 2".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 10);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "spaced out");
        assert_eq!(diags[0].file_line, 14); // 10 + 4
        assert_eq!(diags[0].column, Some(2));
    }

    #[test]
    fn test_mixed_structured_and_error_line_parsers() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // Mix of ruff-style structured output and tombi-style Error: output
        let output = ToolOutput {
            stdout: "_.py:1:5: E501 Line too long\nError: invalid syntax\n    at line 3 column 1".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 5);
        assert_eq!(diags.len(), 2, "Expected 2 diagnostics, got {diags:?}");
        // First: standard format parser
        assert_eq!(diags[0].message, "E501 Line too long");
        assert_eq!(diags[0].file_line, 6); // 5 + 1
        // Second: Error: + at line parser
        assert_eq!(diags[1].message, "invalid syntax");
        assert_eq!(diags[1].file_line, 8); // 5 + 3
    }

    #[test]
    fn test_at_line_without_preceding_error() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // "at line N column M" without a preceding Error: should not create a diagnostic
        let output = ToolOutput {
            stdout: "at line 2 column 1\nsome other text".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 5);
        // No pending error, so "at line" is just an unmatched line
        // Both lines are unmatched, fallback fires with combined output
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].message, "at line 2 column 1");
        assert_eq!(diags[1].message, "some other text");
    }

    // =========================================================================
    // Issue #527: formatter that produces empty output should not erase content
    // =========================================================================

    /// A formatter that produces no stdout (like `tombi lint -` mistakenly used
    /// as a formatter) should not replace non-empty content with an empty string.
    /// This test uses `true` which exits 0 with no output, simulating the bug.
    #[test]
    fn test_format_empty_output_does_not_erase_content() {
        use super::super::config::LanguageToolConfig;

        let mut config = default_config();
        config.languages.insert(
            "toml".to_string(),
            LanguageToolConfig {
                format: vec!["empty-formatter".to_string()],
                ..Default::default()
            },
        );
        // Define a tool that exits 0 but produces no stdout (simulates `tombi lint -`)
        config.tools.insert(
            "empty-formatter".to_string(),
            super::super::config::ToolDefinition {
                command: vec!["true".to_string()],
                stdin: true,
                stdout: true,
                lint_args: vec![],
                format_args: vec![],
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```toml\nkey = \"value\"\n```\n";
        let result = processor.format(content);

        assert!(result.is_ok(), "Format should not error");
        let output = result.unwrap();

        // The content must NOT be erased — original content should be preserved
        assert!(
            output.content.contains("key = \"value\""),
            "Empty formatter output should not erase content. Got: {:?}",
            output.content
        );
    }

    /// A formatter that echoes input back (like `cat`) should preserve content.
    #[test]
    fn test_format_identity_formatter_preserves_content() {
        use super::super::config::LanguageToolConfig;

        let mut config = default_config();
        config.languages.insert(
            "toml".to_string(),
            LanguageToolConfig {
                format: vec!["cat-formatter".to_string()],
                ..Default::default()
            },
        );
        config.tools.insert(
            "cat-formatter".to_string(),
            super::super::config::ToolDefinition {
                command: vec!["cat".to_string()],
                stdin: true,
                stdout: true,
                lint_args: vec![],
                format_args: vec![],
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```toml\nkey = \"value\"\n```\n";
        let result = processor.format(content);

        assert!(result.is_ok(), "Format should not error");
        let output = result.unwrap();
        assert_eq!(
            output.content, content,
            "Identity formatter should preserve content exactly"
        );
    }

    /// Verify that the context-aware tool resolution resolves bare "tombi"
    /// to "tombi:format" in format context and "tombi:lint" in lint context.
    #[test]
    fn test_resolve_tool_context_aware_tombi() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // In format context, bare "tombi" should resolve to "tombi:format"
        let format_def = processor
            .resolve_tool("tombi", ToolContext::Format)
            .expect("Should resolve tombi in format context");
        assert!(
            format_def.command.iter().any(|arg| arg == "format"),
            "Bare 'tombi' in format context should resolve to 'tombi format', got: {:?}",
            format_def.command
        );

        // In lint context, bare "tombi" should resolve to "tombi:lint"
        let lint_def = processor
            .resolve_tool("tombi", ToolContext::Lint)
            .expect("Should resolve tombi in lint context");
        assert!(
            lint_def.command.iter().any(|arg| arg == "lint"),
            "Bare 'tombi' in lint context should resolve to 'tombi lint', got: {:?}",
            lint_def.command
        );

        // Explicit suffix should bypass context-aware resolution
        let explicit_def = processor
            .resolve_tool("tombi:lint", ToolContext::Format)
            .expect("Should resolve explicit tombi:lint even in format context");
        assert!(
            explicit_def.command.iter().any(|arg| arg == "lint"),
            "Explicit 'tombi:lint' should always use lint, got: {:?}",
            explicit_def.command
        );
    }

    /// Verify context-aware resolution for ruff (uses "check" suffix, not "lint").
    #[test]
    fn test_resolve_tool_context_aware_ruff() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // In lint context, bare "ruff" should resolve to "ruff:check"
        let lint_def = processor
            .resolve_tool("ruff", ToolContext::Lint)
            .expect("Should resolve ruff in lint context");
        assert!(
            lint_def.command.iter().any(|arg| arg == "check"),
            "Bare 'ruff' in lint context should resolve to 'ruff check', got: {:?}",
            lint_def.command
        );

        // In format context, bare "ruff" should resolve to "ruff:format"
        let format_def = processor
            .resolve_tool("ruff", ToolContext::Format)
            .expect("Should resolve ruff in format context");
        assert!(
            format_def.command.iter().any(|arg| arg == "format"),
            "Bare 'ruff' in format context should resolve to 'ruff format', got: {:?}",
            format_def.command
        );
    }

    /// Tools without context-specific variants should still resolve via bare name.
    #[test]
    fn test_resolve_tool_bare_name_fallback() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // "shellcheck" has no :lint or :format variant — should fall back to bare name
        let def = processor
            .resolve_tool("shellcheck", ToolContext::Lint)
            .expect("Should resolve shellcheck via fallback");
        assert!(
            def.command.iter().any(|arg| arg == "shellcheck"),
            "shellcheck should resolve to itself, got: {:?}",
            def.command
        );
    }

    /// Context-aware resolution for tools with non-standard format suffixes.
    #[test]
    fn test_resolve_tool_context_aware_sqlfluff() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // sqlfluff uses ":fix" as its format variant
        let format_def = processor
            .resolve_tool("sqlfluff", ToolContext::Format)
            .expect("Should resolve sqlfluff in format context");
        assert!(
            format_def.command.iter().any(|arg| arg == "fix"),
            "Bare 'sqlfluff' in format context should resolve to 'sqlfluff fix', got: {:?}",
            format_def.command
        );
    }

    /// Context-aware resolution for djlint (:reformat suffix).
    #[test]
    fn test_resolve_tool_context_aware_djlint() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // djlint uses ":reformat" as its format variant
        let format_def = processor
            .resolve_tool("djlint", ToolContext::Format)
            .expect("Should resolve djlint in format context");
        assert!(
            format_def.command.iter().any(|arg| arg.contains("reformat")),
            "Bare 'djlint' in format context should resolve to djlint reformat, got: {:?}",
            format_def.command
        );
    }

    /// User-defined tools with context-specific variants resolve correctly.
    #[test]
    fn test_resolve_tool_user_defined_with_context_variant() {
        use super::super::config::ToolDefinition;

        let mut config = default_config();
        config.tools.insert(
            "mytool".to_string(),
            ToolDefinition {
                command: vec!["mytool".to_string(), "--lint".to_string()],
                ..Default::default()
            },
        );
        config.tools.insert(
            "mytool:format".to_string(),
            ToolDefinition {
                command: vec!["mytool".to_string(), "--format".to_string()],
                ..Default::default()
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // In format context, should resolve to "mytool:format"
        let def = processor
            .resolve_tool("mytool", ToolContext::Format)
            .expect("Should resolve user tool in format context");
        assert!(
            def.command.iter().any(|arg| arg == "--format"),
            "User 'mytool' in format context should resolve to mytool:format, got: {:?}",
            def.command
        );

        // In lint context, should fall back to bare "mytool" (no mytool:lint exists)
        let def = processor
            .resolve_tool("mytool", ToolContext::Lint)
            .expect("Should resolve user tool in lint context via fallback");
        assert!(
            def.command.iter().any(|arg| arg == "--lint"),
            "User 'mytool' in lint context should fall back to bare name, got: {:?}",
            def.command
        );
    }

    /// Nonexistent tool returns None.
    #[test]
    fn test_resolve_tool_nonexistent_returns_none() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        assert!(
            processor
                .resolve_tool("nonexistent-tool-xyz", ToolContext::Lint)
                .is_none(),
            "Nonexistent tool should return None in lint context"
        );
        assert!(
            processor
                .resolve_tool("nonexistent-tool-xyz", ToolContext::Format)
                .is_none(),
            "Nonexistent tool should return None in format context"
        );
    }

    #[test]
    fn test_strip_ansi_codes_edge_cases() {
        // Lone ESC without CSI bracket — non-printable, safely dropped
        assert_eq!(strip_ansi_codes("before\x1bafter"), "beforeafter");
        // ESC at end of string
        assert_eq!(strip_ansi_codes("trailing\x1b"), "trailing");
        // Nested/consecutive sequences
        assert_eq!(strip_ansi_codes("\x1b[1m\x1b[31mbold red\x1b[0m"), "bold red");
        // 256-color and RGB sequences
        assert_eq!(strip_ansi_codes("\x1b[38;5;196mred\x1b[0m"), "red");
        assert_eq!(strip_ansi_codes("\x1b[38;2;255;0;0mred\x1b[0m"), "red");
    }
}
