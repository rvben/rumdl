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
            0x0000..=0x0008
                | 0x000A..=0x001F // C0 Control characters, excluding TAB (0x0009)
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

    /// Variation selectors modify the *preceding* base character: `U+26A0 U+FE0F`
    /// is the emoji-presentation warning sign `⚠️`, where `U+26A0` alone is the
    /// text-presentation `⚠`.
    fn is_variation_selector(c: char) -> bool {
        matches!(
            c as u32,
            0x180B..=0x180D // Mongolian free variation selectors FVS1..FVS3
                | 0xFE00..=0xFE0F // Variation Selectors VS1..VS16
                | 0xE0100..=0xE01EF // Variation Selectors Supplement VS17..VS256
        )
    }

    /// Whether the character at `index` is a variation selector with a base character
    /// to modify. Attached, it is part of that grapheme cluster rather than hidden
    /// content, so removing it would change the rendered glyph. Orphaned - at the start
    /// of a line, after whitespace, or after another invisible character - it modifies
    /// nothing and stays flaggable.
    fn is_attached_variation_selector(chars: &[char], index: usize) -> bool {
        if !Self::is_variation_selector(chars[index]) {
            return false;
        }

        index
            .checked_sub(1)
            .is_some_and(|prev| !chars[prev].is_whitespace() && !Self::is_invisible_char(chars[prev]))
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
        FixCapability::ConditionallyFixable
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
                                true,
                            )
                        }),
                );
                continue;
            }

            // In non-strict mode, we only flag the three triggers defined in the rule
            // description, and a variation selector attached to a base character is
            // presentation rather than hidden content.
            let mut flagged = vec![false; chars.len()];
            let is_target: Vec<bool> = chars
                .iter()
                .enumerate()
                .map(|(i, &c)| self.is_flaggable(c) && !Self::is_attached_variation_selector(&chars, i))
                .collect();

            // Trigger 1: runs of two or more consecutive invisible characters.
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
                if !is_target[i] || flagged[i] {
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
        assert!(findings[0].fix.is_some());

        assert_eq!(rule.fix(&ctx).unwrap(), "cat");
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
        assert!(findings[0].fix.is_some());
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

    #[test]
    fn test_tab_characters() {
        let rule = MD084InvisibleCharacters::default();
        let content = "text\n\tindented\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let findings = rule.check(&ctx).unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn test_default_ignores_variation_selector_attached_to_base() {
        // U+FE0F gives the preceding character emoji presentation. It legitimately
        // sits at a line end or next to a space, which are two of the default triggers.
        let findings = check("> \u{26A0}\u{FE0F} Note: important\nends with \u{2764}\u{FE0F}\n");
        assert!(findings.is_empty(), "attached variation selectors: {findings:?}");

        let findings = check("# Features \u{25B6}\u{FE0F}\n\ntwo \u{2714}\u{FE0F}\u{2764}\u{FE0F} in a row\n");
        assert!(findings.is_empty(), "attached variation selectors: {findings:?}");
    }

    #[test]
    fn test_default_fix_preserves_emoji_presentation() {
        let content = "> \u{26A0}\u{FE0F} Note: important\n";
        let rule = MD084InvisibleCharacters::default();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_default_flags_orphaned_variation_selector() {
        // No base character to modify: the selector is hidden content, not presentation.
        let findings = check("\u{FE0F}starts with a selector");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("U+FE0F detected at line boundary"));

        let findings = check("a \u{FE0F}b");
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0]
                .message
                .contains("U+FE0F detected adjacent to visible whitespace")
        );

        // Preceded by another invisible character, so it still modifies nothing.
        let findings = check("a\u{200B}\u{FE0F}b");
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0]
                .message
                .contains("2 multiple consecutive invisible characters")
        );
    }

    #[test]
    fn test_default_flags_redundant_variation_selector() {
        // The first selector is attached to the base; the duplicate after it is not.
        let findings = check("\u{26A0}\u{FE0F}\u{FE0F}");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].column, 3);
        assert!(findings[0].message.contains("U+FE0F detected at line boundary"));
    }

    #[test]
    fn test_strict_still_flags_attached_variation_selector() {
        // Strict mode is deliberately literal: it reports every invisible codepoint,
        // and users who want emoji left alone allow-list U+FE0F.
        let config: Config = toml::from_str(
            r#"
            [MD084]
            strict = true
            "#,
        )
        .unwrap();

        let rule = MD084InvisibleCharacters::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD084InvisibleCharacters>().unwrap();

        let ctx = LintContext::new("\u{26A0}\u{FE0F} Note", MarkdownFlavor::Standard, None);
        let findings = rule.check(&ctx).unwrap();
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("strict mode"));
    }
}
