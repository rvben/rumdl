//! Linting of embedded markdown content inside fenced code blocks.
//!
//! This module provides functions for checking markdown content that appears
//! inside fenced code blocks with `markdown` or `md` language tags. These
//! functions are used by both the CLI and LSP to lint embedded markdown.

use crate::code_block_tools::{CodeBlockToolsConfig, RUMDL_BUILTIN_TOOL};
use crate::config as rumdl_config;
use crate::inline_config::InlineConfig;
use crate::lint_context::LintContext;
use crate::rule::{LintWarning, Rule};
use crate::utils::code_block_utils::CodeBlockUtils;

/// Maximum recursion depth for linting nested markdown blocks.
///
/// Prevents stack overflow from deeply nested or maliciously crafted content.
/// Real-world usage rarely exceeds 2-3 levels.
pub const MAX_EMBEDDED_DEPTH: usize = 5;

/// Check if embedded markdown linting is enabled via code-block-tools configuration.
///
/// Returns true if the special "rumdl" tool is configured for markdown/md language,
/// indicating that rumdl's built-in markdown linting should be applied to markdown code blocks.
pub fn should_lint_embedded_markdown(config: &CodeBlockToolsConfig) -> bool {
    if !config.enabled {
        return false;
    }

    // Check if markdown language is configured with the built-in rumdl tool
    for lang_key in ["markdown", "md"] {
        if let Some(lang_config) = config.languages.get(lang_key)
            && lang_config.enabled
            && lang_config.lint.iter().any(|tool| tool == RUMDL_BUILTIN_TOOL)
        {
            return true;
        }
    }

    false
}

/// Check if content contains fenced code block markers.
pub fn has_fenced_code_blocks(content: &str) -> bool {
    content.contains("```") || content.contains("~~~")
}

/// Check markdown content embedded in fenced code blocks with `markdown` or `md` language.
///
/// Detects markdown code blocks and runs lint checks on their content,
/// returning warnings with adjusted line numbers that point to the correct location
/// in the parent file.
pub fn check_embedded_markdown_blocks(
    content: &str,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
) -> Vec<LintWarning> {
    check_embedded_markdown_blocks_recursive(content, rules, config, 0)
}

/// Internal recursive implementation with depth tracking.
fn check_embedded_markdown_blocks_recursive(
    content: &str,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    depth: usize,
) -> Vec<LintWarning> {
    if depth >= MAX_EMBEDDED_DEPTH {
        return Vec::new();
    }
    if !has_fenced_code_blocks(content) {
        return Vec::new();
    }

    let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);

    if blocks.is_empty() {
        return Vec::new();
    }

    let inline_config = InlineConfig::from_content(content);
    let mut all_warnings = Vec::new();

    for block in blocks {
        let block_content = &content[block.content_start..block.content_end];

        if block_content.trim().is_empty() {
            continue;
        }

        // Calculate the line offset for this block
        let line_offset = content[..block.content_start].matches('\n').count();

        // Compute the 1-indexed line number of the opening fence
        let block_line = line_offset + 1;

        // Filter rules based on inline config at this block's location
        let block_rules: Vec<&Box<dyn Rule>> = rules
            .iter()
            .filter(|rule| !inline_config.is_rule_disabled(rule.name(), block_line))
            .collect();

        let (stripped_content, _common_indent) = strip_common_indent(block_content);

        // Recursively check nested markdown blocks
        let block_rules_owned: Vec<Box<dyn Rule>> = block_rules.iter().map(|r| dyn_clone::clone_box(&***r)).collect();
        let nested_warnings =
            check_embedded_markdown_blocks_recursive(&stripped_content, &block_rules_owned, config, depth + 1);

        // Adjust nested warning line numbers
        for mut warning in nested_warnings {
            warning.line += line_offset;
            warning.end_line += line_offset;
            warning.fix = None;
            all_warnings.push(warning);
        }

        // Lint the embedded content, skipping file-scoped rules
        let ctx = LintContext::new(&stripped_content, config.markdown_flavor(), None);
        for rule in &block_rules {
            match rule.name() {
                "MD041" => continue, // "First line in file should be heading" - not a file
                "MD047" => continue, // "File should end with newline" - not a file
                _ => {}
            }

            if let Ok(rule_warnings) = rule.check(&ctx) {
                for warning in rule_warnings {
                    let adjusted_warning = LintWarning {
                        message: warning.message.clone(),
                        line: warning.line + line_offset,
                        column: warning.column,
                        end_line: warning.end_line + line_offset,
                        end_column: warning.end_column,
                        severity: warning.severity,
                        fix: None,
                        rule_name: warning.rule_name,
                    };
                    all_warnings.push(adjusted_warning);
                }
            }
        }
    }

    all_warnings
}

/// Strip common leading indentation from all non-empty lines.
/// Returns the stripped content and the common indent string.
pub fn strip_common_indent(content: &str) -> (String, String) {
    let lines: Vec<&str> = content.lines().collect();
    let has_trailing_newline = content.ends_with('\n');

    let line_indents: Vec<Vec<char>> = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|c| c.is_whitespace()).collect())
        .collect();

    let common_indent_chars = if line_indents.is_empty() {
        Vec::new()
    } else {
        let mut common = line_indents[0].clone();
        for indent in &line_indents[1..] {
            let mut new_common = Vec::new();
            for (c1, c2) in common.iter().zip(indent.iter()) {
                if c1 == c2 {
                    new_common.push(*c1);
                } else {
                    break;
                }
            }
            common = new_common;
        }
        common
    };

    let indent_str: String = common_indent_chars.iter().collect();
    let common_prefix_byte_len = indent_str.len();

    let mut stripped: String = lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                ""
            } else if line.starts_with(&indent_str) {
                &line[common_prefix_byte_len..]
            } else {
                line.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if has_trailing_newline && !stripped.ends_with('\n') {
        stripped.push('\n');
    }

    (stripped, indent_str)
}
