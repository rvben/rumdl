use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::anchor_styles::AnchorStyle;
use crate::utils::regex_cache::get_cached_regex;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

lazy_static! {
    // Pre-compiled optimized patterns for quick checks
    static ref QUICK_MARKDOWN_CHECK: Regex = Regex::new(r"[*_`\[\]]").unwrap();
    // GitHub only strips asterisks (*), not underscores (_) - underscores are preserved
    static ref EMPHASIS_PATTERN: Regex = Regex::new(r"\*+([^*]+)\*+").unwrap();
    static ref CODE_PATTERN: Regex = Regex::new(r"`([^`]+)`").unwrap();
    static ref LINK_PATTERN: Regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)|\[([^\]]+)\]\[[^\]]*\]").unwrap();
    static ref TOC_SECTION_START: Regex = Regex::new(r"(?i)^#+\s*(table\s+of\s+contents?|contents?|toc)\s*$").unwrap();
}

/// Rule MD051: Link fragments
///
/// See [docs/md051.md](../../docs/md051.md) for full documentation, configuration, and examples.
///
/// This rule validates that link anchors (the part after #) exist in the current document.
/// Only applies to internal document links (like #heading), not to external URLs or cross-file links.
#[derive(Clone)]
pub struct MD051LinkFragments {
    /// Anchor style to use for validation
    anchor_style: AnchorStyle,
}

impl Default for MD051LinkFragments {
    fn default() -> Self {
        Self::new()
    }
}

impl MD051LinkFragments {
    pub fn new() -> Self {
        Self {
            anchor_style: AnchorStyle::GitHub,
        }
    }

    /// Create with specific anchor style
    pub fn with_anchor_style(style: AnchorStyle) -> Self {
        Self { anchor_style: style }
    }

    /// Extract headings from cached LintContext information
    fn extract_headings_from_context(&self, ctx: &crate::lint_context::LintContext) -> HashSet<String> {
        let mut headings = HashSet::with_capacity(32);
        let mut fragment_counts = std::collections::HashMap::new();
        let mut in_toc = false;

        // Single pass through lines, only processing lines with headings
        for line_info in &ctx.lines {
            // Skip front matter
            if line_info.in_front_matter {
                continue;
            }

            if let Some(heading) = &line_info.heading {
                let line = &line_info.content;

                // Check if we're entering a TOC section
                let is_toc_heading = TOC_SECTION_START.is_match(line);

                // If we were in TOC and hit another heading, we're out of TOC
                if in_toc && !is_toc_heading {
                    in_toc = false;
                }

                // Skip if we're inside a TOC section (but not the TOC heading itself)
                if in_toc && !is_toc_heading {
                    continue;
                }

                // If heading has a custom ID, add it as a valid anchor
                if let Some(custom_id) = &heading.custom_id {
                    headings.insert(custom_id.clone());
                }

                // ALWAYS generate the normal anchor too (for backward compatibility)
                // This ensures both the custom ID and the generated anchor work
                let fragment = self.anchor_style.generate_fragment(&heading.text);

                if !fragment.is_empty() {
                    // Handle duplicate fragments by appending numbers
                    let final_fragment = if let Some(count) = fragment_counts.get_mut(&fragment) {
                        let suffix = *count;
                        *count += 1;
                        format!("{fragment}-{suffix}")
                    } else {
                        fragment_counts.insert(fragment.clone(), 1);
                        fragment
                    };
                    headings.insert(final_fragment);
                }

                // After processing the TOC heading, mark that we're in a TOC section
                if is_toc_heading {
                    in_toc = true;
                }
            }
        }

        headings
    }

    /// Fragment generation using GitHub.com's ACTUAL algorithm (verified against GitHub Gists)
    /// This is the exact algorithm discovered by testing against GitHub.com itself, not third-party packages
    #[inline]
    pub fn heading_to_fragment_github_official(&self, heading: &str) -> String {
        if heading.is_empty() {
            return String::new();
        }

        // Strip markdown formatting first
        let text = if QUICK_MARKDOWN_CHECK.is_match(heading) {
            self.strip_markdown_formatting_fast(heading)
        } else {
            heading.to_string()
        };

        // GitHub.com's ACTUAL algorithm discovered through Gist testing:
        // Based on analysis of https://gist.github.com/rvben/da6f7faf265f69fd8d6fd247ee526beb

        let text = text.to_lowercase();
        let mut result = String::with_capacity(text.len());
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();

        let mut i = 0;
        while i < len {
            let c = chars[i];

            // Handle multi-character patterns first (order matters)
            if i + 2 < len && chars[i] == '-' && chars[i + 1] == '-' && chars[i + 2] == '>' {
                // --> becomes ----
                result.push_str("----");
                i += 3;
                continue;
            }

            if i + 2 < len && chars[i] == '<' && chars[i + 1] == '-' && chars[i + 2] == '>' {
                // <-> becomes ---
                result.push_str("---");
                i += 3;
                continue;
            }

            if i + 2 < len && chars[i] == '=' && chars[i + 1] == '=' && chars[i + 2] == '>' {
                // ==> becomes --
                result.push_str("--");
                i += 3;
                continue;
            }

            if i + 1 < len && chars[i] == '-' && chars[i + 1] == '>' {
                // -> becomes ---
                result.push_str("---");
                i += 2;
                continue;
            }

            // Single character processing
            match c {
                // Preserve all letters (including Unicode)
                c if c.is_alphabetic() => result.push(c),

                // Preserve all digits
                c if c.is_ascii_digit() => result.push(c),

                // Preserve underscores (critical difference from other implementations)
                '_' => result.push(c),

                // Preserve hyphens (including multiple consecutive)
                '-' => result.push(c),

                // Convert spaces to hyphens, but check for upcoming arrow patterns
                ' ' => {
                    // Check if this space is followed by an arrow pattern that we'll handle
                    let followed_by_arrow =
                        (i + 3 < len && chars[i + 1] == '-' && chars[i + 2] == '-' && chars[i + 3] == '>')
                            || (i + 4 < len && chars[i + 1] == '<' && chars[i + 2] == '-' && chars[i + 3] == '>')
                            || (i + 4 < len && chars[i + 1] == '=' && chars[i + 2] == '=' && chars[i + 3] == '>')
                            || (i + 2 < len && chars[i + 1] == '-' && chars[i + 2] == '>');

                    if !followed_by_arrow {
                        result.push('-');
                    }
                    // If followed by arrow, skip the space (it gets absorbed into the arrow replacement)
                }

                // Special symbols become double hyphens
                '&' => result.push_str("--"),
                '©' => result.push_str("--"),

                // Emojis become single hyphens (broader Unicode range for emojis)
                c if (c as u32 >= 0x1F300 && c as u32 <= 0x1F9FF)
                    || (c as u32 >= 0x1F600 && c as u32 <= 0x1F64F)
                    || (c as u32 >= 0x2600 && c as u32 <= 0x26FF) =>
                {
                    result.push('-')
                }

                // Most other punctuation is removed (dots, colons, brackets, etc.)
                _ => {
                    // Skip all other characters (remove them)
                }
            }

            i += 1;
        }

        result
    }

