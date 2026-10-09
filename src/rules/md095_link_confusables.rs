use std::collections::HashSet;

use crate::lint_context::LintContext;
use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::html_elements::{extract_attribute_with_range, is_html_whitespace, start_tag_end};
use crate::utils::regex_cache::URL_SIMPLE_REGEX;

/// ASCII-skeleton mappings from Unicode UTS #39, version 18.0.0,
/// selected by the Latin, Greek, Cyrillic and Armenian Script properties,
/// plus mathematical/fullwidth scalars and four URL punctuation look-alikes.
/// Includes raw confusables-table rows in those scopes whose target is
/// printable ASCII; normalization-only mappings are outside this focused list.
/// Data: https://www.unicode.org/Public/security/latest/confusables.txt
/// Scripts: https://www.unicode.org/Public/UCD/latest/ucd/Scripts.txt
/// Unicode data © 2026 Unicode, Inc.; https://www.unicode.org/terms_of_use.html
/// Confusables SHA-256: 6ed3ee967c9dfdf6677d563c9985182fbc50a2efb7d6059cd57b2e2ce18f5b92
/// Scripts SHA-256: 0071fd81b6aeae25f6e8bce8efec3066a6476a91b49bdb2f52dc76e817862a6a
const ASCII_CONFUSABLE_RANGES: &[(char, char)] = &[
    ('\u{00C6}', '\u{00C6}'),
    ('\u{00E6}', '\u{00E6}'),
    ('\u{00FE}', '\u{00FE}'),
    ('\u{0131}', '\u{0133}'),
    ('\u{0149}', '\u{0149}'),
    ('\u{0152}', '\u{0153}'),
    ('\u{017F}', '\u{017F}'),
    ('\u{0181}', '\u{0181}'),
    ('\u{0184}', '\u{0184}'),
    ('\u{0187}', '\u{0187}'),
    ('\u{018A}', '\u{018A}'),
    ('\u{018D}', '\u{018D}'),
    ('\u{0192}', '\u{0193}'),
    ('\u{0196}', '\u{0196}'),
    ('\u{0198}', '\u{0198}'),
    ('\u{01A4}', '\u{01A4}'),
    ('\u{01A6}', '\u{01A7}'),
    ('\u{01AC}', '\u{01AC}'),
    ('\u{01B3}', '\u{01B3}'),
    ('\u{01B7}', '\u{01B7}'),
    ('\u{01BC}', '\u{01BD}'),
    ('\u{01BF}', '\u{01C1}'),
    ('\u{01C3}', '\u{01C3}'),
    ('\u{01C7}', '\u{01CC}'),
    ('\u{01F1}', '\u{01F3}'),
    ('\u{021C}', '\u{021C}'),
    ('\u{0222}', '\u{0223}'),
    ('\u{0237}', '\u{0237}'),
    ('\u{0241}', '\u{0241}'),
    ('\u{024C}', '\u{024C}'),
    ('\u{0251}', '\u{0251}'),
    ('\u{0261}', '\u{0261}'),
    ('\u{0263}', '\u{0263}'),
    ('\u{0269}', '\u{026A}'),
    ('\u{026F}', '\u{026F}'),
    ('\u{0284}', '\u{0284}'),
    ('\u{028B}', '\u{028B}'),
    ('\u{028F}', '\u{028F}'),
    ('\u{0294}', '\u{0294}'),
    ('\u{02A3}', '\u{02A3}'),
    ('\u{02A6}', '\u{02A6}'),
    ('\u{02AA}', '\u{02AB}'),
    ('\u{037F}', '\u{037F}'),
    ('\u{0384}', '\u{0384}'),
    ('\u{0391}', '\u{0392}'),
    ('\u{0395}', '\u{0397}'),
    ('\u{0399}', '\u{039A}'),
    ('\u{039C}', '\u{039D}'),
    ('\u{039F}', '\u{039F}'),
    ('\u{03A1}', '\u{03A1}'),
    ('\u{03A4}', '\u{03A5}'),
    ('\u{03A7}', '\u{03A7}'),
    ('\u{03B1}', '\u{03B1}'),
    ('\u{03B3}', '\u{03B3}'),
    ('\u{03B9}', '\u{03B9}'),
    ('\u{03BD}', '\u{03BD}'),
    ('\u{03BF}', '\u{03BF}'),
    ('\u{03C1}', '\u{03C1}'),
    ('\u{03C3}', '\u{03C3}'),
    ('\u{03C5}', '\u{03C5}'),
    ('\u{03D2}', '\u{03D2}'),
    ('\u{03DC}', '\u{03DC}'),
    ('\u{03F1}', '\u{03F3}'),
    ('\u{03F8}', '\u{03FA}'),
    ('\u{0405}', '\u{0406}'),
    ('\u{0408}', '\u{0408}'),
    ('\u{0410}', '\u{0410}'),
    ('\u{0412}', '\u{0412}'),
    ('\u{0415}', '\u{0415}'),
    ('\u{0417}', '\u{0417}'),
    ('\u{041A}', '\u{041A}'),
    ('\u{041C}', '\u{041E}'),
    ('\u{0420}', '\u{0423}'),
    ('\u{0425}', '\u{0425}'),
    ('\u{042B}', '\u{042C}'),
    ('\u{042E}', '\u{042E}'),
    ('\u{0430}', '\u{0431}'),
    ('\u{0433}', '\u{0433}'),
    ('\u{0435}', '\u{0435}'),
    ('\u{043E}', '\u{043E}'),
    ('\u{0440}', '\u{0441}'),
    ('\u{0443}', '\u{0443}'),
    ('\u{0445}', '\u{0445}'),
    ('\u{0448}', '\u{0448}'),
    ('\u{0455}', '\u{0456}'),
    ('\u{0458}', '\u{0458}'),
    ('\u{0461}', '\u{0461}'),
    ('\u{0474}', '\u{0475}'),
    ('\u{0478}', '\u{0479}'),
    ('\u{0491}', '\u{0491}'),
    ('\u{04AE}', '\u{04AF}'),
    ('\u{04BA}', '\u{04BB}'),
    ('\u{04BD}', '\u{04BD}'),
    ('\u{04C0}', '\u{04C0}'),
    ('\u{04CF}', '\u{04CF}'),
    ('\u{04D4}', '\u{04D5}'),
    ('\u{04E0}', '\u{04E0}'),
    ('\u{0501}', '\u{0501}'),
    ('\u{050C}', '\u{050C}'),
    ('\u{051A}', '\u{051D}'),
    ('\u{0545}', '\u{0545}'),
    ('\u{054D}', '\u{054D}'),
    ('\u{054F}', '\u{054F}'),
    ('\u{0555}', '\u{0555}'),
    ('\u{055A}', '\u{055B}'),
    ('\u{055D}', '\u{055D}'),
    ('\u{0560}', '\u{0561}'),
    ('\u{0563}', '\u{0563}'),
    ('\u{0566}', '\u{0566}'),
    ('\u{0570}', '\u{0570}'),
    ('\u{0575}', '\u{0575}'),
    ('\u{0578}', '\u{0578}'),
    ('\u{057C}', '\u{057D}'),
    ('\u{0581}', '\u{0582}'),
    ('\u{0584}', '\u{0586}'),
    ('\u{0589}', '\u{0589}'),
    ('\u{1C82}', '\u{1C84}'),
    ('\u{1D04}', '\u{1D04}'),
    ('\u{1D0F}', '\u{1D0F}'),
    ('\u{1D11}', '\u{1D11}'),
    ('\u{1D1C}', '\u{1D1C}'),
    ('\u{1D20}', '\u{1D22}'),
    ('\u{1D26}', '\u{1D26}'),
    ('\u{1D6B}', '\u{1D6B}'),
    ('\u{1D83}', '\u{1D83}'),
    ('\u{1D8C}', '\u{1D8C}'),
    ('\u{1DBA}', '\u{1DBA}'),
    ('\u{1E9A}', '\u{1E9A}'),
    ('\u{1E9D}', '\u{1E9D}'),
    ('\u{1EFA}', '\u{1EFA}'),
    ('\u{1EFF}', '\u{1EFF}'),
    ('\u{1FBD}', '\u{1FBD}'),
    ('\u{1FBF}', '\u{1FC0}'),
    ('\u{1FFE}', '\u{1FFE}'),
    ('\u{2010}', '\u{2011}'),
    ('\u{2044}', '\u{2044}'),
    ('\u{2160}', '\u{217F}'),
    ('\u{2215}', '\u{2215}'),
    ('\u{2C6B}', '\u{2C6C}'),
    ('\u{A644}', '\u{A644}'),
    ('\u{A647}', '\u{A647}'),
    ('\u{A698}', '\u{A699}'),
    ('\u{A728}', '\u{A728}'),
    ('\u{A731}', '\u{A73D}'),
    ('\u{A74E}', '\u{A74F}'),
    ('\u{A75A}', '\u{A75A}'),
    ('\u{A76A}', '\u{A76A}'),
    ('\u{A76E}', '\u{A76F}'),
    ('\u{A777}', '\u{A778}'),
    ('\u{A781}', '\u{A781}'),
    ('\u{A78B}', '\u{A78C}'),
    ('\u{A798}', '\u{A799}'),
    ('\u{A79F}', '\u{A79F}'),
    ('\u{A7AB}', '\u{A7AB}'),
    ('\u{A7AE}', '\u{A7AE}'),
    ('\u{A7B2}', '\u{A7B4}'),
    ('\u{A7FA}', '\u{A7FA}'),
    ('\u{A7FE}', '\u{A7FE}'),
    ('\u{AB32}', '\u{AB32}'),
    ('\u{AB35}', '\u{AB35}'),
    ('\u{AB3D}', '\u{AB3D}'),
    ('\u{AB43}', '\u{AB43}'),
    ('\u{AB47}', '\u{AB48}'),
    ('\u{AB4E}', '\u{AB4E}'),
    ('\u{AB52}', '\u{AB52}'),
    ('\u{AB5A}', '\u{AB5A}'),
    ('\u{AB63}', '\u{AB64}'),
    ('\u{FB00}', '\u{FB06}'),
    ('\u{FF01}', '\u{FF04}'),
    ('\u{FF06}', '\u{FF0C}'),
    ('\u{FF0E}', '\u{FF5E}'),
    ('\u{10140}', '\u{10141}'),
    ('\u{1015B}', '\u{1015B}'),
    ('\u{1017E}', '\u{1017E}'),
    ('\u{1018B}', '\u{1018B}'),
    ('\u{1D206}', '\u{1D207}'),
    ('\u{1D20C}', '\u{1D20D}'),
    ('\u{1D20F}', '\u{1D20F}'),
    ('\u{1D212}', '\u{1D213}'),
    ('\u{1D216}', '\u{1D216}'),
    ('\u{1D22A}', '\u{1D22A}'),
    ('\u{1D236}', '\u{1D237}'),
    ('\u{1D23A}', '\u{1D23B}'),
    ('\u{1D400}', '\u{1D454}'),
    ('\u{1D456}', '\u{1D49C}'),
    ('\u{1D49E}', '\u{1D49F}'),
    ('\u{1D4A2}', '\u{1D4A2}'),
    ('\u{1D4A5}', '\u{1D4A6}'),
    ('\u{1D4A9}', '\u{1D4AC}'),
    ('\u{1D4AE}', '\u{1D4B9}'),
    ('\u{1D4BB}', '\u{1D4BB}'),
    ('\u{1D4BD}', '\u{1D4C3}'),
    ('\u{1D4C5}', '\u{1D505}'),
    ('\u{1D507}', '\u{1D50A}'),
    ('\u{1D50D}', '\u{1D514}'),
    ('\u{1D516}', '\u{1D51C}'),
    ('\u{1D51E}', '\u{1D539}'),
    ('\u{1D53B}', '\u{1D53E}'),
    ('\u{1D540}', '\u{1D544}'),
    ('\u{1D546}', '\u{1D546}'),
    ('\u{1D54A}', '\u{1D550}'),
    ('\u{1D552}', '\u{1D6A5}'),
    ('\u{1D6A8}', '\u{1D6A9}'),
    ('\u{1D6AC}', '\u{1D6AE}'),
    ('\u{1D6B0}', '\u{1D6B1}'),
    ('\u{1D6B3}', '\u{1D6B4}'),
    ('\u{1D6B6}', '\u{1D6B6}'),
    ('\u{1D6B8}', '\u{1D6B8}'),
    ('\u{1D6BB}', '\u{1D6BC}'),
    ('\u{1D6BE}', '\u{1D6BE}'),
    ('\u{1D6C2}', '\u{1D6C2}'),
    ('\u{1D6C4}', '\u{1D6C4}'),
    ('\u{1D6CA}', '\u{1D6CA}'),
    ('\u{1D6CE}', '\u{1D6CE}'),
    ('\u{1D6D0}', '\u{1D6D0}'),
    ('\u{1D6D2}', '\u{1D6D2}'),
    ('\u{1D6D4}', '\u{1D6D4}'),
    ('\u{1D6D6}', '\u{1D6D6}'),
    ('\u{1D6E0}', '\u{1D6E0}'),
    ('\u{1D6E2}', '\u{1D6E3}'),
    ('\u{1D6E6}', '\u{1D6E8}'),
    ('\u{1D6EA}', '\u{1D6EB}'),
    ('\u{1D6ED}', '\u{1D6EE}'),
    ('\u{1D6F0}', '\u{1D6F0}'),
    ('\u{1D6F2}', '\u{1D6F2}'),
    ('\u{1D6F5}', '\u{1D6F6}'),
    ('\u{1D6F8}', '\u{1D6F8}'),
    ('\u{1D6FC}', '\u{1D6FC}'),
    ('\u{1D6FE}', '\u{1D6FE}'),
    ('\u{1D704}', '\u{1D704}'),
    ('\u{1D708}', '\u{1D708}'),
    ('\u{1D70A}', '\u{1D70A}'),
    ('\u{1D70C}', '\u{1D70C}'),
    ('\u{1D70E}', '\u{1D70E}'),
    ('\u{1D710}', '\u{1D710}'),
    ('\u{1D71A}', '\u{1D71A}'),
    ('\u{1D71C}', '\u{1D71D}'),
    ('\u{1D720}', '\u{1D722}'),
    ('\u{1D724}', '\u{1D725}'),
    ('\u{1D727}', '\u{1D728}'),
    ('\u{1D72A}', '\u{1D72A}'),
    ('\u{1D72C}', '\u{1D72C}'),
    ('\u{1D72F}', '\u{1D730}'),
    ('\u{1D732}', '\u{1D732}'),
    ('\u{1D736}', '\u{1D736}'),
    ('\u{1D738}', '\u{1D738}'),
    ('\u{1D73E}', '\u{1D73E}'),
    ('\u{1D742}', '\u{1D742}'),
    ('\u{1D744}', '\u{1D744}'),
    ('\u{1D746}', '\u{1D746}'),
    ('\u{1D748}', '\u{1D748}'),
    ('\u{1D74A}', '\u{1D74A}'),
    ('\u{1D754}', '\u{1D754}'),
    ('\u{1D756}', '\u{1D757}'),
    ('\u{1D75A}', '\u{1D75C}'),
    ('\u{1D75E}', '\u{1D75F}'),
    ('\u{1D761}', '\u{1D762}'),
    ('\u{1D764}', '\u{1D764}'),
    ('\u{1D766}', '\u{1D766}'),
    ('\u{1D769}', '\u{1D76A}'),
    ('\u{1D76C}', '\u{1D76C}'),
    ('\u{1D770}', '\u{1D770}'),
    ('\u{1D772}', '\u{1D772}'),
    ('\u{1D778}', '\u{1D778}'),
    ('\u{1D77C}', '\u{1D77C}'),
    ('\u{1D77E}', '\u{1D77E}'),
    ('\u{1D780}', '\u{1D780}'),
    ('\u{1D782}', '\u{1D782}'),
    ('\u{1D784}', '\u{1D784}'),
    ('\u{1D78E}', '\u{1D78E}'),
    ('\u{1D790}', '\u{1D791}'),
    ('\u{1D794}', '\u{1D796}'),
    ('\u{1D798}', '\u{1D799}'),
    ('\u{1D79B}', '\u{1D79C}'),
    ('\u{1D79E}', '\u{1D79E}'),
    ('\u{1D7A0}', '\u{1D7A0}'),
    ('\u{1D7A3}', '\u{1D7A4}'),
    ('\u{1D7A6}', '\u{1D7A6}'),
    ('\u{1D7AA}', '\u{1D7AA}'),
    ('\u{1D7AC}', '\u{1D7AC}'),
    ('\u{1D7B2}', '\u{1D7B2}'),
    ('\u{1D7B6}', '\u{1D7B6}'),
    ('\u{1D7B8}', '\u{1D7B8}'),
    ('\u{1D7BA}', '\u{1D7BA}'),
    ('\u{1D7BC}', '\u{1D7BC}'),
    ('\u{1D7BE}', '\u{1D7BE}'),
    ('\u{1D7C8}', '\u{1D7C8}'),
    ('\u{1D7CA}', '\u{1D7CA}'),
    ('\u{1D7CE}', '\u{1D7FF}'),
    ('\u{1DF5A}', '\u{1DF5A}'),
    ('\u{1DF5D}', '\u{1DF5F}'),
    ('\u{1DF64}', '\u{1DF65}'),
    ('\u{1DF6A}', '\u{1DF6A}'),
    ('\u{1DF7D}', '\u{1DF7D}'),
    ('\u{1DF81}', '\u{1DF81}'),
];

