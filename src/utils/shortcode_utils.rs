//! Hugo/Quarto shortcode boundaries, including quoted argument values.

use crate::utils::regex_cache::HUGO_SHORTCODE_REGEX;

/// Yield shortcode byte ranges without splitting a quoted argument at `>}}`
/// or `%}}`. The iterator only scans through the next match, so reflow can
/// reuse its existing cached searches without collecting the remaining text.
pub(crate) fn shortcode_ranges(content: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    ShortcodeRanges {
        content,
        cursor: 0,
        recover_tail: false,
    }
}

struct ShortcodeRanges<'a> {
    content: &'a str,
    cursor: usize,
    recover_tail: bool,
}

impl Iterator for ShortcodeRanges<'_> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if !self.recover_tail {
            let bytes = self.content.as_bytes();
            let relative = bytes[self.cursor..]
                .windows(3)
                .position(|window| window == b"{{<" || window == b"{{%")?;
            let start = self.cursor + relative;
            let marker = if bytes[start + 2] == b'<' { b'>' } else { b'%' };
            let mut pos = start + 3;
            let mut quote = None;
            let mut escaped_outer = false;
            let mut parameter_start = true;
            let mut has_equals = false;

            while pos < bytes.len() {
                let byte = bytes[pos];
                if let Some(delimiter) = quote {
                    // Hugo raw strings end at the next backtick. Normal
                    // double quotes ignore a quote preceded by a backslash;
                    // the escaped-outer-quote syntax ends at the next quote.
                    if byte == delimiter && (delimiter == b'`' || escaped_outer || bytes[pos - 1] != b'\\') {
                        quote = None;
                        parameter_start = false;
                    }
                } else if matches!(byte, b'>' | b'%') && bytes.get(pos + 1..pos + 3) == Some(b"}}") {
                    if byte == marker {
                        self.cursor = pos + 3;
                        return Some((start, self.cursor));
                    }
                    // Preserve the legacy boundary for mismatched delimiters.
                    break;
                } else if parameter_start && matches!(byte, b'`' | b'"') {
                    quote = Some(byte);
                    escaped_outer = false;
                } else if parameter_start && byte == b'\\' && bytes.get(pos + 1) == Some(&b'"') {
                    quote = Some(b'"');
                    escaped_outer = true;
                    pos += 1;
                } else {
                    // Apostrophes and quotes inside bare values are ordinary
                    // argument characters, not Hugo string delimiters.
                    if byte.is_ascii_whitespace() {
                        parameter_start = true;
                        has_equals = false;
                    } else if byte == b'=' && !has_equals {
                        parameter_start = true;
                        has_equals = true;
                    } else {
                        parameter_start = false;
                    }
                }
                pos += 1;
            }

            // Unterminated strings/tags retain the previous recovery behavior.
            // Recover the whole remaining tail with the regex once instead of
            // rescanning it for each later malformed opener.
            self.cursor = start;
            self.recover_tail = true;
        }

        let found = HUGO_SHORTCODE_REGEX.find(&self.content[self.cursor..])?;
        let range = (self.cursor + found.start(), self.cursor + found.end());
        self.cursor = range.1;
        Some(range)
    }
}