    /// Fragment generation following GitHub's official slugger algorithm
    /// Uses the exact algorithm from github-slugger npm package used by GitHub.com
    #[inline]
    pub fn heading_to_fragment_github(&self, heading: &str) -> String {
        if heading.is_empty() {
            return String::new();
        }

        // Strip markdown formatting first
        let text = if QUICK_MARKDOWN_CHECK.is_match(heading) {
            self.strip_markdown_formatting_fast(heading)
        } else {
            heading.to_string()
        };

        // NOTE: GitHub does NOT trim whitespace - it preserves leading/trailing spaces
        // and converts them to hyphens. This matches the official github-slugger behavior.

        // GitHub's EXACT algorithm from github-slugger npm package:
        // function slug(value, maintainCase) {
        //   if (typeof value !== 'string') return ''
        //   if (!maintainCase) value = value.toLowerCase()
        //   return value.replace(regex, '').replace(/ /g, '-')
        // }

        // 1. Convert to lowercase (maintainCase = false)
        let text = text.to_lowercase();

        // 2. Apply GitHub's exact regex pattern to remove punctuation/symbols
        // This is the EXACT regex from github-slugger/regex.js
        // Note: In Rust, we need to use \u{} syntax for Unicode above \uFFFF
        // The original JavaScript regex includes surrogate pairs that we convert to direct Unicode ranges
        let github_regex = get_cached_regex(
            r"[\x00-\x1F!-,\./:-@\[-\^`\{-\xA9\xAB-\xB4\xB6-\xB9\xBB-\xBF\xD7\xF7\u{02C2}-\u{02C5}\u{02D2}-\u{02DF}\u{02E5}-\u{02EB}\u{02ED}\u{02EF}-\u{02FF}\u{0375}\u{0378}\u{0379}\u{037E}\u{0380}-\u{0385}\u{0387}\u{038B}\u{038D}\u{03A2}\u{03F6}\u{0482}\u{0530}\u{0557}\u{0558}\u{055A}-\u{055F}\u{0589}-\u{0590}\u{05BE}\u{05C0}\u{05C3}\u{05C6}\u{05C8}-\u{05CF}\u{05EB}-\u{05EE}\u{05F3}-\u{060F}\u{061B}-\u{061F}\u{066A}-\u{066D}\u{06D4}\u{06DD}\u{06DE}\u{06E9}\u{06FD}\u{06FE}\u{0700}-\u{070F}\u{074B}\u{074C}\u{07B2}-\u{07BF}\u{07F6}-\u{07F9}\u{07FB}\u{07FC}\u{07FE}\u{07FF}\u{082E}-\u{083F}\u{085C}-\u{085F}\u{086B}-\u{089F}\u{08B5}\u{08C8}-\u{08D2}\u{08E2}\u{0964}\u{0965}\u{0970}\u{0984}\u{098D}\u{098E}\u{0991}\u{0992}\u{09A9}\u{09B1}\u{09B3}-\u{09B5}\u{09BA}\u{09BB}\u{09C5}\u{09C6}\u{09C9}\u{09CA}\u{09CF}-\u{09D6}\u{09D8}-\u{09DB}\u{09DE}\u{09E4}\u{09E5}\u{09F2}-\u{09FB}\u{09FD}\u{09FF}\u{0A00}\u{0A04}\u{0A0B}-\u{0A0E}\u{0A11}\u{0A12}\u{0A29}\u{0A31}\u{0A34}\u{0A37}\u{0A3A}\u{0A3B}\u{0A3D}\u{0A43}-\u{0A46}\u{0A49}\u{0A4A}\u{0A4E}-\u{0A50}\u{0A52}-\u{0A58}\u{0A5D}\u{0A5F}-\u{0A65}\u{0A76}-\u{0A80}\u{0A84}\u{0A8E}\u{0A92}\u{0AA9}\u{0AB1}\u{0AB4}\u{0ABA}\u{0ABB}\u{0AC6}\u{0ACA}\u{0ACE}\u{0ACF}\u{0AD1}-\u{0ADF}\u{0AE4}\u{0AE5}\u{0AF0}-\u{0AF8}\u{0B00}\u{0B04}\u{0B0D}\u{0B0E}\u{0B11}\u{0B12}\u{0B29}\u{0B31}\u{0B34}\u{0B3A}\u{0B3B}\u{0B45}\u{0B46}\u{0B49}\u{0B4A}\u{0B4E}-\u{0B54}\u{0B58}-\u{0B5B}\u{0B5E}\u{0B64}\u{0B65}\u{0B70}\u{0B72}-\u{0B81}\u{0B84}\u{0B8B}-\u{0B8D}\u{0B91}\u{0B96}-\u{0B98}\u{0B9B}\u{0B9D}\u{0BA0}-\u{0BA2}\u{0BA5}-\u{0BA7}\u{0BAB}-\u{0BAD}\u{0BBA}-\u{0BBD}\u{0BC3}-\u{0BC5}\u{0BC9}\u{0BCE}\u{0BCF}\u{0BD1}-\u{0BD6}\u{0BD8}-\u{0BE5}\u{0BF0}-\u{0BFF}\u{0C0D}\u{0C11}\u{0C29}\u{0C3A}-\u{0C3C}\u{0C45}\u{0C49}\u{0C4E}-\u{0C54}\u{0C57}\u{0C5B}-\u{0C5F}\u{0C64}\u{0C65}\u{0C70}-\u{0C7F}\u{0C84}\u{0C8D}\u{0C91}\u{0CA9}\u{0CB4}\u{0CBA}\u{0CBB}\u{0CC5}\u{0CC9}\u{0CCE}-\u{0CD4}\u{0CD7}-\u{0CDD}\u{0CDF}\u{0CE4}\u{0CE5}\u{0CF0}\u{0CF3}-\u{0CFF}\u{0D0D}\u{0D11}\u{0D45}\u{0D49}\u{0D4F}-\u{0D53}\u{0D58}-\u{0D5E}\u{0D64}\u{0D65}\u{0D70}-\u{0D79}\u{0D80}\u{0D84}\u{0D97}-\u{0D99}\u{0DB2}\u{0DBC}\u{0DBE}\u{0DBF}\u{0DC7}-\u{0DC9}\u{0DCB}-\u{0DCE}\u{0DD5}\u{0DD7}\u{0DE0}-\u{0DE5}\u{0DF0}\u{0DF1}\u{0DF4}-\u{0E00}\u{0E3B}-\u{0E3F}\u{0E4F}\u{0E5A}-\u{0E80}\u{0E83}\u{0E85}\u{0E8B}\u{0EA4}\u{0EA6}\u{0EBE}\u{0EBF}\u{0EC5}\u{0EC7}\u{0ECE}\u{0ECF}\u{0EDA}\u{0EDB}\u{0EE0}-\u{0EFF}\u{0F01}-\u{0F17}\u{0F1A}-\u{0F1F}\u{0F2A}-\u{0F34}\u{0F36}\u{0F38}\u{0F3A}-\u{0F3D}\u{0F48}\u{0F6D}-\u{0F70}\u{0F85}\u{0F98}\u{0FBD}-\u{0FC5}\u{0FC7}-\u{0FFF}\u{104A}-\u{104F}\u{109E}\u{109F}\u{10C6}\u{10C8}-\u{10CC}\u{10CE}\u{10CF}\u{10FB}\u{1249}\u{124E}\u{124F}\u{1257}\u{1259}\u{125E}\u{125F}\u{1289}\u{128E}\u{128F}\u{12B1}\u{12B6}\u{12B7}\u{12BF}\u{12C1}\u{12C6}\u{12C7}\u{12D7}\u{1311}\u{1316}\u{1317}\u{135B}\u{135C}\u{1360}-\u{137F}\u{1390}-\u{139F}\u{13F6}\u{13F7}\u{13FE}-\u{1400}\u{166D}\u{166E}\u{1680}\u{169B}-\u{169F}\u{16EB}-\u{16ED}\u{16F9}-\u{16FF}\u{170D}\u{1715}-\u{171F}\u{1735}-\u{173F}\u{1754}-\u{175F}\u{176D}\u{1771}\u{1774}-\u{177F}\u{17D4}-\u{17D6}\u{17D8}-\u{17DB}\u{17DE}\u{17DF}\u{17EA}-\u{180A}\u{180E}\u{180F}\u{181A}-\u{181F}\u{1879}-\u{187F}\u{18AB}-\u{18AF}\u{18F6}-\u{18FF}\u{191F}\u{192C}-\u{192F}\u{193C}-\u{1945}\u{196E}\u{196F}\u{1975}-\u{197F}\u{19AC}-\u{19AF}\u{19CA}-\u{19CF}\u{19DA}-\u{19FF}\u{1A1C}-\u{1A1F}\u{1A5F}\u{1A7D}\u{1A7E}\u{1A8A}-\u{1A8F}\u{1A9A}-\u{1AA6}\u{1AA8}-\u{1AAF}\u{1AC1}-\u{1AFF}\u{1B4C}-\u{1B4F}\u{1B5A}-\u{1B6A}\u{1B74}-\u{1B7F}\u{1BF4}-\u{1BFF}\u{1C38}-\u{1C3F}\u{1C4A}-\u{1C4C}\u{1C7E}\u{1C7F}\u{1C89}-\u{1C8F}\u{1CBB}\u{1CBC}\u{1CC0}-\u{1CCF}\u{1CD3}\u{1CFB}-\u{1CFF}\u{1DFA}\u{1F16}\u{1F17}\u{1F1E}\u{1F1F}\u{1F46}\u{1F47}\u{1F4E}\u{1F4F}\u{1F58}\u{1F5A}\u{1F5C}\u{1F5E}\u{1F7E}\u{1F7F}\u{1FB5}\u{1FBD}\u{1FBF}-\u{1FC1}\u{1FC5}\u{1FCD}-\u{1FCF}\u{1FD4}\u{1FD5}\u{1FDC}-\u{1FDF}\u{1FED}-\u{1FF1}\u{1FF5}\u{1FFD}-\u{203E}\u{2041}-\u{2053}\u{2055}-\u{2070}\u{2072}-\u{207E}\u{2080}-\u{208F}\u{209D}-\u{20CF}\u{20F1}-\u{2101}\u{2103}-\u{2106}\u{2108}\u{2109}\u{2114}\u{2116}-\u{2118}\u{211E}-\u{2123}\u{2125}\u{2127}\u{2129}\u{212E}\u{213A}\u{213B}\u{2140}-\u{2144}\u{214A}-\u{214D}\u{214F}-\u{215F}\u{2189}-\u{24B5}\u{24EA}-\u{2BFF}\u{2C2F}\u{2C5F}\u{2CE5}-\u{2CEA}\u{2CF4}-\u{2CFF}\u{2D26}\u{2D28}-\u{2D2C}\u{2D2E}\u{2D2F}\u{2D68}-\u{2D6E}\u{2D70}-\u{2D7E}\u{2D97}-\u{2D9F}\u{2DA7}\u{2DAF}\u{2DB7}\u{2DBF}\u{2DC7}\u{2DCF}\u{2DD7}\u{2DDF}\u{2E00}-\u{2E2E}\u{2E30}-\u{3004}\u{3008}-\u{3020}\u{3030}\u{3036}\u{3037}\u{303D}-\u{3040}\u{3097}\u{3098}\u{309B}\u{309C}\u{30A0}\u{30FB}\u{3100}-\u{3104}\u{3130}\u{318F}-\u{319F}\u{31C0}-\u{31EF}\u{3200}-\u{33FF}\u{4DC0}-\u{4DFF}\u{9FFD}-\u{9FFF}\u{A48D}-\u{A4CF}\u{A4FE}\u{A4FF}\u{A60D}-\u{A60F}\u{A62C}-\u{A63F}\u{A673}\u{A67E}\u{A6F2}-\u{A716}\u{A720}\u{A721}\u{A789}\u{A78A}\u{A7C0}\u{A7C1}\u{A7CB}-\u{A7F4}\u{A828}-\u{A82B}\u{A82D}-\u{A83F}\u{A874}-\u{A87F}\u{A8C6}-\u{A8CF}\u{A8DA}-\u{A8DF}\u{A8F8}-\u{A8FA}\u{A8FC}\u{A92E}\u{A92F}\u{A954}-\u{A95F}\u{A97D}-\u{A97F}\u{A9C1}-\u{A9CE}\u{A9DA}-\u{A9DF}\u{A9FF}\u{AA37}-\u{AA3F}\u{AA4E}\u{AA4F}\u{AA5A}-\u{AA5F}\u{AA77}-\u{AA79}\u{AAC3}-\u{AADA}\u{AADE}\u{AADF}\u{AAF0}\u{AAF1}\u{AAF7}-\u{AB00}\u{AB07}\u{AB08}\u{AB0F}\u{AB10}\u{AB17}-\u{AB1F}\u{AB27}\u{AB2F}\u{AB5B}\u{AB6A}-\u{AB6F}\u{ABEB}\u{ABEE}\u{ABEF}\u{ABFA}-\u{ABFF}\u{D7A4}-\u{D7AF}\u{D7C7}-\u{D7CA}\u{D7FC}-\u{D7FF}\u{E000}-\u{F8FF}\u{FA6E}\u{FA6F}\u{FADA}-\u{FAFF}\u{FB07}-\u{FB12}\u{FB18}-\u{FB1C}\u{FB29}\u{FB37}\u{FB3D}\u{FB3F}\u{FB42}\u{FB45}\u{FBB2}-\u{FBD2}\u{FD3E}-\u{FD4F}\u{FD90}\u{FD91}\u{FDC8}-\u{FDEF}\u{FDFC}-\u{FDFF}\u{FE10}-\u{FE1F}\u{FE30}-\u{FE32}\u{FE35}-\u{FE4C}\u{FE50}-\u{FE6F}\u{FE75}\u{FEFD}-\u{FF0F}\u{FF1A}-\u{FF20}\u{FF3B}-\u{FF3E}\u{FF40}\u{FF5B}-\u{FF65}\u{FFBF}-\u{FFC1}\u{FFC8}\u{FFC9}\u{FFD0}\u{FFD1}\u{FFD8}\u{FFD9}\u{FFDD}-\u{FFFF}\u{1000C}\u{10027}\u{1003B}\u{1003E}\u{1004E}\u{1004F}\u{1005E}-\u{1007F}\u{100FB}-\u{1013F}\u{10175}-\u{101FC}\u{101FE}-\u{1027F}\u{1029D}-\u{1029F}\u{102D1}-\u{102DF}\u{102E1}-\u{102FF}\u{10320}-\u{1032C}\u{1034B}-\u{1034F}\u{1037B}-\u{1037F}\u{1039E}\u{1039F}\u{103C4}-\u{103C7}\u{103D0}\u{103D6}-\u{103FF}\u{1049E}\u{1049F}\u{104AA}-\u{104AF}\u{104D4}-\u{104D7}\u{104FC}-\u{104FF}\u{10528}-\u{1052F}\u{10564}-\u{105FF}\u{10737}-\u{1073F}\u{10756}-\u{1075F}\u{10768}-\u{107FF}\u{10806}\u{10807}\u{10809}\u{10836}\u{10839}-\u{1083B}\u{1083D}\u{1083E}\u{10856}-\u{1085F}\u{10877}-\u{1087F}\u{1089F}-\u{108DF}\u{108F3}\u{108F6}-\u{108FF}\u{10916}-\u{1091F}\u{1093A}-\u{1097F}\u{109B8}-\u{109BD}\u{109C0}-\u{109FF}\u{10A04}\u{10A07}-\u{10A0B}\u{10A14}\u{10A18}\u{10A36}\u{10A37}\u{10A3B}-\u{10A3E}\u{10A40}-\u{10A5F}\u{10A7D}-\u{10A7F}\u{10A9D}-\u{10ABF}\u{10AC8}\u{10AE7}-\u{10AFF}\u{10B36}-\u{10B3F}\u{10B56}-\u{10B5F}\u{10B73}-\u{10B7F}\u{10B92}-\u{10BFF}\u{10C49}-\u{10C7F}\u{10CB3}-\u{10CBF}\u{10CF3}-\u{10CFF}\u{10D28}-\u{10D2F}\u{10D3A}-\u{10E7F}\u{10EAA}\u{10EAD}-\u{10EAF}\u{10EB2}-\u{10EFF}\u{10F1D}-\u{10F26}\u{10F28}-\u{10F2F}\u{10F51}-\u{10FAF}\u{10FC5}-\u{10FDF}\u{10FF7}-\u{10FFF}\u{11047}-\u{11065}\u{11070}-\u{1107E}\u{110BB}-\u{110CF}\u{110E9}-\u{110EF}\u{110FA}-\u{110FF}\u{11135}\u{11140}-\u{11143}\u{11148}-\u{1114F}\u{11174}\u{11175}\u{11177}-\u{1117F}\u{111C5}-\u{111C8}\u{111CD}\u{111DB}\u{111DD}-\u{111FF}\u{11212}\u{11238}-\u{1123D}\u{1123F}-\u{1127F}\u{11287}\u{11289}\u{1128E}\u{1129E}\u{112A9}-\u{112AF}\u{112EB}-\u{112EF}\u{112FA}-\u{112FF}\u{11304}\u{1130D}\u{1130E}\u{11311}\u{11312}\u{11329}\u{11331}\u{11334}\u{1133A}\u{11345}\u{11346}\u{11349}\u{1134A}\u{1134E}\u{1134F}\u{11351}-\u{11356}\u{11358}-\u{1135C}\u{11364}\u{11365}\u{1136D}-\u{1136F}\u{11375}-\u{113FF}\u{1144B}-\u{1144F}\u{1145A}-\u{1145D}\u{1145F}-\u{1147F}\u{114C6}\u{114C8}-\u{114CF}\u{114DA}-\u{1157F}\u{115B6}\u{115B7}\u{115C1}-\u{115D7}\u{115DE}-\u{115FF}\u{11641}-\u{11643}\u{11645}-\u{1164F}\u{1165A}-\u{1167F}\u{116B9}-\u{116BF}\u{116CA}-\u{116FF}\u{1171B}-\u{1171C}\u{1172C}-\u{1172F}\u{1173A}-\u{117FF}\u{1183B}-\u{1189F}\u{118EA}-\u{118FE}\u{11900}-\u{1199F}\u{119A8}\u{119A9}\u{119D8}\u{119D9}\u{119E2}\u{119E5}-\u{119FF}\u{11A3F}-\u{11A46}\u{11A48}-\u{11A4F}\u{11A9A}-\u{11A9C}\u{11A9E}-\u{11ABF}\u{11AF9}-\u{11BFF}\u{11C09}\u{11C37}\u{11C41}-\u{11C4F}\u{11C5A}-\u{11C71}\u{11C90}\u{11C91}\u{11CA8}\u{11CB7}-\u{11CFF}\u{11D07}\u{11D0A}\u{11D37}-\u{11D39}\u{11D3B}\u{11D3E}\u{11D48}-\u{11D4F}\u{11D5A}-\u{11D5F}\u{11D66}\u{11D69}\u{11D8F}\u{11D92}\u{11D99}-\u{11D9F}\u{11DAA}-\u{11EDF}\u{11EF7}-\u{11FAF}\u{11FB1}-\u{11FFF}\u{1239A}-\u{123FF}\u{1246F}-\u{1247F}\u{12544}-\u{12FFF}\u{1342F}-\u{143FF}\u{14647}-\u{167FF}\u{16A39}-\u{16A3F}\u{16A5F}\u{16A6A}-\u{16ACF}\u{16AEE}\u{16AEF}\u{16AF5}-\u{16AFF}\u{16B37}-\u{16B3F}\u{16B44}-\u{16B4F}\u{16B5A}-\u{16B62}\u{16B78}-\u{16B7C}\u{16B90}-\u{16EFF}\u{16F4B}-\u{16F4E}\u{16F88}-\u{16F8E}\u{16FA0}-\u{16FDF}\u{16FE2}\u{16FE5}-\u{16FEF}\u{16FF2}-\u{16FFF}\u{187F8}-\u{187FF}\u{18CD6}-\u{18CFF}\u{18D09}-\u{1AFFF}\u{1B11F}-\u{1B14F}\u{1B153}-\u{1B163}\u{1B168}-\u{1B16F}\u{1B2FC}-\u{1BBFF}\u{1BC6B}-\u{1BC6F}\u{1BC7D}-\u{1BC7F}\u{1BC89}-\u{1BC8F}\u{1BC9A}-\u{1BC9C}\u{1BC9F}-\u{1CFFF}\u{1D0F6}-\u{1D0FF}\u{1D127}-\u{1D128}\u{1D173}-\u{1D17A}\u{1D1E9}-\u{1D1FF}\u{1D246}-\u{1D2DF}\u{1D2F4}-\u{1D2FF}\u{1D357}-\u{1D35F}\u{1D379}-\u{1D3FF}\u{1D455}\u{1D49D}\u{1D4A0}\u{1D4A1}\u{1D4A3}\u{1D4A4}\u{1D4A7}\u{1D4A8}\u{1D4AD}\u{1D4BA}\u{1D4BC}\u{1D4C4}\u{1D506}\u{1D50B}\u{1D50C}\u{1D515}\u{1D51D}\u{1D53A}\u{1D53F}\u{1D545}\u{1D547}-\u{1D549}\u{1D551}\u{1D6A6}\u{1D6A7}\u{1D6C1}\u{1D6DB}\u{1D6FB}\u{1D715}\u{1D735}\u{1D74F}\u{1D76F}\u{1D789}\u{1D7A9}\u{1D7C3}\u{1D7CC}\u{1D7CD}\u{1D800}-\u{1D9FF}\u{1DA37}-\u{1DA3A}\u{1DA6D}-\u{1DA74}\u{1DA76}-\u{1DA83}\u{1DA85}-\u{1DA9A}\u{1DAA0}\u{1DAB0}-\u{1DFFF}\u{1E007}\u{1E019}\u{1E01A}\u{1E022}\u{1E025}\u{1E02B}-\u{1E0FF}\u{1E12D}-\u{1E12F}\u{1E13E}\u{1E13F}\u{1E14A}-\u{1E14D}\u{1E14F}-\u{1E2BF}\u{1E2FA}-\u{1E2FF}\u{1E4FA}-\u{1E7FF}\u{1E8C5}-\u{1E8CF}\u{1E8D7}-\u{1E8FF}\u{1E94C}-\u{1E94F}\u{1E95A}-\u{1E9FF}\u{1EA00}-\u{1EDFF}\u{1EE04}\u{1EE20}\u{1EE23}\u{1EE25}\u{1EE26}\u{1EE28}\u{1EE33}\u{1EE38}\u{1EE3A}\u{1EE3C}-\u{1EE41}\u{1EE43}-\u{1EE46}\u{1EE48}\u{1EE4A}\u{1EE4C}\u{1EE50}\u{1EE53}\u{1EE55}\u{1EE56}\u{1EE58}\u{1EE5A}\u{1EE5C}\u{1EE5E}\u{1EE60}\u{1EE63}\u{1EE65}\u{1EE66}\u{1EE6B}\u{1EE73}\u{1EE78}\u{1EE7D}\u{1EE7F}\u{1EE8A}\u{1EE9C}-\u{1EEA0}\u{1EEA4}\u{1EEAA}\u{1EEBC}-\u{1EFFF}\u{1F000}-\u{1F02B}\u{1F030}-\u{1F093}\u{1F0A0}-\u{1F0AE}\u{1F0B1}-\u{1F0BF}\u{1F0C1}-\u{1F0CF}\u{1F0D1}-\u{1F0F5}\u{1F100}-\u{1F1AD}\u{1F1E6}-\u{1F202}\u{1F210}-\u{1F23B}\u{1F240}-\u{1F248}\u{1F250}\u{1F251}\u{1F260}-\u{1F265}\u{1F300}-\u{1F6D7}\u{1F6DD}-\u{1F6EC}\u{1F6F0}-\u{1F6FC}\u{1F700}-\u{1F773}\u{1F780}-\u{1F7D8}\u{1F7E0}-\u{1F7EB}\u{1F7F0}\u{1F800}-\u{1F80B}\u{1F810}-\u{1F847}\u{1F850}-\u{1F859}\u{1F860}-\u{1F887}\u{1F890}-\u{1F8AD}\u{1F8B0}\u{1F8B1}\u{1F900}-\u{1F978}\u{1F97A}-\u{1F9CB}\u{1F9CD}-\u{1FA53}\u{1FA60}-\u{1FA6D}\u{1FA70}-\u{1FA74}\u{1FA78}-\u{1FA7C}\u{1FA80}-\u{1FA86}\u{1FA90}-\u{1FAAC}\u{1FAB0}-\u{1FABA}\u{1FAC0}-\u{1FAC5}\u{1FAD0}-\u{1FAD9}\u{1FAE0}-\u{1FAE7}\u{1FAF0}-\u{1FAF6}\u{1FB00}-\u{1FB92}\u{1FB94}-\u{1FBCA}\u{1FBF0}-\u{1FBF9}\u{1FC00}-\u{1FFFF}\u{2A6DE}-\u{2A6FF}\u{2B735}-\u{2B73F}\u{2B81E}\u{2B81F}\u{2CEA2}-\u{2CEAF}\u{2EBE1}-\u{2F7FF}\u{2FA1E}-\u{2FFFF}\u{3134B}-\u{E00FF}\u{E01F0}-\u{10FFFF}]"
        ).expect("Valid GitHub regex pattern");

        // Remove all punctuation and symbols matched by the regex
        let result = github_regex.replace_all(&text, "");

        // 3. Replace spaces with hyphens (/ /g, '-')
        let with_hyphens = result.replace(' ', "-");

        // 4. Apply kramdown-style hyphen consolidation for kramdown-gfm compatibility
        Self::consolidate_hyphens_kramdown_style(&with_hyphens)
    }

