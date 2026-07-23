//! Rule MD083: Detect mojibake due to encoding issues.
//!
//! Mojibake is the result of text being decoded using an unintended character encoding.
//! It is often caused by a mismatch between the encoding used to create a file and the encoding used to read it.
//! This rule detects common mojibake sequences, which are typically caused by UTF-8 text being interpreted as Windows-1252 or ISO-8859-1.
//!
//! The Mojibake detection regex is based on the work of `ftfy` by Robyn Speer, at https://github.com/rspeer/python-ftfy, under Apache 2.0 License.
//! The test cases are based on https://github.com/kevinhu/plsfix/blob/main/core/src/badness.rs by Kevin Hu, under Apache 2.0 License.

use crate::filtered_lines::FilteredLinesExt;
use crate::lint_context::LintContext;
use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::byte_to_char_count;

fn build_mojibake_regex() -> regex::Regex {
    regex::Regex::new(
        &format!(
r#"[{c1}]
|
[{bad}{lower_accented}{upper_accented}{box}{start_punctuation}{end_punctuation}{currency}{numeric}] [{bad}]
|
[a-zA-Z] [{lower_common}{upper_common}] [{bad}]
|
[{bad}] [{lower_accented}{upper_accented}{box}{start_punctuation}{end_punctuation}{currency}{numeric}]
|
[{lower_accented}{lower_common}{box}{end_punctuation}{currency}{numeric}] [{upper_accented}]
|
[{box}{end_punctuation}{currency}{numeric}] [{lower_accented}]
|
[{lower_accented}{box}{end_punctuation}] [{currency}]
|
\s [{upper_accented}] [{currency}]
|
[{upper_accented}{box}] [{numeric}]
|
[{lower_accented}{upper_accented}{box}{currency}{end_punctuation}] [{start_punctuation}] [{numeric}]
|
[{lower_accented}{upper_accented}{currency}{numeric}{box}] [{end_punctuation}] [{start_punctuation}]
|
[{currency}{numeric}{box}] [{start_punctuation}]
|
[a-z] [{upper_accented}] [{start_punctuation}{currency}]
|
[{box}] [{kaomoji}]
|
[{lower_accented}{upper_accented}{currency}{numeric}{start_punctuation}{end_punctuation}] [{box}]
|
[{box}] [{end_punctuation}]
|
[{lower_accented}{upper_accented}] [{end_punctuation}] \w
|
[Œœ][^A-Za-z\s"'»”’]
|
[ÂÃÎÐ][€Šš¢£Ÿž{nbsp}{soft_hyphen}®©°·»{start_punctuation}{end_punctuation}–—´]
|
× [²³]
|
[ØÙ] [{common}{currency}{bad}{numeric}{start_punctuation}ŸŠ®°µ»]
[ØÙ] [{common}{currency}{bad}{numeric}{start_punctuation}ŸŠ®°µ»]
|
à[²µ¹¼½¾]
|
√[±∂†≠®™´≤≥¥µø]
|
≈[°¢]
|
‚Ä[ìîïòôúùû†°¢π]
|
‚[âó][àä°ê]
|
вЂ
|
[ВГРС][{c1}{bad}{start_punctuation}{end_punctuation}{currency}°µ][ВГРС]
|
ГўВЂВ.[A-Za-z]
|
Ã[{nbsp}¡]
|
^[ÃÂ][\s]
|
[a-z.,?!{end_punctuation}] Â [ {start_punctuation}{end_punctuation}]
|
β€[™{nbsp}Ά{soft_hyphen}®°]
|
[ΒΓΞΟ][{c1}{bad}{start_punctuation}{end_punctuation}{currency}°][ΒΓΞΟ]"#,
        c1 = "\u{80}\u{81}\u{82}\u{83}\u{84}\u{85}\u{86}\u{87}\u{88}\u{89}\u{8a}\u{8b}\u{8c}\u{8d}\u{8e}\u{8f}\u{90}\u{91}\u{92}\u{93}\u{94}\u{95}\u{96}\u{97}\u{98}\u{99}\u{9a}\u{9b}\u{9c}\u{9d}\u{9e}\u{9f}",
        bad = "¦¤¨¬¯¶§¸ƒˆˇ˘˛˜†‡‰⌐◊�ªº",
        lower_accented = "ßà-ñăąćčďđęěğĺľłœŕśşšťüźżžґﬁﬂ",
        upper_accented = "À-ÑØÜÝĂĄĆČĎĐĘĚĞİĹĽŁŃŇŒŘŚŞŠŢŤŮŰŸŹŻŽҐ",
        box = "│┌┐┘├┤┬┼═-╬▀▄█▌▐░▒▓",
        start_punctuation = "¡«¿©΄΅‘‚“„•‹\u{f8ff}",
        end_punctuation = "®»˝”›™",
        currency = "¢£¥₧€",
        numeric = "²³¹±¼½¾×µ÷⁄∂∆∏∑√∞∩∫≈≠≡≤≥№",
        kaomoji = "Ò-ÖÙ-Üò-öø-üŐ°",
        lower_common = "α-ωάέήίΰа-џ",
        upper_common = "ÞΑ-ΩΆΈΉΊΌΎΏΪΫЁ-Я",
        common = "\u{a0}\u{ad}\u{b7}\u{b4}\u{2013}\u{2014}\u{2015}\u{2026}\u{2019}",
        nbsp = "`\u{a0}",
        soft_hyphen = "\u{ad}",
    ).replace("\n", "").replace(' ', "")
    ).unwrap()
}

#[derive(Debug, Clone)]
pub struct MD083DetectMojibake {
    badness_re: regex::Regex,
}

impl Default for MD083DetectMojibake {
    fn default() -> Self {
        Self {
            badness_re: build_mojibake_regex(),
        }
    }
}

impl Rule for MD083DetectMojibake {
    fn name(&self) -> &'static str {
        "MD083"
    }

    fn description(&self) -> &'static str {
        "Detect mojibake due to encoding issues"
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD083DetectMojibake::default())
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Other
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();
        for line in ctx.filtered_lines() {
            for mat in self.badness_re.find_iter(line.content) {
                let (start, end) = (mat.start(), mat.end());
                let line_num = line.line_num;
                let column = byte_to_char_count(line.content, start);
                let end_column = byte_to_char_count(line.content, end);
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: line_num,
                    column,
                    end_line: line_num,
                    end_column,
                    severity: Severity::Warning,
                    message: "Mojibake detected; text may be mis-encoded".to_string(),
                    fix: None,
                });
            }
        }
        Ok(warnings)
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        // Detection only: use external tools to fix encoding issues
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::rule::LintWarning;

    fn check(content: &str) -> Vec<LintWarning> {
        let rule = MD083DetectMojibake::default();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        rule.check(&ctx).unwrap()
    }

    fn assert_mojibake_at(
        warning: &LintWarning,
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
    ) {
        assert!(warning.rule_name.as_deref() == Some("MD083"));
        assert!(warning.message.contains("Mojibake detected"));
        assert_eq!(warning.line, line);
        assert_eq!(warning.column, column);
        assert_eq!(warning.end_line, end_line);
        assert_eq!(warning.end_column, end_column);
    }

    #[test]
    fn test_mojibake() {
        let results = check("This is a test with mojibake: â€“\n");
        assert_eq!(results.len(), 1);
        assert_mojibake_at(&results[0], 1, 31, 1, 33);
    }

    #[test]
    fn test_normal_text() {
        let results = check("This is a normal line of text.\n");
        assert!(results.is_empty());
    }

    #[test]
    fn test_special_char_1() {
        let results = check("\u{80}\n");
        assert_eq!(results.len(), 1);
        assert_mojibake_at(&results[0], 1, 1, 1, 2);
    }

    #[test]
    fn test_special_2() {
        let results = check("Ã¡.");
        assert_eq!(results.len(), 1);
        assert_mojibake_at(&results[0], 1, 1, 1, 3);
    }

    #[test]
    fn test_empty() {
        let results = check("");
        assert!(results.is_empty());
    }

    // Test checks badness count of a simple sentence with mixed character categories
    #[test]
    fn test_mixed_chars() {
        let results = check("À-Ñ this is some text \u{a0}\u{ad} to test on \u{80}");
        assert_eq!(results.len(), 1);
        assert_mojibake_at(&results[0], 1, 37, 1, 38);
    }

    // Test checks badness count of different capital char sequence
    #[test]
    fn test_upper_accented_chars() {
        let results = check("ÀÑØÜÝĂĄĆČĎĐĘ");
        assert_eq!(results.len(), 0);
    }

    // Checks if basic alphanumeric are not considered as bad
    #[test]
    fn test_alphanumeric() {
        let results = check("abc123XYZ");
        assert_eq!(results.len(), 0);
    }

    // Checks a text with known badness, should return true
    #[test]
    fn test_known_badness() {
        let results = check("Ã¡.");
        assert_eq!(results.len(), 1);
        assert_mojibake_at(&results[0], 1, 1, 1, 3);
    }

    #[test]
    fn test_numeric_char() {
        assert!(check("²³¹±¼½¾×µ÷⁄∂∆").len() == 0);
    }

    #[test]
    fn test_kaomoji_char() {
        assert!(check("Ò-ÖÙ-Üò-öø-üŐ°").len() == 0);
    }

    #[test]
    fn test_upper_common_chars() {
        assert!(check("ÞΑ-ΩΆΈΉΊΌΎΏΪΫЁ-Я").len() == 0);
    }

    #[test]
    fn test_lower_common_chars() {
        assert!(check("α-ωάέήίΰа-џ").len() == 0);
    }

    #[test]
    fn test_currency_chars() {
        assert!(check("¢£¥₧€").len() == 0);
    }

    #[test]
    fn test_punctuation_chars() {
        assert!(check("¡«¿©΄΅‘‚“„•‹\u{f8ff}").len() == 0);
        assert!(check("®»˝”›™").len() == 0);
    }

    #[test]
    fn test_full_text_with_boundaries() {
        let results = check("¦¤");
        assert!(results.len() == 1);
        assert_mojibake_at(&results[0], 1, 1, 1, 3);
    }

    #[test]
    fn test_box_drawing_chars() {
        assert!(check("│┌┐┘├┤┬┼═-╬▀▄█▌▐░▒▓").len() == 0);
    }

    #[test]
    fn test_known_badness_emoji() {
        assert!(check("😀").len() == 0);
    }

    #[test]
    fn test_spaced_bad_char() {
        let results = check("   \u{80}   ");
        assert_eq!(results.len(), 1);
        assert!(results[0].message.contains("Mojibake detected"));
        assert_mojibake_at(&results[0], 1, 4, 1, 5);
    }

    // Test checks badness count of a simple sentence with all bad characters
    #[test]
    fn test_all_bad_chars() {
        let results = check("¦¤¨¬¯¶§¸ƒˆˇ˘˛˜†‡‰⌐◊�ªº");
        assert_eq!(results.len(), 11);
        assert!(results[0].message.contains("Mojibake detected"));
        assert_mojibake_at(&results[0], 1, 1, 1, 3);
        assert_mojibake_at(&results[1], 1, 3, 1, 5);
        assert_mojibake_at(&results[2], 1, 5, 1, 7);
        assert_mojibake_at(&results[3], 1, 7, 1, 9);
        assert_mojibake_at(&results[4], 1, 9, 1, 11);
        assert_mojibake_at(&results[5], 1, 11, 1, 13);
        assert_mojibake_at(&results[6], 1, 13, 1, 15);
        assert_mojibake_at(&results[7], 1, 15, 1, 17);
        assert_mojibake_at(&results[8], 1, 17, 1, 19);
        assert_mojibake_at(&results[9], 1, 19, 1, 21);
        assert_mojibake_at(&results[10], 1, 21, 1, 23);
    }

    // Checks if punctuation character are not considered as bad
    #[test]
    fn test_punctuation() {
        assert!(check("!@#$%^&*()_-+={}|[]\\:\";'<>,.?/").len() == 0);
    }

    // Checks a sentence including lower common characters and numbers, should return false
    #[test]
    fn test_lower_common_chars_and_numbers() {
        assert!(check("Один два απο ένα δύο α-ωάέήίΰа-џ 123 £$%").len() == 0);
    }

    // Test checks if non-breaking space and soft hyphen are not considered as bad
    #[test]
    fn test_control_chars() {
        assert_eq!(check("\u{a0}\u{ad}").len(), 0);
    }

    // Test checks badness of complex sentence with multiple categories
    #[test]
    fn test_complex_sentence() {
        let results = check("Hello, this sentence will have a badness score of 1, because of this \u{80} char.");
        assert_eq!(results.len(), 1);
        assert!(results[0].message.contains("Mojibake detected"));
        assert_mojibake_at(&results[0], 1, 70, 1, 71);
    }

    #[test]
    fn test_multiline_mixed_content() {
        let results = check(
            "Alpha clean line.\n\
Broken dash â€“ here.\n\
Normal text again.\n\
Another bad pair: Ã¡.\n\
Numbers 12345.\n\
Trailing bad \u{80}\n\
Русский текст.\n\
Symbols are fine: £$%\n\
Mojibake word â€“ end.\n\
Last clean line.\n",
        );

        assert_eq!(results.len(), 4);
        assert_mojibake_at(&results[0], 2, 13, 2, 15);
        assert_mojibake_at(&results[1], 4, 19, 4, 21);
        assert_mojibake_at(&results[2], 6, 14, 6, 15);
        assert_mojibake_at(&results[3], 9, 15, 9, 17);
    }

    // Check that a simple English sentence is not considered "bad"
    #[test]
    fn test_simple_sentence() {
        assert_eq!(check("The quick brown fox jumps over the lazy dog.").len(), 0);
    }

    // Test checks badness count of an emoji
    #[test]
    fn test_emoji() {
        assert_eq!(check("😀").len(), 0);
    }

    // Checks a text with single space, should return false
    #[test]
    fn test_single_space() {
        assert_eq!(check(" ").len(), 0);
    }

    // Test checks badness count of one specific bad character
    #[test]
    fn test_single_bad_char() {
        assert_eq!(check("¦").len(), 0);
    }

    // Check a text with a non-breaking space, should return false
    #[test]
    fn test_non_breaking_space() {
        assert_eq!(check("Hello, World!\u{a0}").len(), 0);
    }

    // Check badness calculation with all character categories
    #[test]
    fn test_all_categories() {
        let results = check("¢£¥₧€¡«¿©΄΅‘‚“„•‹\u{f8ff}®»˝”›™²³¹±¼½¾×µ÷⁄∂∆ÞΑ-ΩΆΈΉΊΌΎΏΪΫЁ-Яα-ωάέήίΰа-џ│┌┐┘├┤┬┼═-╬▀▄█▌▐░▒▓");
        assert_eq!(results.len(), 1);
        assert!(results[0].message.contains("Mojibake detected"));
        assert_mojibake_at(&results[0], 1, 5, 1, 7);
    }

    // Check a text with full-width white space, should return false
    #[test]
    fn test_full_width_space() {
        assert_eq!(check("Hello, World!\u{3000}").len(), 0);
    }

    // Check badness calculation with a range of special characters
    #[test]
    fn test_special() {
        assert_eq!(check("&quot;ًٌٍََُِّْٕٖٜٟٓٔٗ٘ٙٚٛٝٞ").len(), 0);
    }

    // Test checks a sentence including upper common characters and numbers, should return false
    #[test]
    fn test_upper_common_chars_and_numbers() {
        assert_eq!(check("One two Α-Ω Ί Ώ Ύ Ό РУС 123 £$%").len(), 0);
    }

    // Test checks if a simple Japanese sentence is not considered as bad
    #[test]
    fn test_japanese() {
        assert_eq!(check("こんにちは、世界！").len(), 0);
    }

    // Checks a text fully composed of badness, should return true
    #[test]
    fn test_full_badness() {
        let results = check("Ã\u{80}\u{82}€‚");
        assert_eq!(results.len(), 3);
        assert!(results[0].message.contains("Mojibake detected"));
        assert_mojibake_at(&results[0], 1, 2, 1, 3);
        assert_mojibake_at(&results[1], 1, 3, 1, 4);
        assert_mojibake_at(&results[2], 1, 4, 1, 6);
    }

    // Test checks badness count of a simple sentence with various special characters
    #[test]
    fn test_special_chars() {
        let results = check("This sentence contains these \u{a0}\u{ad}\u{80} special characters.");
        assert_eq!(results.len(), 1);
        assert!(results[0].message.contains("Mojibake detected"));
        assert_mojibake_at(&results[0], 1, 32, 1, 33);
    }

    // Test checks if a simple Chinese sentence is not considered as bad
    #[test]
    fn test_chinese() {
        assert_eq!(check("你好，世界！").len(), 0);
    }

    // Test checks badness of sentence with mixed languages with bad character
    #[test]
    fn test_mixed_languages() {
        let results = check("This is English and これは日本語です and dies ist Deutsch \u{80}");
        assert_eq!(results.len(), 1);
        assert_mojibake_at(&results[0], 1, 51, 1, 52);
    }

    // Test checks if a simple Arabic sentence is not considered as bad
    #[test]
    fn test_arabic() {
        assert_eq!(check("مرحبا بك في النص باللغة العربية!").len(), 0);
    }

    // Test checks badness of a sentence with kaomoji
    #[test]
    fn test_kaomoji_sentence() {
        assert_eq!(check("This is a sentence with kaomoji (ˆ_ˆ)").len(), 0);
    }

    // Test checks if a simple Russian sentence is not considered as bad
    #[test]
    fn test_russian() {
        assert_eq!(check("Всем привет, мир!").len(), 0);
    }

    // Test checks badness of sentence with various punctuation and numeric characters
    #[test]
    fn test_punctuation_numeric() {
        assert_eq!(check("This (®»˝”›™²³¹±¼½¾×µ÷⁄∂∆) is text.").len(), 0);
    }

    // Checks if a sentence that contains all common upper chars is not considered bad
    #[test]
    fn test_all_upper_common() {
        assert_eq!(check("ÞΑ-ΩΆΈΉΊΌΎΏΪΫЁ-Я").len(), 0);
    }

    // Checks a sentence with consecutive bad ```rust characters
    #[test]
    fn test_consecutive_bad() {
        assert_eq!(
            check("This sentence has consecutive bad characters \u{80}\u{80}\u{80}\u{80}").len(),
            4
        );

        let results = check("This sentence has consecutive bad characters \u{80}\u{80}\u{80}\u{80}");
        assert_mojibake_at(&results[0], 1, 46, 1, 47);
        assert_mojibake_at(&results[1], 1, 47, 1, 48);
        assert_mojibake_at(&results[2], 1, 48, 1, 49);
        assert_mojibake_at(&results[3], 1, 49, 1, 50);
    }

    // Checks that valid french sentences with a œ ligature are not considered as bad
    #[test]
    fn test_french_oe_ligature() {
        assert_eq!(check("Œuvre d'art").len(), 0);
        assert_eq!(check("Cœur de l'œuvre").len(), 0);
        assert_eq!(check("L'œuvre est magnifique").len(), 0);
        assert_eq!(check("Ce remarquable Œuf Fabergé est une œuvre d'art en forme d'œuf.").len(), 0);
        assert_eq!(check("La ligature œ est formée de la contraction des caractères o et e.").len(), 0);
        assert_eq!(check("La ligature \"œ\" s'écrit en majuscules \"Œ\".").len(), 0);
    }

    // Test single letters enumerations
    #[test]
    fn test_single_letters() {
        assert_eq!(check("a b c d e f g h i j k l m n o p q r s t u v w x y z").len(), 0);
        assert_eq!(check("A B C D E F G H I J K L M N O P Q R S T U V W X Y Z").len(), 0);
        assert_eq!(check("sunt cuvinte cu Â și Î").len(), 0);
        assert_eq!(check("Vietnamese nguyên âm có dấu mũ: a Â e Ê o Ô").len(), 0);
    }
}
