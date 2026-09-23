//! MD094 reports input that is not valid UTF-8, and the decoder that reads it.
//!
//! A document with a few invalid bytes (a Latin-1 `é`, a truncated sequence) is
//! still Markdown: it is decoded lossily, each invalid sequence becomes one
//! U+FFFD, the text is linted as usual, and MD094 reports every replacement at
//! its position. Invalid input that looks binary (a NUL early on, or a UTF-16
//! byte order mark) is not linted at all, because lossy text from an image or
//! archive yields hundreds of meaningless findings from other rules.
//!
//! The lossy text is never written back: its U+FFFD characters stand for bytes
//! the file really holds, so every adapter reports on it and leaves the file
//! untouched.

use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

pub const RULE_NAME: &str = "MD094";

/// How many invalid sequences are reported individually before the rest are
/// summarized in one finding.
pub const MAX_REPORTED: usize = 20;

/// How far into the input a NUL byte marks it as binary. The same window Git
/// uses to decide whether a file is binary.
const BINARY_SNIFF_LEN: usize = 8000;

/// One invalid UTF-8 sequence in the original bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidSeq {
    /// The invalid bytes as they appear in the input.
    pub bytes: Vec<u8>,
    /// Offset of the first invalid byte in the input.
    pub byte_offset: usize,
    /// Which U+FFFD of the decoded text replaced this sequence, counting every
    /// U+FFFD, including ones the input spelled out validly. Positions are found
    /// by this ordinal rather than by offset so they survive line-ending
    /// normalization of the decoded text.
    pub ordinal: usize,
}

/// The result of decoding a document's bytes.
#[derive(Debug, PartialEq, Eq)]
pub enum Decoded<'a> {
    /// Valid UTF-8, borrowed unchanged.
    Utf8(&'a str),
    /// Invalid UTF-8 that looks like binary data or UTF-16; not linted.
    Binary { utf16: bool },
    /// Invalid UTF-8 decoded with one U+FFFD per invalid sequence.
    Lossy { text: String, invalid: Vec<InvalidSeq> },
}

/// Decode a document's bytes.
///
/// Valid UTF-8 is never classified as binary, whatever it contains: such a file
/// is linted exactly as it always was.
pub fn decode(bytes: &[u8]) -> Decoded<'_> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Decoded::Utf8(text);
    }
    let utf16 = bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]);
    if utf16 || bytes[..bytes.len().min(BINARY_SNIFF_LEN)].contains(&0) {
        return Decoded::Binary { utf16 };
    }

    let mut text = String::with_capacity(bytes.len() + 16);
    let mut invalid = Vec::new();
    let mut replacements = 0;
    let mut offset = 0;
    for chunk in bytes.utf8_chunks() {
        let valid = chunk.valid();
        text.push_str(valid);
        replacements += valid.matches(char::REPLACEMENT_CHARACTER).count();
        offset += valid.len();
        let bad = chunk.invalid();
        if !bad.is_empty() {
            text.push(char::REPLACEMENT_CHARACTER);
            invalid.push(InvalidSeq {
                bytes: bad.to_vec(),
                byte_offset: offset,
                ordinal: replacements,
            });
            replacements += 1;
            offset += bad.len();
        }
    }
    Decoded::Lossy { text, invalid }
}

/// Read a Markdown file for indexing, decoding invalid UTF-8 lossily.
///
/// `Ok(None)` means the file is binary and has no Markdown to contribute.
pub fn read_markdown_lossy(path: &std::path::Path) -> std::io::Result<Option<String>> {
    let bytes = std::fs::read(path)?;
    Ok(decode_owned(bytes))
}

/// Decode owned bytes to text, `None` for binary input.
pub fn decode_owned(bytes: Vec<u8>) -> Option<String> {
    match String::from_utf8(bytes) {
        Ok(text) => Some(text),
        Err(error) => match decode(error.as_bytes()) {
            Decoded::Lossy { text, .. } => Some(text),
            Decoded::Binary { .. } => None,
            Decoded::Utf8(_) => unreachable!("from_utf8 rejected these bytes"),
        },
    }
}

#[derive(Debug, Clone, Default)]
pub struct MD094InvalidEncoding;

