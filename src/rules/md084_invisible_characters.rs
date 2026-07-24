//! Rule MD084: Invisible Unicode characters.
//!
//! This rule detects hidden Unicode code points that can create confusing text,
//! copy/paste bugs, or rendering differences across tools.
//!
//! By default, it tries to avoid false positives by only flagging:
//! 1. Multiple consecutive invisible characters,
//! 2. Invisible characters at the start or end of a line,
//! 3. Invisible characters adjacent to any visible whitespace.
//!
//! In strict mode, it flags any invisible character that is not explicitly allowed in the configuration.

mod md084_config;

use crate::lint_context::LintContext;
use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use md084_config::MD084Config;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct MD084InvisibleCharacters {
    config: MD084Config,
    allowed_codepoints: HashSet<u32>,
}

impl Default for MD084InvisibleCharacters {
    fn default() -> Self {
        Self::from_config_struct(MD084Config::default())
    }
}

impl MD084InvisibleCharacters {
    fn from_config_struct(config: MD084Config) -> Self {
        let allowed_codepoints = config
            .allow
            .iter()
            .filter_map(|token| parse_codepoint_token(token))
            .collect();

        Self {
            config,
            allowed_codepoints,
        }
    }

    #[inline]
    fn is_allowed(&self, c: char) -> bool {
        self.allowed_codepoints.contains(&(c as u32))
    }

    fn format_codepoint(c: char) -> String {
        let cp = c as u32;
        if cp <= 0xFFFF {
            format!("U+{cp:04X}")
        } else {
            format!("U+{cp:06X}")
        }
    }

    fn is_invisible_char(c: char) -> bool {
        let cp = c as u32;
        matches!(
            cp,
            0x0000..=0x001F // C0 control characters
                | 0x007F..=0x009F // DEL + C1 control characters
                | 0x00AD // SOFT HYPHEN
                | 0x034F // COMBINING GRAPHEME JOINER
                | 0x061C // ARABIC LETTER MARK
                | 0x115F // HANGUL CHOSEONG FILLER
                | 0x1160 // HANGUL JUNGSEONG FILLER
                | 0x17B4 // KHMER VOWEL INHERENT AQ
                | 0x17B5 // KHMER VOWEL INHERENT AA
                | 0x180B..=0x180E // Mongolian variation selectors + MONGOLIAN VOWEL SEPARATOR
                | 0x200B..=0x200F // ZWSP, ZWNJ, ZWJ, LRM, RLM
                | 0x202A..=0x202E // Bidi embedding/override controls
                | 0x2060..=0x206F // WORD JOINER, invisibles, and bidi isolate controls
                | 0x3164 // HANGUL FILLER
                | 0xFE00..=0xFE0F // Variation Selectors (VS1..VS16)
                | 0xFEFF // ZERO WIDTH NO-BREAK SPACE (BOM)
                | 0xFFA0 // HALFWIDTH HANGUL FILLER
                | 0xFFF0..=0xFFF8 // Interlinear annotation and reserved non-rendering specials
                | 0x1BCA0..=0x1BCA3 // Shorthand format controls
                | 0x1D173..=0x1D17A // Musical symbol format controls
                | 0xE0000..=0xE0FFF // Tags block + Variation Selectors Supplement
        )
    }

    /// Whether `c` is a codepoint this rule cares about: invisible and not allow-listed.
    #[inline]
    fn is_flaggable(&self, c: char) -> bool {
        Self::is_invisible_char(c) && !self.is_allowed(c)
    }

    /// Build a single-character-run warning, optionally with a removal fix.
    fn build_warning(
        rule_name: &str,
        ctx: &LintContext,
        line: usize,
        start_col: usize,
        len_chars: usize,
        message: String,
        fixable: bool,
    ) -> LintWarning {
        let fix = fixable.then(|| {
            Fix::new(
                ctx.line_index
                    .line_col_to_byte_range_with_length(line, start_col, len_chars),
                String::new(),
            )
        });

        LintWarning {
            rule_name: Some(rule_name.to_string()),
            line,
            column: start_col,
            end_line: line,
            end_column: start_col + len_chars,
            severity: Severity::Warning,
            message,
            fix,
        }
    }
}