    /// Fragment generation following kramdown GFM's EXACT algorithm
    /// Based on comprehensive testing with kramdown 2.5.1 using official Ruby gems
    /// This implementation matches the exact behavior verified through kramdown GFM processing
    #[inline]
    pub fn heading_to_fragment_jekyll_official(&self, heading: &str) -> String {
        if heading.is_empty() {
            return String::new();
        }

        // Strip markdown formatting first
        let text = if QUICK_MARKDOWN_CHECK.is_match(heading) {
            self.strip_markdown_formatting_fast(heading)
        } else {
            heading.to_string()
        };

        // Kramdown GFM's EXACT algorithm verified through official gem testing:

        // 1. Convert to lowercase
        let text = text.to_lowercase();

        let mut result = String::with_capacity(text.len());

        // First, do pattern-based replacements (verified kramdown behavior)
        let mut processed_text = text.clone();

        // Replace space-ampersand-space with double hyphen (verified behavior)
        processed_text = processed_text.replace(" & ", "--");
        processed_text = processed_text.replace(" © ", "--");

        // Now process character by character
        for c in processed_text.chars() {
            // Single character processing based on verified kramdown behavior
            match c {
                // Preserve all letters (including Unicode) - verified behavior
                c if c.is_alphabetic() => result.push(c),

                // Preserve all digits - verified behavior
                c if c.is_ascii_digit() => result.push(c),

                // Preserve underscores (critical difference from GitHub) - verified behavior
                '_' => result.push(c),

                // Preserve hyphens (no special arrow handling) - verified behavior
                '-' => result.push('-'),

                // Convert spaces to hyphens - verified behavior
                ' ' => result.push('-'),

                // Remaining symbols that weren't pattern-replaced
                '&' => result.push_str("--"), // For cases without surrounding spaces
                '©' => result.push_str("--"), // For cases without surrounding spaces

                // Emojis become single hyphens - verified behavior
                c if (c as u32 >= 0x1F300 && c as u32 <= 0x1F9FF)
                    || (c as u32 >= 0x1F600 && c as u32 <= 0x1F64F)
                    || (c as u32 >= 0x2600 && c as u32 <= 0x26FF) =>
                {
                    result.push('-')
                }

                // Most punctuation is removed - verified behavior
                ':' | '.' | '!' | '?' | '$' | '%' | '@' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '/'
                | '\\' | '|' | '"' | '\'' | '`' | '~' | '^' | '*' | '+' | '=' | '#' | '°' => {
                    // Remove these characters
                }

                // Unicode punctuation and symbols - handle em-dashes specifically
                '—' | '–' => {
                    // Em-dashes and en-dashes are removed (verified behavior)
                }

                // Handle ellipsis and other Unicode punctuation
                '…' => {
                    // Remove ellipsis
                }

                // Default: preserve other Unicode characters (verified behavior)
                _ => {
                    // Keep other Unicode characters as kramdown preserves them
                    result.push(c);
                }
            }
        }

        // Remove leading non-letter characters until first letter (verified behavior)
        let mut start_pos = 0;
        for (pos, c) in result.char_indices() {
            if c.is_alphabetic() {
                start_pos = pos;
                break;
            }
        }

        let result = if start_pos > 0 {
            result[start_pos..].to_string()
        } else {
            result
        };

        // Only remove trailing punctuation that isn't hyphens/underscores (verified behavior)
        // Kramdown preserves trailing hyphens but removes other punctuation
        let result = result
            .trim_end_matches(|c: char| {
                matches!(
                    c,
                    ':' | '.'
                        | '!'
                        | '?'
                        | '$'
                        | '%'
                        | '@'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '<'
                        | '>'
                        | '/'
                        | '\\'
                        | '|'
                        | '"'
                        | '\''
                        | '`'
                        | '~'
                        | '^'
                        | '*'
                        | '+'
                        | '='
                        | '#'
                        | '°'
                )
            })
            .to_string();

        // If empty or only punctuation, return "section" (verified kramdown behavior)
        if result.is_empty() || result.chars().all(|c| !c.is_alphanumeric() && c != '_') {
            "section".to_string()
        } else {
            result
        }
    }

