use super::*;

fn signature(scan: HtmlCommentScan) -> (Vec<(usize, usize)>, Option<usize>) {
    (
        scan.ranges.into_iter().map(|range| (range.start, range.end)).collect(),
        scan.unterminated,
    )
}

#[test]
fn comment_exclusion_index_preserves_nested_ranges_and_half_open_edges() {
    // These examples separately reject raw-end lookup, an exclusive start,
    // and an inclusive end in the exclusion index.
    let nested = format!("{}<!--x-->", " ".repeat(26));
    assert!(
        scan_html_comments(&nested, &[(3, 40), (20, 25)], &[], 0)
            .ranges
            .is_empty()
    );
    assert!(scan_html_comments("<!--x-->", &[(0, 1)], &[], 0).ranges.is_empty());
    assert_eq!(scan_html_comments("<!--x-->", &[(0, 0)], &[], 0).ranges.len(), 1);
}

#[test]
fn sorted_comment_line_containment_preserves_empty_and_reversed_queries() {
    let ranges = [
        ByteRange { start: 3, end: 8 },
        ByteRange { start: 8, end: 8 },
        ByteRange { start: 10, end: 16 },
    ];
    for start in 0..20 {
        for end in 0..24 {
            assert_eq!(
                is_line_entirely_in_sorted_html_comment(&ranges, start, end),
                is_line_entirely_in_html_comment(&ranges, start, end),
                "start={start}, end={end}",
            );
        }
    }
    assert!(!is_line_entirely_in_sorted_html_comment(&[], 0, 0));
}

#[test]
fn indexed_comment_scan_preserves_range_and_delimiter_boundaries() {
    for content in [
        "",
        "é日本🙂 <!--> <!---> <!-- real --> <!-- unclosed",
        "`<!--` <!-- real --> `-->` <!-- later -->",
        "<!-- open `-->` real --> <!-- end",
        "<!-- <!-- --> --> <!-- -->",
        "---\r\nx: '<!--'\r\n---\r\n<!-- body -->",
    ] {
        let len = content.len();
        let cases = [
            vec![],
            vec![(0, len)],
            vec![(len, len), (usize::MAX, 0), (2, 1)],
            vec![(8, 15), (0, 3), (2, 10), (10, 10)],
            vec![(20, 25), (0, 20), (5, 8), (3, 30)],
            vec![(0, usize::MAX)],
        ];
        let starts: Vec<_> = content
            .char_indices()
            .map(|(byte, _)| byte)
            .chain([len, len + 1, usize::MAX])
            .collect();
        for spans in &cases {
            for blocks in &cases {
                for &start in &starts {
                    assert_eq!(
                        signature(scan_html_comments(content, spans, blocks, start)),
                        signature(legacy_scan_html_comments(content, spans, blocks, start)),
                        "{content:?}, spans={spans:?}, blocks={blocks:?}, start={start}",
                    );
                }
            }
        }
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]
    #[test]
    fn indexed_comment_scan_matches_scalar_with_arbitrary_ranges(
        tokens in proptest::collection::vec(
            proptest::sample::select(vec!["<!--", "-->", "<!-->", "<!--->", "`", "``", "\n", "\r\n", "é日🙂", "text", "<", "!", "-", " "]),
            0..100,
        ),
        span_specs in proptest::collection::vec((0usize..2000, 0usize..40, proptest::bool::ANY), 0..40),
        block_specs in proptest::collection::vec((0usize..2000, 0usize..40, proptest::bool::ANY), 0..40),
        start_choice in 0usize..2000,
    ) {
        let content = tokens.concat();
        let convert = |specs: Vec<(usize, usize, bool)>| -> Vec<(usize, usize)> {
            specs.into_iter().map(|(offset, length, reversed)| {
                let start = offset % (content.len() + 16);
                let end = start + length;
                if reversed { (end, start) } else { (start, end) }
            }).collect()
        };
        let spans = convert(span_specs);
        let blocks = convert(block_specs);
        let boundaries: Vec<_> = content.char_indices().map(|(byte, _)| byte).chain([content.len(), content.len() + 1]).collect();
        let start = boundaries[start_choice % boundaries.len()];
        proptest::prop_assert_eq!(
            signature(scan_html_comments(&content, &spans, &blocks, start)),
            signature(legacy_scan_html_comments(&content, &spans, &blocks, start)),
        );
    }

    #[test]
    fn sorted_comment_line_containment_matches_scalar(
        specs in proptest::collection::vec((0usize..10, 0usize..80), 0..60),
        start in 0usize..4000,
        width in 0usize..100,
        reverse in proptest::bool::ANY,
    ) {
        let mut next = 0;
        let ranges: Vec<_> = specs.into_iter().map(|(gap, len)| {
            let start = next + gap;
            let end = start + len;
            next = end;
            ByteRange { start, end }
        }).collect();
        let end = if reverse { start.saturating_sub(width) } else { start + width };
        proptest::prop_assert_eq!(
            is_line_entirely_in_sorted_html_comment(&ranges, start, end),
            is_line_entirely_in_html_comment(&ranges, start, end),
        );
    }
}

// Frozen scalar implementation, including delimiter and unterminated-comment behavior.
fn legacy_scan_html_comments(
    content: &str,
    code_span_ranges: &[(usize, usize)],
    code_block_ranges: &[(usize, usize)],
    scan_from: usize,
) -> HtmlCommentScan {
    let in_code = |pos: usize| {
        code_span_ranges.iter().any(|&(start, end)| pos >= start && pos < end)
            || code_block_ranges.iter().any(|&(start, end)| pos >= start && pos < end)
    };

    let mut ranges = Vec::new();
    let mut search_from = scan_from.min(content.len());
    while let Some(rel) = crate::utils::find_ascii_delimiter(&content[search_from..], "<!--") {
        let open = search_from + rel;
        if in_code(open) {
            // Literal `<!--` inside code: not a comment opener.
            search_from = open + "<!--".len();
            continue;
        }
        // Find the next `-->` that is not itself inside code. The search starts
        // two bytes into the opener, because the opener's own trailing dashes can
        // serve as the closer's leading dashes: `<!-->` and `<!--->` are complete
        // comments, and a line holding one ends an HTML block.
        let mut close_from = open + 2;
        let end = loop {
            let Some(crel) = crate::utils::find_ascii_delimiter(&content[close_from..], "-->") else {
                break None;
            };
            let close = close_from + crel;
            if in_code(close) {
                close_from = close + "-->".len();
                continue;
            }
            break Some(close + "-->".len());
        };
        match end {
            Some(end) => {
                ranges.push(ByteRange { start: open, end });
                search_from = end;
            }
            // Unterminated comment (no closer anywhere): it covers no range,
            // and no opener after it can close either, so the scan is done.
            None => {
                return HtmlCommentScan {
                    ranges,
                    unterminated: Some(open),
                };
            }
        }
    }
    HtmlCommentScan {
        ranges,
        unterminated: None,
    }
}
