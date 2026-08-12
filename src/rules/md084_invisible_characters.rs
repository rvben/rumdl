//! Rule MD084: Invisible and discouraged Unicode characters.
//!
//! This rule detects hidden Unicode code points that can create confusing text,
//! copy/paste bugs, or rendering differences across tools. It also flags code points
//! Unicode itself steers authors away from: those carrying the `Deprecated` property,
//! and those UTR#20 lists as unsuitable for use with markup.
//!
//! By default, it tries to avoid false positives by only flagging:
//! 1. Multiple consecutive invisible characters,
//! 2. Invisible characters at the start or end of a line,
//! 3. Invisible characters adjacent to any visible whitespace,
//! 4. Deprecated and markup-unsuitable code points, fixable only where a substitution
//!    preserves the text exactly.
//!
//! In strict mode, it flags any invisible character that is not explicitly allowed in the configuration.

mod md084_config;

use crate::lint_context::LintContext;
use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::unicode;
use md084_config::MD084Config;

#[derive(Debug, Clone)]
pub struct MD084InvisibleCharacters {
    config: MD084Config,
}

impl Default for MD084InvisibleCharacters {
    fn default() -> Self {
        Self::from_config_struct(MD084Config::default())
    }
}

impl MD084InvisibleCharacters {
    fn from_config_struct(config: MD084Config) -> Self {
        Self { config }
    }

    #[inline]
    fn is_allowed(&self, c: char) -> bool {
        self.config.allow.contains(&c)
    }

    /// Whether either set above claims this code point.
    #[inline]
    fn is_markup_char(c: char) -> bool {
        unicode::is_deprecated_char(c) || unicode::is_unsuitable_for_markup_char(c)
    }

    /// The interlinear annotation delimiters, which draw no glyph of their own but
    /// bracket the text between them, so they are kept out of the deletable invisible
    /// set: removing one would leave the annotation half-open.
    #[inline]
    fn is_annotation_delimiter(c: char) -> bool {
        matches!(c as u32, 0xFFF9..=0xFFFB)
    }

    /// A line ending, which CommonMark counts as document structure rather than text
    /// (2.1 Characters and lines) - a lone carriage return ends a line just as a line
    /// feed does. This rule edits characters within a line, so removing one of these
    /// would join two lines or drop the document's final line ending.
    ///
    /// Both reach this rule. Lines arrive already split on `\n` and `\r\n`, so a
    /// carriage return surviving into one is a lone classic-Mac line ending; the line
    /// feeds are only seen by the whole-document scan in `should_skip`, where counting
    /// them as reportable would leave that guard unable to skip anything.
    #[inline]
    fn is_line_ending(c: char) -> bool {
        c == '\n' || c == '\r'
    }

    /// Whether this code point puts no glyph on the page, whether or not the rule is
    /// willing to delete it. This is what a variation selector or joiner needs beside
    /// it to be doing its job, and what makes a stretch of characters a cluster.
    #[inline]
    fn draws_no_glyph(c: char) -> bool {
        unicode::is_invisible_char(c) || Self::is_annotation_delimiter(c)
    }

