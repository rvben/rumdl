//! Rule MD084: Invisible and deprecated Unicode characters.
//!
//! This rule detects hidden Unicode code points that can create confusing text,
//! copy/paste bugs, or rendering differences across tools.
//! It also flags deprecated Unicode code points that are no longer recommended for use.
//!
//! By default, it tries to avoid false positives by only flagging:
//! 1. Multiple consecutive invisible characters,
//! 2. Invisible characters at the start or end of a line,
//! 3. Invisible characters adjacent to any visible whitespace.
//! 4. Deprecated Unicode code points (never fixable).
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
                | 0xFFF0..=0xFFFB // Interlinear annotation and reserved non-rendering specials
                | 0x1BCA0..=0x1BCA3 // Shorthand format controls
                | 0x1D173..=0x1D17A // Musical symbol format controls
                | 0xE0000..=0xE0FFF // Tags block + Variation Selectors Supplement
        )
    }

    #[inline]
    fn is_deprecated_char(c: char) -> bool {
        let cp = c as u32;
        matches!(
            cp,
            0x0340 // COMBINING GRAVE TONE MARK
                | 0x0341 // COMBINING ACUTE TONE MARK
                | 0xFFFC // OBJECT REPLACEMENT CHARACTER
                | 0x0149 // LATIN SMALL LETTER N PRECEDED BY APOSTROPHE
                | 0x0673 // ARABIC LETTER ALEF WITH WAVY HAMZA ABOVE
                | 0x0F77 // TIBETAN VOWEL SIGN VOCALIC LL
                | 0x0F79 // TIBETAN VOWEL SIGN VOCALIC LR
                | 0x17A3..=0x17A4 // KHMER INHERENT VOWEL SIGN AA..KHMER INHERENT VOWEL SIGN AE
                | 0x206A..=0x206F // INHIBIT SYMMETRIC SWAPPING..NOMINAL DIGIT SHAPES
                | 0x2329 // LEFT-POINTING ANGLE BRACKET
                | 0x232A // RIGHT-POINTING ANGLE BRACKET
                | 0xE0001 // LANGUAGE TAG
        )
    }

    #[inline]
    fn replacement_for(c: char) -> Option<String> {
        match c as u32 {
            0x0340 => Some("\u{0300}".to_string()), // COMBINING GRAVE ACCENT
            0x0341 => Some("\u{0301}".to_string()), // COMBINING ACUTE ACCENT
            _ => None,
        }
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

    /// ZERO WIDTH JOINER, which fuses adjacent characters into one glyph.
    const ZWJ: char = '\u{200D}';

    /// Whether the character at `index` is visible content: present, not whitespace,
    /// and not one of the invisible code points this rule tracks.
    fn is_visible_base(chars: &[char], index: usize) -> bool {
        chars
            .get(index)
            .is_some_and(|&c| !c.is_whitespace() && !Self::is_invisible_char(c))
    }

    /// Whether the character before `index` resolves to visible content, looking past
    /// a variation selector that is itself attached to a base. That is what lets the
    /// joiner in `U+1F3F3 U+FE0F U+200D U+1F308` (the rainbow flag) see its base.
    fn follows_visible_base(chars: &[char], index: usize) -> bool {
        let Some(prev) = index.checked_sub(1) else {
            return false;
        };

        Self::is_visible_base(chars, prev)
            || (Self::is_variation_selector(chars[prev])
                && prev
                    .checked_sub(1)
                    .is_some_and(|base| Self::is_visible_base(chars, base)))
    }

    /// Whether the character at `index` is presentation rather than hidden content.
    /// Both forms below are part of the grapheme cluster a reader sees, so removing
    /// one changes the rendered text: a variation selector picks the glyph form of the
    /// character before it, and a joiner fuses the characters on either side of it.
    /// Orphaned - at the start or end of a line, next to whitespace, or with another
    /// invisible character where its base should be - neither is doing that job, and
    /// stays reportable.
    fn is_presentation(chars: &[char], index: usize) -> bool {
        let c = chars[index];

        if Self::is_variation_selector(c) {
            // A selector modifies exactly the character before it, so a duplicated
            // selector has nothing left of its own to modify.
            return index
                .checked_sub(1)
                .is_some_and(|prev| Self::is_visible_base(chars, prev));
        }

        c == Self::ZWJ && Self::follows_visible_base(chars, index) && Self::is_visible_base(chars, index + 1)
    }

    /// Message for a reportable stretch inside a run of consecutive invisible
    /// characters. A stretch shortens to one character when presentation sits next
    /// to it, which is still a cluster worth reporting.
    fn cluster_message(len: usize, first: char) -> String {
        let codepoint = Self::format_codepoint(first);
        if len >= 2 {
            format!("{len} multiple consecutive invisible characters detected, first one is {codepoint}")
        } else {
            format!("Invisible character {codepoint} detected next to another invisible character")
        }
    }

    #[inline]
    /// Build a single-character-run warning, optionally with a removal fix.
    fn build_warning(
        &self,
        ctx: &LintContext,
        line: usize,
        start_col: usize,
        len_chars: usize,
        message: String,
        replacement: Option<String>,
    ) -> LintWarning {
        let fix = replacement.map(|replacement| {
            Fix::new(
                ctx.line_index
                    .line_col_to_byte_range_with_length(line, start_col, len_chars),
                replacement,
            )
        });

        LintWarning {
            rule_name: Some(self.name().to_string()),
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
        "Invisible or Deprecated Unicode characters are present"
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
                .any(|c| (Self::is_invisible_char(c) || Self::is_deprecated_char(c)) && !self.is_allowed(c))
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
                warnings.extend(chars.iter().enumerate().filter_map(|(i, &c)| {
                    if self.is_allowed(c) {
                        None
                    } else if Self::is_invisible_char(c) {
                        Some(self.build_warning(
                            ctx,
                            line_num,
                            i + 1,
                            1,
                            format!(
                                "Invisible character {} detected (strict mode)",
                                Self::format_codepoint(c)
                            ),
                            Some(String::new()),
                        ))
                    } else if Self::is_deprecated_char(c) {
                        Some(self.build_warning(
                            ctx,
                            line_num,
                            i + 1,
                            1,
                            format!("Deprecated Unicode code point {} detected", Self::format_codepoint(c)),
                            Self::replacement_for(c),
                        ))
                    } else {
                        None
                    }
                }));
                continue;
            }

            // In non-strict mode, we only flag the three triggers defined in the rule
            // description. Presentation characters are never reported or removed, but
            // they stay invisible characters for the purpose of detecting a cluster,
            // so nothing can hide behind an emoji.
            let mut flagged = vec![false; chars.len()];
            let flaggable: Vec<bool> = chars
                .iter()
                .map(|&c| Self::is_invisible_char(c) && !self.is_allowed(c))
                .collect();
            let exempt: Vec<bool> = (0..chars.len()).map(|i| Self::is_presentation(&chars, i)).collect();
            let is_target: Vec<bool> = (0..chars.len()).map(|i| flaggable[i] && !exempt[i]).collect();

            // Trigger 1: runs of two or more consecutive invisible characters. The run
            // is measured over every invisible character, then reported one reportable
            // stretch at a time so presentation inside it is left intact.
            let mut offset = 0;
            for group in flaggable.chunk_by(|a, b| a == b) {
                let len = group.len();
                if group[0] && len >= 2 {
                    let mut start = offset;
                    for stretch in exempt[offset..offset + len].chunk_by(|a, b| a == b) {
                        let stretch_len = stretch.len();
                        if !stretch[0] {
                            flagged[start..start + stretch_len].fill(true);
                            warnings.push(self.build_warning(
                                ctx,
                                line_num,
                                start + 1,
                                stretch_len,
                                Self::cluster_message(stretch_len, chars[start]),
                                Some(String::new()),
                            ));
                        }
                        start += stretch_len;
                    }
                }
                offset += len;
            }

            // Triggers 2, 3 and 4 need to inspect each remaining candidate's neighbors.
            for (i, &c) in chars.iter().enumerate() {
                // Trigger 4: deprecated Unicode code points.
                // Some deprecated code points have recommended replacements, so they have a fix.
                // Other do not, so they are reported but not fixable.
                if Self::is_deprecated_char(c) && !self.is_allowed(c) {
                    flagged[i] = true;
                    warnings.push(self.build_warning(
                        ctx,
                        line_num,
                        i + 1,
                        1,
                        format!("Deprecated Unicode code point {} detected", Self::format_codepoint(c)),
                        Self::replacement_for(c),
                    ));
                }

                // For triggers 2 and 3, skip any character that is not a target or has already been flagged.
                if !is_target[i] || flagged[i] {
                    continue;
                }

                // Trigger 2: any invisible character at line boundaries.
                if i == 0 || i == chars.len() - 1 {
                    flagged[i] = true;
                    warnings.push(self.build_warning(
                        ctx,
                        line_num,
                        i + 1,
                        1,
                        format!(
                            "Invisible character {} detected at line boundary",
                            Self::format_codepoint(c)
                        ),
                        Some(String::new()),
                    ));
                    continue;
                }

                // Trigger 3: invisible char adjacent to any whitespace. `i` is guaranteed
                // interior here (the boundary case above already handled 0 and len - 1),
                // so both neighbors can be indexed directly.
                if chars[i - 1].is_whitespace() || chars[i + 1].is_whitespace() {
                    flagged[i] = true;
                    warnings.push(self.build_warning(
                        ctx,
                        line_num,
                        i + 1,
                        1,
                        format!(
                            "Invisible character {} detected adjacent to visible whitespace",
                            Self::format_codepoint(c)
                        ),
                        Some(String::new()),
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
    use crate::config::MarkdownFlavor;

    fn check_with_config(content: &str, strict: bool, allow: &Vec<&str>) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let config = MD084Config {
            strict,
            allow: allow.iter().map(std::string::ToString::to_string).collect(),
        };
        MD084InvisibleCharacters::from_config_struct(config)
            .check(&ctx)
            .unwrap()
    }

    fn check(content: &str) -> Vec<LintWarning> {
        check_with_config(content, false, &vec![])
    }

    fn fix_with_config(content: &str, strict: bool, allow: &Vec<&str>) -> String {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let config = MD084Config {
            strict,
            allow: allow.iter().map(std::string::ToString::to_string).collect(),
        };
        MD084InvisibleCharacters::from_config_struct(config).fix(&ctx).unwrap()
    }

    fn fix(content: &str) -> String {
        fix_with_config(content, false, &vec![])
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
        assert_eq!(fix("x\u{200B}\u{200C}y\nleft \u{2060} right"), "xy\nleft  right");
    }

    #[test]
    fn test_strict_flags_any_invisible_character() {
        let findings = check_with_config("ca\u{200C}t", true, &vec![]);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("strict mode"));
        assert!(findings[0].fix.is_some());

        assert_eq!(fix_with_config("ca\u{200C}t", true, &vec![]), "cat");
    }

    #[test]
    fn test_allow_list_suppresses_findings() {
        assert!(check_with_config("\u{200B}ok\u{200B}", false, &vec!["U+200B"]).is_empty());
    }

    #[test]
    fn test_md084_default_triggers_are_targeted() {
        let findings = check("a\u{200B}\u{200C}b\nleft \u{2060} right\n\u{2060}edge\nend\u{200B}");
        assert_eq!(findings.len(), 4);

        // Default mode should provide auto-fixes.
        assert!(findings.iter().all(|w| w.fix.is_some()));
    }

    #[test]
    fn test_md084_strict_mode_flags_any_invisible() {
        let findings = check_with_config("in\u{200C}word", true, &vec![]);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].fix.is_some());
    }

    #[test]
    fn test_md084_allow_list_by_codepoint() {
        let findings = check_with_config("\u{200B}safe\u{200B}", false, &vec!["U+200B"]);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_tab_characters() {
        let findings = check("text\n\tindented\n");
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
        assert_eq!(fix(content), content);
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
        // Mid-line matters here: the duplicate is neither at a boundary nor next to
        // whitespace, so it is only caught by counting the attached selector as part
        // of the cluster.
        for content in ["\u{26A0}\u{FE0F}\u{FE0F}", "\u{26A0}\u{FE0F}\u{FE0F}x"] {
            let findings = check(content);
            assert_eq!(findings.len(), 1, "content {content:?}");
            assert_eq!(findings[0].column, 3, "content {content:?}");
            assert_eq!(findings[0].end_column, 4, "content {content:?}");
            assert!(
                findings[0]
                    .message
                    .contains("U+FE0F detected next to another invisible character"),
                "content {content:?}: {}",
                findings[0].message
            );
        }
    }

    #[test]
    fn test_default_ignores_emoji_zwj_sequences() {
        // Each of these is a single glyph held together by joiners, and some carry a
        // variation selector next to the joiner. Removing either splits the emoji.
        let sequences = [
            "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}",                           // rainbow flag
            "\u{1F469}\u{200D}\u{2764}\u{FE0F}\u{200D}\u{1F468}",           // couple with heart
            "\u{26F9}\u{FE0F}\u{200D}\u{2640}\u{FE0F}",                     // woman bouncing ball
            "\u{1F3F4}\u{200D}\u{2620}\u{FE0F}",                            // pirate flag
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}", // family
        ];

        for sequence in sequences {
            let content = format!("look: {sequence} here");
            let findings = check(&content);
            assert!(findings.is_empty(), "sequence {sequence:?}: {findings:?}");

            assert_eq!(fix(&content), content, "sequence {sequence:?} was rewritten");
        }
    }

    #[test]
    fn test_default_flags_orphaned_joiner() {
        // A joiner only earns its exemption by fusing visible characters on both sides.
        let findings = check("joins nothing\u{200D}");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("U+200D detected at line boundary"));

        let findings = check("a \u{200D}b");
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0]
                .message
                .contains("U+200D detected adjacent to visible whitespace")
        );

        // Joiner followed by a zero-width space rather than a visible character.
        let findings = check("a\u{200D}\u{200B}b");
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0]
                .message
                .contains("2 multiple consecutive invisible characters")
        );
    }

    #[test]
    fn test_default_flags_invisible_hiding_behind_an_emoji() {
        // A zero-width space tucked between an emoji and the next word is surrounded
        // by an attached selector on one side, so it only surfaces if the selector
        // still counts toward the cluster.
        let content = "\u{26A0}\u{FE0F}\u{200B}x";
        let findings = check(content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].column, 3);
        assert!(
            findings[0]
                .message
                .contains("U+200B detected next to another invisible character")
        );

        // The fix removes only the zero-width space, leaving the emoji intact.
        assert_eq!(fix(content), "\u{26A0}\u{FE0F}x");
    }

    #[test]
    fn test_strict_still_flags_attached_variation_selector() {
        // Strict mode is deliberately literal: it reports every invisible codepoint,
        // and users who want emoji left alone allow-list U+FE0F.
        let findings = check_with_config("\u{26A0}\u{FE0F} Note", true, &vec![]);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("strict mode"));
    }

    #[test]
    fn test_default_deprecated_characters_are_flagged_and_not_fixable() {
        // Check that the deprecated characters are flagged and not fixable, even in default mode.
        let findings = check("\u{0340}deprecated\u{0341}\u{FFFC}");
        assert_eq!(findings.len(), 3, "Got {findings:?}");
        assert!(findings[0].message.contains("U+0340"));
        assert!(findings[0].fix.is_some() && findings[0].fix.as_ref().unwrap().replacement == "\u{0300}"); // U+0340 has a recommended replacement
        assert!(findings[1].message.contains("U+0341"));
        assert!(findings[1].fix.is_some() && findings[1].fix.as_ref().unwrap().replacement == "\u{0301}"); // U+0341 has a recommended replacement
        assert!(findings[2].message.contains("U+FFFC"));
        assert!(findings[2].fix.is_none()); // U+FFFC has no recommended replacement
    }

    #[test]
    fn test_strict_deprecated_characters_are_flagged_and_not_fixable() {
        // Check that the deprecated characters are flagged and not fixable, even in strict mode.
        let findings = check_with_config("\u{0340}deprecated\u{0341}\u{FFFC}", true, &vec![]);
        assert_eq!(findings.len(), 3, "Got {findings:?}");
        assert!(findings[0].fix.is_some() && findings[0].fix.as_ref().unwrap().replacement == "\u{0300}"); // U+0340 has a recommended replacement
        assert!(findings[1].message.contains("U+0341"));
        assert!(findings[1].fix.is_some() && findings[1].fix.as_ref().unwrap().replacement == "\u{0301}"); // U+0341 has a recommended replacement
        assert!(findings[2].message.contains("U+FFFC"));
        assert!(findings[2].fix.is_none()); // U+FFFC has no recommended replacement
    }

    #[test]
    fn test_allowed_deprecated_characters_are_not_flagged() {
        // Check that the deprecated characters are not flagged if they are allow-listed.
        let allow = vec!["U+0340", "U+0341", "U+FFFC"];
        let findings = check_with_config("\u{0340}deprecated\u{0341}\u{FFFC}", false, &allow);
        assert!(findings.is_empty());
    }
}
