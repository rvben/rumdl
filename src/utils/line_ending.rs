use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
    Mixed,
}

/// Maps byte offsets in LF-normalized content back to the original input.
///
/// rumdl normalizes CRLF to LF while linting. Machine-readable fixes, however,
/// must address the bytes supplied by the caller, including mixed-line-ending
/// input. Each entry records the normalized offset of a newline whose preceding
/// carriage return was removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizedLineEndingMap {
    crlf_newline_offsets: Vec<usize>,
    /// Normalized offsets of the newlines that were a bare LF, recorded only
    /// when the original mixes line endings: text written into such a file
    /// takes the ending of the line it follows.
    mixed_lf_newline_offsets: Vec<usize>,
    /// Whether every line of the original ends in CRLF, so text written into it
    /// takes CRLF as well.
    all_crlf: bool,
}

impl NormalizedLineEndingMap {
    pub fn new(original: &str) -> Self {
        let bytes = original.as_bytes();
        let mut crlf_newline_offsets = Vec::new();
        let mut lf_newline_offsets = Vec::new();
        let mut original_offset = 0;
        let mut normalized_offset = 0;

        while original_offset < bytes.len() {
            if bytes[original_offset] == b'\r'
                && original_offset + 1 < bytes.len()
                && bytes[original_offset + 1] == b'\n'
            {
                crlf_newline_offsets.push(normalized_offset);
                original_offset += 2;
                normalized_offset += 1;
            } else {
                if bytes[original_offset] == b'\n' {
                    lf_newline_offsets.push(normalized_offset);
                }
                original_offset += 1;
                normalized_offset += 1;
            }
        }

        let all_crlf = !crlf_newline_offsets.is_empty() && lf_newline_offsets.is_empty();
        if crlf_newline_offsets.is_empty() {
            lf_newline_offsets.clear();
        }
        Self {
            crlf_newline_offsets,
            mixed_lf_newline_offsets: lf_newline_offsets,
            all_crlf,
        }
    }

    fn is_mixed(&self) -> bool {
        !self.mixed_lf_newline_offsets.is_empty()
    }