    /// Strip markdown formatting from heading text (optimized for common patterns)
    fn strip_markdown_formatting_fast(&self, text: &str) -> String {
        let mut result = text.to_string();

        // Strip emphasis (only asterisks, underscores are preserved per GitHub spec)
        if result.contains('*') {
            result = EMPHASIS_PATTERN.replace_all(&result, "$1").to_string();
        }

        // Strip inline code
        if result.contains('`') {
            result = CODE_PATTERN.replace_all(&result, "$1").to_string();
        }

        // Strip links (GitHub keeps both link text and URL)
        if result.contains('[') {
            result = LINK_PATTERN.replace_all(&result, "$1$2$3").to_string();
        }

        result
    }

    /// Fast check if URL is external (doesn't need to be validated)
    #[inline]
    fn is_external_url_fast(url: &str) -> bool {
        // Quick prefix checks for common protocols
        url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("ftp://")
            || url.starts_with("mailto:")
            || url.starts_with("tel:")
            || url.starts_with("//")
    }

    /// Check if URL is a cross-file link (contains a file path before #)
    #[inline]
    fn is_cross_file_link(url: &str) -> bool {
        if let Some(fragment_pos) = url.find('#') {
            let path_part = &url[..fragment_pos];

            // If there's no path part, it's just a fragment (#heading)
            if path_part.is_empty() {
                return false;
            }

            // Check for Liquid syntax used by Jekyll and other static site generators
            // Liquid tags: {% ... %} for control flow and includes
            // Liquid variables: {{ ... }} for outputting values
            // These are template directives that reference external content and should be skipped
            // We check for proper bracket order to avoid false positives
            if let Some(tag_start) = path_part.find("{%")
                && path_part[tag_start + 2..].contains("%}")
            {
                return true;
            }
            if let Some(var_start) = path_part.find("{{")
                && path_part[var_start + 2..].contains("}}")
            {
                return true;
            }

            // Check if it's an absolute path (starts with /)
            // These are links to other pages on the same site
            if path_part.starts_with('/') {
                return true;
            }

            // Check if it looks like a file path:
            // - Contains a file extension (dot followed by letters)
            // - Contains path separators
            // - Contains relative path indicators
            path_part.contains('.')
                && (
                    // Has file extension pattern (handle query parameters by splitting on them first)
                    {
                    let clean_path = path_part.split('?').next().unwrap_or(path_part);
                    // Handle files starting with dot
                    if let Some(after_dot) = clean_path.strip_prefix('.') {
                        let dots_count = clean_path.matches('.').count();
                        if dots_count == 1 {
                            // Could be ".ext" (just extension) or ".hidden" (hidden file)
                            // If it's a known file extension, treat as cross-file link
                            !after_dot.is_empty() && after_dot.len() <= 10 &&
                            after_dot.chars().all(|c| c.is_ascii_alphanumeric()) &&
                            // Additional check: common file extensions are likely cross-file
                            (after_dot.len() <= 4 || matches!(after_dot, "html" | "json" | "yaml" | "toml"))
                        } else {
                            // Hidden file with extension like ".hidden.txt"
                            clean_path.split('.').next_back().is_some_and(|ext| {
                                !ext.is_empty() && ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric())
                            })
                        }
                    } else {
                        // Regular file path
                        clean_path.split('.').next_back().is_some_and(|ext| {
                            !ext.is_empty() && ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric())
                        })
                    }
                } ||
                // Or contains path separators
                path_part.contains('/') || path_part.contains('\\') ||
                // Or starts with relative path indicators
                path_part.starts_with("./") || path_part.starts_with("../")
                )
        } else {
            false
        }
    }

    /// Apply kramdown-style hyphen consolidation based on observed patterns
    /// This is a pragmatic implementation to match the most common kramdown GFM cases
    fn consolidate_hyphens_kramdown_style(text: &str) -> String {
        // For the specific problematic cases, apply targeted fixes
        // This handles the major kramdown GFM compatibility issues

        // Pattern: ---- (4 hyphens) becomes -- (2 hyphens)
        // Pattern: --- (3 hyphens) becomes - (1 hyphen)
        // Pattern: -- (2 hyphens) stays -- (2 hyphens)
        // Pattern: - (1 hyphen) stays - (1 hyphen)

        let consolidated = text
            .replace("----", "--")  // 4 → 2
            .replace("---", "-"); // 3 → 1

        // Remove leading and trailing hyphens (kramdown behavior)
        consolidated.trim_matches('-').to_string()
    }
}

