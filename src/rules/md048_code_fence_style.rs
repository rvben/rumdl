use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::code_fence_utils::CodeFenceStyle;
use crate::utils::range_utils::calculate_match_range;
use toml;

mod md048_config;
use md048_config::MD048Config;

/// Rule MD048: Code fence style
///
/// See [docs/md048.md](../../docs/md048.md) for full documentation, configuration, and examples.
#[derive(Clone)]
pub struct MD048CodeFenceStyle {
    config: MD048Config,
}

impl MD048CodeFenceStyle {
    pub fn new(style: CodeFenceStyle) -> Self {
        Self {
            config: MD048Config { style },
        }
    }

    pub fn from_config_struct(config: MD048Config) -> Self {
        Self { config }
    }

    fn detect_style(&self, ctx: &crate::lint_context::LintContext) -> Option<CodeFenceStyle> {
        // Count occurrences of each fence style (prevalence-based approach)
        let mut backtick_count = 0;
        let mut tilde_count = 0;
        let mut in_code_block = false;

        for line in ctx.content.lines() {
            let trimmed = line.trim_start();

            // Check for code fence markers
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let fence_char = if trimmed.starts_with("```") { '`' } else { '~' };

                if !in_code_block {
                    // Opening fence - count it
                    if fence_char == '`' {
                        backtick_count += 1;
                    } else {
                        tilde_count += 1;
                    }
                    in_code_block = true;
                } else {
                    // Potential closing fence - exit code block
                    in_code_block = false;
                }
            }
        }

        // Use the most prevalent style
        // In case of a tie, prefer backticks (more common, widely supported)
        if backtick_count >= tilde_count && backtick_count > 0 {
            Some(CodeFenceStyle::Backtick)
        } else if tilde_count > 0 {
            Some(CodeFenceStyle::Tilde)
        } else {
            None
        }
    }
}

/// Find the maximum fence length using `target_char` within the body of a fenced block.
///
/// Scans from the line after `opening_line` until the matching closing fence
/// (same `opening_char`, length >= `opening_fence_len`, no trailing content).
/// Returns the maximum number of consecutive `target_char` characters found at
/// the start of any interior line (after stripping leading whitespace).
///
/// This is used to compute the minimum fence length needed when converting a
/// fence from one style to another so that nesting remains unambiguous.
/// For example, converting a `~~~` outer fence that contains ```` ``` ```` inner
/// fences to backtick style requires using ```` ```` ```` (4 backticks) so that
/// the inner 3-backtick fences cannot inadvertently close the outer block.
///
/// `outer_has_info` controls whether interior fence-like lines with info strings
/// (e.g. `` ```rust ``) are counted. Per CommonMark, a line with trailing content
/// after the fence characters can never be a closing fence, so it poses no
/// ambiguity risk in isolation. However, when the outer fence itself has an info
/// string the block is a display block intentionally showing inner fence syntax
/// as examples, and those inner sequences still determine the required outer
/// length so that the outer closing fence (which has no info string) is not
/// mistaken for one of them. When the outer fence has no info string, only bare
/// interior sequences (no info) are counted, since those are the only lines that
/// could close a code block.
fn max_inner_fence_length_of_char(
    lines: &[&str],
    opening_line: usize,
    opening_fence_len: usize,
    opening_char: char,
    target_char: char,
    outer_has_info: bool,
) -> usize {
    let mut max_len = 0usize;

    for line in lines.iter().skip(opening_line + 1) {
        let trimmed = line.trim_start();

        // Stop at the closing fence of the outer block.
        if trimmed.starts_with(opening_char) {
            let len = trimmed.chars().take_while(|&c| c == opening_char).count();
            if len >= opening_fence_len && trimmed[len..].trim().is_empty() {
                break;
            }
        }

        // Track the longest run of target_char at the start of any interior line.
        // Only count sequences that could matter for ambiguity: bare sequences
        // (no info string) are always counted; info-string sequences are counted
        // only when the outer fence also has an info string (display blocks).
        if trimmed.starts_with(target_char) {
            let len = trimmed.chars().take_while(|&c| c == target_char).count();
            let has_info = !trimmed[len..].trim().is_empty();
            if !has_info || outer_has_info {
                max_len = max_len.max(len);
            }
        }
    }

    max_len
}

