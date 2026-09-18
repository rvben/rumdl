//! Linting of embedded markdown content inside fenced code blocks.
//!
//! This module provides functions for checking markdown content that appears
//! inside fenced code blocks with `markdown` or `md` language tags. These
//! functions are used by both the CLI and LSP to lint embedded markdown.

use crate::code_block_tools::{CodeBlockToolsConfig, RUMDL_BUILTIN_TOOL, is_rumdl_builtin};
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
/// Returns true if a built-in rumdl tool is in the lint slot for markdown/md,
/// indicating that rumdl's built-in markdown linting should be applied to markdown code blocks.
pub fn should_lint_embedded_markdown(config: &CodeBlockToolsConfig) -> bool {
    if !config.enabled {
        return false;
    }

    // Check if markdown language is configured with the built-in rumdl tool
    for lang_key in ["markdown", "md"] {
        if let Some(lang_config) = config.languages.get(lang_key)
            && lang_config.enabled
            && lang_config.lint.iter().any(|tool| is_rumdl_builtin(tool))
        {
            return true;
        }
    }

    false
}

/// Enable formatting independently of linting for explicit tool variants.
/// The legacy `lint = ["rumdl"]` setting continues to enable both phases.
pub fn should_format_embedded_markdown(config: &CodeBlockToolsConfig) -> bool {
    config.enabled
        && ["markdown", "md"].iter().any(|lang| {
            config.languages.get(*lang).is_some_and(|language| {
                language.enabled
                    && (language
                        .format
                        .iter()
                        .any(|tool| matches!(tool.as_str(), "rumdl" | "rumdl:format"))
                        || language.lint.iter().any(|tool| tool == RUMDL_BUILTIN_TOOL))
            })
        })
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

    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    let mut stripped: String = lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                ""
            } else if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if has_trailing_newline && !stripped.ends_with('\n') {
        stripped.push('\n');
    }

    let indent_str = " ".repeat(min_indent);
    (stripped, indent_str)
}

#[cfg(test)]
mod activation_tests {
    use super::*;
    use crate::code_block_tools::LanguageToolConfig;

    #[test]
    fn explicit_variants_respect_master_and_language_switches() {
        for language in ["markdown", "md"] {
            for master in [false, true] {
                for enabled in [false, true] {
                    let mut config = CodeBlockToolsConfig {
                        enabled: master,
                        ..Default::default()
                    };
                    config.languages.insert(
                        language.to_string(),
                        LanguageToolConfig {
                            enabled,
                            lint: vec!["rumdl:lint".to_string()],
                            format: vec!["rumdl:format".to_string()],
                            ..Default::default()
                        },
                    );
                    assert_eq!(should_lint_embedded_markdown(&config), master && enabled);
                    assert_eq!(should_format_embedded_markdown(&config), master && enabled);
                }
            }
        }
    }
}
