//! Scalar oracles preserve the original greedy pairing and crossing rules.
use super::*;
use proptest::prelude::*;

fn original_find_single_emphasis_spans_with<T>(
    line: &str,
    markers: &[EmphasisMarker],
    mut emit: impl FnMut(&EmphasisMarker, &EmphasisMarker, &str, bool, bool) -> T,
) -> Vec<T> {
    // Early return for insufficient markers
    if markers.len() < 2 {
        return Vec::new();
    }

    // CommonMark left/right-flanking (whitespace clause): an emphasis opener
    // must not be immediately followed by whitespace, and a closer must not be
    // immediately preceded by it. A marker that can do neither is a literal
    // `*`/`_` (e.g. a list-marker `*`, or a `*` flanked by spaces) and is
    // transparent to delimiter matching. `find_emphasis_spans` (MD037)
    // deliberately keeps such runs so MD037 can flag the spaces inside them;
    // this single-emphasis finder, used only by MD049, must not.
    let bytes = line.as_bytes();
    let is_ws = |b: u8| b == b' ' || b == b'\t';
    let can_open = |m: &EmphasisMarker| {
        let after = m.end_pos();
        after < bytes.len() && !is_ws(bytes[after])
    };
    let can_close = |m: &EmphasisMarker| m.start_pos > 0 && !is_ws(bytes[m.start_pos - 1]);

    let mut spans = Vec::new();
    let mut used_markers = vec![false; markers.len()];

    // Process markers in pairs more efficiently
    for i in 0..markers.len() {
        if used_markers[i] || markers[i].count != 1 || !can_open(&markers[i]) {
            continue;
        }

        let opening = &markers[i];

        // Look for the nearest matching closing marker using optimized search
        for j in (i + 1)..markers.len() {
            if used_markers[j] {
                continue;
            }

            let closing = &markers[j];

            // Quick type and count check - only single emphasis that can close
            if closing.marker_type == opening.marker_type && closing.count == 1 && can_close(closing) {
                let content_start = opening.end_pos();
                let content_end = closing.start_pos;

                if content_end > content_start {
                    let content = &line[content_start..content_end];

                    // Optimized validation checks
                    if is_valid_emphasis_content_fast(content) && is_valid_emphasis_span_fast(line, opening, closing) {
                        // A pairing is only blocked by an intervening *viable*
                        // delimiter (one that can itself open or close); a
                        // transparent literal marker does not interfere, which
                        // lets `*foo * bar*` pair its outer asterisks the way
                        // CommonMark does.
                        let crosses_markers = markers[i + 1..j].iter().any(|marker| {
                            marker.marker_type == opening.marker_type
                                && marker.count == 1
                                && (can_open(marker) || can_close(marker))
                        });

                        if !crosses_markers {
                            // Flanking guarantees the content is not whitespace-
                            // padded, but keep the fields honest for callers.
                            let has_leading_space = content.starts_with(' ') || content.starts_with('\t');
                            let has_trailing_space = content.ends_with(' ') || content.ends_with('\t');

                            spans.push(emit(opening, closing, content, has_leading_space, has_trailing_space));

                            // Mark both markers as used
                            used_markers[i] = true;
                            used_markers[j] = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    spans
}

fn original_find_emphasis_spans_with<T>(
    line: &str,
    markers: &[EmphasisMarker],
    mut emit: impl FnMut(&EmphasisMarker, &EmphasisMarker, &str, bool, bool) -> T,
) -> Vec<T> {
    // Early return for insufficient markers
    if markers.len() < 2 {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut used_markers = vec![false; markers.len()];

    // Process markers in pairs more efficiently
    for i in 0..markers.len() {
        if used_markers[i] {
            continue;
        }

        let opening = &markers[i];

        // Look for the nearest matching closing marker using optimized search
        for j in (i + 1)..markers.len() {
            if used_markers[j] {
                continue;
            }

            let closing = &markers[j];

            // Quick type and count check
            if closing.marker_type == opening.marker_type && closing.count == opening.count {
                let content_start = opening.end_pos();
                let content_end = closing.start_pos;

                if content_end > content_start {
                    let content = &line[content_start..content_end];

                    // Optimized validation checks
                    if is_valid_emphasis_content_fast(content) && is_valid_emphasis_span_fast(line, opening, closing) {
                        // Quick check for crossing markers
                        let crosses_markers = markers[i + 1..j]
                            .iter()
                            .any(|marker| marker.marker_type == opening.marker_type);

                        if !crosses_markers {
                            let has_leading_space = content.starts_with(' ') || content.starts_with('\t');
                            let has_trailing_space = content.ends_with(' ') || content.ends_with('\t');

                            spans.push(emit(opening, closing, content, has_leading_space, has_trailing_space));

                            // Mark both markers as used
                            used_markers[i] = true;
                            used_markers[j] = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    spans
}

fn original_find_valid_emphasis_ranges(line: &str, markers: &[EmphasisMarker]) -> Vec<(usize, usize)> {
    if markers.len() < 2 {
        return Vec::new();
    }

    let bytes = line.as_bytes();
    let is_ws = |b: u8| b == b' ' || b == b'\t';
    let can_open = |m: &EmphasisMarker| {
        let after = m.end_pos();
        after < bytes.len() && !is_ws(bytes[after])
    };
    let can_close = |m: &EmphasisMarker| m.start_pos > 0 && !is_ws(bytes[m.start_pos - 1]);

    let mut ranges = Vec::new();
    let mut used = vec![false; markers.len()];

    for i in 0..markers.len() {
        if used[i] || !can_open(&markers[i]) {
            continue;
        }

        let opening = &markers[i];

        for j in (i + 1)..markers.len() {
            if used[j] {
                continue;
            }

            let closing = &markers[j];

            // Same marker run (type and strength) that can validly close.
            if closing.marker_type == opening.marker_type && closing.count == opening.count && can_close(closing) {
                let content_start = opening.end_pos();
                let content_end = closing.start_pos;

                if content_end > content_start {
                    let content = &line[content_start..content_end];

                    if is_valid_emphasis_content_fast(content) && is_valid_emphasis_span_fast(line, opening, closing) {
                        // Only an intervening *viable* delimiter of the same
                        // type blocks the pairing; transparent literals do not.
                        let crosses = markers[i + 1..j]
                            .iter()
                            .any(|m| m.marker_type == opening.marker_type && (can_open(m) || can_close(m)));

                        if !crosses {
                            ranges.push((opening.start_pos, closing.end_pos()));
                            used[i] = true;
                            used[j] = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    ranges
}

fn span_fields(spans: Vec<EmphasisSpan>) -> Vec<(EmphasisMarker, EmphasisMarker, String, bool, bool)> {
    spans
        .into_iter()
        .map(|span| {
            (
                span.opening,
                span.closing,
                span.content,
                span.has_leading_space,
                span.has_trailing_space,
            )
        })
        .collect()
}

fn assert_pairing_contract(source: &str, markers: &[EmphasisMarker]) {
    assert_eq!(
        span_fields(find_single_emphasis_spans(source, markers)),
        span_fields(original_find_single_emphasis_spans_with(source, markers, owned_span)),
        "single emphasis: {source:?}",
    );
    assert_eq!(
        span_fields(find_emphasis_spans(source, markers)),
        span_fields(original_find_emphasis_spans_with(source, markers, owned_span)),
        "all emphasis: {source:?}",
    );
    assert_eq!(
        find_valid_emphasis_ranges(source, markers),
        original_find_valid_emphasis_ranges(source, markers),
        "valid ranges: {source:?}",
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn pairing_matches_original(
        tokens in prop::collection::vec(prop::sample::select(vec![
            "*", "**", "***", "****", "_", "__", "___", "____", "a", "word",
            "é日本🙂", "e\u{301}", " ", "\t", "\n", "\r\n", "\u{a0}",
            "(", ")", "[", "]", "{", "}", "'", "\"", ">", "<", "!", ".", "`", "$",
        ]), 0..80),
    ) {
        let source = tokens.concat();
        let markers = find_emphasis_markers(&source);
        assert_pairing_contract(&source, &markers);
        // Public helpers also accept caller-supplied subsets and orderings.
        let subset: Vec<_> = markers.iter().step_by(2).cloned().collect();
        assert_pairing_contract(&source, &subset);
        let reversed: Vec<_> = markers.into_iter().rev().collect();
        assert_pairing_contract(&source, &reversed);
    }
}

#[test]
fn exhaustive_short_pairings_match_original() {
    let tokens = ["*", "**", "_", "__", "a", " ", "\t", "(", ")"];
    for mut value in 0..tokens.len().pow(5) {
        let mut source = String::new();
        for _ in 0..5 {
            source.push_str(tokens[value % tokens.len()]);
            value /= tokens.len();
        }
        assert_pairing_contract(&source, &find_emphasis_markers(&source));
    }
}

#[test]
fn used_mixed_strength_and_literal_markers_remain_blockers() {
    for source in [
        "*a* *b*",
        "**a* b**",
        "*a** b*",
        "__a__ _b_",
        "*a * b*",
        "**a ** b**",
        "*a _ b_ *",
        "*a _b_ c*",
        "*a*word *later*",
        "*word *word *word",
        "__word __word __word",
        "* a * b * c *",
        "é日本🙂 * good * _ fine _",
        "*\u{a0}*",
        "*\n*",
        "*\r\n*",
    ] {
        assert_pairing_contract(source, &find_emphasis_markers(source));
    }
}