impl Rule for MD048CodeFenceStyle {
    fn name(&self) -> &'static str {
        "MD048"
    }

    fn description(&self) -> &'static str {
        "Code fence style should be consistent"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let line_index = &ctx.line_index;

        let mut warnings = Vec::new();

        let target_style = match self.config.style {
            CodeFenceStyle::Consistent => self.detect_style(ctx).unwrap_or(CodeFenceStyle::Backtick),
            _ => self.config.style,
        };

        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        let mut code_block_fence_char = '`';
        let mut code_block_fence_len = 0usize;
        // The fence length to use when writing the converted/lengthened closing fence.
        // May be longer than the original when inner fences require disambiguation by length.
        let mut converted_fence_len = 0usize;
        // True when the opening fence was already the correct style but its length is
        // ambiguous (interior has same-style fences of equal or greater length).
        let mut needs_lengthening = false;

        for (line_num, &line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            if !trimmed.starts_with("```") && !trimmed.starts_with("~~~") {
                continue;
            }

            let fence_char = if trimmed.starts_with("```") { '`' } else { '~' };
            let fence_len = trimmed.chars().take_while(|&c| c == fence_char).count();

            if !in_code_block {
                in_code_block = true;
                code_block_fence_char = fence_char;
                code_block_fence_len = fence_len;

                let needs_conversion = (fence_char == '`' && target_style == CodeFenceStyle::Tilde)
                    || (fence_char == '~' && target_style == CodeFenceStyle::Backtick);

                if needs_conversion {
                    let target_char = if target_style == CodeFenceStyle::Backtick {
                        '`'
                    } else {
                        '~'
                    };

                    // Compute how many target_char characters the converted fence needs.
                    // Must be strictly greater than any inner fence of the target style.
                    let prefix = &line[..line.len() - trimmed.len()];
                    let info = &trimmed[fence_len..];
                    let outer_has_info = !info.trim().is_empty();
                    let max_inner = max_inner_fence_length_of_char(
                        &lines,
                        line_num,
                        fence_len,
                        fence_char,
                        target_char,
                        outer_has_info,
                    );
                    converted_fence_len = fence_len.max(max_inner + 1);
                    needs_lengthening = false;

                    let replacement = format!("{prefix}{}{info}", target_char.to_string().repeat(converted_fence_len));

                    let fence_start = line.len() - trimmed.len();
                    let fence_end = fence_start + fence_len;
                    let (start_line, start_col, end_line, end_col) =
                        calculate_match_range(line_num + 1, line, fence_start, fence_end - fence_start);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        message: format!(
                            "Code fence style: use {} instead of {}",
                            if target_style == CodeFenceStyle::Backtick {
                                "```"
                            } else {
                                "~~~"
                            },
                            if fence_char == '`' { "```" } else { "~~~" }
                        ),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range_with_length(line_num + 1, 1, line.len()),
                            replacement,
                        }),
                    });
                } else {
                    // Already the correct style. Check for fence-length ambiguity:
                    // if the interior contains same-style fences of equal or greater
                    // length, the outer fence cannot be distinguished from an inner
                    // closing fence and must be made longer.
                    let prefix = &line[..line.len() - trimmed.len()];
                    let info = &trimmed[fence_len..];
                    let outer_has_info = !info.trim().is_empty();
                    let max_inner = max_inner_fence_length_of_char(
                        &lines,
                        line_num,
                        fence_len,
                        fence_char,
                        fence_char,
                        outer_has_info,
                    );
                    if max_inner >= fence_len {
                        converted_fence_len = max_inner + 1;
                        needs_lengthening = true;

                        let replacement =
                            format!("{prefix}{}{info}", fence_char.to_string().repeat(converted_fence_len));

                        let fence_start = line.len() - trimmed.len();
                        let fence_end = fence_start + fence_len;
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num + 1, line, fence_start, fence_end - fence_start);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            message: format!(
                                "Code fence length is ambiguous: outer fence ({fence_len} {}) \
                                 contains interior fence sequences of equal length; \
                                 use {converted_fence_len}",
                                if fence_char == '`' { "backticks" } else { "tildes" },
                            ),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range_with_length(line_num + 1, 1, line.len()),
                                replacement,
                            }),
                        });
                    } else {
                        converted_fence_len = fence_len;
                        needs_lengthening = false;
                    }
                }
            } else {
                // Inside a code block — check if this is the closing fence.
                let is_closing = fence_char == code_block_fence_char
                    && fence_len >= code_block_fence_len
                    && trimmed[fence_len..].trim().is_empty();

                if is_closing {
                    let needs_conversion = (fence_char == '`' && target_style == CodeFenceStyle::Tilde)
                        || (fence_char == '~' && target_style == CodeFenceStyle::Backtick);

                    if needs_conversion || needs_lengthening {
                        let target_char = if needs_conversion {
                            if target_style == CodeFenceStyle::Backtick {
                                '`'
                            } else {
                                '~'
                            }
                        } else {
                            fence_char
                        };

                        let prefix = &line[..line.len() - trimmed.len()];
                        let replacement = format!("{prefix}{}", target_char.to_string().repeat(converted_fence_len));

                        let fence_start = line.len() - trimmed.len();
                        let fence_end = fence_start + fence_len;
                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num + 1, line, fence_start, fence_end - fence_start);

                        let message = if needs_conversion {
                            format!(
                                "Code fence style: use {} instead of {}",
                                if target_style == CodeFenceStyle::Backtick {
                                    "```"
                                } else {
                                    "~~~"
                                },
                                if fence_char == '`' { "```" } else { "~~~" }
                            )
                        } else {
                            format!(
                                "Code fence length is ambiguous: closing fence ({fence_len} {}) \
                                 must match the lengthened outer fence; use {converted_fence_len}",
                                if fence_char == '`' { "backticks" } else { "tildes" },
                            )
                        };

                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            message,
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range_with_length(line_num + 1, 1, line.len()),
                                replacement,
                            }),
                        });
                    }

                    in_code_block = false;
                    code_block_fence_len = 0;
                    converted_fence_len = 0;
                    needs_lengthening = false;
                }
                // Lines inside the block that are not the closing fence are left alone.
            }
        }

        Ok(warnings)
    }

    /// Check if this rule should be skipped for performance
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no code fence markers
        ctx.content.is_empty() || (!ctx.likely_has_code() && !ctx.has_char('~'))
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        let target_style = match self.config.style {
            CodeFenceStyle::Consistent => self.detect_style(ctx).unwrap_or(CodeFenceStyle::Backtick),
            _ => self.config.style,
        };

        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::new();
        let mut in_code_block = false;
        let mut code_block_fence_char = '`';
        let mut code_block_fence_len = 0usize;
        let mut converted_fence_len = 0usize;
        let mut needs_lengthening = false;

        for (line_idx, &line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();

            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let fence_char = if trimmed.starts_with("```") { '`' } else { '~' };
                let fence_len = trimmed.chars().take_while(|&c| c == fence_char).count();

                if !in_code_block {
                    in_code_block = true;
                    code_block_fence_char = fence_char;
                    code_block_fence_len = fence_len;

                    let needs_conversion = (fence_char == '`' && target_style == CodeFenceStyle::Tilde)
                        || (fence_char == '~' && target_style == CodeFenceStyle::Backtick);

                    let prefix = &line[..line.len() - trimmed.len()];
                    let info = &trimmed[fence_len..];
                    let outer_has_info = !info.trim().is_empty();

                    if needs_conversion {
                        let target_char = if target_style == CodeFenceStyle::Backtick {
                            '`'
                        } else {
                            '~'
                        };

                        let max_inner = max_inner_fence_length_of_char(
                            &lines,
                            line_idx,
                            fence_len,
                            fence_char,
                            target_char,
                            outer_has_info,
                        );
                        converted_fence_len = fence_len.max(max_inner + 1);
                        needs_lengthening = false;

                        result.push_str(prefix);
                        result.push_str(&target_char.to_string().repeat(converted_fence_len));
                        result.push_str(info);
                    } else {
                        // Already the correct style. Check for fence-length ambiguity.
                        let max_inner = max_inner_fence_length_of_char(
                            &lines,
                            line_idx,
                            fence_len,
                            fence_char,
                            fence_char,
                            outer_has_info,
                        );
                        if max_inner >= fence_len {
                            converted_fence_len = max_inner + 1;
                            needs_lengthening = true;

                            result.push_str(prefix);
                            result.push_str(&fence_char.to_string().repeat(converted_fence_len));
                            result.push_str(info);
                        } else {
                            converted_fence_len = fence_len;
                            needs_lengthening = false;
                            result.push_str(line);
                        }
                    }
                } else {
                    // Inside a code block — check if this is the closing fence.
                    let is_closing = fence_char == code_block_fence_char
                        && fence_len >= code_block_fence_len
                        && trimmed[fence_len..].trim().is_empty();

                    if is_closing {
                        let needs_conversion = (fence_char == '`' && target_style == CodeFenceStyle::Tilde)
                            || (fence_char == '~' && target_style == CodeFenceStyle::Backtick);

                        if needs_conversion || needs_lengthening {
                            let target_char = if needs_conversion {
                                if target_style == CodeFenceStyle::Backtick {
                                    '`'
                                } else {
                                    '~'
                                }
                            } else {
                                fence_char
                            };
                            let prefix = &line[..line.len() - trimmed.len()];
                            result.push_str(prefix);
                            result.push_str(&target_char.to_string().repeat(converted_fence_len));
                        } else {
                            result.push_str(line);
                        }

                        in_code_block = false;
                        code_block_fence_len = 0;
                        converted_fence_len = 0;
                        needs_lengthening = false;
                    } else {
                        // Inside block, not the closing fence — preserve as-is.
                        result.push_str(line);
                    }
                }
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }

        // Remove the last newline if the original content didn't end with one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD048Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_backtick_style_with_backticks() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_backtick_style_with_tildes() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "~~~\ncode\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2); // Opening and closing fence
        assert!(result[0].message.contains("use ``` instead of ~~~"));
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_tilde_style_with_tildes() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
        let content = "~~~\ncode\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_tilde_style_with_backticks() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
        let content = "```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2); // Opening and closing fence
        assert!(result[0].message.contains("use ~~~ instead of ```"));
    }

    #[test]
    fn test_consistent_style_tie_prefers_backtick() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
        // One backtick fence and one tilde fence - tie should prefer backticks
        let content = "```\ncode\n```\n\n~~~\nmore code\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Backticks win due to tie-breaker, so tildes should be flagged
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 5);
        assert_eq!(result[1].line, 7);
    }

    #[test]
    fn test_consistent_style_tilde_most_prevalent() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
        // Two tilde fences and one backtick fence - tildes are most prevalent
        let content = "~~~\ncode\n~~~\n\n```\nmore code\n```\n\n~~~\neven more\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Tildes are most prevalent, so backticks should be flagged
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 5);
        assert_eq!(result[1].line, 7);
    }

    #[test]
    fn test_detect_style_backtick() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
        let ctx = LintContext::new("```\ncode\n```", crate::config::MarkdownFlavor::Standard, None);
        let style = rule.detect_style(&ctx);

        assert_eq!(style, Some(CodeFenceStyle::Backtick));
    }

    #[test]
    fn test_detect_style_tilde() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
        let ctx = LintContext::new("~~~\ncode\n~~~", crate::config::MarkdownFlavor::Standard, None);
        let style = rule.detect_style(&ctx);

        assert_eq!(style, Some(CodeFenceStyle::Tilde));
    }

    #[test]
    fn test_detect_style_none() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
        let ctx = LintContext::new("No code fences here", crate::config::MarkdownFlavor::Standard, None);
        let style = rule.detect_style(&ctx);

        assert_eq!(style, None);
    }

    #[test]
    fn test_fix_backticks_to_tildes() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
        let content = "```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "~~~\ncode\n~~~");
    }

    #[test]
    fn test_fix_tildes_to_backticks() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "~~~\ncode\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "```\ncode\n```");
    }

    #[test]
    fn test_fix_preserves_fence_length() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
        let content = "````\ncode with backtick\n```\ncode\n````";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "~~~~\ncode with backtick\n```\ncode\n~~~~");
    }

    #[test]
    fn test_fix_preserves_language_info() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "~~~rust\nfn main() {}\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "```rust\nfn main() {}\n```");
    }

    #[test]
    fn test_indented_code_fences() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
        let content = "  ```\n  code\n  ```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_fix_indented_fences() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
        let content = "  ```\n  code\n  ```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "  ~~~\n  code\n  ~~~");
    }

    #[test]
    fn test_nested_fences_not_changed() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
        let content = "```\ncode with ``` inside\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "~~~\ncode with ``` inside\n~~~");
    }

    #[test]
    fn test_multiple_code_blocks() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "~~~\ncode1\n~~~\n\nText\n\n~~~python\ncode2\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 4); // 2 opening + 2 closing fences
    }

    #[test]
    fn test_empty_content() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_preserve_trailing_newline() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "~~~\ncode\n~~~\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "```\ncode\n```\n");
    }

    #[test]
    fn test_no_trailing_newline() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "~~~\ncode\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "```\ncode\n```");
    }

    #[test]
    fn test_default_config() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
        let (name, _config) = rule.default_config_section().unwrap();
        assert_eq!(name, "MD048");
    }

    /// Tilde outer fence containing backtick inner fence: converting to backtick
    /// style must use a longer fence (4 backticks) to preserve valid nesting.
    #[test]
    fn test_tilde_outer_with_backtick_inner_uses_longer_fence() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "~~~text\n```rust\ncode\n```\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The outer fence must be 4 backticks to disambiguate from the inner 3-backtick fences.
        assert_eq!(fixed, "````text\n```rust\ncode\n```\n````");
    }

    /// check() warns about the outer tilde fences and the fix replacements use the
    /// correct (longer) fence length.
    #[test]
    fn test_check_tilde_outer_with_backtick_inner_warns_with_correct_replacement() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "~~~text\n```rust\ncode\n```\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Only the outer tilde fences are warned about; inner backtick fences are untouched.
        assert_eq!(warnings.len(), 2);
        let open_fix = warnings[0].fix.as_ref().unwrap();
        let close_fix = warnings[1].fix.as_ref().unwrap();
        assert_eq!(open_fix.replacement, "````text");
        assert_eq!(close_fix.replacement, "````");
    }

    /// When the inner backtick fences use 4 backticks, the outer converted fence
    /// must use at least 5.
    #[test]
    fn test_tilde_outer_with_longer_backtick_inner() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "~~~text\n````rust\ncode\n````\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "`````text\n````rust\ncode\n````\n`````");
    }

    /// Backtick outer fence containing tilde inner fence: converting to tilde
    /// style must use a longer tilde fence.
    #[test]
    fn test_backtick_outer_with_tilde_inner_uses_longer_fence() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
        let content = "```text\n~~~rust\ncode\n~~~\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "~~~~text\n~~~rust\ncode\n~~~\n~~~~");
    }

    // -----------------------------------------------------------------------
    // Option A: fence-length ambiguity detection
    // -----------------------------------------------------------------------

    /// A backtick block that already uses backtick style but whose outer fence
    /// length matches an interior backtick sequence must be flagged and lengthened.
    #[test]
    fn test_ambiguous_backtick_fence_detected() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        // Outer fence is 3 backticks; interior has a line starting with 3 backticks.
        // This is the "retroactive" broken pattern from an old MD048 conversion.
        let content = "```text\n```rust\ncode\n```\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Opening and closing fences should both be flagged for lengthening.
        assert_eq!(warnings.len(), 2, "expected 2 warnings, got {warnings:?}");
        assert!(
            warnings[0].message.contains("ambiguous"),
            "opening message should mention ambiguity: {}",
            warnings[0].message
        );
        assert!(
            warnings[1].message.contains("ambiguous"),
            "closing message should mention ambiguity: {}",
            warnings[1].message
        );
        // Fix should lengthen to 4 backticks.
        assert_eq!(warnings[0].fix.as_ref().unwrap().replacement, "````text");
        assert_eq!(warnings[1].fix.as_ref().unwrap().replacement, "````");
    }

    /// fix() lengthens the outer fence of the already-backtick-style ambiguous block.
    ///
    /// The orphaned fourth fence (` ``` `) is the second code block opened by the
    /// broken conversion and is NOT part of the fixed block — it remains unchanged.
    /// Option A only fixes the ambiguous outer+close fence pair.
    #[test]
    fn test_ambiguous_backtick_fence_fixed() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        // Retroactive broken-conversion pattern:
        //   line 0: ```text   ← opens block (3 backticks)
        //   line 1: ```rust   ← interior content (info string prevents closing)
        //   line 2: code
        //   line 3: ```       ← closes block 1 (CommonMark: 3 >= 3, no info)
        //   line 4: ```       ← orphaned opening of block 2
        let content = "```text\n```rust\ncode\n```\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Lines 0 and 3 are lengthened; line 4 (orphaned fence) is unchanged.
        assert_eq!(fixed, "````text\n```rust\ncode\n````\n```");
    }

    /// Same for tilde style: a tilde outer fence with a same-length tilde interior
    /// should be flagged and lengthened.
    #[test]
    fn test_ambiguous_tilde_fence_fixed() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
        let content = "~~~text\n~~~rust\ncode\n~~~\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Lines 0 and 3 lengthened; line 4 (orphaned) unchanged.
        assert_eq!(fixed, "~~~~text\n~~~rust\ncode\n~~~~\n~~~");
    }

    /// No warning when the outer fence is already longer than any interior fence.
    #[test]
    fn test_no_ambiguity_when_outer_is_longer() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "````text\n```rust\ncode\n```\n````";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(
            warnings.len(),
            0,
            "should have no warnings when outer is already longer"
        );
    }

    /// When interior fence is longer than the outer, the outer must be lengthened
    /// to max_inner + 1, not just outer + 1.
    ///
    /// Structure: 3-backtick outer, 5-backtick interior (with info string → not
    /// a closing fence), 5-backtick closer (IS the closing fence since 5 >= 3),
    /// then an orphaned 3-backtick second block.
    #[test]
    fn test_ambiguity_with_longer_inner_fence() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        // line 0: ```text    ← opens block (3 backticks)
        // line 1: `````rust  ← interior content (5 backticks, info string)
        // line 2: code
        // line 3: `````      ← closes block 1 (5 >= 3, no info)
        // line 4: ```        ← orphaned second block
        let content = "```text\n`````rust\ncode\n`````\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Must use 6 backticks (max_inner=5, so 5+1=6). Line 4 (orphaned) unchanged.
        assert_eq!(fixed, "``````text\n`````rust\ncode\n``````\n```");
    }

    /// Consistent style with ambiguous fence: lengthening fires even when using Consistent.
    #[test]
    fn test_ambiguity_detected_with_consistent_style() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
        let content = "```text\n```rust\ncode\n```\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].fix.as_ref().unwrap().replacement, "````text");
        assert_eq!(warnings[1].fix.as_ref().unwrap().replacement, "````");
    }

    // -----------------------------------------------------------------------
    // outer_has_info filtering: boundary cases
    // -----------------------------------------------------------------------

    /// Cross-style conversion where outer has NO info string: interior info-string
    /// sequences are not counted, only bare sequences are.
    ///
    /// Without this filtering, the longer info-string sequence would inflate
    /// the required length beyond what is actually needed.
    #[test]
    fn test_cross_style_no_info_outer_counts_only_bare_interior() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        // Outer tilde fence (no info). Interior has a 5-backtick info-string sequence
        // AND a 3-backtick bare sequence. With outer_has_info=false, only the bare
        // sequence (len=3) is counted → outer becomes 4, not 6.
        let content = "~~~\n`````rust\ncode\n```\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // 4 backticks (bare seq len=3 → 3+1=4). If info-string seq were counted
        // instead, this would incorrectly be 6 backticks.
        assert_eq!(fixed, "````\n`````rust\ncode\n```\n````");
    }

    /// Cross-style conversion where outer HAS an info string: interior info-string
    /// sequences ARE counted, ensuring the outer is long enough.
    #[test]
    fn test_cross_style_with_info_outer_counts_info_sequences() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        // Outer tilde fence (info "text"). Interior has only info-string backtick
        // sequences (no bare close inside). With outer_has_info=true, these ARE
        // counted → outer becomes 4.
        let content = "~~~text\n```rust\nexample\n```rust\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "````text\n```rust\nexample\n```rust\n````");
    }

    /// Same-style block where outer has an info string but interior contains only
    /// bare sequences SHORTER than the outer fence: no ambiguity, no warning.
    #[test]
    fn test_same_style_info_outer_shorter_bare_interior_no_warning() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        // Outer is 4 backticks with info "text". Interior shows raw fence syntax
        // (3-backtick bare lines). These are shorter than outer (3 < 4) so they
        // cannot close the outer block → no ambiguity.
        let content = "````text\n```\nshowing raw fence\n```\n````";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(
            warnings.len(),
            0,
            "shorter bare interior sequences cannot close a 4-backtick outer"
        );
    }

    /// Same-style block where outer has NO info string and interior has shorter
    /// bare sequences: no ambiguity, no warning.
    #[test]
    fn test_same_style_no_info_outer_shorter_bare_interior_no_warning() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        // Outer is 4 backticks (no info). Interior has 3-backtick bare sequences.
        // 3 < 4 → they cannot close the outer block → no ambiguity.
        let content = "````\n```\nsome code\n```\n````";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(
            warnings.len(),
            0,
            "shorter bare interior sequences cannot close a 4-backtick outer (no info)"
        );
    }

    /// The combined MD013+MD048 fix must be idempotent: applying the fix twice
    /// must produce the same result as applying it once, and must not introduce
    /// double blank lines (MD012).
    #[test]
    fn test_fix_idempotent_no_double_blanks_with_nested_fences() {
        use crate::fix_coordinator::FixCoordinator;
        use crate::rules::Rule;
        use crate::rules::md013_line_length::MD013LineLength;

        // This is the exact pattern that caused double blank lines when MD048 and
        // MD013 were applied together: a tilde outer fence with an inner backtick
        // fence inside a list item that is too long.
        let content = "\
- **edition**: Rust edition to use by default for the code snippets. Default is `\"2015\"`. \
Individual code blocks can be controlled with the `edition2015`, `edition2018`, `edition2021` \
or `edition2024` annotations, such as:

  ~~~text
  ```rust,edition2015
  // This only works in 2015.
  let try = true;
  ```
  ~~~

### Build options
";
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MD013LineLength::new(80, false, false, false, true)),
            Box::new(MD048CodeFenceStyle::new(CodeFenceStyle::Backtick)),
        ];

        let mut first_pass = content.to_string();
        let coordinator = FixCoordinator::new();
        coordinator
            .apply_fixes_iterative(&rules, &[], &mut first_pass, &Default::default(), 10, None)
            .expect("fix should not fail");

        // No double blank lines after first pass.
        let lines: Vec<&str> = first_pass.lines().collect();
        for i in 0..lines.len().saturating_sub(1) {
            assert!(
                !(lines[i].is_empty() && lines[i + 1].is_empty()),
                "Double blank at lines {},{} after first pass:\n{first_pass}",
                i + 1,
                i + 2
            );
        }

        // Second pass must produce identical output (idempotent).
        let mut second_pass = first_pass.clone();
        let rules2: Vec<Box<dyn Rule>> = vec![
            Box::new(MD013LineLength::new(80, false, false, false, true)),
            Box::new(MD048CodeFenceStyle::new(CodeFenceStyle::Backtick)),
        ];
        let coordinator2 = FixCoordinator::new();
        coordinator2
            .apply_fixes_iterative(&rules2, &[], &mut second_pass, &Default::default(), 10, None)
            .expect("fix should not fail");

        assert_eq!(
            first_pass, second_pass,
            "Fix is not idempotent:\nFirst pass:\n{first_pass}\nSecond pass:\n{second_pass}"
        );
    }
}