    /// Text to insert into the original input at `normalized_offset`:
    /// LF-normalized `text` with the line ending `fmt` would give it there.
    /// That is the file's own ending, or in a file with mixed endings the
    /// ending of the line the insertion follows.
    pub fn original_text_at<'a>(&self, text: &'a str, normalized_offset: usize) -> Cow<'a, str> {
        if !text.contains('\n') {
            return Cow::Borrowed(text);
        }
        if self.all_crlf {
            return normalize_line_ending(text, LineEnding::Crlf);
        }
        if self.is_mixed() && self.ending_before(normalized_offset) == "\r\n" {
            return normalize_line_ending(text, LineEnding::Crlf);
        }
        Cow::Borrowed(text)
    }

    /// The ending of the line holding the byte before `normalized_offset`, or
    /// of the first line at the start of the text. An unterminated last line
    /// has none of its own and gives the ending of the line before it.
    fn ending_before(&self, normalized_offset: usize) -> &'static str {
        let position = normalized_offset.saturating_sub(1);
        let next = |offsets: &[usize]| {
            offsets
                .get(offsets.partition_point(|&offset| offset < position))
                .copied()
        };
        match (next(&self.crlf_newline_offsets), next(&self.mixed_lf_newline_offsets)) {
            (Some(crlf), Some(lf)) => {
                if crlf < lf {
                    "\r\n"
                } else {
                    "\n"
                }
            }
            (Some(_), None) => "\r\n",
            (None, Some(_)) => "\n",
            (None, None) => {
                if self.crlf_newline_offsets.last() > self.mixed_lf_newline_offsets.last() {
                    "\r\n"
                } else {
                    "\n"
                }
            }
        }
    }

    /// Write `fixed`, the LF-normalized content after fixes, back in the line
    /// endings of the original this map was built from. `normalized_original`
    /// is the LF-normalized original the fixes started from.
    ///
    /// A file with one line ending gets it throughout. In a file with mixed
    /// endings, a diff of the two texts decides each line ending: a newline the
    /// fixes kept keeps its original ending, and a newline they inserted takes
    /// the ending of the line it was inserted into, the same ending
    /// [`Self::original_text_at`] gives a JSON fix inserting it there. A line
    /// diff finds the changed regions and a byte diff aligns the newlines
    /// inside each one, so the cost follows the size of the edits.
    pub fn restore_fixed<'a>(&self, normalized_original: &str, fixed: &'a str) -> Cow<'a, str> {
        if self.all_crlf {
            return normalize_line_ending(fixed, LineEnding::Crlf);
        }
        if !self.is_mixed() {
            return Cow::Borrowed(fixed);
        }

        let old: Vec<&str> = normalized_original.split_inclusive('\n').collect();
        let new: Vec<&str> = fixed.split_inclusive('\n').collect();
        let line_starts = |lines: &[&str]| {
            let mut starts = Vec::with_capacity(lines.len() + 1);
            starts.push(0);
            for line in lines {
                starts.push(starts.last().unwrap() + line.len());
            }
            starts
        };
        let old_starts = line_starts(&old);
        let new_starts = line_starts(&new);

        // The ending of each newline in `fixed`, in order.
        let mut endings: Vec<&'static str> = Vec::with_capacity(new.len());
        // A newline kept from the original at `offset`, or inserted at `offset`.
        let kept = |offset: usize| self.ending_before(offset + 1);
        let inserted = |offset: usize| self.ending_before(offset);
        let newlines = |text: &str| text.bytes().filter(|&b| b == b'\n').count();

        for op in similar::capture_diff_slices(similar::Algorithm::Myers, &old, &new) {
            match op {
                similar::DiffOp::Equal { old_index, len, .. } => {
                    for line in old_index..old_index + len {
                        if old[line].ends_with('\n') {
                            endings.push(kept(old_starts[line + 1] - 1));
                        }
                    }
                }
                similar::DiffOp::Delete { .. } => {}
                similar::DiffOp::Insert {
                    old_index,
                    new_index,
                    new_len,
                } => {
                    let at = inserted(old_starts[old_index]);
                    let count = newlines(&fixed[new_starts[new_index]..new_starts[new_index + new_len]]);
                    endings.extend(std::iter::repeat_n(at, count));
                }
                similar::DiffOp::Replace {
                    old_index,
                    old_len,
                    new_index,
                    new_len,
                } => {
                    let base = old_starts[old_index];
                    let old_bytes = &normalized_original.as_bytes()[base..old_starts[old_index + old_len]];
                    let new_bytes = &fixed.as_bytes()[new_starts[new_index]..new_starts[new_index + new_len]];
                    for byte_op in similar::capture_diff_slices(similar::Algorithm::Myers, old_bytes, new_bytes) {
                        let (old_at, new_range, is_kept) = match byte_op {
                            similar::DiffOp::Equal {
                                old_index,
                                new_index,
                                len,
                            } => (old_index, new_index..new_index + len, true),
                            similar::DiffOp::Delete { .. } => continue,
                            similar::DiffOp::Insert {
                                old_index,
                                new_index,
                                new_len,
                            }
                            | similar::DiffOp::Replace {
                                old_index,
                                new_index,
                                new_len,
                                ..
                            } => (old_index, new_index..new_index + new_len, false),
                        };
                        for (k, &byte) in new_bytes[new_range].iter().enumerate() {
                            if byte == b'\n' {
                                endings.push(if is_kept {
                                    kept(base + old_at + k)
                                } else {
                                    inserted(base + old_at)
                                });
                            }
                        }
                    }
                }
            }
        }

        let mut restored = String::with_capacity(fixed.len() + endings.len());
        let mut endings = endings.into_iter();
        for line in &new {
            match line.strip_suffix('\n') {
                Some(body) => {
                    restored.push_str(body);
                    restored.push_str(endings.next().unwrap_or("\n"));
                }
                None => restored.push_str(line),
            }
        }
        Cow::Owned(restored)
    }

    /// Convert a byte boundary in normalized content to the corresponding byte
    /// boundary in the original input.
    pub fn original_offset(&self, normalized_offset: usize) -> usize {
        normalized_offset
            + self
                .crlf_newline_offsets
                .partition_point(|newline_offset| *newline_offset < normalized_offset)
    }

    /// The original input, rebuilt from the LF-normalized content this map was
    /// built from by putting back each carriage return normalization removed.
    /// Exact for any input, mixed line endings included, where re-normalizing to
    /// the detected line ending is not.
    pub fn restore(&self, normalized: &str) -> String {
        let mut restored = String::with_capacity(normalized.len() + self.crlf_newline_offsets.len());
        let mut copied = 0;
        for &newline_offset in &self.crlf_newline_offsets {
            restored.push_str(&normalized[copied..newline_offset]);
            restored.push('\r');
            copied = newline_offset;
        }
        restored.push_str(&normalized[copied..]);
        restored
    }
}

