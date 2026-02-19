use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::code_fence_utils::CodeFenceStyle;
use crate::utils::range_utils::calculate_match_range;
use toml;

mod md048_config;
use md048_config::MD048Config;

/// Parsed fence marker candidate on a single line.
#[derive(Debug, Clone, Copy)]
struct FenceMarker<'a> {
    /// Fence character (` or ~).
    fence_char: char,
    /// Length of the contiguous fence run.
    fence_len: usize,
    /// Byte index where the fence run starts.
    fence_start: usize,
    /// Remaining text after the fence run.
    rest: &'a str,
}

/// Parse a candidate fence marker line.
///
/// CommonMark only recognizes fenced code block markers when indented by at most
/// three spaces (outside container contexts). This parser enforces that bound and
/// returns the marker run and trailing text for further opening/closing checks.
#[inline]
fn parse_fence_marker(line: &str) -> Option<FenceMarker<'_>> {
    let bytes = line.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() && bytes[pos] == b' ' {
        pos += 1;
    }
    if pos > 3 {
        return None;
    }

    let fence_char = match bytes.get(pos).copied() {
        Some(b'`') => '`',
        Some(b'~') => '~',
        _ => return None,
    };

    let marker = if fence_char == '`' { b'`' } else { b'~' };
    let mut end = pos;
    while end < bytes.len() && bytes[end] == marker {
        end += 1;
    }
    let fence_len = end - pos;
    if fence_len < 3 {
        return None;
    }

    Some(FenceMarker {
        fence_char,
        fence_len,
        fence_start: pos,
        rest: &line[end..],
    })
}