fn ascii_confusable(c: char) -> bool {
    if c.is_ascii() {
        return false;
    }
    // U+3002 is also a URL-host separator look-alike, even though its UTS #39
    // skeleton is not ASCII. IDNA maps it to the ordinary domain separator.
    if c == '\u{3002}' {
        return true;
    }
    let after = ASCII_CONFUSABLE_RANGES.partition_point(|(start, _)| *start <= c);
    after > 0 && c <= ASCII_CONFUSABLE_RANGES[after - 1].1
}

#[derive(Clone, Copy, PartialEq)]
enum DestinationSyntax {
    Literal,
    Markdown,
    Html,
}

// Legacy semicolon-less HTML references, longest name first.
// Source: https://html.spec.whatwg.org/entities.json
const HTML_LEGACY_ENTITIES: &[(&str, &str)] = &[
    ("Aacute", "\u{C1}"),
    ("Agrave", "\u{C0}"),
    ("Atilde", "\u{C3}"),
    ("Ccedil", "\u{C7}"),
    ("Eacute", "\u{C9}"),
    ("Egrave", "\u{C8}"),
    ("Iacute", "\u{CD}"),
    ("Igrave", "\u{CC}"),
    ("Ntilde", "\u{D1}"),
    ("Oacute", "\u{D3}"),
    ("Ograve", "\u{D2}"),
    ("Oslash", "\u{D8}"),
    ("Otilde", "\u{D5}"),
    ("Uacute", "\u{DA}"),
    ("Ugrave", "\u{D9}"),
    ("Yacute", "\u{DD}"),
    ("aacute", "\u{E1}"),
    ("agrave", "\u{E0}"),
    ("atilde", "\u{E3}"),
    ("brvbar", "\u{A6}"),
    ("ccedil", "\u{E7}"),
    ("curren", "\u{A4}"),
    ("divide", "\u{F7}"),
    ("eacute", "\u{E9}"),
    ("egrave", "\u{E8}"),
    ("frac12", "\u{BD}"),
    ("frac14", "\u{BC}"),
    ("frac34", "\u{BE}"),
    ("iacute", "\u{ED}"),
    ("igrave", "\u{EC}"),
    ("iquest", "\u{BF}"),
    ("middot", "\u{B7}"),
    ("ntilde", "\u{F1}"),
    ("oacute", "\u{F3}"),
    ("ograve", "\u{F2}"),
    ("oslash", "\u{F8}"),
    ("otilde", "\u{F5}"),
    ("plusmn", "\u{B1}"),
    ("uacute", "\u{FA}"),
    ("ugrave", "\u{F9}"),
    ("yacute", "\u{FD}"),
    ("AElig", "\u{C6}"),
    ("Acirc", "\u{C2}"),
    ("Aring", "\u{C5}"),
    ("Ecirc", "\u{CA}"),
    ("Icirc", "\u{CE}"),
    ("Ocirc", "\u{D4}"),
    ("THORN", "\u{DE}"),
    ("Ucirc", "\u{DB}"),
    ("acirc", "\u{E2}"),
    ("acute", "\u{B4}"),
    ("aelig", "\u{E6}"),
    ("aring", "\u{E5}"),
    ("cedil", "\u{B8}"),
    ("ecirc", "\u{EA}"),
    ("icirc", "\u{EE}"),
    ("iexcl", "\u{A1}"),
    ("laquo", "\u{AB}"),
    ("micro", "\u{B5}"),
    ("ocirc", "\u{F4}"),
    ("pound", "\u{A3}"),
    ("raquo", "\u{BB}"),
    ("szlig", "\u{DF}"),
    ("thorn", "\u{FE}"),
    ("times", "\u{D7}"),
    ("ucirc", "\u{FB}"),
    ("Auml", "\u{C4}"),
    ("COPY", "\u{A9}"),
    ("Euml", "\u{CB}"),
    ("Iuml", "\u{CF}"),
    ("Ouml", "\u{D6}"),
    ("QUOT", "\u{22}"),
    ("Uuml", "\u{DC}"),
    ("auml", "\u{E4}"),
    ("cent", "\u{A2}"),
    ("copy", "\u{A9}"),
    ("euml", "\u{EB}"),
    ("iuml", "\u{EF}"),
    ("macr", "\u{AF}"),
    ("nbsp", "\u{A0}"),
    ("ordf", "\u{AA}"),
    ("ordm", "\u{BA}"),
    ("ouml", "\u{F6}"),
    ("para", "\u{B6}"),
    ("quot", "\u{22}"),
    ("sect", "\u{A7}"),
    ("sup1", "\u{B9}"),
    ("sup2", "\u{B2}"),
    ("sup3", "\u{B3}"),
    ("uuml", "\u{FC}"),
    ("yuml", "\u{FF}"),
    ("AMP", "\u{26}"),
    ("ETH", "\u{D0}"),
    ("REG", "\u{AE}"),
    ("amp", "\u{26}"),
    ("deg", "\u{B0}"),
    ("eth", "\u{F0}"),
    ("not", "\u{AC}"),
    ("reg", "\u{AE}"),
    ("shy", "\u{AD}"),
    ("uml", "\u{A8}"),
    ("yen", "\u{A5}"),
    ("GT", "\u{3E}"),
    ("LT", "\u{3C}"),
    ("gt", "\u{3E}"),
    ("lt", "\u{3C}"),
];