impl Rule for MD084InvisibleCharacters {
    fn name(&self) -> &'static str {
        "MD084"
    }

    fn description(&self) -> &'static str {
        "Invisible Unicode characters should be intentional"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn fix_capability(&self) -> FixCapability {
        if self.config.strict {
            FixCapability::Unfixable
        } else {
            FixCapability::ConditionallyFixable
        }
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        ctx.content.is_empty()
            || !ctx
                .content
                .chars()
                .any(|c| Self::is_invisible_char(c) && !self.is_allowed(c))
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();

        for (line_idx, line) in ctx.raw_lines().iter().enumerate() {
            let line_num = line_idx + 1;
            let chars: Vec<char> = line.chars().collect();

            if chars.is_empty() {
                continue;
            }

            // Quick return for strict mode: flag any invisible character that is not allow-listed.
            if self.config.strict {
                warnings.extend(
                    chars
                        .iter()
                        .enumerate()
                        .filter(|&(_, &c)| self.is_flaggable(c))
                        .map(|(i, &c)| {
                            Self::build_warning(
                                self.name(),
                                ctx,
                                line_num,
                                i + 1,
                                1,
                                format!(
                                    "Invisible character {} detected (strict mode)",
                                    Self::format_codepoint(c)
                                ),
                                false,
                            )
                        }),
                );
                continue;
            }

            // In non-strict mode, we only flag the three triggers defined in the rule description.
            let mut flagged = vec![false; chars.len()];

            // Trigger 1: runs of two or more consecutive invisible characters.
            let is_target: Vec<bool> = chars.iter().map(|&c| self.is_flaggable(c)).collect();
            let mut offset = 0;
            for group in is_target.chunk_by(|a, b| a == b) {
                let len = group.len();
                if group[0] && len >= 2 {
                    flagged[offset..offset + len].fill(true);
                    warnings.push(Self::build_warning(
                        self.name(),
                        ctx,
                        line_num,
                        offset + 1,
                        len,
                        format!(
                            "{} multiple consecutive invisible characters detected, first one is {}",
                            group.len(),
                            Self::format_codepoint(chars[offset])
                        ),
                        true,
                    ));
                }
                offset += len;
            }

            // Triggers 2 and 3 need to inspect each remaining candidate's neighbors.
            for (i, &c) in chars.iter().enumerate() {
                if !self.is_flaggable(c) || flagged[i] {
                    continue;
                }

                // Trigger 2: any invisible character at line boundaries.
                if i == 0 || i == chars.len() - 1 {
                    flagged[i] = true;
                    warnings.push(Self::build_warning(
                        self.name(),
                        ctx,
                        line_num,
                        i + 1,
                        1,
                        format!(
                            "Invisible character {} detected at line boundary",
                            Self::format_codepoint(c)
                        ),
                        true,
                    ));
                    continue;
                }

                // Trigger 3: invisible char adjacent to any whitespace. `i` is guaranteed
                // interior here (the boundary case above already handled 0 and len - 1),
                // so both neighbors can be indexed directly.
                if chars[i - 1].is_whitespace() || chars[i + 1].is_whitespace() {
                    flagged[i] = true;
                    warnings.push(Self::build_warning(
                        self.name(),
                        ctx,
                        line_num,
                        i + 1,
                        1,
                        format!(
                            "Invisible character {} detected adjacent to visible whitespace",
                            Self::format_codepoint(c)
                        ),
                        true,
                    ));
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        if self.config.strict {
            return Ok(ctx.content.to_string());
        }

        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }

        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD084Config);
}

fn parse_codepoint_token(token: &str) -> Option<u32> {
    let trimmed = token.trim();
    let hex = trimmed.strip_prefix("U+").or_else(|| trimmed.strip_prefix("u+"))?;
    if !(4..=6).contains(&hex.len()) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let value = u32::from_str_radix(hex, 16).ok()?;
    if value > 0x10FFFF || (0xD800..=0xDFFF).contains(&value) {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, MarkdownFlavor};

    fn check(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        MD084InvisibleCharacters::default().check(&ctx).unwrap()
    }

    #[test]
    fn test_default_no_findings_on_plain_text() {
        let findings = check("plain text\nsecond line\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_default_flags_multiple_consecutive_invisibles() {
        let findings = check("a\u{200B}\u{200C}b");
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0]
                .message
                .contains("2 multiple consecutive invisible characters detected")
        );
        assert_eq!(findings[0].column, 2);
        assert_eq!(findings[0].end_column, 4);
        assert!(findings[0].fix.is_some());
    }

    #[test]
    fn test_default_flags_invisible_chars_at_line_boundaries() {
        let findings = check("\u{2060}start\nend\u{200B}");
        assert_eq!(findings.len(), 2);
        assert!(
            findings[0]
                .message
                .contains("Invisible character U+2060 detected at line boundary")
        );
        assert!(
            findings[1]
                .message
                .contains("Invisible character U+200B detected at line boundary")
        );
    }

    #[test]
    fn test_default_flags_invisible_adjacent_to_whitespace() {
        let findings = check("a \u{2060}b");
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0]
                .message
                .contains("Invisible character U+2060 detected adjacent to visible whitespace")
        );
    }

    #[test]
    fn test_default_fix_removes_triggered_characters() {
        let content = "x\u{200B}\u{200C}y\nleft \u{2060} right";
        let rule = MD084InvisibleCharacters::default();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "xy\nleft  right");
    }

    #[test]
    fn test_strict_flags_any_invisible_character() {
        let config: Config = toml::from_str(
            r#"
            [MD084]
            strict = true
            "#,
        )
        .unwrap();

        let rule = MD084InvisibleCharacters::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD084InvisibleCharacters>().unwrap();

        let ctx = LintContext::new("ca\u{200C}t", MarkdownFlavor::Standard, None);
        let findings = rule.check(&ctx).unwrap();
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("strict mode"));
        assert!(findings[0].fix.is_none());

        assert_eq!(rule.fix(&ctx).unwrap(), "ca\u{200C}t");
    }

    #[test]
    fn test_allow_list_suppresses_findings() {
        let config: Config = toml::from_str(
            r#"
            [MD084]
            allow = ["U+200B"]
            "#,
        )
        .unwrap();

        let rule = MD084InvisibleCharacters::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD084InvisibleCharacters>().unwrap();

        let ctx = LintContext::new("\u{200B}ok\u{200B}", MarkdownFlavor::Standard, None);
        let findings = rule.check(&ctx).unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn test_md084_default_triggers_are_targeted() {
        let rule = MD084InvisibleCharacters::default();
        let content = "a\u{200B}\u{200C}b\nleft \u{2060} right\n\u{2060}edge\nend\u{200B}";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let findings = rule.check(&ctx).unwrap();
        assert_eq!(findings.len(), 4);

        // Default mode should provide auto-fixes.
        assert!(findings.iter().all(|w| w.fix.is_some()));
    }

    #[test]
    fn test_md084_strict_mode_flags_any_invisible() {
        let config: Config = toml::from_str(
            r#"
        [MD084]
        strict = true
        "#,
        )
        .unwrap();
        let rule = MD084InvisibleCharacters::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD084InvisibleCharacters>().unwrap();

        let ctx = LintContext::new("in\u{200C}word", MarkdownFlavor::Standard, None);
        let findings = rule.check(&ctx).unwrap();

        assert_eq!(findings.len(), 1);
        assert!(findings[0].fix.is_none());
    }

    #[test]
    fn test_md084_allow_list_by_codepoint() {
        let config: Config = toml::from_str(
            r#"
        [MD084]
        allow = ["U+200B"]
        "#,
        )
        .unwrap();
        let rule = MD084InvisibleCharacters::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD084InvisibleCharacters>().unwrap();

        let ctx = LintContext::new("\u{200B}safe\u{200B}", MarkdownFlavor::Standard, None);
        let findings = rule.check(&ctx).unwrap();
        assert!(findings.is_empty());
    }
}
