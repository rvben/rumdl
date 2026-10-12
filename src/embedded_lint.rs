//! Linting of embedded markdown content inside fenced code blocks.
//!
//! This module provides functions for checking markdown content that appears
//! inside fenced code blocks with `markdown` or `md` language tags. These
//! functions are used by both the CLI and LSP to lint embedded markdown.

use crate::code_block_tools::{
    CodeBlockToolsConfig, FencedCodeBlockInfo, RUMDL_BUILTIN_TOOL, extract_fenced_code_blocks, is_rumdl_builtin,
};
use crate::config as rumdl_config;
use crate::inline_config::InlineConfig;
use crate::lint_context::LintContext;
use crate::rule::{LintWarning, Rule};

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

    let blocks = markdown_code_blocks(content);

    if blocks.is_empty() {
        return Vec::new();
    }

    let inline_config = InlineConfig::from_content(content);
    let mut all_warnings = Vec::new();

    for block in blocks {
        if block.code.trim().is_empty() {
            continue;
        }

        // Filter rules based on inline config at the block's first line of code
        let block_rules: Vec<Box<dyn Rule>> = rules
            .iter()
            .filter(|rule| !inline_config.is_rule_disabled(rule.name(), block.start_line + 2))
            .map(|rule| dyn_clone::clone_box(&**rule))
            .collect();

        // Nested blocks report positions in this block's code, like its own rules
        let mut block_warnings = check_embedded_markdown_blocks_recursive(&block.code, &block_rules, config, depth + 1);

        // Lint the embedded content, skipping file-scoped rules
        let ctx = LintContext::new(&block.code, config.markdown_flavor(), None);
        for rule in &block_rules {
            match rule.name() {
                "MD041" => continue, // "First line in file should be heading" - not a file
                "MD047" => continue, // "File should end with newline" - not a file
                _ => {}
            }

            if let Ok(rule_warnings) = rule.check(&ctx) {
                block_warnings.extend(rule_warnings);
            }
        }

        for warning in block_warnings {
            all_warnings.push(place_in_document(warning, &block));
        }
    }

    all_warnings
}

/// The fenced code blocks in `content` whose language is `markdown` or `md`.
pub fn markdown_code_blocks(content: &str) -> Vec<FencedCodeBlockInfo> {
    let mut blocks = extract_fenced_code_blocks(content);
    blocks.retain(|block| block.language.eq_ignore_ascii_case("markdown") || block.language.eq_ignore_ascii_case("md"));
    blocks
}

/// Move a warning on a block's code to where that code sits in the document.
///
/// Line `n` of the code is the `n`th line after the opening fence, and each
/// column moves right by the container prefix the parser removed from its line.
/// The fix is dropped: its byte range is in the code, not the document.
fn place_in_document(warning: LintWarning, block: &FencedCodeBlockInfo) -> LintWarning {
    let line_offset = block.start_line + 1;
    LintWarning {
        line: warning.line + line_offset,
        column: block
            .document_column(warning.line, warning.column)
            .unwrap_or(warning.column),
        end_line: warning.end_line + line_offset,
        end_column: block
            .document_column(warning.end_line, warning.end_column)
            .unwrap_or(warning.end_column),
        fix: None,
        ..warning
    }
}

#[cfg(test)]
mod markdown_code_blocks_tests {
    use super::*;

    fn codes(content: &str) -> Vec<String> {
        markdown_code_blocks(content)
            .into_iter()
            .map(|block| block.code)
            .collect()
    }

    #[test]
    fn keeps_only_markdown_and_md_blocks() {
        let content = "# Example\n\n```markdown\n# Heading\nContent here\n```\n\n```md\nAnother heading\n```\n\n```rust\nfn main() {}\n```\n";
        assert_eq!(codes(content), ["# Heading\nContent here\n", "Another heading\n"]);
    }

    #[test]
    fn matches_the_language_case_insensitively() {
        assert_eq!(codes("```MARKDOWN\nContent\n```\n"), ["Content\n"]);
        assert_eq!(codes("~~~Md\nContent\n~~~\n"), ["Content\n"]);
    }

    #[test]
    fn finds_nothing_without_markdown_blocks() {
        assert!(codes("# Just a heading\n\nNo code blocks here\n").is_empty());
        assert!(codes("```\nplain\n```\n").is_empty());
        assert!(codes("").is_empty());
    }

    #[test]
    fn code_omits_container_prefixes_and_fence_indentation() {
        assert_eq!(codes("> ```markdown\n> # H\n>\n> text\n> ```\n"), ["# H\n\ntext\n"]);
        assert_eq!(codes("- ```md\n  # H\n      code\n  ```\n"), ["# H\n    code\n"]);
        assert_eq!(codes("   ```markdown\n   X\n   ```"), ["X\n"]);
    }

    #[test]
    fn content_range_covers_the_code_lines_in_the_document() {
        let cases = [
            ("```markdown\nX\n```\n", "X\n"),
            ("# Doc\n\n```markdown\nContent\n```", "Content\n"),
            ("> ```markdown\n> X\n> ```\n", "> X\n"),
            ("```markdown\n```\n", ""),
            ("```markdown\nX", "X"),
            ("```markdown", ""),
        ];
        for (content, source) in cases {
            let blocks = markdown_code_blocks(content);
            assert_eq!(blocks.len(), 1, "content: {content:?}");
            assert_eq!(
                &content[blocks[0].content_start..blocks[0].content_end],
                source,
                "content: {content:?}"
            );
        }
    }
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