/// Read one entity, once. Markdown requires a semicolon and bounds numeric
/// references; HTML also accepts a numeric reference without a semicolon.
fn decode_entity(raw: &str, syntax: DestinationSyntax) -> Option<(String, usize)> {
    if syntax == DestinationSyntax::Literal || !raw.starts_with('&') {
        return None;
    }
    if syntax == DestinationSyntax::Html && raw.starts_with("&#") {
        let hex = raw.as_bytes().get(2).is_some_and(|b| matches!(b, b'x' | b'X'));
        let start = if hex { 3 } else { 2 };
        let radix = if hex { 16 } else { 10 };
        let mut end = start;
        let mut value = 0u32;
        while let Some(digit) = raw.as_bytes().get(end).and_then(|b| (*b as char).to_digit(radix)) {
            value = value.saturating_mul(radix).saturating_add(digit);
            end += 1;
        }
        if end == start {
            return None;
        }
        if raw.as_bytes().get(end) == Some(&b';') {
            end += 1;
        }
        // HTML replaces C1 numeric references with their Windows-1252 values.
        const C1: [u32; 32] = [
            0x20AC, 0x81, 0x201A, 0x192, 0x201E, 0x2026, 0x2020, 0x2021, 0x2C6, 0x2030, 0x160, 0x2039, 0x152, 0x8D,
            0x17D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x2DC, 0x2122, 0x161, 0x203A,
            0x153, 0x9D, 0x17E, 0x178,
        ];
        let value = if (0x80..=0x9F).contains(&value) {
            C1[(value - 0x80) as usize]
        } else {
            value
        };
        let character = char::from_u32(value).filter(|c| *c != '\0').unwrap_or('\u{FFFD}');
        return Some((character.to_string(), end));
    }
    if syntax == DestinationSyntax::Html {
        for (name, value) in HTML_LEGACY_ENTITIES {
            if let Some(rest) = raw[1..].strip_prefix(name)
                && rest
                    .as_bytes()
                    .first()
                    .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'=')
                && !rest.starts_with(';')
            {
                return Some(((*value).to_string(), name.len() + 1));
            }
        }
    }
    let end = raw.as_bytes().iter().take(34).position(|b| *b == b';')? + 1;
    let entity = &raw[..end];
    if !entity[1..end - 1]
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'#')
    {
        return None;
    }
    let decoded: String = pulldown_cmark::Parser::new(entity)
        .filter_map(|event| match event {
            pulldown_cmark::Event::Text(text) => Some(text.into_string()),
            _ => None,
        })
        .collect();
    (decoded != entity).then_some((decoded, end))
}