/// The line ending text inserted at byte `offset` of `content` takes, by the
/// rule `NormalizedLineEndingMap::original_text_at` applies to normalized
/// offsets: the ending of the line holding the byte before `offset` (the first
/// line at offset 0), or for an unterminated last line the ending before it.
/// `\n` when `content` has no line ending at all.
pub fn line_ending_before(content: &str, offset: usize) -> &'static str {
    let bytes = content.as_bytes();
    let position = offset.saturating_sub(1).min(bytes.len());
    let newline = bytes[position..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|found| position + found)
        .or_else(|| bytes.iter().rposition(|&b| b == b'\n'));
    match newline {
        Some(newline) if newline > 0 && bytes[newline - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

pub fn detect_line_ending_enum(content: &str) -> LineEnding {
    let bytes = content.as_bytes();
    let mut has_crlf = false;
    let mut has_standalone_lf = false;
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
            has_crlf = true;
            i += 2;
        } else if bytes[i] == b'\n' {
            has_standalone_lf = true;
            i += 1;
        } else {
            i += 1;
        }
        // Early exit once both types are found
        if has_crlf && has_standalone_lf {
            return LineEnding::Mixed;
        }
    }

    match (has_crlf, has_standalone_lf) {
        (true, true) => LineEnding::Mixed,
        (true, false) => LineEnding::Crlf,
        (false, _) => LineEnding::Lf,
    }
}

pub fn detect_line_ending(content: &str) -> &'static str {
    // Compatibility function matching the old signature
    let crlf_count = content.matches("\r\n").count();
    let lf_count = content.matches('\n').count() - crlf_count;

    if crlf_count > lf_count { "\r\n" } else { "\n" }
}

pub fn normalize_line_ending<'a>(content: &'a str, target: LineEnding) -> Cow<'a, str> {
    match target {
        LineEnding::Lf => {
            if !content.contains('\r') {
                Cow::Borrowed(content)
            } else {
                Cow::Owned(content.replace("\r\n", "\n"))
            }
        }
        LineEnding::Crlf => {
            // First normalize everything to LF, then convert to CRLF
            let normalized = content.replace("\r\n", "\n");
            Cow::Owned(normalized.replace('\n', "\r\n"))
        }
        LineEnding::Mixed => Cow::Borrowed(content),
    }
}

pub fn ensure_consistent_line_endings(original: &str, modified: &str) -> String {
    let original_ending = detect_line_ending_enum(original);

    // For mixed line endings, normalize to the most common one (like detect_line_ending does)
    let target_ending = if original_ending == LineEnding::Mixed {
        // Use the same logic as detect_line_ending: prefer the more common one
        let crlf_count = original.matches("\r\n").count();
        let lf_count = original.matches('\n').count() - crlf_count;
        if crlf_count > lf_count {
            LineEnding::Crlf
        } else {
            LineEnding::Lf
        }
    } else {
        original_ending
    };

    let modified_ending = detect_line_ending_enum(modified);

    if target_ending != modified_ending {
        normalize_line_ending(modified, target_ending).into_owned()
    } else {
        modified.to_string()
    }
}