    /// A code point flagged by one of the two sets above, with the message that is
    /// true of it and the replacement that preserves the text, if one exists.
    fn markup_finding(c: char) -> Option<(String, Option<String>)> {
        let codepoint = unicode::format_codepoint(c);
        if unicode::is_deprecated_char(c) {
            return Some((format!("Deprecated Unicode code point {codepoint} detected"), None));
        }
        if !unicode::is_unsuitable_for_markup_char(c) {
            return None;
        }
        // The tone marks are canonical singletons: every normalization form already
        // rewrites them this way, so the substitution cannot change what renders.
        let replacement = match c as u32 {
            0x0340 => Some("\u{0300}".to_string()), // COMBINING GRAVE ACCENT
            0x0341 => Some("\u{0301}".to_string()), // COMBINING ACUTE ACCENT
            _ => None,
        };
        Some((
            format!("Unicode code point {codepoint} is not suitable for use with markup"),
            replacement,
        ))
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

    /// Whether the character at `index` renders a glyph a variation selector can pick
    /// a form of, or a joiner can fuse: present, not whitespace, and not one of the
    /// code points this rule knows draws nothing.
    fn is_visible_base(chars: &[char], index: usize) -> bool {
        chars
            .get(index)
            .is_some_and(|&c| !c.is_whitespace() && !Self::draws_no_glyph(c))
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
    /// Orphaned - at the start or end of a line, next to whitespace, or with a
    /// character that draws no glyph where its base should be - neither is doing that
    /// job, and stays reportable.
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
        let codepoint = unicode::format_codepoint(first);
        if len >= 2 {
            format!("{len} multiple consecutive invisible characters detected, first one is {codepoint}")
        } else {
            format!("Invisible character {codepoint} detected next to another invisible character")
        }
    }

    /// Build a warning covering `len_chars` characters, with a fix rewriting them to
    /// `replacement` when one is given. An empty replacement deletes the run.
    #[inline]
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
                ctx.line_column_byte_range_with_length(line, start_col, len_chars),
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
        "Invisible or discouraged Unicode characters should be intentional"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::ConditionallyFixable
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        ctx.content.is_empty()
            || !ctx.content.chars().any(|c| {
                (unicode::is_invisible_char(c) || Self::is_markup_char(c))
                    && !Self::is_line_ending(c)
                    && !self.is_allowed(c)
            })
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();

        for (line_idx, line) in ctx.raw_lines().iter().enumerate() {
            let line_num = line_idx + 1;
            let chars: Vec<char> = line.chars().collect();

            if chars.is_empty() {
                continue;
            }

            // Quick return for strict mode: flag any invisible character that is neither
            // allow-listed nor a line ending. Strict widens which hidden characters are
            // worth reporting, which is a judgment the allow list already lets users
            // make; it is not a licence to restructure the document.
            if self.config.strict {
                warnings.extend(chars.iter().enumerate().filter_map(|(i, &c)| {
                    if self.is_allowed(c) || Self::is_line_ending(c) {
                        None
                    } else if unicode::is_invisible_char(c) {
                        Some(self.build_warning(
                            ctx,
                            line_num,
                            i + 1,
                            1,
                            format!(
                                "Invisible character {} detected (strict mode)",
                                unicode::format_codepoint(c)
                            ),
                            Some(String::new()),
                        ))
                    } else {
                        Self::markup_finding(c).map(|(message, replacement)| {
                            self.build_warning(ctx, line_num, i + 1, 1, message, replacement)
                        })
                    }
                }));
                continue;
            }

            // In non-strict mode, we only flag the three triggers defined in the rule
            // description. Presentation characters, annotation delimiters and line
            // endings are never reported or removed by those triggers, but they still
            // draw no glyph, so they count toward a cluster and nothing can hide behind
            // one: a zero-width space pressed against a line ending is still reported,
            // and only it is removed.
            let mut flagged = vec![false; chars.len()];
            let flaggable: Vec<bool> = chars
                .iter()
                .map(|&c| Self::draws_no_glyph(c) && !self.is_allowed(c))
                .collect();
            let exempt: Vec<bool> = (0..chars.len())
                .map(|i| {
                    Self::is_annotation_delimiter(chars[i])
                        || Self::is_line_ending(chars[i])
                        || Self::is_presentation(&chars, i)
                })
                .collect();
            let is_target: Vec<bool> = (0..chars.len()).map(|i| flaggable[i] && !exempt[i]).collect();

            // Trigger 1: runs of two or more consecutive characters that draw nothing.
            // The run is measured over all of them, then reported one reportable stretch
            // at a time so exempt characters inside it are left intact.
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

            // Triggers 2 and 3 need to inspect each remaining candidate's neighbors.
            for (i, &c) in chars.iter().enumerate() {
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
                            unicode::format_codepoint(c)
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
                            unicode::format_codepoint(c)
                        ),
                        Some(String::new()),
                    ));
                }
            }

            // Trigger 4: code points Unicode itself steers authors away from, reported
            // wherever no invisible-character trigger already spoke. Several of them are
            // invisible too, and the invisible triggers carry a removal fix this one
            // cannot offer, so running last keeps the more actionable diagnostic and
            // reports each character once.
            for (i, &c) in chars.iter().enumerate() {
                if flagged[i] || self.is_allowed(c) {
                    continue;
                }
                let Some((message, replacement)) = Self::markup_finding(c) else {
                    continue;
                };
                flagged[i] = true;
                warnings.push(self.build_warning(ctx, line_num, i + 1, 1, message, replacement));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    fn check_with_config(content: &str, strict: bool, allow: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let config = MD084Config {
            strict,
            allow: allow.chars().collect(),
        };
        MD084InvisibleCharacters::from_config_struct(config)
            .check(&ctx)
            .unwrap()
    }

    fn check(content: &str) -> Vec<LintWarning> {
        check_with_config(content, false, "")
    }

    fn fix_with_config(content: &str, strict: bool, allow: &str) -> String {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let config = MD084Config {
            strict,
            allow: allow.chars().collect(),
        };
        MD084InvisibleCharacters::from_config_struct(config).fix(&ctx).unwrap()
    }

    fn fix(content: &str) -> String {
        fix_with_config(content, false, "")
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
        let findings = check_with_config("ca\u{200C}t", true, "");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("strict mode"));
        assert!(findings[0].fix.is_some());

        assert_eq!(fix_with_config("ca\u{200C}t", true, ""), "cat");
    }

    #[test]
    fn test_allow_list_suppresses_findings() {
        assert!(check_with_config("\u{200B}ok\u{200B}", false, "\u{200B}").is_empty());
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
        let findings = check_with_config("in\u{200C}word", true, "");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].fix.is_some());
    }

    #[test]
    fn test_md084_allow_list_by_codepoint() {
        let findings = check_with_config("\u{200B}safe\u{200B}", false, "\u{200B}");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_tab_characters() {
        let findings = check("text\n\tindented\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_carriage_returns_are_line_endings_not_hidden_content() {
        // A document written with classic-Mac line endings is a single line to
        // `str::lines()`, which splits on `\n` and `\r\n` only. Every carriage return
        // in it is therefore visible to this rule, and each of the three default
        // triggers used to reach one: the last is at a line boundary, a doubled pair
        // is a run of two, and one after a space is adjacent to whitespace. Removing
        // any of them joins two lines or drops the document's final line ending.
        for content in [
            "# Title\rSome text\rMore text\r",
            "# Title\r\rSome text\r",
            "# Title \rSome text\r",
            "a\rb\n",
        ] {
            let findings = check(content);
            assert!(findings.is_empty(), "{content:?} gave {findings:?}");
            assert_eq!(fix(content), content, "fixing {content:?}");
        }
    }

    #[test]
    fn test_strict_mode_keeps_carriage_returns() {
        // Strict mode widens which hidden characters are reported. Line endings are
        // not among them at any strictness: deleting all three here would collapse
        // the document onto one line.
        for content in ["# Title\rSome text\rMore text\r", "# Title\r\nSome text\r\n"] {
            let findings = check_with_config(content, true, "");
            assert!(findings.is_empty(), "{content:?} gave {findings:?}");
            assert_eq!(fix_with_config(content, true, ""), content, "fixing {content:?}");
        }
    }

    #[test]
    fn test_hidden_character_beside_a_carriage_return_is_still_removed() {
        // Sparing the line ending must not spare what hides against it. A carriage
        // return draws no glyph, so it still forms a run with its neighbor and still
        // counts as the whitespace trigger 3 looks for; only the line ending survives
        // the fix.
        for (content, strict) in [("a\u{200B}\rb", false), ("a\r\u{200B}b", false), ("a\u{200C}\rb", true)] {
            let findings = check_with_config(content, strict, "");
            assert_eq!(findings.len(), 1, "{content:?} (strict={strict}) gave {findings:?}");
            assert_eq!(fix_with_config(content, strict, ""), "a\rb", "fixing {content:?}");
        }
    }

    #[test]
    fn test_line_feeds_do_not_defeat_the_skip_guard() {
        // `should_skip` scans the whole document rather than the split lines, so it
        // sees line feeds. They are in the invisible set, so counting them as
        // reportable left the guard unable to skip any document with more than one
        // line.
        let ctx = LintContext::new("plain text\nsecond line\n", MarkdownFlavor::Standard, None);
        assert!(MD084InvisibleCharacters::default().should_skip(&ctx));

        let ctx = LintContext::new("hidden\u{200B}\n", MarkdownFlavor::Standard, None);
        assert!(!MD084InvisibleCharacters::default().should_skip(&ctx));
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
        let findings = check_with_config("\u{26A0}\u{FE0F} Note", true, "");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("strict mode"));
    }

    #[test]
    fn test_default_markup_unsuitable_characters_are_flagged() {
        // Only a substitution that preserves the text comes with a fix: the tone marks
        // have canonical equivalents, the object replacement character does not.
        let findings = check("\u{0340}deprecated\u{0341}\u{FFFC}");
        assert_eq!(findings.len(), 3, "Got {findings:?}");
        assert!(
            findings[0]
                .message
                .contains("U+0340 is not suitable for use with markup")
        );
        assert_eq!(findings[0].fix.as_ref().unwrap().replacement, "\u{0300}");
        assert!(
            findings[1]
                .message
                .contains("U+0341 is not suitable for use with markup")
        );
        assert_eq!(findings[1].fix.as_ref().unwrap().replacement, "\u{0301}");
        assert!(
            findings[2]
                .message
                .contains("U+FFFC is not suitable for use with markup")
        );
        assert!(findings[2].fix.is_none());
    }

    #[test]
    fn test_strict_markup_unsuitable_characters_are_flagged() {
        let findings = check_with_config("\u{0340}deprecated\u{0341}\u{FFFC}", true, "");
        assert_eq!(findings.len(), 3, "Got {findings:?}");
        assert_eq!(findings[0].fix.as_ref().unwrap().replacement, "\u{0300}");
        assert!(findings[1].message.contains("U+0341"));
        assert_eq!(findings[1].fix.as_ref().unwrap().replacement, "\u{0301}");
        assert!(findings[2].message.contains("U+FFFC"));
        assert!(findings[2].fix.is_none());
    }

    #[test]
    fn test_allowed_markup_unsuitable_characters_are_not_flagged() {
        let findings = check_with_config("\u{0340}deprecated\u{0341}\u{FFFC}", false, "\u{0340}\u{0341}\u{FFFC}");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_default_deprecated_visible_character_is_flagged_without_a_fix() {
        // U+0149 renders, so only the author knows what it should say instead.
        let findings = check("Cote d\u{0149}Ivoire");
        assert_eq!(findings.len(), 1, "Got {findings:?}");
        assert!(
            findings[0]
                .message
                .contains("Deprecated Unicode code point U+0149 detected")
        );
        assert!(findings[0].fix.is_none());
        assert_eq!(fix("Cote d\u{0149}Ivoire"), "Cote d\u{0149}Ivoire");
    }

    #[test]
    fn test_deprecated_and_invisible_keeps_the_removal_fix() {
        // U+206A is both invisible and deprecated. The invisible triggers carry a
        // removal fix, so they must win over the unfixable deprecated diagnostic.
        for (content, expected_fix) in [
            ("\u{206A}x", "x"),
            ("x\u{206A}", "x"),
            ("x \u{206A}y", "x y"),
            ("x\u{206A}\u{206B}y", "xy"),
        ] {
            let findings = check(content);
            assert_eq!(findings.len(), 1, "{content:?} gave {findings:?}");
            assert!(
                findings[0].message.starts_with("Invisible character")
                    || findings[0].message.contains("consecutive invisible characters"),
                "{content:?} gave {:?}",
                findings[0].message
            );
            assert_eq!(fix(content), expected_fix, "fixing {content:?}");
        }
    }

    #[test]
    fn test_deprecated_and_invisible_is_reported_once() {
        // Interior, non-adjacent to whitespace: no invisible trigger applies, so the
        // deprecated diagnostic is the only one, and it carries no fix.
        let findings = check("x\u{206A}y");
        assert_eq!(findings.len(), 1, "Got {findings:?}");
        assert!(
            findings[0]
                .message
                .contains("Deprecated Unicode code point U+206A detected")
        );
        assert!(findings[0].fix.is_none());
        assert_eq!(fix("x\u{206A}y"), "x\u{206A}y");
    }

    #[test]
    fn test_interlinear_annotation_is_reported_but_never_stripped() {
        // U+FFF9..U+FFFB delimit ruby text. They are not default-ignorable, and deleting
        // one would leave the annotation half-open, so they are reported without a fix.
        let content = "\u{FFF9}base\u{FFFA}gloss\u{FFFB}";
        let findings = check(content);
        assert_eq!(findings.len(), 3, "Got {findings:?}");
        for finding in &findings {
            assert!(finding.message.contains("is not suitable for use with markup"));
            assert!(finding.fix.is_none());
        }
        assert_eq!(fix(content), content);
    }

    #[test]
    fn test_annotation_delimiter_is_not_a_presentation_base() {
        // An annotation delimiter draws no glyph, so a selector or joiner beside one
        // has nothing to modify and stays reportable, mid-line as much as at a line
        // boundary: the pair is a cluster of characters that draw nothing. The fix
        // removes only the orphan; the delimiter itself is never stripped.
        for (content, expected_fix) in [
            ("\u{FFF9}\u{FE0F}", "\u{FFF9}"),
            ("\u{FFF9}\u{200D}", "\u{FFF9}"),
            ("base\u{FFF9}\u{FE0F}", "base\u{FFF9}"),
            ("x\u{FFF9}\u{FE0F}y", "x\u{FFF9}y"),
            ("x\u{FFF9}\u{200D}y", "x\u{FFF9}y"),
        ] {
            let findings = check(content);
            assert_eq!(findings.len(), 2, "{content:?} gave {findings:?}");
            assert!(
                findings.iter().any(|f| f.message.contains("Invisible character")
                    || f.message.contains("consecutive invisible characters")),
                "{content:?} gave {findings:?}"
            );
            assert_eq!(fix(content), expected_fix, "fixing {content:?}");
        }
    }

    #[test]
    fn test_reserved_specials_below_the_annotation_block_stay_invisible() {
        // U+FFF0..U+FFF8 are default-ignorable, so they keep the removal fix.
        let findings = check("\u{FFF8}x");
        assert_eq!(findings.len(), 1, "Got {findings:?}");
        assert!(
            findings[0]
                .message
                .contains("Invisible character U+FFF8 detected at line boundary")
        );
        assert_eq!(fix("\u{FFF8}x"), "x");
    }
}