/// Return the exact source span of the first decoded confusable. Mapping as
/// we scan avoids guessing by searching labels/titles for the decoded URL.
fn first_source_confusable(raw: &str, syntax: DestinationSyntax) -> Option<(std::ops::Range<usize>, char)> {
    let mut offset = 0;
    while offset < raw.len() {
        let c = raw[offset..].chars().next()?;
        if syntax == DestinationSyntax::Markdown
            && c == '\\'
            && raw.as_bytes().get(offset + 1).is_some_and(u8::is_ascii_punctuation)
        {
            offset += 2;
            continue;
        }
        if c == '&'
            && let Some((decoded, consumed)) = decode_entity(&raw[offset..], syntax)
        {
            if let Some(character) = decoded.chars().find(|c| ascii_confusable(*c)) {
                return Some((offset..offset + consumed, character));
            }
            offset += consumed;
            continue;
        }
        if ascii_confusable(c) {
            return Some((offset..offset + c.len_utf8(), c));
        }
        offset += c.len_utf8();
    }
    None
}

/// Isolate the raw destination, excluding surrounding syntax and titles.
fn destination_range(source: &str, start: usize, inline: bool) -> Option<std::ops::Range<usize>> {
    let rest = source.get(start..)?;
    let start = start + rest.len() - rest.trim_start_matches([' ', '\t', '\r', '\n']).len();
    let angle = source.as_bytes().get(start) == Some(&b'<');
    let begin = start + usize::from(angle);
    let mut depth = 0usize;
    let mut chars = source.get(begin..)?.char_indices();
    while let Some((i, c)) = chars.next() {
        if c == '\\'
            && source
                .as_bytes()
                .get(begin + i + 1)
                .is_some_and(u8::is_ascii_punctuation)
        {
            chars.next();
            continue;
        }
        if angle {
            if c == '>' {
                return Some(begin..begin + i);
            }
        } else if (c == ' ' || c.is_ascii_control()) || (inline && c == ')' && depth == 0) {
            return Some(begin..begin + i);
        } else if inline && c == '(' {
            depth += 1;
        } else if inline && c == ')' {
            depth = depth.saturating_sub(1);
        }
    }
    (!angle).then_some(begin..source.len())
}

fn reference_destination_start(raw: &str) -> Option<usize> {
    let mut chars = raw.char_indices();
    while let Some((offset, c)) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c == ']' && raw.as_bytes().get(offset + 1) == Some(&b':') {
            return Some(offset + 2);
        }
    }
    None
}

#[derive(Debug, Default, Clone)]
pub struct MD095LinkConfusables;