impl Rule for MD051LinkFragments {
    fn name(&self) -> &'static str {
        "MD051"
    }

    fn description(&self) -> &'static str {
        "Link fragments should reference valid headings"
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if no link fragments present
        !ctx.content.contains("#")
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let content = ctx.content;

        // Skip empty content
        if content.is_empty() {
            return Ok(warnings);
        }

        // Extract all valid heading anchors
        let valid_headings = self.extract_headings_from_context(ctx);

        // Find all links with fragments
        let link_regex = get_cached_regex(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            // Skip front matter
            if ctx.lines[line_num].in_front_matter {
                continue;
            }

            // Skip code blocks
            if ctx.lines[line_num].in_code_block {
                continue;
            }

            for cap in link_regex.captures_iter(line) {
                if let Some(url_match) = cap.get(2) {
                    let url = url_match.as_str();
                    let full_match = cap.get(0).unwrap(); // Get the entire link match

                    // Calculate byte position for this match within the entire content
                    let line_byte_offset = if line_num == 0 {
                        0
                    } else {
                        content.lines().take(line_num).map(|l| l.len() + 1).sum::<usize>() // +1 for newline
                    };
                    let match_byte_pos = line_byte_offset + full_match.start();

                    // Skip links in code blocks or inline code spans
                    if ctx.is_in_code_block_or_span(match_byte_pos) {
                        continue;
                    }

                    // Check if this URL contains a fragment
                    if url.contains('#') && !Self::is_external_url_fast(url) {
                        // If it's a cross-file link, skip validation as the target file may not be in the current context
                        if Self::is_cross_file_link(url) {
                            continue;
                        }

                        // Extract fragment (everything after #)
                        if let Some(fragment_pos) = url.find('#') {
                            let fragment = &url[fragment_pos + 1..];

                            // Handle Liquid template syntax with filters
                            // If we're inside a Liquid variable ({{ ... }}), extract just the fragment part
                            // before any filters (marked by |)
                            if url.contains("{{") && fragment.contains('|') {
                                // For patterns like {{ "/tags#alternative-data-streams" | relative_url }}
                                // We want to skip this entirely as it's a Liquid template
                                continue;
                            }

                            // Also check if the fragment ends with template syntax that wasn't caught
                            if fragment.ends_with("}}") || fragment.ends_with("%}") {
                                // This is likely part of a Liquid template, skip it
                                continue;
                            }

                            // Skip empty fragments
                            if fragment.is_empty() {
                                continue;
                            }

                            // Check if fragment exists in document (case-insensitive)
                            let fragment_lower = fragment.to_lowercase();
                            let found = valid_headings.iter().any(|h| h.to_lowercase() == fragment_lower);
                            if !found {
                                let column = full_match.start() + 1; // Point to start of entire link

                                warnings.push(LintWarning {
                                    rule_name: Some(self.name()),
                                    message: format!("Link anchor '#{fragment}' does not exist in document headings"),
                                    line: line_num + 1,
                                    column,
                                    end_line: line_num + 1,
                                    end_column: full_match.end() + 1, // End of entire link
                                    severity: Severity::Warning,
                                    fix: None, // No auto-fix per industry standard
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // MD051 does not provide auto-fix
        // Link fragment corrections require human judgment to avoid incorrect fixes
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        // Config keys are normalized to kebab-case by the config system
        let anchor_style = if let Some(rule_config) = config.rules.get("MD051") {
            if let Some(style_str) = rule_config.values.get("anchor-style").and_then(|v| v.as_str()) {
                match style_str.to_lowercase().as_str() {
                    "kramdown" => AnchorStyle::Kramdown,
                    "kramdown-gfm" => AnchorStyle::KramdownGfm,
                    "jekyll" => AnchorStyle::KramdownGfm, // Backward compatibility alias
                    _ => AnchorStyle::GitHub,
                }
            } else {
                AnchorStyle::GitHub
            }
        } else {
            AnchorStyle::GitHub
        };

        Box::new(MD051LinkFragments::with_anchor_style(anchor_style))
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let value: toml::Value = toml::from_str(
            r#"
# Anchor generation style to match your target platform
# Options: "github" (default), "kramdown-gfm", "kramdown"
# Note: "jekyll" is accepted as an alias for "kramdown-gfm" (backward compatibility)
anchor-style = "github"
"#,
        )
        .ok()?;
        Some(("MD051".to_string(), value))
    }
}

#[cfg(test)]
mod tests {}
