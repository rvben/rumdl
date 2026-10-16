//! Main processor for code block linting and formatting.
//!
//! This module coordinates language resolution, tool lookup, execution,
//! and result collection for processing code blocks in markdown files.

use super::config::{CodeBlockToolsConfig, FormatMode, LanguageToolConfig, NormalizeLanguage, OnError, OnMissing};
use super::executor::{ExecutorError, FormatCheckOutput, ToolExecutor, ToolOutput};
use super::linguist::LinguistResolver;
use super::registry::{BuiltinLintMode, ToolRegistry, ToolSlot};
use crate::config::MarkdownFlavor;
use crate::rule::{LintWarning, Severity};
use crate::utils::parser_options::rumdl_parser_options_for;
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

/// Special built-in tool name for rumdl's own markdown linting.
/// When this tool is configured for markdown blocks, the processor skips
/// external execution since it's handled by embedded markdown linting.
pub const RUMDL_BUILTIN_TOOL: &str = "rumdl";

/// Embedded Markdown tools are handled in-process, never as external commands.
pub fn is_rumdl_builtin(tool_id: &str) -> bool {
    matches!(tool_id, RUMDL_BUILTIN_TOOL | "rumdl:lint" | "rumdl:format")
}

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

/// Decode the percent escapes a GitHub Actions workflow command puts in its message.
///
/// `%25` encodes `%` itself, so decoding runs left to right in a single pass: replacing
/// the escapes one after another would turn a literal `%250A` into a line break. A
/// diagnostic occupies one line, so an encoded line break decodes to a space.
fn decode_workflow_command(message: &str) -> String {
    let mut decoded = String::with_capacity(message.len());
    let mut rest = message;

    while let Some(index) = rest.find('%') {
        decoded.push_str(&rest[..index]);
        let escape = rest[index..].get(..3).unwrap_or_default();
        let replacement = if escape.eq_ignore_ascii_case("%25") {
            "%"
        } else if escape.eq_ignore_ascii_case("%0a") || escape.eq_ignore_ascii_case("%0d") {
            " "
        } else if escape.eq_ignore_ascii_case("%3a") {
            ":"
        } else if escape.eq_ignore_ascii_case("%2c") {
            ","
        } else {
            // Not an escape this format defines; the `%` is literal text.
            decoded.push('%');
            rest = &rest[index + 1..];
            continue;
        };
        decoded.push_str(replacement);
        rest = &rest[index + 3..];
    }

    decoded.push_str(rest);
    decoded
}

/// Ensure content handed to an external tool is newline-terminated.
///
/// Code block extraction strips the line ending before the closing fence, but
/// per CommonMark that final newline is part of the block's content. External
/// tools treat their stdin as a complete file, so a tool with an end-of-file
/// newline rule (e.g. yamllint/ryl `new-line-at-end-of-file`, ruff `W292`)
/// would otherwise emit a false positive. Genuinely empty content is left
/// untouched so we never synthesize content where there is none.
fn ensure_trailing_newline(content: &str) -> std::borrow::Cow<'_, str> {
    if content.is_empty() || content.ends_with('\n') {
        std::borrow::Cow::Borrowed(content)
    } else {
        std::borrow::Cow::Owned(format!("{content}\n"))
    }
}

/// Information about a fenced code block for processing.
#[derive(Debug, Clone)]
pub struct FencedCodeBlockInfo {
    /// 0-indexed line number where opening fence starts.
    pub start_line: usize,
    /// 0-indexed line number of the block's last line: its closing fence, or its
    /// last code line when the block is unterminated.
    pub end_line: usize,
    /// Byte offset where the source of the code starts (the first content line,
    /// container prefix included).
    pub content_start: usize,
    /// Byte offset where the source of the code ends: after the line ending of
    /// its last line, or at the end of the document for an unterminated last line.
    pub content_end: usize,
    /// Language tag extracted from info string (first token).
    pub language: String,
    /// Full info string from the fence.
    pub info_string: String,
    /// The fence character used (` or ~).
    pub fence_char: char,
    /// Length of the fence (3 or more).
    pub fence_length: usize,
    /// What precedes every content line for the containers the block sits in:
    /// blockquote markers, list continuation indent and the fence's own indent.
    /// Formatted code is written back behind it.
    pub content_prefix: String,
    /// The code as CommonMark reads it, with every container prefix removed.
    /// This is what a tool receives.
    pub code: String,
    /// Width in characters of the source prefix removed from each line of `code`,
    /// so a tool's column can be placed in the document.
    pub line_prefix_widths: Vec<usize>,
}

impl FencedCodeBlockInfo {
    /// The source that replaces `content_start..content_end` so the block's code
    /// becomes `code`, every line inside the block's containers.
    pub fn source_for(&self, code: &str) -> String {
        apply_content_prefix(code, &self.content_prefix)
    }

    /// The document column of a 1-indexed `column` on 1-indexed `line` of the
    /// code. `None` for a line the code does not have.
    pub fn document_column(&self, line: usize, column: usize) -> Option<usize> {
        let width = self.line_prefix_widths.get(line.checked_sub(1)?)?;
        Some(column + width)
    }
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
    /// The report for a block whose formatted code was not written back
    /// because it would change the document's structure.
    pub fn refused_write_back(line: usize) -> Self {
        Self {
            file_line: line,
            column: None,
            message:
                "Formatted code cannot be written back without changing the document's structure; block left unchanged"
                    .to_string(),
            severity: DiagnosticSeverity::Error,
            tool: CODE_BLOCK_TOOLS_DIAGNOSTIC_NAME.to_string(),
            code_block_start: line,
        }
    }

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
    /// A coverage or configuration policy stopped processing.
    Policy { message: String, line: usize },
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
                write!(
                    f,
                    "line {line} ({language}): tool '{tool}' not found in allowed lookup locations"
                )
            }
            Self::Policy { message, line } => write!(f, "line {line}: {message}"),
            Self::Aborted { message } => write!(f, "Processing aborted: {message}"),
        }
    }
}

impl ProcessorError {
    /// The 1-indexed line of the code block the error is about, when it is about one.
    pub fn line(&self) -> Option<usize> {
        match self {
            Self::ToolErrorAt { line, .. }
            | Self::NoToolsConfigured { line, .. }
            | Self::ToolBinaryNotFound { line, .. }
            | Self::Policy { line, .. } => Some(*line),
            Self::ToolError(_) | Self::Aborted { .. } => None,
        }
    }

    /// The error without the position [`Display`](std::fmt::Display) prefixes it with.
    fn detail(&self) -> String {
        match self {
            Self::ToolErrorAt { error, .. } => error.to_string(),
            Self::NoToolsConfigured { language, .. } => format!("No tools configured for language '{language}'"),
            Self::ToolBinaryNotFound { tool, .. } => {
                format!("Tool binary '{tool}' not found in allowed lookup locations")
            }
            Self::Policy { message, .. } => message.clone(),
            Self::ToolError(_) | Self::Aborted { .. } => self.to_string(),
        }
    }

    /// Report the error as a finding, at its code block when it names one.
    ///
    /// A run that stopped on this error still has to report it where the reader
    /// can find it, in every output format, and under the same name whichever
    /// path (lint, format, LSP) hit it.
    pub fn to_lint_warning(&self) -> LintWarning {
        let line = self.line().unwrap_or(1);
        LintWarning {
            message: self.detail(),
            line,
            column: 1,
            end_line: line,
            end_column: 1,
            severity: Severity::Error,
            fix: None,
            rule_name: Some(CODE_BLOCK_TOOLS_DIAGNOSTIC_NAME.to_string()),
        }
    }
}

/// The name a failure of the code-block-tools machinery itself is reported under.
///
/// Not a rule name and not a tool id: it names the class of problem, so nothing
/// looks it up in the rule registry.
pub const CODE_BLOCK_TOOLS_DIAGNOSTIC_NAME: &str = "code-block-tools";

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
    /// Whether any messages were collected. Warn-level messages count too.
    pub had_errors: bool,
    /// Error messages for blocks that couldn't be formatted, in the prose form
    /// the terminal shows. Both levels appear here: a message collected under
    /// `on-error = "warn"` is for the reader and nothing more.
    pub error_messages: Vec<String>,
    /// The fail-level subset of the above, as diagnostics rather than prose.
    ///
    /// A tool could not run under a setting of `fail`, so the document was only
    /// partly formatted and the run must not be reported as clean. The lint path
    /// records the same fact as a diagnostic and every output format carries it;
    /// these exist so a format run says it in the same machine-readable place
    /// instead of only in a line of stderr. Messages collected under
    /// `on-error = "warn"` are not failures and are not here.
    pub failures: Vec<CodeBlockDiagnostic>,
}

/// Result of linting the code blocks in a document.
#[derive(Debug, Default)]
pub struct LintOutput {
    /// Findings, including a tool that could not run under `on-error = "fail"`.
    pub diagnostics: Vec<CodeBlockDiagnostic>,
    /// Tools that could not run under `on-error = "warn"`, in the prose form
    /// [`FormatOutput::error_messages`] uses. They are for the reader and are not
    /// findings: `warn` asks to be told and to carry on.
    pub warnings: Vec<String>,
    /// Whether a tool failed to run under any `on-error` setting, so that some
    /// block went unchecked. Such a result says nothing about that block and is
    /// not one to cache.
    pub incomplete: bool,
}

impl FormatOutput {
    /// Whether part of the document went unformatted because a tool could not run.
    pub fn failed(&self) -> bool {
        !self.failures.is_empty()
    }
}

/// Main processor for code block tools.
pub struct CodeBlockToolProcessor<'a> {
    config: &'a CodeBlockToolsConfig,
    flavor: MarkdownFlavor,
    linguist: LinguistResolver,
    registry: ToolRegistry,
    executor: ToolExecutor,
    run_state: std::sync::Arc<super::run_state::RunState>,
    builtin_checks: bool,
    user_aliases: std::collections::HashMap<String, String>,
    /// `config.languages`, keyed by lowercased language name.
    languages: std::collections::HashMap<String, &'a LanguageToolConfig>,
    custom_languages: Vec<String>,
}