/// Discover start tags inside HTML regions recognized by CommonMark.
/// A code example or an escaped `<` cannot open a tag that consumes a later
/// real link. Complete tags are consumed once within each parser-bounded span.
fn html_source_tags<'a>(
    ctx: &LintContext<'_>,
    content: &'a str,
    html_regions: &[std::ops::Range<usize>],
) -> Vec<(&'a str, std::ops::Range<usize>)> {
    let mut tags = Vec::new();
    for region in html_regions {
        let mut offset = region.start;
        while let Some(relative) = content[offset..region.end].find('<') {
            let start = offset + relative;
            offset = start + 1;
            let (line, _) = ctx.offset_to_line_col(start);
            if ctx.is_in_html_comment(start)
                || ctx.is_byte_offset_in_code_span(start)
                || ctx.line_info(line).is_some_and(|info| {
                    info.in_code_block
                        || info.in_front_matter
                        || info.in_obsidian_comment
                        || info.in_mdx_comment
                        || info.is_myst_comment
                })
            {
                continue;
            }
            let bytes = content.as_bytes();
            if !bytes.get(offset).is_some_and(u8::is_ascii_alphabetic) {
                continue;
            }
            let name_start = offset;
            while offset < region.end && (bytes[offset].is_ascii_alphanumeric() || matches!(bytes[offset], b'-' | b':'))
            {
                offset += 1;
            }
            let name_end = offset;
            if offset >= region.end || !(is_html_whitespace(&bytes[offset]) || matches!(bytes[offset], b'/' | b'>')) {
                continue;
            }
            let Some(length) = start_tag_end(&content[start..region.end]) else {
                break;
            };
            offset = start + length;
            let name = &content[name_start..name_end];
            tags.push((name, start..offset));
            // Browsers treat these elements' bodies as text, not nested tags.
            // Consume their closing tag before looking for another start tag.
            if [
                "script", "style", "textarea", "title", "xmp", "iframe", "noembed", "noframes",
            ]
            .iter()
            .any(|expected| name.eq_ignore_ascii_case(expected))
            {
                let closing = format!("</{}", name.to_ascii_lowercase());
                let remaining = &content[offset..region.end];
                let mut search = 0;
                let mut end = None;
                while let Some(relative) = remaining.as_bytes()[search..]
                    .windows(closing.len())
                    .position(|candidate| candidate.eq_ignore_ascii_case(closing.as_bytes()))
                {
                    let candidate = search + relative;
                    let after_name = candidate + closing.len();
                    if remaining
                        .as_bytes()
                        .get(after_name)
                        .is_some_and(|b| is_html_whitespace(b) || matches!(b, b'/' | b'>'))
                    {
                        let Some(close) = remaining[after_name..].find('>') else {
                            break;
                        };
                        end = Some(offset + after_name + close + 1);
                        break;
                    }
                    search = after_name;
                }
                let Some(end) = end else {
                    break;
                };
                offset = end;
            }
        }
    }
    tags
}

fn merge_source_spans(mut spans: Vec<std::ops::Range<usize>>) -> Vec<std::ops::Range<usize>> {
    spans.sort_by_key(|span| (span.start, span.end));
    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for span in spans {
        if let Some(previous) = merged.last_mut()
            && span.start <= previous.end
        {
            previous.end = previous.end.max(span.end);
        } else {
            merged.push(span);
        }
    }
    merged
}