#[inline]
fn is_closing_fence(marker: FenceMarker<'_>, opening_fence_char: char, opening_fence_len: usize) -> bool {
    marker.fence_char == opening_fence_char && marker.fence_len >= opening_fence_len && marker.rest.trim().is_empty()
}

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
        let mut opening_fence_char = '`';
        let mut opening_fence_len = 0usize;

        for line in ctx.content.lines() {
            let Some(marker) = parse_fence_marker(line) else {
                continue;
            };

            if !in_code_block {
                // Opening fence - count it
                if marker.fence_char == '`' {
                    backtick_count += 1;
                } else {
                    tilde_count += 1;
                }
                in_code_block = true;
                opening_fence_char = marker.fence_char;
                opening_fence_len = marker.fence_len;
            } else if is_closing_fence(marker, opening_fence_char, opening_fence_len) {
                in_code_block = false;
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
/// the start of any interior bare fence line (after stripping leading whitespace).
///
/// This is used to compute the minimum fence length needed when converting a
/// fence from one style to another so that nesting remains unambiguous.
/// For example, converting a `~~~` outer fence that contains ```` ``` ```` inner
/// fences to backtick style requires using ```` ```` ```` (4 backticks) so that
/// the inner 3-backtick bare fences cannot inadvertently close the outer block.
///
/// Only bare interior sequences (no trailing content) are counted. Per CommonMark
/// spec section 4.5, a closing fence must be followed only by optional whitespace —
/// lines with info strings (e.g. `` ```rust ``) can never be closing fences, so
/// they never create ambiguity regardless of the outer fence's style.
fn max_inner_fence_length_of_char(
    lines: &[&str],
    opening_line: usize,
    opening_fence_len: usize,
    opening_char: char,
    target_char: char,
) -> usize {
    let mut max_len = 0usize;

    for line in lines.iter().skip(opening_line + 1) {
        let Some(marker) = parse_fence_marker(line) else {
            continue;
        };

        // Stop at the closing fence of the outer block.
        if is_closing_fence(marker, opening_char, opening_fence_len) {
            break;
        }

        // Count only bare sequences (no info string). Lines with info strings
        // can never be closing fences per CommonMark and pose no ambiguity risk.
        if marker.fence_char == target_char && marker.rest.trim().is_empty() {
            max_len = max_len.max(marker.fence_len);
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
            let Some(marker) = parse_fence_marker(line) else {
                continue;
            };
            let fence_char = marker.fence_char;
            let fence_len = marker.fence_len;

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
                    // Must be strictly greater than any inner bare fence of the target style.
                    let prefix = &line[..marker.fence_start];
                    let info = marker.rest;
                    let max_inner =
                        max_inner_fence_length_of_char(&lines, line_num, fence_len, fence_char, target_char);
                    converted_fence_len = fence_len.max(max_inner + 1);
                    needs_lengthening = false;

                    let replacement = format!("{prefix}{}{info}", target_char.to_string().repeat(converted_fence_len));

                    let fence_start = marker.fence_start;
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
                    // if the interior contains same-style bare fences of equal or greater
                    // length, the outer fence cannot be distinguished from an inner
                    // closing fence and must be made longer.
                    let prefix = &line[..marker.fence_start];
                    let info = marker.rest;
                    let max_inner = max_inner_fence_length_of_char(&lines, line_num, fence_len, fence_char, fence_char);
                    if max_inner >= fence_len {
                        converted_fence_len = max_inner + 1;
                        needs_lengthening = true;

                        let replacement =
                            format!("{prefix}{}{info}", fence_char.to_string().repeat(converted_fence_len));

                        let fence_start = marker.fence_start;
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
                let is_closing = is_closing_fence(marker, code_block_fence_char, code_block_fence_len);

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

                        let prefix = &line[..marker.fence_start];
                        let replacement = format!(
                            "{prefix}{}{}",
                            target_char.to_string().repeat(converted_fence_len),
                            marker.rest
                        );

                        let fence_start = marker.fence_start;
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
            if let Some(marker) = parse_fence_marker(line) {
                let fence_char = marker.fence_char;
                let fence_len = marker.fence_len;

                if !in_code_block {
                    in_code_block = true;
                    code_block_fence_char = fence_char;
                    code_block_fence_len = fence_len;

                    let needs_conversion = (fence_char == '`' && target_style == CodeFenceStyle::Tilde)
                        || (fence_char == '~' && target_style == CodeFenceStyle::Backtick);

                    let prefix = &line[..marker.fence_start];
                    let info = marker.rest;

                    if needs_conversion {
                        let target_char = if target_style == CodeFenceStyle::Backtick {
                            '`'
                        } else {
                            '~'
                        };

                        let max_inner =
                            max_inner_fence_length_of_char(&lines, line_idx, fence_len, fence_char, target_char);
                        converted_fence_len = fence_len.max(max_inner + 1);
                        needs_lengthening = false;

                        result.push_str(prefix);
                        result.push_str(&target_char.to_string().repeat(converted_fence_len));
                        result.push_str(info);
                    } else {
                        // Already the correct style. Check for fence-length ambiguity.
                        let max_inner =
                            max_inner_fence_length_of_char(&lines, line_idx, fence_len, fence_char, fence_char);
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
                    let is_closing = is_closing_fence(marker, code_block_fence_char, code_block_fence_len);

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
                            let prefix = &line[..marker.fence_start];
                            result.push_str(prefix);
                            result.push_str(&target_char.to_string().repeat(converted_fence_len));
                            result.push_str(marker.rest);
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
    // Fence-length ambiguity detection
    // -----------------------------------------------------------------------

    /// A backtick block containing only an info-string interior sequence (not bare)
    /// is NOT ambiguous: info-string sequences cannot be closing fences per CommonMark,
    /// so the bare ``` at line 3 is simply the closing fence — no lengthening needed.
    #[test]
    fn test_info_string_interior_not_ambiguous() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        // line 0: ```text   ← opens block (len=3, info="text")
        // line 1: ```rust   ← interior content, has info "rust" → cannot close outer
        // line 2: code
        // line 3: ```       ← bare, len=3 >= 3 → closes block 1 (per CommonMark)
        // line 4: ```       ← orphaned second block
        let content = "```text\n```rust\ncode\n```\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // No ambiguity: ```rust cannot close the outer, and the bare ``` IS the
        // unambiguous closing fence. No lengthening needed.
        assert_eq!(warnings.len(), 0, "expected 0 warnings, got {warnings:?}");
    }

    /// fix() leaves a block with only info-string interior sequences unchanged.
    #[test]
    fn test_info_string_interior_fix_unchanged() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "```text\n```rust\ncode\n```\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // No conversion needed (already backtick), no lengthening needed → unchanged.
        assert_eq!(fixed, content);
    }

    /// Same for tilde style: an info-string tilde interior is not ambiguous.
    #[test]
    fn test_tilde_info_string_interior_not_ambiguous() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
        let content = "~~~text\n~~~rust\ncode\n~~~\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // ~~~rust cannot close outer (has info); ~~~ IS the closing fence → unchanged.
        assert_eq!(fixed, content);
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

    /// An outer block containing a longer info-string sequence and a bare closing
    /// fence is not ambiguous: the bare closing fence closes the outer normally,
    /// and the info-string sequence is just content.
    #[test]
    fn test_longer_info_string_interior_not_ambiguous() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        // line 0: ```text    ← opens block (len=3, info="text")
        // line 1: `````rust  ← interior, 5 backticks with info → cannot close outer
        // line 2: code
        // line 3: `````      ← bare, len=5 >= 3, no info → closes block 1
        // line 4: ```        ← orphaned second block
        let content = "```text\n`````rust\ncode\n`````\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // `````rust cannot close the outer. ````` IS the closing fence. No lengthening.
        assert_eq!(fixed, content);
    }

    /// Consistent style: info-string interior sequences are not ambiguous.
    #[test]
    fn test_info_string_interior_consistent_style_no_warning() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);
        let content = "```text\n```rust\ncode\n```\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 0);
    }

    // -----------------------------------------------------------------------
    // Cross-style conversion: bare-only inner sequence counting
    // -----------------------------------------------------------------------

    /// Cross-style conversion where outer has NO info string: interior info-string
    /// sequences are not counted, only bare sequences are.
    #[test]
    fn test_cross_style_bare_inner_requires_lengthening() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        // Outer tilde fence (no info). Interior has a 5-backtick info-string sequence
        // AND a 3-backtick bare sequence. Only the bare sequence (len=3) is counted
        // → outer becomes 4, not 6.
        let content = "~~~\n`````rust\ncode\n```\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // 4 backticks (bare seq len=3 → 3+1=4). The 5-backtick info-string seq is
        // not counted since it cannot be a closing fence.
        assert_eq!(fixed, "````\n`````rust\ncode\n```\n````");
    }

    /// Cross-style conversion where outer HAS an info string but interior has only
    /// info-string sequences: no bare inner sequences means no lengthening needed.
    /// The outer converts at its natural length.
    #[test]
    fn test_cross_style_info_only_interior_no_lengthening() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        // Outer tilde fence (info "text"). Interior has only info-string backtick
        // sequences — no bare closing sequence. Info-string sequences cannot be
        // closing fences, so no lengthening is needed → outer converts at len=3.
        let content = "~~~text\n```rust\nexample\n```rust\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "```text\n```rust\nexample\n```rust\n```");
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

    /// Regression: over-indented inner same-style sequence (4 spaces) is content,
    /// not a closing fence, and must not trigger ambiguity warnings.
    #[test]
    fn test_overindented_inner_sequence_not_ambiguous() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "```text\n    ```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(warnings.len(), 0, "over-indented inner fence should not warn");
        assert_eq!(fixed, content, "over-indented inner fence should remain unchanged");
    }

    /// Regression: when converting outer style, over-indented same-style content
    /// lines must not be mistaken for an outer closing fence.
    #[test]
    fn test_conversion_ignores_overindented_inner_sequence_for_closing_detection() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Backtick);
        let content = "~~~text\n    ~~~\n```rust\ncode\n```\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "````text\n    ~~~\n```rust\ncode\n```\n````");
    }

    /// CommonMark: a top-level fence marker indented 4 spaces is an indented code
    /// block line, not a fenced code block marker, so MD048 should ignore it.
    #[test]
    fn test_top_level_four_space_fence_marker_is_ignored() {
        let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Tilde);
        let content = "    ```\n    code\n    ```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(warnings.len(), 0);
        assert_eq!(fixed, content);
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