impl<'a> CodeBlockToolProcessor<'a> {
    /// Create a new processor with the given configuration and markdown flavor.
    pub fn new(config: &'a CodeBlockToolsConfig, flavor: MarkdownFlavor) -> Self {
        let user_aliases = config
            .language_aliases
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v.to_lowercase()))
            .collect();
        // A fence tag is resolved to a lowercase canonical name, so a `languages` key has
        // to be lowercased to be reachable at all. `BTreeMap` iteration is sorted and
        // uppercase sorts first, so an exact-lowercase key wins over a mixed-case one.
        let languages = config.languages.iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
        let run_state = config.run_state.clone().unwrap_or_default();
        Self {
            builtin_checks: true,
            custom_languages: Vec::new(),
            run_state: std::sync::Arc::clone(&run_state),
            config,
            flavor,
            linguist: LinguistResolver::new(),
            registry: ToolRegistry::new(config.tools.clone()),
            executor: ToolExecutor::new(config.timeout)
                .with_binary_resolution(
                    config.binary_preferences.clone(),
                    super::binary::project_root(&std::env::current_dir().unwrap_or_default()),
                )
                .with_run_state(run_state),
            user_aliases,
            languages,
        }
    }

    /// Invocation accounting and once-per-run policy warnings.
    pub fn run_state(&self) -> &super::run_state::RunState {
        &self.run_state
    }

    fn needs_coverage_scan(&self) -> bool {
        self.config.on_missing_language_tag != OnMissing::Ignore
            || self.config.unknown_language_policy() != OnMissing::Ignore
            || self.config.on_missing_mode_definition != OnMissing::Ignore
            || self.config.on_missing_language_definition == OnMissing::Warn
    }

    fn block_policy(&self, block: &FencedCodeBlockInfo, lint: bool) -> Option<(OnMissing, String, String)> {
        use crate::utils::fence_language::{FenceLanguage, classify};
        match classify(&block.info_string, self.flavor) {
            FenceLanguage::Exempt => return Some((OnMissing::Ignore, String::new(), String::new())),
            FenceLanguage::Language | FenceLanguage::CodeSyntax => {}
            FenceLanguage::Missing => {
                return Some((
                    self.config.on_missing_language_tag,
                    "missing-tag".into(),
                    "Fenced code block has no language tag".into(),
                ));
            }
        }
        let canonical = self.resolve_language(&block.language);
        let known = self.linguist.is_known(&block.language)
            || self
                .custom_languages
                .iter()
                .any(|label| label.eq_ignore_ascii_case(&block.language))
            || self.user_aliases.contains_key(&block.language.to_lowercase())
            || self.languages.contains_key(&block.language.to_lowercase())
            || self.languages.contains_key(&canonical);
        if !known {
            return Some((
                self.config.unknown_language_policy(),
                format!("unknown-tag:{}", block.language.to_lowercase()),
                format!("Unrecognized code block language tag '{}'", block.language),
            ));
        }
        if let Some(language) = self.language_config(&canonical) {
            if !language.enabled {
                return None;
            }
            let (current, opposite, mode) = if lint {
                (&language.lint, &language.format, "lint")
            } else {
                (&language.format, &language.lint, "format")
            };
            if current.is_empty() && !opposite.is_empty() {
                return Some((
                    self.config.on_missing_mode_definition,
                    format!("missing-mode:{canonical}:{mode}"),
                    format!("No {mode} tools configured for language '{canonical}'"),
                ));
            }
        }
        None
    }

    fn policy_diagnostic(
        &self,
        policy: OnMissing,
        key: String,
        message: String,
        line: usize,
    ) -> Result<Option<CodeBlockDiagnostic>, ProcessorError> {
        match policy {
            OnMissing::Ignore => Ok(None),
            OnMissing::Warn => {
                self.run_state.warn(key, message);
                Ok(None)
            }
            OnMissing::Fail => Ok(Some(CodeBlockDiagnostic {
                file_line: line,
                column: None,
                message,
                severity: DiagnosticSeverity::Error,
                tool: CODE_BLOCK_TOOLS_DIAGNOSTIC_NAME.into(),
                code_block_start: line,
            })),
            OnMissing::FailFast => {
                self.run_state.abort();
                Err(ProcessorError::Policy { message, line })
            }
        }
    }

    /// Replay coverage warnings without re-executing tools on a cache hit.
    pub fn replay_cached_warnings(&self, content: &str) {
        for block in self.extract_code_blocks(content) {
            if let Some((policy, key, message)) = self.block_policy(&block, true) {
                if policy == OnMissing::Warn {
                    self.run_state.warn(key, message);
                }
            } else if !block.language.is_empty() && self.config.on_missing_language_definition == OnMissing::Warn {
                let canonical = self.resolve_language(&block.language);
                if self
                    .language_config(&canonical)
                    .is_none_or(|lc| lc.enabled && lc.lint.is_empty() && lc.format.is_empty())
                {
                    self.run_state.warn(
                        format!("missing-definition:{canonical}"),
                        format!("No tools configured for language '{canonical}'"),
                    );
                }
            }
        }
    }

    /// Whether a valid cached lint result represents at least one usable tool.
    pub fn has_lint_work(&self, content: &str) -> bool {
        self.has_work(content, true)
    }

    pub fn has_work(&self, content: &str, lint: bool) -> bool {
        let slot = if lint { ToolSlot::Lint } else { ToolSlot::Format };
        self.extract_code_blocks(content).iter().any(|block| {
            if self.block_policy(block, lint).is_some() || block.content_start == block.content_end {
                return false;
            }
            let canonical = self.resolve_language(&block.language);
            self.language_config(&canonical).is_some_and(|language| {
                language.enabled
                    && (if lint { &language.lint } else { &language.format }).iter().any(|id| {
                        if self.registry.definition_problem(id, slot).is_some() {
                            return false;
                        }
                        if is_rumdl_builtin(id) && is_markdown_language(&canonical) {
                            return self.builtin_checks;
                        }
                        self.registry
                            .resolve(id, slot)
                            .and_then(|tool| tool.command.first())
                            .is_some_and(|binary| self.executor.is_tool_available(binary))
                    })
            })
        })
    }

    /// Share project-declared MD040 language labels with tools-only validation.
    pub fn with_document_config(mut self, config: &crate::config::Config) -> Self {
        let md040: crate::rules::md040_fenced_code_language::md040_config::MD040Config =
            crate::rule_config_serde::load_rule_config(config);
        self.custom_languages = md040.custom_languages;
        self
    }

    /// External commands may read configuration, environment or wrappers that
    /// rumdl cannot validate. A combined file cache must not reuse their verdicts.
    pub fn has_external_lint_work(&self, content: &str) -> bool {
        self.extract_code_blocks(content).iter().any(|block| {
            if self.block_policy(block, true).is_some() || block.content_start == block.content_end {
                return false;
            }
            let canonical = self.resolve_language(&block.language);
            self.language_config(&canonical).is_some_and(|language| {
                language.enabled
                    && language.lint.iter().any(|id| {
                        !(is_rumdl_builtin(id) && is_markdown_language(&canonical))
                            && self.registry.definition_problem(id, ToolSlot::Lint).is_none()
                    })
            })
        })
    }

    pub fn with_builtin_checks(mut self, enabled: bool) -> Self {
        self.builtin_checks = enabled;
        self
    }

    /// Resolve project-local executables relative to this document's project.
    pub fn for_path(mut self, path: &std::path::Path) -> Self {
        self.executor = self.executor.with_binary_resolution(
            self.config.binary_preferences.clone(),
            super::binary::project_root(path),
        );
        self
    }

    /// Configuration for a canonical (lowercase) language name.
    fn language_config(&self, canonical_lang: &str) -> Option<&'a LanguageToolConfig> {
        self.languages.get(canonical_lang).copied()
    }

    /// Report a tool id no registry entry answers to.
    ///
    /// Both the id and the language it was configured under are text out of whichever
    /// file supplied the section (the language through `language-aliases`), so a section
    /// reached through `extends` is described rather than quoted. See
    /// [`crate::config::Config::withheld_rule_values`] for the same policy on rule options.
    fn warn_unknown_tool(&self, tool_id: &str, canonical_lang: &str) {
        if self.config.values_withheld {
            let withheld = crate::config::WITHHELD;
            log::warn!("Unknown tool {withheld} configured for language {withheld}");
        } else {
            log::warn!("Unknown tool '{tool_id}' configured for language '{canonical_lang}'");
        }
    }

    /// Conservatively skip parsing documents without any possible fence.
    /// Fence length and container prefixes must not hide applicable tool work.
    fn has_potential_matching_blocks(&self, content: &str, lint_mode: bool) -> bool {
        self.languages.values().any(|language| {
            language.enabled
                && if lint_mode {
                    !language.lint.is_empty()
                } else {
                    !language.format.is_empty()
                }
        }) && (content.contains("```") || content.contains("~~~"))
    }

    /// Extract all fenced code blocks from content, including those the flavor
    /// adds (MkDocs admonitions and tabs).
    pub fn extract_code_blocks(&self, content: &str) -> Vec<FencedCodeBlockInfo> {
        let mut blocks = extract_fenced_code_blocks(content);

        // For MkDocs flavor, also extract code blocks inside admonitions and tabs
        if self.flavor == MarkdownFlavor::MkDocs {
            let mkdocs_blocks = self.extract_mkdocs_code_blocks(content, &blocks);
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
    /// This method manually scans for them, skipping the lines of the
    /// `fenced` blocks the CommonMark parser found: what looks like an
    /// admonition there is code, such as an example in a `markdown` block.
    fn extract_mkdocs_code_blocks(&self, content: &str, fenced: &[FencedCodeBlockInfo]) -> Vec<FencedCodeBlockInfo> {
        use crate::utils::mkdocs_admonitions;
        use crate::utils::mkdocs_tabs;

        let mut blocks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_fenced_block = vec![false; lines.len()];
        for block in fenced {
            let end = block.end_line.min(lines.len().saturating_sub(1));
            if let Some(span) = in_fenced_block.get_mut(block.start_line..=end) {
                span.fill(true);
            }
        }

        // Track current MkDocs context indent level
        // We only need to know if we're inside any MkDocs block, so a simple stack suffices.
        let mut context_indent_stack: Vec<usize> = Vec::new();

        // Track fence state inside MkDocs context
        let mut in_fence = false;
        let mut fence_start_line: usize = 0;
        let mut fence_content_start: usize = 0;
        let mut fence_char: char = '`';
        let mut fence_length: usize = 0;
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

            // A line of a fenced block can end a context, but never opens one
            // or a fence: it is code.
            if in_fenced_block[i] {
                continue;
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
                        let (code, line_prefix_widths) =
                            strip_indent_from_block(&content[fence_content_start..content_end], &fence_indent_prefix);
                        blocks.push(FencedCodeBlockInfo {
                            start_line: fence_start_line,
                            end_line: i,
                            content_start: fence_content_start,
                            content_end,
                            language: fence_language.clone(),
                            info_string: fence_info_string.clone(),
                            fence_char,
                            fence_length,
                            content_prefix: fence_indent_prefix.clone(),
                            code,
                            line_prefix_widths,
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
        self.language_config(language)
            .and_then(|lc| lc.on_error)
            .unwrap_or(self.config.on_error)
    }

    /// Lint all code blocks in the content.
    ///
    /// Returns diagnostics from all configured linters. Tools that could not run
    /// under `on-error = "warn"` are left out; [`Self::lint_output`] returns them.
    pub fn lint(&self, content: &str) -> Result<Vec<CodeBlockDiagnostic>, ProcessorError> {
        self.lint_output(content).map(|output| output.diagnostics)
    }

    /// Lint all code blocks in the content, with the tool failures `warn` reports.
    ///
    /// Returns `Err` only for `fail-fast` settings. A tool that cannot run under
    /// `on-error = "fail"` stops the document and is reported as a finding at its
    /// block, beside the findings of the blocks checked before it.
    pub fn lint_output(&self, content: &str) -> Result<LintOutput, ProcessorError> {
        // Skip the expensive parse when no tools could possibly produce output.
        // With on_missing=Ignore (default) and no languages with lint tools configured,
        // every block would be skipped, so the parse is wasted work.
        if !self.needs_coverage_scan()
            && self.config.on_missing_language_definition.skips_the_block()
            && !self
                .config
                .languages
                .values()
                .any(|lc| lc.enabled && !lc.lint.is_empty())
        {
            return Ok(LintOutput::default());
        }

        // Quick content check: skip parsing if no configured language appears in the content.
        // This avoids the expensive pulldown-cmark parse when there are no matching code blocks.
        if !self.needs_coverage_scan()
            && self.config.on_missing_language_definition.skips_the_block()
            && !self.has_potential_matching_blocks(content, true)
        {
            return Ok(LintOutput::default());
        }

        let mut incomplete = false;
        let mut all_diagnostics = Vec::new();
        let mut warnings = Vec::new();
        let blocks = self.extract_code_blocks(content);

        for block in blocks {
            if let Some((policy, key, message)) = self.block_policy(&block, true) {
                if let Some(diagnostic) = self.policy_diagnostic(policy, key, message, block.start_line + 1)? {
                    all_diagnostics.push(diagnostic);
                }
                continue;
            }
            if block.language.is_empty() {
                continue;
            }

            let canonical_lang = self.resolve_language(&block.language);

            // Get lint tools for this language
            let lang_config = self.language_config(&canonical_lang);

            // If language is explicitly configured with enabled=false, skip silently
            if let Some(lc) = lang_config
                && !lc.enabled
            {
                continue;
            }

            let lint_tools = match lang_config {
                Some(lc) if !lc.lint.is_empty() => &lc.lint,
                // Defined with format tools only. That is a complete definition:
                // `on-missing-language-definition` speaks for a language the config never
                // gave any tool, not for a mode a defined language does not use.
                Some(lc) if !lc.format.is_empty() => continue,
                _ => {
                    // The language has no tools in either mode
                    match self.config.on_missing_language_definition {
                        // Which languages appear in a document is not known before
                        // reading it, so there is no once-per-run place to say this
                        // and `warn` has nothing to add over `ignore`. Config
                        // validation tells the user that, rather than leaving a
                        // setting quietly doing nothing.
                        OnMissing::Ignore => continue,
                        OnMissing::Warn => {
                            self.run_state.warn(
                                format!("missing-definition:{canonical_lang}"),
                                format!("No tools configured for language '{canonical_lang}'"),
                            );
                            continue;
                        }
                        OnMissing::Fail => {
                            all_diagnostics.push(CodeBlockDiagnostic {
                                file_line: block.start_line + 1,
                                column: None,
                                message: format!("No lint tools configured for language '{canonical_lang}'"),
                                severity: DiagnosticSeverity::Error,
                                tool: CODE_BLOCK_TOOLS_DIAGNOSTIC_NAME.to_string(),
                                code_block_start: block.start_line + 1,
                            });
                            continue;
                        }
                        OnMissing::FailFast => {
                            self.run_state.abort();
                            return Err(ProcessorError::NoToolsConfigured {
                                language: canonical_lang,
                                line: block.start_line + 1,
                            });
                        }
                    }
                }
            };

            // Extract code block content
            if block.content_start >= block.content_end || block.content_end > content.len() {
                continue;
            }
            let code_content = &block.code;

            // Run each lint tool
            for tool_id in lint_tools {
                // Skip built-in "rumdl" tool for markdown - handled separately by embedded markdown linting
                if is_rumdl_builtin(tool_id) && is_markdown_language(&canonical_lang) {
                    if self.builtin_checks {
                        self.run_state.record_execution();
                    }
                    continue;
                }

                if self.registry.definition_problem(tool_id, ToolSlot::Lint).is_some() {
                    continue;
                }
                let Some(resolved_id) = self.registry.resolve_id(tool_id, ToolSlot::Lint) else {
                    self.warn_unknown_tool(tool_id, &canonical_lang);
                    continue;
                };
                let Some(tool_def) = self.registry.get(&resolved_id) else {
                    self.warn_unknown_tool(tool_id, &canonical_lang);
                    continue;
                };

                // Check if tool binary exists before running
                let tool_name = tool_def.command.first().map_or("", String::as_str);
                if !tool_name.is_empty() && !self.executor.is_tool_available(tool_name) {
                    match self.config.on_missing_tool_binary {
                        // `warn` is announced once for the whole run, beside the
                        // configuration that named the tool, rather than once per
                        // block. Here it skips exactly as `ignore` does.
                        OnMissing::Ignore | OnMissing::Warn => {
                            log::debug!("Tool binary '{tool_name}' not found, skipping");
                            continue;
                        }
                        OnMissing::Fail => {
                            all_diagnostics.push(CodeBlockDiagnostic {
                                file_line: block.start_line + 1,
                                column: None,
                                message: format!("Tool binary '{tool_name}' not found in allowed lookup locations"),
                                severity: DiagnosticSeverity::Error,
                                tool: CODE_BLOCK_TOOLS_DIAGNOSTIC_NAME.to_string(),
                                code_block_start: block.start_line + 1,
                            });
                            continue;
                        }
                        OnMissing::FailFast => {
                            self.run_state.abort();
                            return Err(ProcessorError::ToolBinaryNotFound {
                                tool: tool_name.to_string(),
                                language: canonical_lang.clone(),
                                line: block.start_line + 1,
                            });
                        }
                    }
                }

                let tool_input = ensure_trailing_newline(code_content);
                let run = match self.registry.lint_mode(&resolved_id) {
                    // Explicit djLint and Shuck modes prefer verified native checks;
                    // all other formatters and unsupported versions compare output.
                    Some(BuiltinLintMode::FormatCheck) => self
                        .executor
                        .format_check(
                            tool_def,
                            self.registry.native_format_check(&resolved_id).as_ref(),
                            &tool_input,
                            Some(self.config.timeout),
                        )
                        .map(|output| match output {
                            FormatCheckOutput::Formatted(output) => {
                                self.format_check_diagnostics(&output, code_content, tool_id, block.start_line + 1)
                            }
                            FormatCheckOutput::Native { changed: false } => Vec::new(),
                            FormatCheckOutput::Native { changed: true } => {
                                self.unformatted_diagnostic(tool_id, block.start_line + 1)
                            }
                        }),
                    _ => self
                        .executor
                        .lint(tool_def, &tool_input, Some(self.config.timeout))
                        .map(|output| {
                            self.parse_tool_output(
                                &output,
                                tool_id,
                                block.start_line + 1, // Convert to 1-indexed
                            )
                        }),
                };
                match run {
                    Ok(mut diagnostics) => {
                        // A tool counts columns in the code it was given, which
                        // starts after the line's container prefix in the document.
                        for diagnostic in &mut diagnostics {
                            let code_line = diagnostic.file_line.checked_sub(block.start_line + 1);
                            if let (Some(column), Some(line)) = (diagnostic.column.as_mut(), code_line)
                                && let Some(document_column) = block.document_column(line, *column)
                            {
                                *column = document_column;
                            }
                        }
                        all_diagnostics.extend(diagnostics);
                    }
                    Err(error) => match self.get_on_error(&canonical_lang) {
                        // `fail` stops the document here. What the blocks before
                        // this one reported is still true, so it is kept, and the
                        // failure is reported at the block it happened in.
                        OnError::Fail => {
                            let failure = ProcessorError::ToolErrorAt {
                                error,
                                line: block.start_line + 1,
                                language: canonical_lang,
                            };
                            all_diagnostics.push(CodeBlockDiagnostic {
                                file_line: block.start_line + 1,
                                column: None,
                                message: failure.detail(),
                                severity: DiagnosticSeverity::Error,
                                tool: CODE_BLOCK_TOOLS_DIAGNOSTIC_NAME.to_string(),
                                code_block_start: block.start_line + 1,
                            });
                            return Ok(LintOutput {
                                diagnostics: all_diagnostics,
                                warnings,
                                incomplete: true,
                            });
                        }
                        OnError::Warn => {
                            incomplete = true;
                            warnings.push(format!("line {} ({canonical_lang}): {error}", block.start_line + 1));
                        }
                        OnError::Skip => incomplete = true,
                    },
                }
            }
        }

        Ok(LintOutput {
            diagnostics: all_diagnostics,
            warnings,
            incomplete,
        })
    }

    /// Format all code blocks in analysis text (the document's encoding BOM
    /// already removed by the caller, as for `format_with_config`).
    ///
    /// Returns the modified content with formatted code blocks and any errors that occurred.
    /// With `on-missing-*` = `fail`, errors are collected but formatting continues.
    /// With `on-missing-*` = `fail-fast`, returns Err immediately on first error.
    pub fn format(&self, content: &str) -> Result<FormatOutput, ProcessorError> {
        self.format_with_config(content, &crate::config::Config::default(), None)
    }

    /// Format with the document's conflict-marker configuration and per-file exceptions.
    ///
    /// The conflict guard reads the configuration alone. A caller that resolved a
    /// rule selection of its own passes it to `format_for_rules` instead.
    ///
    /// `content` is analysis text: a caller holding a file with a leading byte
    /// order mark removes it first (`utils::bom::strip_bom`) and puts it back on
    /// the result, so a second U+FEFF stays ordinary content.
    pub fn format_with_config(
        &self,
        content: &str,
        config: &crate::config::Config,
        path: Option<&std::path::Path>,
    ) -> Result<FormatOutput, ProcessorError> {
        self.format_guarded(content, None, config, path)
    }

    /// Format under the invocation's rule selection.
    ///
    /// A run that dropped MD092 formats a conflicted document like any other, so
    /// its fenced code is formatted too: the alternative is an outer document
    /// rewritten around code blocks the same run silently left alone.
    pub fn format_for_rules(
        &self,
        content: &str,
        rules: &[Box<dyn crate::rule::Rule>],
        config: &crate::config::Config,
        path: Option<&std::path::Path>,
    ) -> Result<FormatOutput, ProcessorError> {
        self.format_guarded(content, Some(rules), config, path)
    }

    fn format_guarded(
        &self,
        content: &str,
        rules: Option<&[Box<dyn crate::rule::Rule>]>,
        config: &crate::config::Config,
        path: Option<&std::path::Path>,
    ) -> Result<FormatOutput, ProcessorError> {
        let no_output = FormatOutput {
            content: content.to_string(),
            had_errors: false,
            error_messages: Vec::new(),
            failures: Vec::new(),
        };

        let conflicted = match rules {
            Some(rules) => crate::merge_conflict::detect_for_analysis(content, rules, config, path).is_some(),
            None => crate::merge_conflict::detect_configured_for_analysis(content, config, path).is_some(),
        };
        if conflicted {
            return Ok(no_output);
        }

        // Skip the expensive parse when no tools could produce output
        if !self.needs_coverage_scan()
            && self.config.on_missing_language_definition.skips_the_block()
            && !self
                .config
                .languages
                .values()
                .any(|lc| lc.enabled && !lc.format.is_empty())
        {
            return Ok(no_output);
        }

        // Quick content check: skip parsing if no configured language appears in the content
        if !self.needs_coverage_scan()
            && self.config.on_missing_language_definition.skips_the_block()
            && !self.has_potential_matching_blocks(content, false)
        {
            return Ok(no_output);
        }

        let blocks = self.extract_code_blocks(content);

        if blocks.is_empty() {
            return Ok(no_output);
        }

        let mut error_messages = Vec::new();
        let mut failures = Vec::new();
        for block in &blocks {
            if let Some((policy, key, message)) = self.block_policy(block, false)
                && let Some(diagnostic) = self.policy_diagnostic(policy, key, message, block.start_line + 1)?
            {
                error_messages.push(diagnostic.message.clone());
                failures.push(diagnostic);
            }
        }
        // Check fail-fast gaps in source order before reverse-offset execution.
        for block in &blocks {
            if self.block_policy(block, false).is_some() {
                continue;
            }
            let canonical = self.resolve_language(&block.language);
            let language = self.language_config(&canonical);
            if language.is_some_and(|lc| !lc.enabled || lc.format.is_empty() && !lc.lint.is_empty()) {
                continue;
            }
            let Some(language) = language.filter(|lc| !lc.format.is_empty()) else {
                if self.config.on_missing_language_definition == OnMissing::FailFast {
                    self.run_state.abort();
                    return Err(ProcessorError::NoToolsConfigured {
                        language: canonical,
                        line: block.start_line + 1,
                    });
                }
                continue;
            };
            if self.config.on_missing_tool_binary != OnMissing::FailFast {
                continue;
            }
            for id in &language.format {
                if is_rumdl_builtin(id) && is_markdown_language(&canonical) {
                    continue;
                }
                if self.registry.definition_problem(id, ToolSlot::Format).is_some() {
                    continue;
                }
                let Some(tool) = self.registry.resolve(id, ToolSlot::Format) else {
                    continue;
                };
                let Some(binary) = tool.command.first() else {
                    continue;
                };
                if !self.executor.is_tool_available(binary) {
                    self.run_state.abort();
                    return Err(ProcessorError::ToolBinaryNotFound {
                        tool: binary.clone(),
                        language: canonical.clone(),
                        line: block.start_line + 1,
                    });
                }
                // Fallback stages after an available formatter are only known
                // to be needed if it fails; runtime retains that check.
                if language.format_mode == FormatMode::Fallback {
                    break;
                }
            }
        }
        // Execution uses original offsets; coverage policies are checked above
        // in source order, so fail-fast never starts later blocks speculatively.
        let mut planned = Vec::new();
        for (index, block) in blocks.iter().enumerate().rev() {
            if self.block_policy(block, false).is_some() {
                continue;
            }
            if block.language.is_empty() {
                continue;
            }

            let canonical_lang = self.resolve_language(&block.language);

            // Get format tools for this language
            let lang_config = self.language_config(&canonical_lang);

            // If language is explicitly configured with enabled=false, skip silently
            if let Some(lc) = lang_config
                && !lc.enabled
            {
                continue;
            }

            let (format_tools, format_mode) = match lang_config {
                Some(lc) if !lc.format.is_empty() => (&lc.format, lc.format_mode),
                // Defined with lint tools only. See the matching arm in `lint`.
                Some(lc) if !lc.lint.is_empty() => continue,
                _ => {
                    // The language has no tools in either mode
                    match self.config.on_missing_language_definition {
                        // See the matching arm in `lint`.
                        OnMissing::Ignore => continue,
                        OnMissing::Warn => {
                            self.run_state.warn(
                                format!("missing-definition:{canonical_lang}"),
                                format!("No tools configured for language '{canonical_lang}'"),
                            );
                            continue;
                        }
                        OnMissing::Fail => {
                            error_messages.push(format!(
                                "No format tools configured for language '{canonical_lang}' at line {}",
                                block.start_line + 1
                            ));
                            failures.push(CodeBlockDiagnostic {
                                file_line: block.start_line + 1,
                                column: None,
                                message: format!("No format tools configured for language '{canonical_lang}'"),
                                severity: DiagnosticSeverity::Error,
                                tool: CODE_BLOCK_TOOLS_DIAGNOSTIC_NAME.to_string(),
                                code_block_start: block.start_line + 1,
                            });
                            continue;
                        }
                        OnMissing::FailFast => {
                            self.run_state.abort();
                            return Err(ProcessorError::NoToolsConfigured {
                                language: canonical_lang,
                                line: block.start_line + 1,
                            });
                        }
                    }
                }
            };

            // Extract code block content
            if block.content_start >= block.content_end || block.content_end > content.len() {
                continue;
            }
            let code_content_raw = &content[block.content_start..block.content_end];
            let code_content = block.code.clone();

            // In fallback mode the first formatter that succeeds supplies the
            // block. In pipeline mode every formatter runs, each on the output of
            // the last one that succeeded; a formatter that fails, or is skipped,
            // never replaces that output, so the next one starts from it.
            let mut formatted = code_content.clone();
            let mut tool_ran = false;
            for tool_id in format_tools {
                // Skip built-in "rumdl" tool for markdown - handled separately by embedded markdown formatting
                if is_rumdl_builtin(tool_id) && is_markdown_language(&canonical_lang) {
                    if self.builtin_checks {
                        self.run_state.record_execution();
                    }
                    continue;
                }

                if self.registry.definition_problem(tool_id, ToolSlot::Format).is_some() {
                    continue;
                }
                let Some(tool_def) = self.registry.resolve(tool_id, ToolSlot::Format) else {
                    self.warn_unknown_tool(tool_id, &canonical_lang);
                    continue;
                };

                // A linter writes its report to stdout, so running one here replaces the
                // block with that report: `ruff:check` in a format slot turns a clean
                // Python block into the literal text `All checks passed!`. The
                // empty-output guard below does not catch it, because a linter with
                // nothing to say still exits 0 and can still print a summary line.
                // Config validation reports this too, but a warning cannot undo an
                // overwritten block, so the run declines the tool as well.
                if self.registry.fills_format_slot(tool_id) == Some(false) {
                    log::warn!(
                        "Tool '{tool_id}' is a linter and cannot format '{canonical_lang}' code blocks; move it to the lint slot"
                    );
                    continue;
                }

                // Check if tool binary exists before running
                let tool_name = tool_def.command.first().map_or("", String::as_str);
                if !tool_name.is_empty() && !self.executor.is_tool_available(tool_name) {
                    match self.config.on_missing_tool_binary {
                        // See the matching arm in `lint`.
                        OnMissing::Ignore | OnMissing::Warn => {
                            log::debug!("Tool binary '{tool_name}' not found, skipping");
                            continue;
                        }
                        OnMissing::Fail => {
                            error_messages.push(format!(
                                "Tool binary '{tool_name}' not found in allowed lookup locations for language '{canonical_lang}' at line {}",
                                block.start_line + 1
                            ));
                            failures.push(CodeBlockDiagnostic {
                                file_line: block.start_line + 1,
                                column: None,
                                message: format!("Tool binary '{tool_name}' not found in allowed lookup locations"),
                                severity: DiagnosticSeverity::Error,
                                tool: CODE_BLOCK_TOOLS_DIAGNOSTIC_NAME.to_string(),
                                code_block_start: block.start_line + 1,
                            });
                            continue;
                        }
                        OnMissing::FailFast => {
                            self.run_state.abort();
                            return Err(ProcessorError::ToolBinaryNotFound {
                                tool: tool_name.to_string(),
                                language: canonical_lang.clone(),
                                line: block.start_line + 1,
                            });
                        }
                    }
                }

                let tool_input = ensure_trailing_newline(&formatted);
                match self.executor.format(tool_def, &tool_input, Some(self.config.timeout)) {
                    Ok(output) => {
                        // Ensure trailing newline matches original (unindented)
                        formatted = output;
                        if code_content.ends_with('\n') && !formatted.ends_with('\n') {
                            formatted.push('\n');
                        } else if !code_content.ends_with('\n') && formatted.ends_with('\n') {
                            formatted.pop();
                        }
                        tool_ran = true;
                        if format_mode == FormatMode::Fallback {
                            break;
                        }
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
            if tool_ran && formatted != code_content && block.source_for(&formatted) != code_content_raw {
                planned.push(WriteBack { index, code: formatted });
            }
        }

        let (result, refused) = write_back_verified(content, &blocks, &planned, |document| {
            self.extract_code_blocks(document)
        });
        for index in refused {
            let block = &blocks[index];
            let diagnostic = CodeBlockDiagnostic::refused_write_back(block.start_line + 1);
            error_messages.push(format!(
                "line {} ({}): {}",
                diagnostic.file_line,
                self.resolve_language(&block.language),
                diagnostic.message
            ));
            failures.push(diagnostic);
        }

        Ok(FormatOutput {
            content: result,
            had_errors: !error_messages.is_empty(),
            error_messages,
            failures,
        })
    }

    /// Diagnostics for a built-in formatter used in a `lint` slot.
    ///
    /// The formatter's own output is the answer: a block that comes back changed is not
    /// formatted. This comparison handles ordinary formatter IDs and the fallback
    /// for native-check IDs whose installed version does not support reliable stdin checks.
    ///
    /// The comparison mirrors the one [`Self::format`] makes before rewriting a
    /// block, so `check` reports exactly the blocks `fmt` would change. A
    /// formatter that emptied a non-empty block never gets here: the executor
    /// reports it as a tool failure, for `on-error` to handle.
    fn format_check_diagnostics(
        &self,
        output: &str,
        code_content: &str,
        tool_id: &str,
        code_block_start_line: usize,
    ) -> Vec<CodeBlockDiagnostic> {
        let mut formatted = output.to_string();
        if code_content.ends_with('\n') && !formatted.ends_with('\n') {
            formatted.push('\n');
        } else if !code_content.ends_with('\n') && formatted.ends_with('\n') {
            formatted.pop();
        }

        if formatted == code_content {
            return Vec::new();
        }

        self.unformatted_diagnostic(tool_id, code_block_start_line)
    }

    fn unformatted_diagnostic(&self, tool_id: &str, code_block_start_line: usize) -> Vec<CodeBlockDiagnostic> {
        vec![CodeBlockDiagnostic {
            file_line: code_block_start_line,
            column: None,
            message: "Code block is not formatted".to_string(),
            severity: DiagnosticSeverity::Warning,
            tool: tool_id.to_string(),
            code_block_start: code_block_start_line,
        }]
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
                // No position line followed; fall back to what the message itself says.
                diagnostics.push(Self::unpositioned_diagnostic(
                    msg,
                    severity,
                    tool_id,
                    code_block_start_line,
                ));
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

            // Try pattern: "::warning file=f,line=N,col=M::message" (GitHub annotation)
            if let Some(diag) = Self::parse_github_annotation(line, tool_id, code_block_start_line) {
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
            diagnostics.push(Self::unpositioned_diagnostic(
                &msg,
                severity,
                tool_id,
                code_block_start_line,
            ));
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
                    diagnostics.push(Self::unpositioned_diagnostic(
                        line_text,
                        DiagnosticSeverity::Error,
                        tool_id,
                        code_block_start_line,
                    ));
                }
            }
        }

        diagnostics
    }

    /// Build a diagnostic for a message that carried no `file:line:col:` prefix.
    ///
    /// A tool that states the position in prose instead ("parse error: ... at line 3,
    /// column 8") is still telling us where the problem is, so the diagnostic goes there.
    /// Only a message that names no position at all anchors at the fence.
    fn unpositioned_diagnostic(
        message: &str,
        severity: DiagnosticSeverity,
        tool_id: &str,
        code_block_start_line: usize,
    ) -> CodeBlockDiagnostic {
        let (line_offset, column) = Self::parse_position_in_message(message).unwrap_or((0, None));
        CodeBlockDiagnostic {
            file_line: code_block_start_line + line_offset,
            column,
            message: message.to_string(),
            severity,
            tool: tool_id.to_string(),
            code_block_start: code_block_start_line,
        }
    }

    /// Find an "at line N" / "at line N, column M" position stated inside a message.
    ///
    /// The last occurrence wins: the message is a sentence, and a tool that mentions more
    /// than one position ends on the one the diagnostic is about.
    fn parse_position_in_message(message: &str) -> Option<(usize, Option<usize>)> {
        // ASCII-lowercasing preserves byte offsets, so positions found here index `message`
        // itself; only ASCII digits are ever parsed out of it.
        let lower = message.to_ascii_lowercase();
        let after_marker = &lower[lower.rfind("at line ")? + "at line ".len()..];

        let leading_number = |text: &str| -> Option<usize> {
            let end = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
            text[..end].parse::<usize>().ok()
        };

        let line_num = leading_number(after_marker)?;
        let after_line = after_marker.trim_start_matches(|c: char| c.is_ascii_digit());
        let after_separator = after_line
            .trim_start()
            .strip_prefix(',')
            .unwrap_or(after_line)
            .trim_start();
        let column = after_separator.strip_prefix("column ").and_then(leading_number);

        Some((line_num, column))
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

    /// Parse a GitHub Actions workflow command: `::<level> <k=v>,...::<message>`.
    ///
    /// Emitted by `--format github-annotation-native` (sqlfluff) and by other tools with a
    /// GitHub Actions mode. Only the three annotation levels are diagnostics: `::group::`
    /// and `::endgroup::` bracket the findings and carry none, so they are skipped rather
    /// than reported. A missing `line` anchors the diagnostic at the fence, the same place
    /// an unpositioned message goes.
    fn parse_github_annotation(line: &str, tool_id: &str, code_block_start_line: usize) -> Option<CodeBlockDiagnostic> {
        let body = line.strip_prefix("::")?;
        let (head, message) = body.split_once("::")?;

        let (level, properties) = head.split_once(' ').unwrap_or((head, ""));
        let severity = match level {
            "error" => DiagnosticSeverity::Error,
            "warning" => DiagnosticSeverity::Warning,
            "notice" => DiagnosticSeverity::Info,
            _ => return None,
        };

        let property = |name: &str| -> Option<usize> {
            properties
                .split(',')
                .filter_map(|pair| pair.split_once('='))
                .find(|(key, _)| *key == name)
                .and_then(|(_, value)| value.parse::<usize>().ok())
        };

        let message = Self::strip_fixable_markers(&decode_workflow_command(message));
        if message.is_empty() {
            return None;
        }

        Some(CodeBlockDiagnostic {
            file_line: code_block_start_line + property("line").unwrap_or(0),
            column: property("col"),
            message,
            severity,
            tool: tool_id.to_string(),
            code_block_start: code_block_start_line,
        })
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

    /// Parse `Error: <message>` or `Warning: <message>` lines.
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

/// Every fenced code block CommonMark sees in `content`, in document order,
/// inside containers included.
pub fn extract_fenced_code_blocks(content: &str) -> Vec<FencedCodeBlockInfo> {
    let mut blocks = Vec::new();
    let mut current_block: Option<FencedCodeBlockBuilder> = None;

    let options = rumdl_parser_options_for(content);
    let parser = Parser::new_ext(content, options).into_offset_iter();

    // Byte offset at which each line starts, so a fence's line number is a binary
    // search rather than a newline count over everything before it.
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(content.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let line_of = |offset: usize| line_starts.partition_point(|&start| start <= offset) - 1;

    // Where each footnote definition enclosing the current event starts.
    let mut open_footnotes: Vec<usize> = Vec::new();

    for (event, range) in parser {
        match event {
            Event::Start(Tag::FootnoteDefinition(_)) => open_footnotes.push(range.start),
            Event::End(TagEnd::FootnoteDefinition) => {
                open_footnotes.pop();
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let info_string = info.to_string();
                let language = info_string.split_whitespace().next().unwrap_or("").to_string();

                let start_line = line_of(range.start);

                // The code starts on the line after the opening fence
                let content_start = content[range.start..]
                    .find('\n')
                    .map_or(content.len(), |i| range.start + i + 1);

                // The event starts at the fence itself, so everything before it
                // on its line belongs to the containers and the fence's indent.
                let fence_line_prefix = continuation_of_footnote_labels(
                    &content[line_starts[start_line]..range.start],
                    open_footnotes
                        .iter()
                        .filter(|&&start| start >= line_starts[start_line])
                        .map(|&start| start - line_starts[start_line]),
                );
                let fence = &content[range.start..];
                let fence_char = if fence.starts_with('~') { '~' } else { '`' };
                let fence_length = fence.chars().take_while(|&c| c == fence_char).count();

                current_block = Some(FencedCodeBlockBuilder {
                    start_line,
                    content_start,
                    language,
                    info_string,
                    fence_char,
                    fence_length,
                    content_prefix: content_prefix_of(&fence_line_prefix),
                    pieces: Vec::new(),
                });
            }
            Event::Text(text) => {
                if let Some(builder) = current_block.as_mut() {
                    builder.pieces.push((range, text.into_string()));
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(builder) = current_block.take() {
                    // The block's range can run through the line ending of its
                    // last line, so its last byte is what places it.
                    let end_line = line_of(range.end.saturating_sub(1).max(range.start));

                    // The source of the code runs to the end of its last line,
                    // line ending included. An empty block has no source at all.
                    let content_end = builder
                        .pieces
                        .last()
                        .map_or(builder.content_start, |(last, _)| last.end);
                    let (code, line_prefix_widths) =
                        block_code_from_pieces(content, &builder.pieces, |offset| line_starts[line_of(offset)]);

                    blocks.push(FencedCodeBlockInfo {
                        start_line: builder.start_line,
                        end_line,
                        content_start: builder.content_start,
                        content_end: content_end.max(builder.content_start),
                        language: builder.language,
                        info_string: builder.info_string,
                        fence_char: builder.fence_char,
                        fence_length: builder.fence_length,
                        content_prefix: builder.content_prefix,
                        code,
                        line_prefix_widths,
                    });
                }
            }
            _ => {}
        }
    }

    blocks
}

/// Formatted code waiting to replace the code of a block.
#[derive(Debug, Clone)]
pub struct WriteBack {
    /// The block's position among the blocks extracted from the document.
    pub index: usize,
    pub code: String,
}

/// `content` with each write-back's code in place of its block's code, keeping
/// only the write-backs after which `extract` still reads the document as the
/// same blocks with exactly the new code; and the indices of the blocks whose
/// write-back was refused.
///
/// Whether new code can sit where the old code did depends on more than the
/// block's containers: code with a line that closes the fence ends the block
/// early, and everything after that line becomes document. A refused block
/// keeps its original code. `extract` must be the function `blocks` came from,
/// so an unchanged document reads back as exactly `blocks`, in document order.
///
/// Blocks are expected not to overlap. Write-backs to blocks that do are
/// refused, since replacing one would leave the other's range pointing at
/// different text.
pub fn write_back_verified(
    content: &str,
    blocks: &[FencedCodeBlockInfo],
    write_backs: &[WriteBack],
    extract: impl Fn(&str) -> Vec<FencedCodeBlockInfo>,
) -> (String, Vec<usize>) {
    if write_backs.is_empty() {
        return (content.to_string(), Vec::new());
    }
    let overlaps = |a: &WriteBack, b: &WriteBack| {
        let (a, b) = (&blocks[a.index], &blocks[b.index]);
        a.content_start < b.content_end && b.content_start < a.content_end
    };
    let (mut ordered, mut refused): (Vec<&WriteBack>, Vec<&WriteBack>) = write_backs.iter().partition(|write_back| {
        !write_backs
            .iter()
            .any(|other| other.index != write_back.index && overlaps(write_back, other))
    });
    // Last block first, so each replacement leaves the offsets of the blocks
    // before it valid.
    ordered.sort_by_key(|write_back| std::cmp::Reverse(write_back.index));

    let replace = |document: &mut String, write_back: &WriteBack| {
        let block = &blocks[write_back.index];
        document.replace_range(
            block.content_start..block.content_end,
            &block.source_for(&write_back.code),
        );
    };
    // Extractors differ in whether code keeps CRLF line endings: the CommonMark
    // parser reads them as LF, the MkDocs extractor keeps them. Code a formatter
    // wrote with either line ending is the same code.
    let reads_back = |document: &str, codes: &[&str]| {
        extract(document)
            .iter()
            .map(|block| block.code.replace("\r\n", "\n"))
            .eq(codes.iter().map(|code| code.replace("\r\n", "\n")))
    };
    let original_codes: Vec<&str> = blocks.iter().map(|block| block.code.as_str()).collect();

    let refused_indices = |refused: Vec<&WriteBack>| {
        let mut indices: Vec<usize> = refused.iter().map(|write_back| write_back.index).collect();
        indices.sort_unstable();
        indices
    };

    let mut codes = original_codes.clone();
    let mut result = content.to_string();
    for write_back in &ordered {
        replace(&mut result, write_back);
        codes[write_back.index] = &write_back.code;
    }
    if reads_back(&result, &codes) {
        return (result, refused_indices(refused));
    }

    // At least one write-back changes the document's structure: apply them one
    // at a time and keep each only if the document still reads back.
    let mut codes = original_codes;
    let mut result = content.to_string();
    for write_back in ordered {
        let mut candidate = result.clone();
        replace(&mut candidate, write_back);
        let original = std::mem::replace(&mut codes[write_back.index], &write_back.code);
        if reads_back(&candidate, &codes) {
            result = candidate;
        } else {
            codes[write_back.index] = original;
            refused.push(write_back);
        }
    }
    (result, refused_indices(refused))
}

/// Builder for FencedCodeBlockInfo during parsing.
struct FencedCodeBlockBuilder {
    start_line: usize,
    content_start: usize,
    language: String,
    info_string: String,
    fence_char: char,
    fence_length: usize,
    content_prefix: String,
    /// The parser's text events for the code, each with its source range.
    pieces: Vec<(std::ops::Range<usize>, String)>,
}

/// The fence line prefix with each footnote label on it (`[^note]:` and the
/// whitespace after it, at the given byte offsets) replaced by the four spaces a
/// footnote definition's continuation lines are indented by.
///
/// A label is as wide as its name, but a footnote definition always continues
/// at four columns, so its width on the fence line says nothing about the
/// lines after it.
fn continuation_of_footnote_labels(fence_line_prefix: &str, label_starts: impl Iterator<Item = usize>) -> String {
    let mut prefix = fence_line_prefix.to_string();
    let mut label_starts: Vec<usize> = label_starts.collect();
    label_starts.sort_unstable();
    // Later labels first, so earlier offsets stay valid.
    for start in label_starts.into_iter().rev() {
        let Some(label_end) = footnote_label_end(&prefix[start..]) else {
            continue;
        };
        let rest = &prefix[start + label_end..];
        let end = start + label_end + rest.len() - rest.trim_start_matches([' ', '\t']).len();
        prefix.replace_range(start..end, "    ");
    }
    prefix
}

/// The length of the `[^label]:` that `text` starts with.
fn footnote_label_end(text: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in text.char_indices().skip(2) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            ']' => return text[i + 1..].starts_with(':').then_some(i + 2),
            _ => {}
        }
    }
    None
}

/// The prefix a content line repeats for the containers around a fence, from
/// what precedes the opening fence on its own line.
///
/// Only blockquote markers, whitespace and list markers can precede a fence, so
/// keeping `>` and turning everything else into spaces gives the continuation of
/// every container. Tabs become the spaces they span on the fence line, so a
/// container that consumes part of a tab still gets exactly its columns.
///
/// A `>` written without its optional space (`>code`, `>- item`) gets one: on a
/// continuation line the parser takes the first space after `>` as that
/// optional space, so without it the line would lose a column to the quote.
fn content_prefix_of(fence_line_prefix: &str) -> String {
    let mut prefix = String::with_capacity(fence_line_prefix.len() + 1);
    let mut column = 0;
    let mut chars = fence_line_prefix.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '>' => {
                prefix.push('>');
                if !matches!(chars.peek(), Some(' ' | '\t')) {
                    prefix.push(' ');
                }
                column += 1;
            }
            '\t' => {
                let width = 4 - column % 4;
                prefix.extend(std::iter::repeat_n(' ', width));
                column += width;
            }
            _ => {
                prefix.push(' ');
                column += 1;
            }
        }
    }
    prefix
}

/// The code of a fenced block and the source prefix width of each of its lines,
/// from the parser's text events.
///
/// The parser has already removed what CommonMark does not count as code
/// (blockquote markers, list indentation, the fence's indentation). A line can
/// arrive in several events (a CRLF line ending splits one), and one event can
/// span several lines when a line carries no prefix at all (a blank line in a
/// list item). A line's prefix is whatever precedes its first event on its
/// source line. An event the parser rewrote (spaces standing in for a tab a
/// container split) is placed at the start of its source range.
fn block_code_from_pieces(
    content: &str,
    pieces: &[(std::ops::Range<usize>, String)],
    line_start_of: impl Fn(usize) -> usize,
) -> (String, Vec<usize>) {
    let mut code = String::new();
    let mut widths = Vec::new();
    // A carriage return before a line's first event is its line ending (the
    // parser reports a CRLF blank line as just its `\n`), never prefix.
    let prefix_width = |offset: usize| {
        let before = &content[line_start_of(offset)..offset];
        before.strip_suffix('\r').unwrap_or(before).chars().count()
    };

    for (range, text) in pieces {
        if content.get(range.clone()) == Some(text.as_str()) {
            let mut offset = range.start;
            for segment in text.split_inclusive('\n') {
                if code.is_empty() || code.ends_with('\n') {
                    widths.push(prefix_width(offset));
                }
                code.push_str(segment);
                offset += segment.len();
            }
        } else {
            // The parser writes out as spaces the columns of a tab a container
            // only partly consumed. Those columns are code, so they are not prefix.
            if code.is_empty() || code.ends_with('\n') {
                widths.push(prefix_width(range.start).saturating_sub(text.chars().count()));
            }
            code.push_str(text);
        }
    }
    (code, widths)
}

/// Strip a fence's indentation from each line of a code block's source, and
/// report how much was stripped from each line.
fn strip_indent_from_block(content: &str, indent_prefix: &str) -> (String, Vec<usize>) {
    let mut code = String::with_capacity(content.len());
    let mut widths = Vec::new();
    for line in content.split_inclusive('\n') {
        match line.strip_prefix(indent_prefix) {
            Some(stripped) => {
                code.push_str(stripped);
                widths.push(indent_prefix.chars().count());
            }
            None => {
                code.push_str(line);
                widths.push(0);
            }
        }
    }
    (code, widths)
}

/// Put the containers' prefix back in front of each line of formatted code.
///
/// A blank line takes the prefix without its trailing whitespace: `>` inside a
/// blockquote, nothing at all inside a list item.
fn apply_content_prefix(content: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return content.to_string();
    }

    let blank_prefix = prefix.trim_end();
    let mut out = String::with_capacity(content.len() + prefix.len());
    for line in content.split_inclusive('\n') {
        if line == "\n" {
            out.push_str(blank_prefix);
        } else {
            out.push_str(prefix);
        }
        out.push_str(line);
    }
    out
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
        assert_eq!(blocks[0].content_prefix, "");
        assert_eq!(blocks[0].code, "def hello():\n    print(\"Hello\")\n");

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
        assert_eq!(blocks[0].content_prefix, "");
        assert_eq!(blocks[0].code, "echo hello\n");
    }

    #[test]
    fn test_extract_code_blocks_with_indent_prefix() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "  - item\n    ```python\n    print('hi')\n    ```";
        let blocks = processor.extract_code_blocks(content);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content_prefix, "    ");
        assert_eq!(blocks[0].code, "print('hi')\n");
        assert_eq!(blocks[0].line_prefix_widths, vec![4]);
    }

    /// Fence line numbers across content where byte offsets, character counts and line
    /// counts all diverge: multibyte text, CRLF endings, a block in a blockquote, and a
    /// final block with no trailing newline.
    #[test]
    fn test_extract_code_blocks_line_numbers() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "# Ünïcödé 标题\r\n\r\n```python\r\nx = 'é'\r\n```\r\n\r\n> ```sh\n> echo 你好\n> ```\n\n~~~rust\nfn main() {}\n~~~";
        let blocks = processor.extract_code_blocks(content);

        let lines: Vec<(usize, usize)> = blocks.iter().map(|b| (b.start_line, b.end_line)).collect();
        assert_eq!(lines, vec![(2, 4), (6, 8), (10, 12)]);
    }

    /// An unterminated block ends on its own last line, not on the line that
    /// follows it, however it is ended.
    #[test]
    fn test_extract_unterminated_code_block_line_numbers() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let cases = [
            ("> ```sh\n> a\nb\n", (0, 1)),
            ("- ```sh\n  a\n\nb\n", (0, 2)),
            ("```sh\na\n", (0, 1)),
            ("```sh\na", (0, 1)),
            ("```sh\r\na\r\n", (0, 1)),
            ("```sh\n", (0, 0)),
        ];
        for (content, expected) in cases {
            let blocks = processor.extract_code_blocks(content);
            let lines: Vec<(usize, usize)> = blocks.iter().map(|b| (b.start_line, b.end_line)).collect();
            assert_eq!(lines, vec![expected], "{content:?}");
        }
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
        let raw = "    def hello():\n        print('hi')";
        let (stripped, widths) = strip_indent_from_block(raw, "    ");
        assert_eq!(stripped, "def hello():\n    print('hi')");
        assert_eq!(widths, vec![4, 4]);

        assert_eq!(apply_content_prefix(&stripped, "    "), raw);
    }

    /// The code a tool receives from a fence inside containers is the code
    /// CommonMark gives that block: no blockquote markers, no list indentation.
    /// Each expectation is cmark's code block text for the same document.
    #[test]
    fn test_extract_code_blocks_strips_container_prefixes() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // (document, code, content prefix, prefix width of each code line)
        let cases: &[(&str, &str, &str, &[usize])] = &[
            ("> ```m\n> a\n>   b\n> ```\n", "a\n  b\n", "> ", &[2, 2]),
            ("> ```m\n> a\n>\n> b\n> ```\n", "a\n\nb\n", "> ", &[2, 1, 2]),
            // The optional space after `>` is not code, so it is part of the prefix.
            (">```m\n>a\n>  b\n>```\n", "a\n b\n", "> ", &[1, 2]),
            ("> > ```m\n> > a\n> > ```\n", "a\n", "> > ", &[4]),
            ("  > ```m\n  > a\n  > ```\n", "a\n", "  > ", &[4]),
            ("- ```m\n  a\n    b\n  ```\n", "a\n  b\n", "  ", &[2, 2]),
            ("1. ```m\n   a\n\n   b\n   ```\n", "a\n\nb\n", "   ", &[3, 0, 3]),
            ("> - ```m\n>   a\n>\n>   b\n>   ```\n", "a\n\nb\n", ">   ", &[4, 1, 4]),
            ("- > ```m\n  > a\n  > b\n  > ```\n", "a\nb\n", "  > ", &[4, 4]),
            (">- ```m\n>   a\n>   ```\n", "a\n", ">   ", &[4]),
            // Blank lines are code too, at the end of a block or as all of it.
            ("> ```m\n> a\n>\n> ```\n", "a\n\n", "> ", &[2, 1]),
            ("> ```m\n>\n> ```\n", "\n", "> ", &[1]),
            // Unclosed: the block runs to the end of its container, last line included.
            ("- ```m\n  a\n  b", "a\nb", "  ", &[2, 2]),
            ("> ```m\n> a\n", "a\n", "> ", &[2]),
            // A tab the quote marker only partly consumes leaves code columns.
            ("> ```m\n>\ta\n> ```\n", "  a\n", "> ", &[0]),
            // The parser ends code lines with LF, and a carriage return is never
            // part of a prefix.
            ("> ```m\r\n> a\r\n>\r\n> b\r\n> ```\r\n", "a\n\nb\n", "> ", &[2, 1, 2]),
            ("```m\r\na\r\n\r\nb\r\n```\r\n", "a\n\nb\n", "", &[0, 0, 0]),
            // A footnote definition continues at four columns, however long the
            // label that opens it on the fence's line.
            ("[^a]: ```m\n    a\n    ```\n", "a\n", "    ", &[4]),
            ("[^long-label]: ```m\n    a\n    ```\n", "a\n", "    ", &[4]),
            ("[^a]:\t```m\n    a\n    ```\n", "a\n", "    ", &[4]),
            ("[^a]:  \t ```m\n    a\n    ```\n", "a\n", "    ", &[4]),
            ("[^a\\]b]: ```m\n    a\n    ```\n", "a\n", "    ", &[4]),
            ("[^a]: Note.\n\n    ```m\n    a\n    ```\n", "a\n", "    ", &[4]),
            ("> [^a]: ```m\n>     a\n>     ```\n", "a\n", ">     ", &[6]),
            ("- [^a]: ```m\n      a\n      ```\n", "a\n", "      ", &[6]),
            ("[^a]: > ```m\n    > a\n    > ```\n", "a\n", "    > ", &[6]),
            ("[^a]: - ```m\n      a\n      ```\n", "a\n", "      ", &[6]),
        ];

        for &(document, code, prefix, widths) in cases {
            let blocks = processor.extract_code_blocks(document);
            assert_eq!(blocks.len(), 1, "{document:?}");
            let block = &blocks[0];
            assert_eq!(block.code, code, "code of {document:?}");
            assert_eq!(block.content_prefix, prefix, "prefix of {document:?}");
            assert_eq!(block.line_prefix_widths, widths, "widths of {document:?}");
            assert_eq!(
                (block.fence_char, block.fence_length),
                ('`', 3),
                "fence of {document:?}"
            );
        }
    }

    /// The source region of a block's code covers its lines exactly, prefixes
    /// and line endings included, in LF and CRLF documents.
    #[test]
    fn test_extract_code_blocks_container_region() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let cases = [
            ("> ```m\n> a\n>\n> b\n> ```\n", "> a\n>\n> b\n"),
            ("- ```m\n  a\n  ```\n", "  a\n"),
            ("- ```m\n  a\n  b", "  a\n  b"),
            ("> ```m\r\n> a\r\n> b\r\n> ```\r\n", "> a\r\n> b\r\n"),
        ];
        for (document, region) in cases {
            let blocks = processor.extract_code_blocks(document);
            assert_eq!(blocks.len(), 1, "{document:?}");
            assert_eq!(
                &document[blocks[0].content_start..blocks[0].content_end],
                region,
                "{document:?}"
            );
        }
    }

    #[test]
    fn test_apply_content_prefix_blank_lines() {
        assert_eq!(apply_content_prefix("a\n\nb", "> "), "> a\n>\n> b");
        assert_eq!(apply_content_prefix("a\n\nb", "  "), "  a\n\n  b");
        assert_eq!(apply_content_prefix("a\n\nb", ">   "), ">   a\n>\n>   b");
        assert_eq!(apply_content_prefix("a\n\nb", ""), "a\n\nb");
    }

    /// A formatter that indents, adds a blank line and changes text, so every
    /// part of the write-back is exercised.
    fn container_formatter_config() -> CodeBlockToolsConfig {
        use super::super::config::{LanguageToolConfig, ToolDefinition};

        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        config.languages.insert(
            "m".to_string(),
            LanguageToolConfig {
                format: vec!["reshape".to_string()],
                ..Default::default()
            },
        );
        config.tools.insert(
            "reshape".to_string(),
            ToolDefinition {
                command: vec![
                    "awk".to_string(),
                    "NR == 1 { print toupper($0); print \"\"; next } { print \"  \" toupper($0) }".to_string(),
                ],
                ..Default::default()
            },
        );
        config
    }

    /// Formatted code that would end its block early is not written back: the
    /// block stays as it was and the refusal is reported, while every other
    /// block is still formatted.
    #[cfg(unix)]
    #[test]
    fn test_format_refuses_write_back_that_changes_structure() {
        use super::super::config::{LanguageToolConfig, ToolDefinition};

        let mut config = container_formatter_config();
        config.languages.insert(
            "f".to_string(),
            LanguageToolConfig {
                format: vec!["fence".to_string()],
                ..Default::default()
            },
        );
        config.tools.insert(
            "fence".to_string(),
            ToolDefinition {
                command: vec!["awk".to_string(), "{ print \"```\"; print toupper($0) }".to_string()],
                ..Default::default()
            },
        );
        for (language, program) in [
            ("c", "{ sub(/\\r$/, \"\"); printf \"%s\\r\\n\", toupper($0) }"),
            ("cf", "{ sub(/\\r$/, \"\"); printf \"```\\r\\n%s\\r\\n\", toupper($0) }"),
        ] {
            config.languages.insert(
                language.to_string(),
                LanguageToolConfig {
                    format: vec![language.to_string()],
                    ..Default::default()
                },
            );
            config.tools.insert(
                language.to_string(),
                ToolDefinition {
                    command: vec!["awk".to_string(), program.to_string()],
                    ..Default::default()
                },
            );
        }
        // (document, document after formatting, lines of the refused blocks)
        let standard_cases: &[(&str, &str, &[usize])] = &[
            ("```f\na\n```\n", "```f\na\n```\n", &[1]),
            ("> ```f\n> a\n> ```\n", "> ```f\n> a\n> ```\n", &[1]),
            (
                "```m\na\n```\n\n```f\na\n```\n\n- ```m\n  b\n  ```\n",
                "```m\nA\n\n```\n\n```f\na\n```\n\n- ```m\n  B\n\n  ```\n",
                &[5],
            ),
            // A longer fence holds the same output.
            ("````f\na\n````\n", "````f\n```\nA\n````\n", &[]),
            // Code written with CRLF line endings reads back as the same code.
            ("```c\na\n```\n", "```c\nA\r\n```\n", &[]),
            ("```c\r\na\r\n```\r\n", "```c\r\nA\r\n```\r\n", &[]),
            ("> ```c\r\n> a\r\n> ```\r\n", "> ```c\r\n> A\r\n> ```\r\n", &[]),
            ("```cf\r\na\r\n```\r\n", "```cf\r\na\r\n```\r\n", &[1]),
        ];
        // The MkDocs extractor keeps CRLF line endings in the code it reads.
        let mkdocs_cases: &[(&str, &str, &[usize])] = &[
            (
                "!!! note\n\n    ```c\n    a\n    ```\n",
                "!!! note\n\n    ```c\n    A\r\n    ```\n",
                &[],
            ),
            (
                "!!! note\r\n\r\n    ```c\r\n    a\r\n    ```\r\n",
                "!!! note\r\n\r\n    ```c\r\n    A\r\n    ```\r\n",
                &[],
            ),
            (
                "!!! note\r\n\r\n    ```cf\r\n    a\r\n    ```\r\n",
                "!!! note\r\n\r\n    ```cf\r\n    a\r\n    ```\r\n",
                &[3],
            ),
        ];

        for (flavor, cases) in [
            (MarkdownFlavor::Standard, standard_cases),
            (MarkdownFlavor::MkDocs, mkdocs_cases),
        ] {
            let processor = CodeBlockToolProcessor::new(&config, flavor);
            for &(document, expected, refused) in cases {
                let output = processor.format(document).expect("format should succeed");
                assert_eq!(output.content, expected, "{document:?}");
                let lines: Vec<usize> = output.failures.iter().map(|failure| failure.file_line).collect();
                assert_eq!(lines, refused, "{document:?}");
                assert_eq!(output.error_messages.len(), refused.len(), "{document:?}");
                assert_eq!(output.had_errors, !refused.is_empty(), "{document:?}");
                assert!(
                    output
                        .failures
                        .iter()
                        .all(|failure| failure.message.contains("block left unchanged")),
                    "{:?}",
                    output.failures
                );
            }
        }
    }

    /// With MkDocs, an admonition example inside a `markdown` block is part of
    /// that block's code, so only the `markdown` block is formatted.
    #[cfg(unix)]
    #[test]
    fn test_format_mkdocs_admonition_example_inside_a_block() {
        use super::super::config::{LanguageToolConfig, ToolDefinition};

        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        for language in ["markdown", "json"] {
            config.languages.insert(
                language.to_string(),
                LanguageToolConfig {
                    format: vec!["replace".to_string()],
                    ..Default::default()
                },
            );
        }
        config.tools.insert(
            "replace".to_string(),
            ToolDefinition {
                command: vec!["awk".to_string(), "END { print \"x\" }".to_string()],
                ..Default::default()
            },
        );
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        let long = "a".repeat(200);
        let document =
            format!("# T\n\n```markdown\n!!! note\n\n    ```json\n    {{\"k\": \"{long}\"}}\n    ```\n```\n");
        let output = processor.format(&document).expect("format should succeed");
        assert_eq!(output.content, "# T\n\n```markdown\nx\n```\n");
        assert!(output.failures.is_empty(), "{:?}", output.failures);
    }

    /// Replacing one of two overlapping blocks would leave the other's range
    /// pointing at different text, so neither is written back.
    #[test]
    fn test_write_back_refuses_overlapping_blocks() {
        // The last block, and a block inside it starting two bytes in.
        let with_inner = |mut blocks: Vec<FencedCodeBlockInfo>| {
            let mut inner = blocks[1].clone();
            inner.content_start += 2;
            inner.code = inner.code[2..].to_string();
            blocks.push(inner);
            blocks
        };
        let content = format!("```m\nz\n```\n\n```m\n{}\n```\n", "a".repeat(100));
        let blocks = with_inner(extract_fenced_code_blocks(&content));
        let write_backs = [
            WriteBack {
                index: 0,
                code: "Z\n".to_string(),
            },
            WriteBack {
                index: 1,
                code: "A\n".to_string(),
            },
            WriteBack {
                index: 2,
                code: "B\n".to_string(),
            },
        ];

        let (result, refused) = write_back_verified(&content, &blocks, &write_backs, |document| {
            with_inner(extract_fenced_code_blocks(document))
        });
        assert_eq!(result, content.replacen("```m\nz\n", "```m\nZ\n", 1));
        assert_eq!(refused, [1, 2]);
    }

    /// Formatted code goes back inside the same containers, so the document
    /// still parses as the same block with the formatted code in it.
    #[cfg(unix)]
    #[test]
    fn test_format_writes_back_inside_containers() {
        let config = container_formatter_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // (document, document after formatting, the formatted block's code)
        let cases = [
            (
                "- ```m\n  a\n  b\n  ```\n- next\n",
                "- ```m\n  A\n\n    B\n  ```\n- next\n",
                "A\n\n  B\n",
            ),
            (
                "1. ```m\n   a\n   b\n   ```\n",
                "1. ```m\n   A\n\n     B\n   ```\n",
                "A\n\n  B\n",
            ),
            (
                "> ```m\n> a\n> b\n> ```\n",
                "> ```m\n> A\n>\n>   B\n> ```\n",
                "A\n\n  B\n",
            ),
            (
                "> > ```m\n> > a\n> > b\n> > ```\n",
                "> > ```m\n> > A\n> >\n> >   B\n> > ```\n",
                "A\n\n  B\n",
            ),
            (
                "> - ```m\n>   a\n>   b\n>   ```\n",
                "> - ```m\n>   A\n>\n>     B\n>   ```\n",
                "A\n\n  B\n",
            ),
            (
                "- > ```m\n  > a\n  > b\n  > ```\n",
                "- > ```m\n  > A\n  >\n  >   B\n  > ```\n",
                "A\n\n  B\n",
            ),
            // The added blank line after a one-line block stays inside the quote.
            ("> ```m\n> a\n> ```\n", "> ```m\n> A\n>\n> ```\n", "A\n\n"),
            // Code indented past `>` without a space keeps every column.
            (
                ">```m\n>a\n>  b\n>```\n",
                ">```m\n> A\n>\n>    B\n>```\n",
                "A\n\n   B\n",
            ),
            // The same for a list item that starts right after `>`.
            (
                ">2) ```m\n>    a\n>    b\n>    ```\n",
                ">2) ```m\n>    A\n>\n>      B\n>    ```\n",
                "A\n\n  B\n",
            ),
            // A tab after the marker reaches column 4, so the item's content
            // does too; one space per character would leave the code outside it.
            (
                "1.\t```m\n    a\n    b\n    ```\n",
                "1.\t```m\n    A\n\n      B\n    ```\n",
                "A\n\n  B\n",
            ),
            // A footnote definition continues at four columns, not at the
            // width of the label on the fence's line.
            (
                "[^long-label]: ```m\n    a\n    b\n    ```\n",
                "[^long-label]: ```m\n    A\n\n      B\n    ```\n",
                "A\n\n  B\n",
            ),
            (
                "> [^a]: ```m\n>     a\n>     b\n>     ```\n",
                "> [^a]: ```m\n>     A\n>\n>       B\n>     ```\n",
                "A\n\n  B\n",
            ),
            (
                "- [^a]: ```m\n      a\n      b\n      ```\n",
                "- [^a]: ```m\n      A\n\n        B\n      ```\n",
                "A\n\n  B\n",
            ),
            ("```m\na\nb\n```\n", "```m\nA\n\n  B\n```\n", "A\n\n  B\n"),
        ];

        for (document, expected, code) in cases {
            let output = processor.format(document).expect("format should succeed");
            assert!(
                output.error_messages.is_empty(),
                "{document:?}: {:?}",
                output.error_messages
            );
            assert_eq!(output.content, expected, "{document:?}");

            let blocks = processor.extract_code_blocks(&output.content);
            assert_eq!(blocks.len(), 1, "{expected:?}");
            assert_eq!(blocks[0].code, code, "{expected:?}");
        }
    }

    /// A tool's column counts from the start of the code it was given; the
    /// document column adds the line's container prefix.
    #[cfg(unix)]
    #[test]
    fn test_lint_columns_include_container_prefix() {
        use super::super::config::{LanguageToolConfig, ToolDefinition};

        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        config.languages.insert(
            "m".to_string(),
            LanguageToolConfig {
                lint: vec!["pos".to_string()],
                ..Default::default()
            },
        );
        config.tools.insert(
            "pos".to_string(),
            ToolDefinition {
                command: vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    "cat >/dev/null; echo '-:1:1: one'; echo '-:2:3: two'".to_string(),
                ],
                ..Default::default()
            },
        );
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let cases = [
            ("```m\nx\nyyy\n```\n", [(2, 1), (3, 3)]),
            ("- ```m\n  x\n  yyy\n  ```\n", [(2, 3), (3, 5)]),
            ("> ```m\n> x\n>\n> ```\n", [(2, 3), (3, 4)]),
            ("text\n\n> - ```m\n>   x\n>   yyy\n>   ```\n", [(4, 5), (5, 7)]),
        ];
        for (document, expected) in cases {
            let diagnostics = processor.lint(document).expect("lint should succeed");
            let positions: Vec<(usize, usize)> = diagnostics
                .iter()
                .map(|d| (d.file_line, d.column.expect("positioned diagnostic")))
                .collect();
            assert_eq!(positions, expected, "{document:?}");
        }
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
        assert!(diagnostics[0].message.contains("not found in allowed lookup locations"));
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
        assert!(output.error_messages[0].contains("not found in allowed lookup locations"));
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

    /// An admonition inside a fenced block is code, not an admonition, and a
    /// fenced block still ends the admonition it is not indented into.
    #[test]
    fn test_mkdocs_fenced_block_lines_are_code() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::MkDocs);

        // (language, first line, last line) of an extracted block
        type Span<'a> = (&'a str, usize, usize);
        let cases: &[(&str, &[Span<'_>])] = &[
            (
                "```markdown\n!!! note\n\n    ```json\n    {}\n    ```\n```\n",
                &[("markdown", 0, 6)],
            ),
            (
                "> ```md\n> === \"Tab\"\n>\n>     ```py\n>     x\n>     ```\n> ```\n",
                &[("md", 0, 6)],
            ),
            (
                "!!! note\n\n    text\n\n```text\nx\n```\n\n    ```json\n    {}\n    ```\n",
                &[("text", 4, 6)],
            ),
            (
                "```markdown\n!!! note\n```\n\n!!! tip\n\n    ```json\n    {}\n    ```\n",
                &[("markdown", 0, 2), ("json", 6, 8)],
            ),
            (
                "> ```up\n> a\n!!! note\n\n    ```up\n    b\n    ```\n",
                &[("up", 0, 1), ("up", 4, 6)],
            ),
        ];
        for &(content, expected) in cases {
            let extracted = processor.extract_code_blocks(content);
            let blocks: Vec<Span<'_>> = extracted
                .iter()
                .map(|block| (block.language.as_str(), block.start_line, block.end_line))
                .collect();
            assert_eq!(blocks, expected, "{content:?}");
        }
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
                ..Default::default()
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

    /// sqlfluff's `github-annotation-native` output, verbatim, for `SELECT   1  FROM   t`.
    ///
    /// The `::group::` / `::endgroup::` markers bracket every run and carry no finding, so
    /// a parser that treated any `::...::` line as a diagnostic would report five.
    const SQLFLUFF_ANNOTATIONS: &str = "\
::group::stdin
::warning title=SQLFluff,file=stdin,line=1,col=7,endLine=1,endColumn=10::LT01: Expected only single space before numeric literal. Found '   '. [layout.spacing]
::warning title=SQLFluff,file=stdin,line=1,col=11,endLine=1,endColumn=13::LT01: Expected only single space before 'FROM' keyword. Found '  '. [layout.spacing]
::warning title=SQLFluff,file=stdin,line=1,col=17,endLine=1,endColumn=20::LT01: Expected only single space before naked identifier. Found '   '. [layout.spacing]
::endgroup::";

    #[test]
    fn test_github_annotation_maps_line_and_column() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let output = ToolOutput {
            stdout: SQLFLUFF_ANNOTATIONS.to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "sqlfluff:lint", 3);
        assert_eq!(diags.len(), 3, "the group markers carry no finding: {diags:?}");
        assert_eq!(diags.iter().map(|d| d.file_line).collect::<Vec<_>>(), vec![4, 4, 4]);
        assert_eq!(
            diags.iter().map(|d| d.column).collect::<Vec<_>>(),
            vec![Some(7), Some(11), Some(17)]
        );
        assert!(diags[0].message.starts_with("LT01: Expected only single space"));
        assert!(diags.iter().all(|d| matches!(d.severity, DiagnosticSeverity::Warning)));
    }

    #[test]
    fn test_github_annotation_severity_per_level() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let output = ToolOutput {
            stdout: "::error line=1::broken\n::warning line=2::suspicious\n::notice line=3::detail".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 10);
        assert_eq!(diags.len(), 3);
        assert!(matches!(diags[0].severity, DiagnosticSeverity::Error));
        assert!(matches!(diags[1].severity, DiagnosticSeverity::Warning));
        assert!(matches!(diags[2].severity, DiagnosticSeverity::Info));
        assert_eq!(diags[2].file_line, 13);
    }

    #[test]
    fn test_github_annotation_without_line_anchors_at_fence() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let output = ToolOutput {
            stdout: "::error::something the tool could not place".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 12);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].file_line, 12);
        assert_eq!(diags[0].column, None);
        assert_eq!(diags[0].message, "something the tool could not place");
    }

    #[test]
    fn test_github_annotation_decodes_escaped_message() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // The format escapes `%`, `:`, `,` and line breaks, because each of them is a
        // separator in the line the message is embedded in.
        let output = ToolOutput {
            stdout: "::error line=1,col=2::100%25 of rows%3A a%2C b%0Aand more".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 5);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "100% of rows: a, b and more");
        assert_eq!(diags[0].file_line, 6);
        assert_eq!(diags[0].column, Some(2));
    }

    #[test]
    fn test_decode_workflow_command_is_a_single_left_to_right_pass() {
        // `%250A` is an escaped `%` followed by the literal text "0A". Decoding `%25` first
        // and then looking for `%0A` in the result would turn it into a line break.
        assert_eq!(decode_workflow_command("%250A"), "%0A");
        assert_eq!(decode_workflow_command("no escapes"), "no escapes");
        assert_eq!(decode_workflow_command("50% off"), "50% off");
        assert_eq!(decode_workflow_command("trailing %"), "trailing %");
        assert_eq!(decode_workflow_command("%0d%0a"), "  ");
        assert_eq!(decode_workflow_command("%3A%2C"), ":,");
    }

    #[test]
    fn test_position_in_message_anchors_a_prose_position() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        // jq names the position inside the message rather than prefixing it, and exits
        // non-zero, so this arrives through the raw-output fallback.
        let output = ToolOutput {
            stdout: String::new(),
            stderr: "jq: error (at <stdin>:0): syntax error, unexpected '}' at line 3, column 8".to_string(),
            exit_code: 2,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "jq", 20);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].file_line, 23);
        assert_eq!(diags[0].column, Some(8));
    }

    #[test]
    fn test_position_in_message_takes_the_last_one_named() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let output = ToolOutput {
            stdout: "unterminated string started at line 2, column 1 at line 6, column 4".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 0);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].file_line, 6, "the sentence ends on the position it is about");
        assert_eq!(diags[0].column, Some(4));
    }

    #[test]
    fn test_message_naming_no_position_stays_on_the_fence() {
        let config = default_config();
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let output = ToolOutput {
            stdout: "could not parse the input".to_string(),
            stderr: String::new(),
            exit_code: 1,
            success: false,
        };

        let diags = processor.parse_tool_output(&output, "tool", 7);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].file_line, 7);
        assert_eq!(diags[0].column, None);
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
    /// as a formatter) must never replace non-empty content with an empty string.
    /// It is a formatter that failed, so `on-error` decides what is reported.
    /// This test uses `true`, which exits 0 with no output.
    #[test]
    fn test_format_empty_output_does_not_erase_content() {
        use super::super::config::LanguageToolConfig;

        let content = "```toml\nkey = \"value\"\n```\n";
        let config_for = |on_error: OnError| {
            let mut config = default_config();
            config.on_error = on_error;
            config.languages.insert(
                "toml".to_string(),
                LanguageToolConfig {
                    format: vec!["empty-formatter".to_string()],
                    ..Default::default()
                },
            );
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
            config
        };

        let config = config_for(OnError::Fail);
        let result = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default()).format(content);
        assert!(
            matches!(result, Err(ProcessorError::ToolErrorAt { line: 1, .. })),
            "fail must stop at the block: {result:?}"
        );

        let config = config_for(OnError::Warn);
        let output = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default())
            .format(content)
            .unwrap();
        assert_eq!(output.content, content, "warn must leave the block as it was");
        assert_eq!(output.error_messages.len(), 1, "{:?}", output.error_messages);
        assert!(output.error_messages[0].starts_with("line 1 (toml): "));

        let config = config_for(OnError::Skip);
        let output = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default())
            .format(content)
            .unwrap();
        assert_eq!(output.content, content, "skip must leave the block as it was");
        assert!(output.error_messages.is_empty(), "{:?}", output.error_messages);
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
            .registry
            .resolve("tombi", ToolSlot::Format)
            .expect("Should resolve tombi in format context");
        assert!(
            format_def.command.iter().any(|arg| arg == "format"),
            "Bare 'tombi' in format context should resolve to 'tombi format', got: {:?}",
            format_def.command
        );

        // In lint context, bare "tombi" should resolve to "tombi:lint"
        let lint_def = processor
            .registry
            .resolve("tombi", ToolSlot::Lint)
            .expect("Should resolve tombi in lint context");
        assert!(
            lint_def.command.iter().any(|arg| arg == "lint"),
            "Bare 'tombi' in lint context should resolve to 'tombi lint', got: {:?}",
            lint_def.command
        );

        // Explicit suffix should bypass context-aware resolution
        let explicit_def = processor
            .registry
            .resolve("tombi:lint", ToolSlot::Format)
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
            .registry
            .resolve("ruff", ToolSlot::Lint)
            .expect("Should resolve ruff in lint context");
        assert!(
            lint_def.command.iter().any(|arg| arg == "check"),
            "Bare 'ruff' in lint context should resolve to 'ruff check', got: {:?}",
            lint_def.command
        );

        // In format context, bare "ruff" should resolve to "ruff:format"
        let format_def = processor
            .registry
            .resolve("ruff", ToolSlot::Format)
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
            .registry
            .resolve("shellcheck", ToolSlot::Lint)
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
            .registry
            .resolve("sqlfluff", ToolSlot::Format)
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
            .registry
            .resolve("djlint", ToolSlot::Format)
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
            .registry
            .resolve("mytool", ToolSlot::Format)
            .expect("Should resolve user tool in format context");
        assert!(
            def.command.iter().any(|arg| arg == "--format"),
            "User 'mytool' in format context should resolve to mytool:format, got: {:?}",
            def.command
        );

        // In lint context, should fall back to bare "mytool" (no mytool:lint exists)
        let def = processor
            .registry
            .resolve("mytool", ToolSlot::Lint)
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
                .registry
                .resolve("nonexistent-tool-xyz", ToolSlot::Lint)
                .is_none(),
            "Nonexistent tool should return None in lint context"
        );
        assert!(
            processor
                .registry
                .resolve("nonexistent-tool-xyz", ToolSlot::Format)
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

    #[test]
    fn merge_conflict_prevents_external_formatting() {
        use super::super::config::{LanguageToolConfig, ToolDefinition};
        let mut config = default_config();
        config.on_missing_tool_binary = OnMissing::Fail;
        config.languages.insert(
            "testlang".to_string(),
            LanguageToolConfig {
                format: vec!["unavailable".to_string()],
                ..Default::default()
            },
        );
        config.tools.insert(
            "unavailable".to_string(),
            ToolDefinition {
                command: vec!["rumdl-858-tool-that-must-not-run".to_string()],
                ..Default::default()
            },
        );
        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());
        let content = "<<<<<<< HEAD\r\n\n```testlang\ntext\n```\n";
        let output = processor.format(content).unwrap();
        assert_eq!(output.content, content);
        assert!(!output.had_errors);
        assert!(output.failures.is_empty());

        // Disabling the safety rule must let the formatter run, rather than
        // silently skipping every code block because of a documented marker.
        let mut document_config = crate::config::Config::default();
        document_config.global.disable.push("MD092".into());
        let output = processor.format_with_config(content, &document_config, None).unwrap();
        assert!(output.had_errors);
        assert!(!output.failures.is_empty());
        let suppressed = format!("<!-- rumdl-disable MD092 -->\n{content}");
        assert!(processor.format(&suppressed).unwrap().had_errors);
    }

    /// A linter that enforces a trailing newline (like ryl/yamllint
    /// `new-line-at-end-of-file`) must not fire on a fenced code block whose
    /// final newline was stripped during extraction. The content piped to the
    /// tool should be newline-terminated, matching how the tool sees a real file.
    #[cfg(unix)]
    #[test]
    fn test_lint_yaml_block_no_false_new_line_at_eof() {
        use super::super::config::{LanguageToolConfig, ToolDefinition};

        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        config.languages.insert(
            "yaml".to_string(),
            LanguageToolConfig {
                lint: vec!["eof-newline-linter".to_string()],
                ..Default::default()
            },
        );
        // Emits a diagnostic only when stdin's last byte is not a newline.
        // `$(tail -c1)` strips trailing newlines, so it is empty iff the input
        // already ends in a newline.
        config.tools.insert(
            "eof-newline-linter".to_string(),
            ToolDefinition {
                command: vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    "if [ -n \"$(tail -c1)\" ]; then echo '1:1: no newline at end of file'; fi".to_string(),
                ],
                ..Default::default()
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```yaml\nfoo: bar\n```\n";
        let diagnostics = processor.lint(content).expect("lint should succeed");

        assert!(
            diagnostics.is_empty(),
            "EOF-newline linter should not fire on an extracted code block, got: {diagnostics:?}"
        );
    }

    /// Format mode should also feed tools newline-terminated input, so a
    /// formatter that enforces an end-of-file newline sees the content as a
    /// complete file rather than re-adding a newline rumdl would then have to
    /// strip. The formatter here reports whether its stdin ended in a newline.
    #[cfg(unix)]
    #[test]
    fn test_format_feeds_newline_terminated_input() {
        use super::super::config::{LanguageToolConfig, ToolDefinition};

        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        config.languages.insert(
            "yaml".to_string(),
            LanguageToolConfig {
                format: vec!["newline-probe".to_string()],
                ..Default::default()
            },
        );
        config.tools.insert(
            "newline-probe".to_string(),
            ToolDefinition {
                command: vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    "if [ -z \"$(tail -c1)\" ]; then echo HAD_NEWLINE; else echo NO_NEWLINE; fi".to_string(),
                ],
                ..Default::default()
            },
        );

        let processor = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default());

        let content = "```yaml\nfoo: bar\n```\n";
        let output = processor.format(content).expect("format should succeed");

        assert!(
            output.content.contains("HAD_NEWLINE"),
            "Formatter should receive newline-terminated stdin, got: {:?}",
            output.content
        );
    }

    /// A config whose `text` blocks are formatted by `tools` in `mode`. The
    /// stages are small shell filters, so each one leaves a visible mark and a
    /// result names exactly which stages ran and in what order.
    #[cfg(unix)]
    fn staged_config(tools: &[&str], mode: FormatMode, on_error: OnError) -> CodeBlockToolsConfig {
        use super::super::config::ToolDefinition;

        let mut config = default_config();
        config.normalize_language = NormalizeLanguage::Exact;
        config.on_error = on_error;
        config.languages.insert(
            "text".to_string(),
            LanguageToolConfig {
                format: tools.iter().map(ToString::to_string).collect(),
                format_mode: mode,
                ..Default::default()
            },
        );
        for (id, script) in [
            ("upper", "tr a-z A-Z"),
            ("exclaim", "sed 's/$/!/'"),
            ("prefix-a", "sed 's/^/a/'"),
            ("prefix-b", "sed 's/^/b/'"),
            ("identity", "cat"),
            ("fails", "cat >/dev/null; exit 3"),
            ("empty", "cat >/dev/null"),
        ] {
            config.tools.insert(
                id.to_string(),
                ToolDefinition {
                    command: vec!["sh".to_string(), "-c".to_string(), script.to_string()],
                    ..Default::default()
                },
            );
        }
        config
    }

    #[cfg(unix)]
    fn format_staged(tools: &[&str], mode: FormatMode, on_error: OnError) -> Result<FormatOutput, ProcessorError> {
        let config = staged_config(tools, mode, on_error);
        CodeBlockToolProcessor::new(&config, MarkdownFlavor::default()).format("```text\nhello\n```\n")
    }

    #[cfg(unix)]
    fn staged_block(tools: &[&str], mode: FormatMode, on_error: OnError) -> String {
        let output = format_staged(tools, mode, on_error).expect("format runs");
        output
            .content
            .strip_prefix("```text\n")
            .and_then(|rest| rest.strip_suffix("```\n"))
            .unwrap_or_else(|| panic!("fence kept: {:?}", output.content))
            .to_string()
    }

    /// The first formatter that succeeds supplies the block, even when it
    /// changes nothing, and the default is that fallback list.
    #[cfg(unix)]
    #[test]
    fn a_fallback_format_list_stops_at_the_first_success() {
        assert_eq!(LanguageToolConfig::default().format_mode, FormatMode::Fallback);
        for (tools, expected) in [
            (&["upper", "exclaim"][..], "HELLO\n"),
            (&["identity", "upper"][..], "hello\n"),
            (&["fails", "upper", "exclaim"][..], "HELLO\n"),
            (&["empty", "upper", "exclaim"][..], "HELLO\n"),
        ] {
            assert_eq!(
                staged_block(tools, FormatMode::Fallback, OnError::Skip),
                expected,
                "{tools:?}"
            );
        }
    }

    /// Every formatter runs in declaration order on the previous one's output,
    /// and a formatter that changes nothing does not end the pipeline.
    #[cfg(unix)]
    #[test]
    fn a_pipeline_runs_every_formatter_on_the_last_output() {
        for (tools, expected) in [
            (&["upper", "exclaim"][..], "HELLO!\n"),
            (&["identity", "upper"][..], "HELLO\n"),
            (&["prefix-a", "prefix-b"][..], "bahello\n"),
            (&["prefix-b", "prefix-a"][..], "abhello\n"),
            (&["identity"][..], "hello\n"),
        ] {
            assert_eq!(
                staged_block(tools, FormatMode::Pipeline, OnError::Fail),
                expected,
                "{tools:?}"
            );
        }
    }

    /// A stage that fails, or prints nothing, is dropped under `warn` and
    /// `skip`: the next stage starts from the last output that succeeded, never
    /// from the failed stage's output. Only `warn` reports it.
    #[cfg(unix)]
    #[test]
    fn a_failed_pipeline_stage_is_dropped_under_warn_and_skip() {
        for failing in ["fails", "empty"] {
            let tools = ["upper", failing, "exclaim"];
            let warned = format_staged(&tools, FormatMode::Pipeline, OnError::Warn).unwrap();
            assert_eq!(warned.content, "```text\nHELLO!\n```\n", "{tools:?}");
            assert_eq!(warned.error_messages.len(), 1, "{:?}", warned.error_messages);
            assert!(warned.error_messages[0].starts_with("line 1 (text): "));

            let skipped = format_staged(&tools, FormatMode::Pipeline, OnError::Skip).unwrap();
            assert_eq!(skipped.content, "```text\nHELLO!\n```\n", "{tools:?}");
            assert!(skipped.error_messages.is_empty(), "{:?}", skipped.error_messages);
        }
        // With no stage left that succeeds, the block stays as written.
        assert_eq!(
            staged_block(&["fails", "empty"], FormatMode::Pipeline, OnError::Skip),
            "hello\n"
        );
    }

    /// Under `fail`, a stage that fails stops the run at its block, after an
    /// earlier stage succeeded just as before any did.
    #[cfg(unix)]
    #[test]
    fn a_failed_pipeline_stage_stops_the_run_under_fail() {
        for tools in [&["upper", "fails", "exclaim"][..], &["upper", "empty"][..]] {
            let result = format_staged(tools, FormatMode::Pipeline, OnError::Fail);
            assert!(
                matches!(result, Err(ProcessorError::ToolErrorAt { line: 1, .. })),
                "{tools:?}: {result:?}"
            );
        }
    }

    /// A stage whose binary is missing follows `on-missing-tool-binary`, and
    /// the pipeline carries on past it.
    #[cfg(unix)]
    #[test]
    fn a_pipeline_stage_with_a_missing_binary_follows_on_missing_tool_binary() {
        use super::super::config::ToolDefinition;

        let mut config = staged_config(&["upper", "gone", "exclaim"], FormatMode::Pipeline, OnError::Fail);
        config.tools.insert(
            "gone".to_string(),
            ToolDefinition {
                command: vec!["rumdl-test-nonexistent-formatter".to_string()],
                ..Default::default()
            },
        );
        let content = "```text\nhello\n```\n";

        config.on_missing_tool_binary = OnMissing::Ignore;
        let output = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default())
            .format(content)
            .unwrap();
        assert_eq!(output.content, "```text\nHELLO!\n```\n");

        config.on_missing_tool_binary = OnMissing::Fail;
        let output = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default())
            .format(content)
            .unwrap();
        assert_eq!(output.content, "```text\nHELLO!\n```\n");
        assert_eq!(output.failures.len(), 1, "{:?}", output.failures);

        config.on_missing_tool_binary = OnMissing::FailFast;
        let result = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default()).format(content);
        assert!(
            matches!(result, Err(ProcessorError::ToolBinaryNotFound { .. })),
            "{result:?}"
        );
    }

    /// Each stage gets newline-terminated input, and the final block keeps the
    /// original's line ending however the stages end theirs.
    #[cfg(unix)]
    #[test]
    fn a_pipeline_keeps_the_block_line_ending_between_stages() {
        use super::super::config::ToolDefinition;

        let mut config = staged_config(&["strip-newline", "probe"], FormatMode::Pipeline, OnError::Fail);
        for (id, script) in [
            ("strip-newline", "printf '%s' \"$(cat)\""),
            (
                "probe",
                "if [ -z \"$(tail -c1)\" ]; then echo HAD_NEWLINE; else echo NO_NEWLINE; fi",
            ),
        ] {
            config.tools.insert(
                id.to_string(),
                ToolDefinition {
                    command: vec!["sh".to_string(), "-c".to_string(), script.to_string()],
                    ..Default::default()
                },
            );
        }
        let output = CodeBlockToolProcessor::new(&config, MarkdownFlavor::default())
            .format("```text\nhello\n```\n")
            .unwrap();
        assert_eq!(output.content, "```text\nHAD_NEWLINE\n```\n");
    }

    #[test]
    fn test_ensure_trailing_newline() {
        // Non-empty content without a trailing newline gets one appended.
        assert_eq!(ensure_trailing_newline("foo: bar"), "foo: bar\n");
        // Multi-line content gets a single newline appended to the last line.
        assert_eq!(ensure_trailing_newline("a: 1\nb: 2"), "a: 1\nb: 2\n");
        // Content already ending in a newline is returned unchanged.
        assert_eq!(ensure_trailing_newline("foo: bar\n"), "foo: bar\n");
        // A trailing blank line (already newline-terminated) is left alone.
        assert_eq!(ensure_trailing_newline("foo: bar\n\n"), "foo: bar\n\n");
        // Genuinely empty content is left empty — no newline synthesized.
        assert_eq!(ensure_trailing_newline(""), "");
    }
}