impl Rule for MD095LinkConfusables {
    fn name(&self) -> &'static str {
        "MD095"
    }

    fn description(&self) -> &'static str {
        "Link destinations should not contain confusable non-ASCII characters"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn skippable_by_category(&self) -> bool {
        // HTML href values may be relative and contain no Markdown link or
        // recognized URL scheme. Our own source check handles that case.
        false
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        !ctx.content.contains('&') && !ctx.content.chars().any(ascii_confusable)
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let mut reported = HashSet::new();

        let mut report = |offset: usize, source: &str, kind: &str, syntax: DestinationSyntax| {
            let Some((character_range, character)) = first_source_confusable(source, syntax) else {
                return;
            };
            let character_byte = offset + character_range.start;
            if !reported.insert(character_byte) {
                return;
            }
            let (line, column) = ctx.offset_to_line_col(character_byte);
            if ctx.is_byte_offset_in_code_span(character_byte)
                || ctx.is_in_html_comment(character_byte)
                || ctx.line_info(line).is_some_and(|info| {
                    info.in_code_block
                        || info.in_front_matter
                        || info.in_obsidian_comment
                        || info.in_mdx_comment
                        || info.is_myst_comment
                })
            {
                return;
            }
            let (end_line, end_column) = ctx.offset_to_line_col(offset + character_range.end);
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line,
                column,
                end_line,
                end_column,
                message: format!(
                    "{kind} contains confusable non-ASCII character U+{:04X} ({character:?})",
                    character as u32
                ),
                severity: Severity::Warning,
                fix: None,
            });
        };

        let native = pulldown_cmark::Parser::new_ext(ctx.content, crate::utils::rumdl_parser_options());
        let native_definitions: Vec<_> = native
            .reference_definitions()
            .iter()
            .map(|(_, definition)| (definition.span.clone(), definition.dest.chars().any(ascii_confusable)))
            .collect();
        let mut html_ranges = Vec::new();
        let mut html_bytes = vec![b' '; ctx.content.len()];
        for (event, span) in native.into_offset_iter() {
            match event {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::HtmlBlock) => html_ranges.push(span),
                pulldown_cmark::Event::Html(text) | pulldown_cmark::Event::InlineHtml(text) => {
                    // Inline HTML can have a single source span containing
                    // container prefixes omitted from the rendered event text.
                    // Match exact line suffixes; never strip a literal `>`.
                    let source = &ctx.content[span.clone()];
                    html_bytes[span.clone()].copy_from_slice(source.as_bytes());
                    let mut offset = span.start;
                    for (raw_line, rendered_line) in source.split_inclusive('\n').zip(text.split_inclusive('\n')) {
                        let raw = raw_line.trim_end_matches(['\r', '\n']);
                        let rendered = rendered_line.trim_end_matches(['\r', '\n']);
                        if raw.ends_with(rendered) {
                            let omitted = raw.len() - rendered.len();
                            html_bytes[offset..offset + omitted].fill(b' ');
                        }
                        offset += raw_line.len();
                    }
                    html_ranges.push(span);
                }
                _ => {}
            }
        }
        let html_source = String::from_utf8(html_bytes).expect("HTML source spans preserve UTF-8 boundaries");
        let html_spans = merge_source_spans(html_ranges);
        let in_raw_html = |offset| {
            let after = html_spans.partition_point(|span| span.start <= offset);
            after > 0
                && offset < html_spans[after - 1].end
                && !ctx
                    .line_info(ctx.offset_to_line_col(offset).0)
                    .is_some_and(crate::lint_context::types::LineInfo::in_mkdocs_container)
        };

        let links = ctx.links();
        for link in links {
            if link.is_reference || in_raw_html(link.byte_offset) {
                continue;
            }
            let raw_link = &ctx.content[link.byte_offset..link.byte_end];
            let (range, syntax) = match link.link_type {
                pulldown_cmark::LinkType::Inline => {
                    let Some(range) = destination_range(raw_link, link.text.len() + 3, true) else {
                        continue;
                    };
                    (range, DestinationSyntax::Markdown)
                }
                _ => {
                    let Some(start) = raw_link.find(link.url.as_ref()) else {
                        continue;
                    };
                    (start..start + link.url.len(), DestinationSyntax::Literal)
                }
            };
            report(
                link.byte_offset + range.start,
                &raw_link[range],
                "Link destination",
                syntax,
            );
        }

        let definitions = ctx.reference_definitions();
        let mut reference_ranges: Vec<_> = definitions
            .iter()
            .map(|definition| definition.byte_offset..definition.byte_end)
            .collect();
        for definition in definitions {
            if in_raw_html(definition.byte_offset) {
                continue;
            }
            let raw_definition = &ctx.content[definition.byte_offset..definition.byte_end];
            if let Some(start) = reference_destination_start(raw_definition)
                && let Some(range) = destination_range(raw_definition, start, false)
            {
                report(
                    definition.byte_offset + range.start,
                    &raw_definition[range],
                    "Link reference destination",
                    DestinationSyntax::Markdown,
                );
            }
        }
        // The context's regex definitions cover flavor fallbacks, but not
        // CommonMark destinations on the next line. Native spans include
        // those, including blockquote continuation prefixes.
        for (span, has_confusable) in native_definitions {
            reference_ranges.push(span.clone());
            if !has_confusable {
                continue;
            }
            let raw = &ctx.content[span.clone()];
            if let Some(start) = reference_destination_start(raw) {
                // The parsed destination is known to contain a confusable:
                // its first raw occurrence precedes the optional title.
                report(
                    span.start + start,
                    &raw[start..],
                    "Link reference destination",
                    DestinationSyntax::Markdown,
                );
            }
        }
        let reference_spans = merge_source_spans(reference_ranges);
        let source_tags = html_source_tags(ctx, &html_source, &html_spans);
        let image_spans = merge_source_spans(
            ctx.images()
                .iter()
                .map(|image| image.byte_offset..image.byte_end)
                .collect(),
        );
        // MD034's cached bare URLs deliberately drop surrounding delimiters.
        // Scan the shared URL pattern directly so parenthesized prose links
        // remain visible, then exclude actual Markdown and HTML syntax by span.
        for url in URL_SIMPLE_REGEX.find_iter(ctx.content) {
            // Attribute values remain HTML syntax even in MkDocs containers
            // whose surrounding body permits Markdown and prose autolinks.
            let after = source_tags.partition_point(|(_, span)| span.start <= url.start());
            // A scheme followed by `://` produces a colon-ending candidate
            // name, not HTML markup, inside MkDocs Markdown containers.
            if after > 0 && url.start() < source_tags[after - 1].1.end && !source_tags[after - 1].0.ends_with(':') {
                continue;
            }
            if in_raw_html(url.start()) {
                continue;
            }
            let after = links.partition_point(|link| link.byte_offset <= url.start());
            if after > 0 && url.start() < links[after - 1].byte_end {
                continue;
            }
            let after = image_spans.partition_point(|span| span.start <= url.start());
            if after > 0 && url.start() < image_spans[after - 1].end {
                continue;
            }
            let after = reference_spans.partition_point(|span| span.start <= url.start());
            if after > 0 && url.start() < reference_spans[after - 1].end {
                continue;
            }
            let mut destination = url.as_str().trim_end_matches(['.', ',', ';', ':', '!', '?']);
            let opens = destination.bytes().filter(|b| *b == b'(').count();
            let mut closes = destination.bytes().filter(|b| *b == b')').count();
            while closes > opens && destination.ends_with(')') {
                destination = &destination[..destination.len() - 1];
                closes -= 1;
            }
            report(url.start(), destination, "Bare URL", DestinationSyntax::Literal);
        }

        for (name, span) in &source_tags {
            if !["a", "area", "link"]
                .iter()
                .any(|expected| name.eq_ignore_ascii_case(expected))
            {
                continue;
            }
            let raw_tag = &html_source[span.clone()];
            let Some((value, value_range)) = extract_attribute_with_range(raw_tag, "href") else {
                continue;
            };
            let absolute_offset = span.start + value_range.start;
            if ctx.is_byte_offset_in_code_span(absolute_offset) || ctx.is_in_html_comment(absolute_offset) {
                continue;
            }
            report(
                absolute_offset,
                &value,
                "HTML link destination",
                DestinationSyntax::Html,
            );
        }

        warnings.sort_by_key(|warning| (warning.line, warning.column));
        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        Ok(ctx.content.to_string())
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(Self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    #[test]
    fn mkdocs_angle_autolinks_are_not_html_attribute_spans() {
        for content in [
            "<div markdown>\n<https://еxample.com>\n</div>\n",
            "<div class=\"grid cards\" markdown>\n- See <https://еxample.com>\n</div>\n",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
            let warnings = MD095LinkConfusables.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{content}");
            assert_eq!(warnings[0].line, 2);
            let line = content.lines().nth(1).unwrap();
            assert_eq!(warnings[0].column, line[..line.find('е').unwrap()].chars().count() + 1);
        }
        let content = "<div>\n<https://еxample.com>\n</div>\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn html_attribute_names_and_mkdocs_titles_keep_browser_semantics() {
        let content = "<p><a href\u{000B} href=\"https://еxample.com\">Docs</a></p>";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(MD095LinkConfusables.check(&ctx).unwrap().len(), 1);
        let content =
            "<div markdown>\n<a title=\"https://еxample.com\" href=\"https://example.com\">Docs</a>\n</div>\n";
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty());
        let content = "<div markdown>\n<a href=\"https://еxample.com\">Docs</a>\n</div>\n";
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let warnings = MD095LinkConfusables.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.starts_with("HTML link destination"));
    }

    #[test]
    fn multiline_inline_html_uses_rendered_prefixes_with_exact_source_spans() {
        for (content, marker) in [
            ("> <a\n> href=\"https://еxample.com\">Docs</a>\n", "е"),
            ("> <a\n> href='&iecy;'>x</a>\n", "&iecy;"),
            ("> <a\r\n> href='&iecy;'>x</a>\r\n", "&iecy;"),
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            let warnings = MD095LinkConfusables.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{content}");
            assert_eq!(warnings[0].line, 2);
            let line = content.lines().nth(1).unwrap();
            assert_eq!(
                warnings[0].column,
                line[..line.find(marker).unwrap()].chars().count() + 1
            );
            assert_eq!(warnings[0].end_column - warnings[0].column, marker.chars().count());
        }
        for content in ["<a\n>Docs</a>", "<a\n> href='&iecy;'>Docs</a>"] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty(), "{content}");
        }
    }

    #[test]
    fn multiline_html_attributes_in_blockquotes_keep_original_source_columns() {
        let content = "> <div>\n> <a\n> href=\"https://&iecy;xample.com\">Docs</a>\n> </div>\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = MD095LinkConfusables.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
        assert_eq!(
            warnings[0].column,
            content.lines().nth(2).unwrap().find("&iecy;").unwrap() + 1
        );
        assert_eq!(warnings[0].end_column - warnings[0].column, "&iecy;".len());
    }

    #[test]
    fn native_html_boundaries_preserve_following_links_and_mkdocs_markdown() {
        for (content, flavor) in [
            (
                "<pre>\nx\n</pre>\n[Download](https://еxample.com/setup.exe)\n",
                MarkdownFlavor::Standard,
            ),
            (
                "<script>\nvar a = 1;\n</script>\n<https://еxample.com>\n",
                MarkdownFlavor::Standard,
            ),
            (
                "<div class=\"grid cards\" markdown>\n- [Docs](https://еxample.com)\n</div>\n",
                MarkdownFlavor::MkDocs,
            ),
            (
                "- <div>\n  <a\n  href=\"https://еxample.com\">Docs</a>\n  </div>\n",
                MarkdownFlavor::Standard,
            ),
        ] {
            let ctx = LintContext::new(content, flavor, None);
            assert_eq!(MD095LinkConfusables.check(&ctx).unwrap().len(), 1, "{content}");
        }
        let content = "<span>\n[id]: https://еxample.com\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn markdown_like_text_in_raw_html_is_not_a_link() {
        for content in [
            "<div>\n[id]: https://еxample.com\n</div>\n",
            "<div>\n[x](https://еxample.com)\n</div>\n",
            "<div>\nSee https://еxample.com today.\n</div>\n",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty(), "{content}");
        }
    }

    #[test]
    fn raw_text_html_bodies_do_not_hide_following_anchors_or_create_links() {
        for name in ["script", "style", "textarea"] {
            let content = format!(
                "<{name}>\nconst template = '<b title=\"draft';\n</{name}>\n<a href=\"https://еxample.com\">Docs</a>\n"
            );
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert_eq!(MD095LinkConfusables.check(&ctx).unwrap().len(), 1, "{content}");
            let content = format!("<{name}>\n<a href=\"https://еxample.com\">Text only</a>\n</{name}>\n");
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty(), "{content}");
        }
    }

    #[test]
    fn code_and_comment_text_cannot_swallow_later_real_links() {
        for content in [
            "Use `a <b c` here.\n\n<a href=\"https://еxample.com\">Docs</a>",
            "```sh\nkubectl apply -f - <<EOF\nkind: Pod\nEOF\n```\n\n<a href=\"https://еxample.com\">Docs</a>",
            "```sh\ncat <<EOF\nhi\nEOF\n```\n\nSee https://еxample.com today.\n\n> quote",
            "<!-- note: <x title=\"draft -->\n\n<a href=\"https://еxample.com\">Docs</a>",
            "Use \\<b here.\n\n<a href=\"https://еxample.com\">Docs</a>",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            assert_eq!(MD095LinkConfusables.check(&ctx).unwrap().len(), 1, "{content}");
        }
    }

    #[test]
    fn image_destinations_and_alt_text_are_excluded() {
        for content in [
            "![logo](https://еxample.com/logo.png)",
            "![https://еxample.com](logo.png)",
            "![logo](<https://еxample.com/logo.png>)",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty(), "{content}");
        }
        let content = "[![b](https://example.com/b.svg)](https://еxample.com)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = MD095LinkConfusables.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.starts_with("Link destination"));
    }

    #[test]
    fn slash_after_tag_name_and_parenthesized_urls_are_checked() {
        for content in [
            "<p><a/href=\"https://еxample.com\">Docs</a></p>",
            "<p><a/ href=\"https://еxample.com\">Docs</a></p>",
            "<div>\n<a/title=\"x\" href=\"https://&iecy;xample.com\">Docs</a>\n</div>",
            "See the docs (https://еxample.com).",
            "Mirror (https://exаmple.com/download)",
            "[t](https://еxample.com)",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let warnings = MD095LinkConfusables.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{content}");
        }
        for content in [
            "`<a/href=\"https://еxample.com\">Docs</a>`",
            "`(https://еxample.com)`",
            "<!-- <a/href=\"https://еxample.com\">Docs</a> -->",
            "```html\n<a/href=\"https://еxample.com\">Docs</a>\n```",
            "<a title=\"(https://еxample.com)\" href=\"https://example.com\">Docs</a>",
            "[t](https://example.com \"(https://еxample.com)\")",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty(), "{content}");
        }
    }

    #[test]
    fn html_legacy_entities_and_c1_numeric_references_follow_attribute_rules() {
        assert_eq!(
            decode_entity("&ordf/path", DestinationSyntax::Html),
            Some(("ª".into(), 5))
        );
        assert_eq!(decode_entity("&ordfx", DestinationSyntax::Html), None);
        assert_eq!(decode_entity("&ordf=", DestinationSyntax::Html), None);
        assert_eq!(decode_entity("&#140;", DestinationSyntax::Html), Some(("Œ".into(), 6)));
        assert_eq!(
            decode_entity("&#140;", DestinationSyntax::Markdown),
            Some(("\u{008C}".into(), 6))
        );
        assert_eq!(decode_entity("&ordf/path", DestinationSyntax::Markdown), None);
    }

    #[test]
    fn detects_markdown_html_bare_and_angle_links() {
        let content = concat!(
            "[markdown](https://еxample.com)\n",
            "<https://example.com/dοcs>\n",
            "Visit https://ｅxample.com\n",
            "<a href=\"https://exаmple.com\">HTML</a>\n",
            "<a href=https://еxample.com>Unquoted HTML</a>\n",
        );
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = MD095LinkConfusables.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 5);
        assert!(warnings.iter().all(|warning| warning.fix.is_none()));
    }

    #[test]
    fn leaves_legitimate_non_ascii_destination_alone() {
        let content = "[Munich](https://münchen.de/straße)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn detects_uts39_ascii_confusables() {
        let content = concat!(
            "[Cyrillic](https://Еxample.com)\n",
            "[Greek](https://Αlpha.example)\n",
            "[Latin](https://ıong.example)\n",
            "[Punctuation](https://example。com)\n",
        );
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = MD095LinkConfusables.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 4);
    }

    #[test]
    fn repeated_url_in_label_does_not_steal_destination_location() {
        let url = "https://еxample.com";
        let content = format!("[{url}]({url})");
        let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
        let warnings = MD095LinkConfusables.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        let destination_character = content.rfind('е').unwrap();
        let (line, column) = ctx.offset_to_line_col(destination_character);
        assert_eq!((warnings[0].line, warnings[0].column), (line, column));
    }
    #[test]
    fn escaped_destination_keeps_source_column() {
        for content in [
            "[x](https://example.com/a\\*е)",
            "[x](https://example.com/a\\*е \"https://example.com/a*е\")",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            let warnings = MD095LinkConfusables.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1);
            let expected = ctx.offset_to_line_col(content.find('е').unwrap());
            assert_eq!((warnings[0].line, warnings[0].column), expected);
        }
    }

    #[test]
    fn html_attribute_urls_only_report_href() {
        let content = "<a title=\"https://еxample.com\" href=\"https://еxample.com\">x</a>";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = MD095LinkConfusables.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        let expected = ctx.offset_to_line_col(content.rfind('е').unwrap());
        assert_eq!((warnings[0].line, warnings[0].column), expected);
        assert!(warnings[0].message.starts_with("HTML link destination"));

        let safe = content.replacen("href=\"https://еxample.com", "href=\"https://example.com", 1);
        let ctx = LintContext::new(&safe, MarkdownFlavor::Standard, None);
        assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty());
    }
    #[test]
    fn repeated_reference_id_url_does_not_steal_destination_location() {
        for content in [
            "[https://еxample.com]: https://еxample.com\n\n[x][https://еxample.com]",
            "[escaped\\]:https://еxample.com]: https://еxample.com\n",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            let warnings = MD095LinkConfusables.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1);
            let destination_line = content.lines().next().unwrap();
            let expected = ctx.offset_to_line_col(destination_line.rfind('е').unwrap());
            assert_eq!((warnings[0].line, warnings[0].column), expected);
        }
    }
    #[test]
    fn encoded_confusables_are_checked_without_a_literal_character() {
        for (content, entity) in [
            ("[x](https://&#x435;xample.com)", "&#x435;"),
            ("[x](https://&#1077;xample.com)", "&#1077;"),
            ("[x](https://&iecy;xample.com)", "&iecy;"),
            ("[x](<https://&iecy;xample.com>)", "&iecy;"),
            ("[id]: https://&iecy;xample.com\n\n[x][id]", "&iecy;"),
            ("[id]: https://&iecy;xample.com\n", "&iecy;"),
            ("<a href=\"https://&#x435;xample.com\">x</a>", "&#x435;"),
            ("<a href='https://&iecy;xample.com'>x</a>", "&iecy;"),
            ("<a href=https://&#1077xample.com>x</a>", "&#1077"),
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            assert!(!MD095LinkConfusables.should_skip(&ctx), "{content}");
            let warnings = MD095LinkConfusables.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{content}");
            let start = content.find(entity).unwrap();
            assert_eq!(
                (warnings[0].line, warnings[0].column),
                ctx.offset_to_line_col(start),
                "{content}"
            );
            assert_eq!(
                (warnings[0].end_line, warnings[0].end_column),
                ctx.offset_to_line_col(start + entity.len()),
                "{content}"
            );
            assert!(warnings[0].message.contains("U+0435"), "{content}");
            assert_eq!(MD095LinkConfusables.fix(&ctx).unwrap(), content);
        }
    }

    #[test]
    fn entity_mapping_preserves_the_location_after_other_escapes() {
        for content in [
            "é [x](https://example.com/\\*&amp;&#x435; \"&#x435;\")",
            "é [&#x435;](https://example.com/&amp;е \"е\")",
            "é <a title=\"&iecy;\" href=\"https://example.com/&amp;&iecy;\">x</a>",
            "[id]: <https://example.com/\\*&amp;&iecy;> \"&iecy;\"\n",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            let warnings = MD095LinkConfusables.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{content}");
            let start = if content.contains("&amp;&#x435;") {
                content.find("&#x435;").unwrap()
            } else if content.contains("&amp;&iecy;") {
                content.find("&amp;&iecy;").unwrap() + "&amp;".len()
            } else {
                content.find('е').unwrap()
            };
            assert_eq!(
                (warnings[0].line, warnings[0].column),
                ctx.offset_to_line_col(start),
                "{content}"
            );
        }
    }

    #[test]
    fn literal_link_forms_do_not_decode_entities_or_percent_escapes() {
        for content in [
            "<https://&iecy;xample.com>",
            "Visit https://&#x435;xample.com",
            "[x](https://%D0%B5xample.com)",
            "[x](https://example.com/&amp;#x435;)",
            "[x](https://example.com/\\&iecy;)",
            "<a href=\"https://example.com/&amp;#x435;\">x</a>",
            "<a href=safe href=\"https://еxample.com\">x</a>",
            "[x](https://example.com/&#x435)",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty(), "{content}");
        }
    }

    #[test]
    fn code_comments_and_visible_text_are_excluded() {
        for content in [
            "`[x](https://&iecy;xample.com)`",
            "```md\n[x](https://еxample.com)\n<a href='https://&iecy;xample.com'>x</a>\n```",
            "    [x](https://еxample.com)",
            "<!-- [x](https://еxample.com) <a href='https://&iecy;xample.com'>x</a> -->",
            "[е](https://example.com \"е\")",
            "Just Cyrillic е or &iecy; in prose.",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty(), "{content}");
        }
    }

    #[test]
    fn malformed_and_long_numeric_entities_do_not_panic() {
        for content in [
            "[x](https://&#x;xample.com)",
            "[x](https://&#99999999;xample.com)",
            "<a href='https://&#999999999999999999999999999999999999999999999;'>x</a>",
            "<a href='https://&#xD800;xample.com'>x</a>",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty(), "{content}");
        }
    }
    #[test]
    fn mathematical_ranges_only_include_actual_ascii_skeletons() {
        assert!(ascii_confusable('\u{1D400}')); // mathematical bold A
        assert!(ascii_confusable('\u{1D6C2}')); // mathematical bold alpha
        assert!(!ascii_confusable('\u{1D6C3}')); // beta has a non-ASCII skeleton
        assert!(!ascii_confusable('\u{1D455}')); // unassigned mathematical scalar
        assert!(!ascii_confusable('β'));
        assert!(ASCII_CONFUSABLE_RANGES.windows(2).all(|pair| pair[0].1 < pair[1].0));
        assert!(
            ASCII_CONFUSABLE_RANGES
                .iter()
                .all(|(start, end)| !start.is_ascii() && start <= end)
        );
    }
    #[test]
    fn full_engine_reports_relative_html_destinations_and_respects_suppression() {
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD095LinkConfusables)];
        for content in ["<a href='е'>x</a>", "<a href='&iecy;'>x</a>", "<area href='&#x435;'>"] {
            let warnings = crate::lint(content, &rules, false, MarkdownFlavor::Standard, None, None).unwrap();
            assert_eq!(warnings.len(), 1, "{content}");
        }
        let content = "<!-- rumdl-disable MD095 -->\n<a href='&iecy;'>x</a>";
        assert!(
            crate::lint(content, &rules, false, MarkdownFlavor::Standard, None, None)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn covers_common_ascii_skeletons_in_every_documented_script() {
        for c in ['Р', 'Ѕ', 'ѕ', 'һ', 'ԁ', 'ԛ', 'ԝ', 'γ', 'σ', 'ϲ', 'ϳ', 'Ɩ', 'ո'] {
            assert!(ascii_confusable(c), "U+{:04X}", c as u32);
            let source = format!("[x](https://{c}example.com)");
            let ctx = LintContext::new(&source, MarkdownFlavor::Standard, None);
            assert_eq!(MD095LinkConfusables.check(&ctx).unwrap().len(), 1, "{source}");
        }
    }

    #[test]
    fn unicode_whitespace_in_a_destination_does_not_hide_a_confusable() {
        for whitespace in ['\u{00A0}', '\u{3000}'] {
            let source = format!("[x](https://example.com/a{whitespace}е)");
            let ctx = LintContext::new(&source, MarkdownFlavor::Standard, None);
            let warnings = MD095LinkConfusables.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{source}");
            assert_eq!(
                (warnings[0].line, warnings[0].column),
                ctx.offset_to_line_col(source.find('е').unwrap())
            );
        }
    }
    #[test]
    fn multiline_reference_destinations_keep_their_source_location() {
        for source in [
            "[id]:\n  https://&iecy;xample.com\n\n[x][id]",
            "> [id]:\n> https://&iecy;xample.com\n\n[x][id]",
            "[id]:\n  https://еxample.com \"https://еxample.com\"\n",
        ] {
            let ctx = LintContext::new(source, MarkdownFlavor::Standard, None);
            let warnings = MD095LinkConfusables.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{source}");
            let offset = source.find("&iecy;").or_else(|| source.find('е')).unwrap();
            assert_eq!(
                (warnings[0].line, warnings[0].column),
                ctx.offset_to_line_col(offset),
                "{source}"
            );
        }
    }

    #[test]
    fn reference_titles_and_hidden_definitions_are_not_destinations() {
        for source in [
            "[id]:\n  https://example.com \"https://еxample.com\"\n\n[x][id]",
            "<!--\n[id]: https://&iecy;xample.com\n-->",
            "---\n[id]: https://&iecy;xample.com\n---",
        ] {
            let ctx = LintContext::new(source, MarkdownFlavor::Standard, None);
            assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty(), "{source}");
        }
    }

    #[test]
    fn malformed_but_rendered_html_links_cannot_hide_their_href() {
        for source in [
            "<p><a / href=\"https://еxample.com\">Docs</a></p>",
            "<p><a title=\"x\"/href=\"https://еxample.com\">Docs</a></p>",
            "<p><a =x href=\"https://еxample.com\">Docs</a></p>",
            "<p><a title=\"x>y\" / href=\"https://еxample.com\">Docs</a></p>",
        ] {
            let ctx = LintContext::new(source, MarkdownFlavor::Standard, None);
            let warnings = MD095LinkConfusables.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{source}");
            assert_eq!(
                (warnings[0].line, warnings[0].column),
                ctx.offset_to_line_col(source.find('е').unwrap())
            );
        }
    }
}