pub fn get_line_ending_str(ending: LineEnding) -> &'static str {
    match ending {
        LineEnding::Lf => "\n",
        LineEnding::Crlf => "\r\n",
        LineEnding::Mixed => "\n", // Default to LF for mixed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_line_ending_enum() {
        assert_eq!(detect_line_ending_enum("hello\nworld"), LineEnding::Lf);
        assert_eq!(detect_line_ending_enum("hello\r\nworld"), LineEnding::Crlf);
        assert_eq!(detect_line_ending_enum("hello\r\nworld\nmixed"), LineEnding::Mixed);
        assert_eq!(detect_line_ending_enum("no line endings"), LineEnding::Lf);
    }

    #[test]
    fn normalized_line_ending_map_handles_mixed_input() {
        let original = "a\r\nb\nc\r\n";
        let map = NormalizedLineEndingMap::new(original);

        // Boundaries surrounding the two CRLF sequences gain one byte each,
        // while the standalone LF does not.
        assert_eq!(map.original_offset(1), 1);
        assert_eq!(map.original_offset(2), 3);
        assert_eq!(map.original_offset(4), 5);
        assert_eq!(map.original_offset(6), 8);
    }

    #[test]
    fn normalized_line_ending_map_writes_text_with_the_ending_fmt_writes() {
        let crlf = NormalizedLineEndingMap::new("a\r\nb\r\n");
        assert_eq!(crlf.original_text_at("x\n\ny\n", 2), "x\r\n\r\ny\r\n");
        assert_eq!(crlf.original_text_at("no newline", 2), "no newline");

        for original in ["a\nb\n", ""] {
            let map = NormalizedLineEndingMap::new(original);
            assert_eq!(map.original_text_at("x\n", 0), "x\n", "{original:?}");
        }

        // In a mixed file, inserted lines take the ending of the line the
        // insertion follows, as `fmt` writes them. Normalized "a\nb\nc" has
        // its newlines at offsets 1 (CRLF) and 3 (LF).
        let mixed = NormalizedLineEndingMap::new("a\r\nb\nc");
        assert_eq!(
            mixed.original_text_at("x\n", 0),
            "x\r\n",
            "start of file: first line's ending"
        );
        assert_eq!(mixed.original_text_at("x\n", 1), "x\r\n", "end of line a");
        assert_eq!(
            mixed.original_text_at("x\n", 2),
            "x\r\n",
            "start of line b, after a's CRLF"
        );
        assert_eq!(mixed.original_text_at("x\n", 3), "x\n", "end of line b");
        assert_eq!(mixed.original_text_at("x\n", 4), "x\n", "start of line c, after b's LF");
        assert_eq!(
            mixed.original_text_at("x\n", 5),
            "x\n",
            "unterminated last line: the ending before it"
        );
    }

    #[test]
    fn line_ending_before_agrees_with_the_map_at_every_offset() {
        for original in ["a\r\nb\nc", "a\nb\r\n", "\r\n\n\r\nxy\n", "é\r\n日本\nz"] {
            let map = NormalizedLineEndingMap::new(original);
            let normalized = normalize_line_ending(original, LineEnding::Lf);
            for offset in 0..=normalized.len() {
                assert_eq!(
                    line_ending_before(original, map.original_offset(offset)),
                    map.ending_before(offset),
                    "{original:?} at normalized offset {offset}"
                );
            }
        }
        assert_eq!(line_ending_before("no newline", 3), "\n");
    }

    #[test]
    fn restore_fixed_keeps_uniform_endings_uniform() {
        let crlf = NormalizedLineEndingMap::new("# A\r\ntext\r\n");
        assert_eq!(
            crlf.restore_fixed("# A\ntext\n", "# A\n\ntext\n"),
            "# A\r\n\r\ntext\r\n"
        );
        let lf = NormalizedLineEndingMap::new("# A\ntext\n");
        assert_eq!(lf.restore_fixed("# A\ntext\n", "# A\n\ntext\n"), "# A\n\ntext\n");
    }

    /// `restore_fixed` on a mixed original, given as (original, fixed LF text, expected).
    fn assert_mixed_restore(original: &str, fixed: &str, expected: &str) {
        let map = NormalizedLineEndingMap::new(original);
        let normalized = normalize_line_ending(original, LineEnding::Lf);
        assert_eq!(
            map.restore_fixed(&normalized, fixed),
            expected,
            "{original:?} -> {fixed:?}"
        );
    }

    #[test]
    fn restore_fixed_keeps_each_untouched_lines_ending_in_a_mixed_file() {
        // Unchanged content comes back byte-identical.
        assert_mixed_restore("# T\r\nText\n## N\r\n", "# T\nText\n## N\n", "# T\r\nText\n## N\r\n");
        // Inserted lines take the ending of the line before them.
        assert_mixed_restore(
            "# T\r\nText\n## N\r\n",
            "# T\n\nText\n\n## N\n",
            "# T\r\n\r\nText\n\n## N\r\n",
        );
        // An edited line keeps the ending of the line it replaces.
        assert_mixed_restore("a  \r\nb\nc  \r\n", "a\nb\nc\n", "a\r\nb\nc\r\n");
        // A deleted line takes its ending with it.
        assert_mixed_restore("a\r\nx\ny\r\nb\n", "a\nb\n", "a\r\nb\n");
        // A blank line inserted before an edited line takes the ending of the
        // line before it; the edited line keeps its own.
        assert_mixed_restore(
            "More\n```\r\ncode\n",
            "More\n\n```text\ncode\n",
            "More\n\n```text\r\ncode\n",
        );
        // A line inserted at the start takes the first line's ending.
        assert_mixed_restore("text\r\nmore\n", "# T\n\ntext\nmore\n", "# T\r\n\r\ntext\r\nmore\n");
        // A final newline added to an unterminated last line takes the ending
        // of the line before it.
        assert_mixed_restore("a\n# b\r\nc", "a\n# b\nc\n", "a\n# b\r\nc\r\n");
        // A line split in two: the inserted break takes the ending of the line
        // it splits, and the second part keeps that line's own.
        assert_mixed_restore("x\nlong line\r\ny\n", "x\nlong\nline\ny\n", "x\nlong\r\nline\r\ny\n");
    }

    #[test]
    fn normalized_line_ending_map_restores_the_original_bytes() {
        for original in [
            "",
            "no newline",
            "a\nb\n",
            "a\r\nb\r\n",
            "a\r\nb\nc\r\n",
            "a\nb\r\nc",
            "\r\n\r\n\n",
            "lone\rcarriage\r\nreturn\r",
            "é\r\n日本\n",
        ] {
            let normalized = normalize_line_ending(original, LineEnding::Lf);
            let map = NormalizedLineEndingMap::new(original);
            assert_eq!(map.restore(&normalized), original, "{original:?}");
        }
    }

    #[test]
    fn test_detect_line_ending() {
        assert_eq!(detect_line_ending("hello\nworld"), "\n");
        assert_eq!(detect_line_ending("hello\r\nworld"), "\r\n");
        assert_eq!(detect_line_ending("hello\r\nworld\nmixed"), "\n"); // More LF than CRLF
        assert_eq!(detect_line_ending("no line endings"), "\n");
    }

    #[test]
    fn test_normalize_line_ending() {
        assert_eq!(normalize_line_ending("hello\r\nworld", LineEnding::Lf), "hello\nworld");
        assert_eq!(
            normalize_line_ending("hello\nworld", LineEnding::Crlf),
            "hello\r\nworld"
        );
        assert_eq!(
            normalize_line_ending("hello\r\nworld\nmixed", LineEnding::Lf),
            "hello\nworld\nmixed"
        );
    }

    #[test]
    fn test_ensure_consistent_line_endings() {
        let original = "hello\r\nworld";
        let modified = "hello\nworld\nextra";
        assert_eq!(
            ensure_consistent_line_endings(original, modified),
            "hello\r\nworld\r\nextra"
        );

        let original = "hello\nworld";
        let modified = "hello\r\nworld\r\nextra";
        assert_eq!(
            ensure_consistent_line_endings(original, modified),
            "hello\nworld\nextra"
        );
    }
}