impl Rule for MD094InvalidEncoding {
    fn name(&self) -> &'static str {
        RULE_NAME
    }
    fn description(&self) -> &'static str {
        "File is not valid UTF-8"
    }
    fn category(&self) -> RuleCategory {
        RuleCategory::Other
    }
    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let Some(invalid) = ctx.invalid_utf8() else {
            return Ok(Vec::new());
        };
        let mut replacements = ctx
            .content
            .match_indices(char::REPLACEMENT_CHARACTER)
            .map(|(offset, _)| offset)
            .enumerate();
        let mut warnings = Vec::with_capacity(invalid.len());
        for seq in invalid {
            let Some((_, offset)) = replacements.find(|(ordinal, _)| *ordinal == seq.ordinal) else {
                break;
            };
            let (line, column) = ctx.offset_to_line_col(offset);
            warnings.push(LintWarning {
                rule_name: Some(RULE_NAME.to_string()),
                message: format!("Invalid UTF-8 byte sequence {} (shown as U+FFFD)", hex(&seq.bytes)),
                line,
                column,
                end_line: line,
                end_column: column + 1,
                severity: Severity::Warning,
                fix: None,
            });
        }
        Ok(warnings)
    }
    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }
    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        Ok(ctx.content.to_string())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule> {
        Box::new(Self)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("0x{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Settle the report for a lossily decoded document.
///
/// Every fix is dropped: the ranges index the decoded text, whose U+FFFD
/// characters are not the bytes on disk, so no fix can be applied or offered.
/// Past [`MAX_REPORTED`] reportable MD094 findings, the rest collapse into one
/// summary at the position of the first one not shown. Runs after suppression,
/// so a suppressed sequence is neither shown nor counted.
pub fn settle_lossy_warnings(warnings: &mut Vec<LintWarning>) {
    for warning in warnings.iter_mut() {
        warning.fix = None;
    }
    let total = warnings
        .iter()
        .filter(|warning| warning.rule_name.as_deref() == Some(RULE_NAME))
        .count();
    if total <= MAX_REPORTED {
        return;
    }
    let mut seen = 0;
    warnings.retain_mut(|warning| {
        if warning.rule_name.as_deref() != Some(RULE_NAME) {
            return true;
        }
        seen += 1;
        if seen == MAX_REPORTED + 1 {
            let hidden = total - MAX_REPORTED;
            let noun = if hidden == 1 { "sequence" } else { "sequences" };
            warning.message = format!("{hidden} more invalid UTF-8 {noun} not shown");
        }
        seen <= MAX_REPORTED + 1
    });
}

/// The finding for a binary input, if the invocation reports MD094 for it.
///
/// `rules` is the invocation's effective rule set, so configuration and CLI
/// rule selection (`--disable`, `--enable`) both apply; per-file-ignores and
/// severity are read from `config`. Binary input is never parsed, so inline
/// comments cannot suppress this finding.
pub fn detect_binary_for_rules(
    utf16: bool,
    rules: &[Box<dyn Rule>],
    config: &crate::config::Config,
    path: Option<&std::path::Path>,
) -> Option<LintWarning> {
    if !rules.iter().any(|rule| rule.name() == RULE_NAME)
        || path.is_some_and(|path| config.get_ignored_rules_for_file(path).contains(RULE_NAME))
    {
        return None;
    }
    let message = if utf16 {
        "File appears to be UTF-16 encoded; not linted, convert it to UTF-8"
    } else {
        "File appears to be binary; not linted"
    };
    Some(LintWarning {
        rule_name: Some(RULE_NAME.to_string()),
        message: message.to_string(),
        line: 1,
        column: 1,
        end_line: 1,
        end_column: 1,
        severity: config.get_rule_severity(RULE_NAME).unwrap_or(Severity::Warning),
        fix: None,
    })
}

/// Whether a lossily decoded document needs MD094 added back to its rule set.
///
/// MD094 is a guard rather than one of the outer document's rules: it says why a
/// file rumdl cannot read is left unlinted, and that answer is owed whatever a
/// mode is doing with the document's own rules. `--only-code-block-tools` drops
/// the document set entirely, which would otherwise leave a lossily decoded file
/// reporting clean while a binary file in the same invocation reports MD094 -
/// the same question answered two ways, and the silent answer is
/// indistinguishable from a file that is genuinely fine.
///
/// `selected` is the invocation's resolved selection, the gate
/// [`detect_binary_for_rules`] applies to the other flavor. Per-file ignores,
/// inline comments and severity stay with the lint pipeline, which is why the
/// rule is restored to the set rather than reported from here.
pub fn guard_missing_from_document_rules(selected: &[Box<dyn Rule>], document: &[Box<dyn Rule>]) -> bool {
    selected.iter().any(|rule| rule.name() == RULE_NAME) && !document.iter().any(|rule| rule.name() == RULE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, MarkdownFlavor};
    use crate::lint_context::LintContext;

    fn lossy(bytes: &[u8]) -> (String, Vec<InvalidSeq>) {
        match decode(bytes) {
            Decoded::Lossy { text, invalid } => (text, invalid),
            other => panic!("expected lossy decoding, got {other:?}"),
        }
    }

    fn positions(bytes: &[u8]) -> Vec<(usize, usize, String)> {
        let (text, invalid) = lossy(bytes);
        let text = crate::utils::normalize_line_ending(&text, crate::utils::LineEnding::Lf);
        let ctx = LintContext::new(&text, MarkdownFlavor::Standard, None).with_invalid_utf8(&invalid);
        MD094InvalidEncoding
            .check(&ctx)
            .unwrap()
            .into_iter()
            .map(|warning| (warning.line, warning.column, warning.message))
            .collect()
    }

    #[test]
    fn valid_utf8_is_borrowed_even_when_it_contains_nul() {
        assert_eq!(decode(b"# Title\n"), Decoded::Utf8("# Title\n"));
        assert_eq!(decode(&[0; 64]), Decoded::Utf8("\0".repeat(64).leak()));
    }

    #[test]
    fn invalid_sequences_keep_their_bytes_and_offsets() {
        let (text, invalid) = lossy(b"caf\xE9 \xE2\x82 ok\xFF");
        assert_eq!(text, "caf\u{FFFD} \u{FFFD} ok\u{FFFD}");
        let found: Vec<_> = invalid.iter().map(|seq| (seq.bytes.clone(), seq.byte_offset)).collect();
        assert_eq!(found, vec![(vec![0xE9], 3), (vec![0xE2, 0x82], 5), (vec![0xFF], 10)]);
    }

    #[test]
    fn truncated_sequence_at_end_of_input_is_one_finding() {
        let (text, invalid) = lossy(b"end \xF0\x9F\x98");
        assert_eq!(text, "end \u{FFFD}");
        assert_eq!(invalid.len(), 1);
        assert_eq!(invalid[0].bytes, vec![0xF0, 0x9F, 0x98]);
    }

    #[test]
    fn overlong_and_surrogate_encodings_are_invalid() {
        // Each maximal invalid prefix is replaced separately, as Rust's own
        // lossy decoding does.
        let (_, overlong) = lossy(b"a\xC0\xAFb");
        assert_eq!(overlong.len(), 2);
        let (_, surrogate) = lossy(b"a\xED\xA0\x80b");
        assert_eq!(surrogate.len(), 3);
        assert_eq!(lossy(b"a\xED\xA0\x80b").0, String::from_utf8_lossy(b"a\xED\xA0\x80b"));
    }

    #[test]
    fn literal_replacement_characters_are_not_findings() {
        let mut input = "\u{FFFD} valid\nbad \u{FFFD} ".as_bytes().to_vec();
        input.push(0xE9);
        let found = positions(&input);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].0, found[0].1), (2, 7));
    }

    #[test]
    fn positions_are_line_and_character_column() {
        let found = positions(&["é line one\nsecond ".as_bytes(), b"\xE9 here"].concat());
        assert_eq!(
            found,
            vec![(2, 8, "Invalid UTF-8 byte sequence 0xE9 (shown as U+FFFD)".into())]
        );
        let found = positions(b"x\xE2\x82y");
        assert_eq!(found[0].2, "Invalid UTF-8 byte sequence 0xE2 0x82 (shown as U+FFFD)");
    }

    #[test]
    fn positions_survive_crlf_and_long_input() {
        let mut input = Vec::new();
        for line in 1..=400 {
            input.extend_from_slice(format!("Line {line} with some padding text\r\n").as_bytes());
        }
        assert!(input.len() > 8192);
        input.extend_from_slice(b"tail \xE9\r\n");
        assert_eq!(
            positions(&input),
            vec![(401, 6, "Invalid UTF-8 byte sequence 0xE9 (shown as U+FFFD)".into())]
        );
    }

    #[test]
    fn nul_within_the_sniff_window_marks_binary() {
        let mut at_edge = vec![b'a'; BINARY_SNIFF_LEN + 10];
        at_edge[0] = 0xE9;
        at_edge[BINARY_SNIFF_LEN - 1] = 0;
        assert_eq!(decode(&at_edge), Decoded::Binary { utf16: false });
        at_edge[BINARY_SNIFF_LEN - 1] = b'a';
        at_edge[BINARY_SNIFF_LEN] = 0;
        assert!(matches!(decode(&at_edge), Decoded::Lossy { .. }));
    }

    #[test]
    fn utf16_byte_order_marks_are_reported_as_utf16() {
        assert_eq!(decode(b"\xFF\xFE#\0 \0"), Decoded::Binary { utf16: true });
        assert_eq!(decode(b"\xFE\xFF\0#\0 "), Decoded::Binary { utf16: true });
        assert_eq!(decode(b"#\0 \0T\0\xE9\0"), Decoded::Binary { utf16: false });
    }

    #[test]
    fn decode_owned_matches_decode() {
        assert_eq!(decode_owned(b"ok".to_vec()).as_deref(), Some("ok"));
        assert_eq!(decode_owned(b"caf\xE9".to_vec()).as_deref(), Some("caf\u{FFFD}"));
        assert_eq!(decode_owned(b"\xE9\0".to_vec()), None);
    }

    #[test]
    fn no_findings_without_decoder_data() {
        let ctx = LintContext::new("bad \u{FFFD}", MarkdownFlavor::Standard, None);
        assert!(MD094InvalidEncoding.check(&ctx).unwrap().is_empty());
    }

    fn finding(line: usize, rule: &str) -> LintWarning {
        LintWarning {
            rule_name: Some(rule.to_string()),
            message: format!("{rule} at {line}"),
            line,
            column: 1,
            end_line: line,
            end_column: 2,
            severity: Severity::Warning,
            fix: Some(crate::rule::Fix::new(0..1, String::new())),
        }
    }

    #[test]
    fn settling_caps_md094_and_drops_every_fix() {
        let mut warnings: Vec<_> = (1..=25).map(|line| finding(line, RULE_NAME)).collect();
        warnings.insert(3, finding(3, "MD009"));
        settle_lossy_warnings(&mut warnings);
        assert!(warnings.iter().all(|warning| warning.fix.is_none()));
        let md094: Vec<_> = warnings
            .iter()
            .filter(|warning| warning.rule_name.as_deref() == Some(RULE_NAME))
            .collect();
        assert_eq!(md094.len(), MAX_REPORTED + 1);
        assert_eq!(md094[MAX_REPORTED].line, 21);
        assert_eq!(md094[MAX_REPORTED].message, "5 more invalid UTF-8 sequences not shown");
        assert_eq!(warnings.len(), MAX_REPORTED + 2);
    }

    #[test]
    fn settling_leaves_up_to_the_cap_alone() {
        let mut warnings: Vec<_> = (1..=MAX_REPORTED).map(|line| finding(line, RULE_NAME)).collect();
        settle_lossy_warnings(&mut warnings);
        assert_eq!(warnings.len(), MAX_REPORTED);
        assert!(warnings.iter().all(|warning| !warning.message.contains("more")));
    }

    #[test]
    fn binary_finding_follows_rule_selection_and_severity() {
        let config = Config::default();
        let md094: Vec<Box<dyn Rule>> = vec![Box::new(MD094InvalidEncoding)];
        let found = detect_binary_for_rules(false, &md094, &config, None).unwrap();
        assert_eq!(found.message, "File appears to be binary; not linted");
        assert_eq!(found.severity, Severity::Warning);
        assert!(
            detect_binary_for_rules(true, &md094, &config, None)
                .unwrap()
                .message
                .contains("UTF-16")
        );

        // A rule set without MD094, as `--disable MD094` or `--enable MD009`
        // produces, reports nothing.
        assert!(detect_binary_for_rules(false, &[], &config, None).is_none());
    }
}
