use super::*;

#[test]
fn link_target_policy_accepts_canonical_working_directory_paths() {
    let base = std::env::current_dir().expect("test process should have a working directory");
    let working_root = base.join("workspace");
    let canonical_root = base.join("canonical-workspace");
    let roots = [working_root.as_path(), canonical_root.as_path()];
    let canonical_target = canonical_root.join("docs").join("b.md");
    let canonical_extensionless_target = canonical_root.join("docs").join("b");

    let policy = LinkTargetPolicy::from_paths_with_roots(["docs/b.md"], true, roots);

    assert_eq!(
        policy.resolve_supplied(&canonical_target),
        Some(canonical_target.clone())
    );
    assert_eq!(
        policy.resolve_supplied(&canonical_extensionless_target),
        Some(canonical_target.clone())
    );

    let working_target = working_root.join("docs").join("b.md");
    let absolute_policy = LinkTargetPolicy::from_paths_with_roots([working_target], true, roots);
    assert!(absolute_policy.contains(&canonical_target));
}

#[test]
fn test_empty_content() {
    let ctx = LintContext::new("", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.content, "");
    assert_eq!(ctx.line_offsets, vec![0]);
    assert_eq!(ctx.offset_to_line_col(0), (1, 1));
    assert_eq!(ctx.lines.len(), 0);
}

#[test]
fn test_single_line() {
    let ctx = LintContext::new("# Hello", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.content, "# Hello");
    assert_eq!(ctx.line_offsets, vec![0]);
    assert_eq!(ctx.offset_to_line_col(0), (1, 1));
    assert_eq!(ctx.offset_to_line_col(3), (1, 4));
}

#[test]
fn parsed_headings_cover_document_and_blockquote_semantics() {
    let content = "# Top\nSetext {#setext}\n=====\n> ## Quoted ## {#quoted}\n>> ### Nested\n#lowercase\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let headings: Vec<_> = ctx.headings().collect();
    assert_eq!(headings.len(), 5);
    assert_eq!(
        headings
            .iter()
            .map(|parsed| (parsed.line_num, parsed.heading.text.as_str(), parsed.blockquote_depth))
            .collect::<Vec<_>>(),
        vec![
            (1, "Top", 0),
            (2, "Setext", 0),
            (4, "Quoted", 1),
            (5, "Nested", 2),
            (6, "lowercase", 0)
        ]
    );

    assert!(headings[1].is_setext());
    assert!(headings[2].is_blockquote());
    assert_eq!(headings[2].heading.custom_id.as_deref(), Some("quoted"));
    assert!(headings[2].heading.has_closing_sequence);
    assert_eq!(headings[2].heading.closing_sequence, "##");
    assert_eq!(headings[2].text_byte_range(content), (5, 11));
    assert!(!headings[4].heading.is_valid);

    let top_level_valid_lines: Vec<_> = ctx.valid_headings().map(|heading| heading.line_num).collect();
    assert_eq!(top_level_valid_lines, vec![1, 2]);
    assert_eq!(ctx.heading_on_line(4).map(|heading| heading.blockquote_depth), Some(1));
    assert!(ctx.heading_on_line(3).is_none());
    assert!(ctx.heading_on_line(0).is_none());
}

#[test]
fn parsed_headings_exclude_non_markdown_regions() {
    let content =
        "---\ntitle: '# Front matter'\n---\n\n```md\n> ## Code\n```\n\n<div>\n> ## Raw HTML\n</div>\n\n# Visible\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let headings: Vec<_> = ctx
        .headings()
        .map(|heading| (heading.line_num, heading.heading.text.as_str()))
        .collect();
    assert_eq!(headings, vec![(13, "Visible")]);
}

#[test]
fn parsed_blockquote_headings_include_markdown_enabled_html() {
    let content = "<div markdown>\n> ## Rendered\n</div>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let headings: Vec<_> = ctx
        .headings()
        .map(|heading| {
            (
                heading.line_num,
                heading.heading.text.as_str(),
                heading.blockquote_depth,
            )
        })
        .collect();
    assert_eq!(headings, vec![(2, "Rendered", 1)]);
}

#[test]
fn parsed_blockquote_headings_respect_mkdocs_snippet_markers() {
    let content = "> # -8<- [start:section]\n> # Actual\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let headings: Vec<_> = ctx
        .headings()
        .map(|heading| (heading.line_num, heading.heading.text.as_str()))
        .collect();
    assert_eq!(headings, vec![(2, "Actual")]);
}

#[test]
fn test_multi_line() {
    let content = "# Title\n\nSecond line\nThird line";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.line_offsets, vec![0, 8, 9, 21]);
    // Test offset to line/col
    assert_eq!(ctx.offset_to_line_col(0), (1, 1)); // start
    assert_eq!(ctx.offset_to_line_col(8), (2, 1)); // start of blank line
    assert_eq!(ctx.offset_to_line_col(9), (3, 1)); // start of 'Second line'
    assert_eq!(ctx.offset_to_line_col(15), (3, 7)); // middle of 'Second line'
    assert_eq!(ctx.offset_to_line_col(21), (4, 1)); // start of 'Third line'
}

#[test]
fn test_line_info() {
    let content = "# Title\n    indented\n\ncode:\n```rust\nfn main() {}\n```";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Test line info
    assert_eq!(ctx.lines.len(), 7);

    // Line 1: "# Title"
    let line1 = &ctx.lines[0];
    assert_eq!(line1.content(ctx.content), "# Title");
    assert_eq!(line1.byte_offset, 0);
    assert_eq!(line1.indent, 0);
    assert!(!line1.is_blank);
    assert!(!line1.in_code_block);
    assert!(line1.list_item.is_none());

    // Line 2: "    indented"
    let line2 = &ctx.lines[1];
    assert_eq!(line2.content(ctx.content), "    indented");
    assert_eq!(line2.byte_offset, 8);
    assert_eq!(line2.indent, 4);
    assert!(!line2.is_blank);

    // Line 3: "" (blank)
    let line3 = &ctx.lines[2];
    assert_eq!(line3.content(ctx.content), "");
    assert!(line3.is_blank);

    // Test helper methods
    assert_eq!(ctx.line_info(1).map(|l| l.indent), Some(0));
    assert_eq!(ctx.line_info(2).map(|l| l.indent), Some(4));
    assert_eq!(ctx.line_info(1).map(|l| l.byte_offset), Some(0));
    assert_eq!(ctx.line_info(2).map(|l| l.byte_offset), Some(8));
}

#[test]
fn test_list_item_detection() {
    let content = "- Unordered item\n  * Nested item\n1. Ordered item\n   2) Nested ordered\n\nNot a list";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Line 1: "- Unordered item"
    let line1 = &ctx.lines[0];
    assert!(line1.list_item.is_some());
    let list1 = line1.list_item.as_ref().unwrap();
    assert_eq!(list1.marker, "-");
    assert!(!list1.is_ordered);
    assert_eq!(list1.marker_column, 0);
    assert_eq!(list1.content_column, 2);

    // Line 2: "  * Nested item"
    let line2 = &ctx.lines[1];
    assert!(line2.list_item.is_some());
    let list2 = line2.list_item.as_ref().unwrap();
    assert_eq!(list2.marker, "*");
    assert_eq!(list2.marker_column, 2);

    // Line 3: "1. Ordered item"
    let line3 = &ctx.lines[2];
    assert!(line3.list_item.is_some());
    let list3 = line3.list_item.as_ref().unwrap();
    assert_eq!(list3.marker, "1.");
    assert!(list3.is_ordered);
    assert_eq!(list3.number, Some(1));

    // Line 6: "Not a list"
    let line6 = &ctx.lines[5];
    assert!(line6.list_item.is_none());
}

#[test]
fn parsed_list_views_preserve_existing_cache_semantics() {
    let fixtures = [
        (
            MarkdownFlavor::Standard,
            "- root\n  1. ordered child\n     continuation\n  * sibling\n\n> 10. quoted\n>     - nested\n\nparagraph\n\t- tabbed\n\n    - indented code\n",
        ),
        (
            MarkdownFlavor::MkDocs,
            "!!! note\n    - item\n        + nested\n\n=== tab\n    1. ordered\n       - child\n",
        ),
        (
            MarkdownFlavor::Kramdown,
            "{::nomarkdown}\n- hidden\n{:/nomarkdown}\n\n- visible\n  - child\n",
        ),
        (MarkdownFlavor::AzureDevOps, ":::python\n- code\n:::\n\n> - visible\n"),
    ];

    for (flavor, content) in fixtures {
        let ctx = LintContext::new(content, flavor, None);
        let raw_items: Vec<_> = ctx
            .lines
            .iter()
            .enumerate()
            .filter_map(|(idx, line)| {
                line.list_item.as_deref().map(|item| {
                    (
                        idx + 1,
                        item.marker.as_str(),
                        item.is_ordered,
                        item.number,
                        item.marker_column,
                        item.content_column,
                        line.blockquote.as_ref().map_or(0, |bq| bq.nesting_level),
                        line.blockquote.as_ref().map_or(0, |bq| bq.prefix.len()),
                    )
                })
            })
            .collect();
        let parsed_items: Vec<_> = ctx
            .list_items()
            .map(|item| {
                (
                    item.line_num(),
                    item.marker(),
                    item.is_ordered(),
                    item.number(),
                    item.marker_column(),
                    item.content_column(),
                    item.blockquote_depth(),
                    item.blockquote_prefix_len(),
                )
            })
            .collect();
        assert_eq!(parsed_items, raw_items, "list item view drifted for {flavor:?}");

        for line_num in 0..=ctx.lines.len() + 1 {
            let raw = line_num
                .checked_sub(1)
                .and_then(|idx| ctx.lines.get(idx))
                .and_then(|line| line.list_item.as_deref());
            let parsed = ctx.list_item_on_line(line_num);
            assert_eq!(parsed.is_some(), raw.is_some(), "lookup drifted at line {line_num}");
            if let (Some(parsed), Some(raw)) = (parsed, raw) {
                assert_eq!(parsed.marker(), raw.marker);
                assert_eq!(
                    parsed.marker_byte_offset(),
                    parsed.line_info().byte_offset + raw.marker_column
                );
            }
        }

        let parsed_blocks = ctx.parsed_list_blocks();
        assert_eq!(parsed_blocks.len(), ctx.list_blocks.len());
        assert_eq!(parsed_blocks.is_empty(), ctx.list_blocks.is_empty());
        for (index, (parsed, raw)) in parsed_blocks.into_iter().zip(&ctx.list_blocks).enumerate() {
            assert_eq!(
                parsed_blocks.get(index).map(ParsedListBlock::start_line),
                Some(raw.start_line)
            );
            assert_eq!(parsed.start_line(), raw.start_line);
            assert_eq!(parsed.end_line(), raw.end_line);
            assert_eq!(parsed.is_ordered(), raw.is_ordered);
            assert_eq!(parsed.marker(), raw.marker.as_deref());
            assert_eq!(parsed.blockquote_prefix(), raw.blockquote_prefix);
            assert_eq!(parsed.nesting_level(), raw.nesting_level);
            assert_eq!(parsed.max_marker_width(), raw.max_marker_width);
            assert_eq!(
                parsed.items().map(ParsedListItem::line_num).collect::<Vec<_>>(),
                raw.item_lines
                    .iter()
                    .copied()
                    .filter(|line| ctx.list_item_on_line(*line).is_some())
                    .collect::<Vec<_>>()
            );
        }
        assert!(parsed_blocks.get(parsed_blocks.len()).is_none());
        assert_eq!(ctx.has_list_items(), !raw_items.is_empty());
        assert_eq!(ctx.has_unordered_list_items(), raw_items.iter().any(|item| !item.2));
    }
}

#[test]
fn parsed_list_block_items_keep_source_order_and_container_depth() {
    let content = "- root\n  - child\n> 1. quoted\n>    + nested\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.list_items()
            .map(|item| (item.line_num(), item.marker(), item.blockquote_depth()))
            .collect::<Vec<_>>(),
        vec![(1, "-", 0), (2, "-", 0), (3, "1.", 1), (4, "+", 1)]
    );
    let block_lines: Vec<Vec<_>> = ctx
        .parsed_list_blocks()
        .into_iter()
        .map(|block| block.items().map(ParsedListItem::line_num).collect())
        .collect();
    assert_eq!(block_lines, vec![vec![1, 2, 3, 4]]);
}

#[test]
fn list_block_item_groups_split_a_block_into_its_lists() {
    // One block holds every level; the groups are the lists inside it, one
    // per run of items at a level, closed by the next shallower item, so the
    // children of `a` and the children of `b` are different lists even though
    // they sit at the same level. A tab and two spaces put `b1x` on column
    // 6, level 3, as six spaces would, and the level skipped on the way down
    // still closes on the way back up.
    let content = "- a\n  - a1\n  - a2\n- b\n  - b1\n\t  - b1x\n  - b2\n- c\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.list_blocks.len(), 1, "{:?}", ctx.list_blocks);
    let groups: Vec<Vec<usize>> = ctx
        .list_blocks
        .iter()
        .flat_map(|block| ctx.list_block_item_groups(block))
        .collect();
    assert_eq!(groups, vec![vec![1, 4, 8], vec![2, 3], vec![5, 7], vec![6]]);
}

#[test]
fn list_block_item_groups_split_on_a_marker_type_change() {
    // Two items at one level are in the same list only when their markers
    // are of one type: the same bullet character, or ordered markers with
    // the same delimiter. A change starts a new list at that level, nested
    // or not, even though the tracker keeps consecutive items of both kinds
    // in one block. Ordered markers may count up freely.
    let groups = |content: &str| -> Vec<Vec<usize>> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(ctx.list_blocks.len(), 1, "{content:?}: {:?}", ctx.list_blocks);
        ctx.list_blocks
            .iter()
            .flat_map(|block| ctx.list_block_item_groups(block))
            .collect()
    };
    for content in [
        "- p\n  - a\n  - b\n  * c\n\n  * d\n",
        "- p\n  - a\n  - b\n  + c\n\n  + d\n",
        "- p\n  1. a\n  2. b\n  1) c\n\n  2) d\n",
        "- p\n  - a\n  - b\n  1. c\n\n  2. d\n",
        "- p\n  1. a\n  2. b\n  - c\n\n  - d\n",
    ] {
        assert_eq!(groups(content), vec![vec![1], vec![2, 3], vec![4, 6]], "{content:?}");
    }
    assert_eq!(groups("- a\n- b\n* c\n\n* d\n"), vec![vec![1, 2], vec![3, 5]]);
    assert_eq!(groups("1. a\n2. b\n1) c\n\n2) d\n"), vec![vec![1, 2], vec![3, 5]]);
    // Controls: one type throughout is one list.
    for content in [
        "- p\n  - a\n  - b\n  - c\n\n  - d\n",
        "- p\n  1. a\n  2. b\n  7. c\n\n  4. d\n",
        "- p\n  1) a\n  2) b\n  3) c\n\n  4) d\n",
    ] {
        assert_eq!(groups(content), vec![vec![1], vec![2, 3, 4, 6]], "{content:?}");
    }
    assert_eq!(groups("- a\n- b\n- c\n\n- d\n"), vec![vec![1, 2, 3, 5]]);
}

#[test]
fn list_block_item_groups_read_ancestry_from_columns() {
    // An item is nested in the open list's latest item when it starts at or
    // right of that item's content column, and a sibling when it starts left
    // of it but not left of the column the list lives in. Siblings may sit at
    // different indents, so a sibling and a nested list can share a marker
    // column and still be different lists; the level is not the column.
    let groups = |content: &str| -> Vec<Vec<usize>> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(ctx.list_blocks.len(), 1, "{content:?}: {:?}", ctx.list_blocks);
        ctx.list_blocks
            .iter()
            .flat_map(|block| ctx.list_block_item_groups(block))
            .collect()
    };
    // The siblings at column 2 sit left of the parent's content column 3, so
    // they are the parent's siblings, not more items of the child list at
    // column 3.
    assert_eq!(
        groups(" - parent\n   - child a\n   - child b\n\n  - sibling a\n\n  - sibling b\n"),
        vec![vec![1, 5, 7], vec![2, 3]]
    );
    // Siblings at indents 0 to 3 are one list, at the top and nested.
    assert_eq!(groups("  - first\n- second\n"), vec![vec![1, 2]]);
    assert_eq!(groups("- a\n - b\n  - c\n   - d\n"), vec![vec![1, 2, 3, 4]]);
    assert_eq!(groups("- a\n    - b\n  - c\n"), vec![vec![1], vec![2, 3]]);
    // Ordered markers of different widths are siblings when the wider one
    // starts left of the content column.
    assert_eq!(groups("9. a\n10. b\n\n11. c\n"), vec![vec![1, 2, 4]]);
    // An item at or right of the content column is nested, however deep.
    assert_eq!(groups("- a\n\n    - b\n"), vec![vec![1], vec![3]]);
    assert_eq!(
        groups("* parent\n    - child a\n    - child b\n  * sibling\n"),
        vec![vec![1], vec![2, 3], vec![4]]
    );
    // A blockquote starting inside the item's content nests its list there;
    // one starting left of the content column ends the list, and an item
    // after it starts another.
    assert_eq!(groups("- a\n  > - b\n  > - c\n- d\n"), vec![vec![1, 4], vec![2, 3]]);
    assert_eq!(groups("> - a\n>   > - b\n> - c\n"), vec![vec![1, 3], vec![2]]);
    assert_eq!(groups("1. a\n> - b\n2. c\n"), vec![vec![1], vec![2], vec![3]]);
    assert_eq!(groups("> - a\n>> - b\n"), vec![vec![1], vec![2]]);
    // Columns inside a blockquote count from the quote's content, so the
    // indent before the `>` and the space after it change nothing: the child
    // items at quote-relative column 2 are nested in a parent whose content
    // starts at 2, and an item at column 1 is the parent's sibling.
    assert_eq!(
        groups(" > - parent\n>   - child a\n>\n>   - child b\n"),
        vec![vec![1], vec![2, 4]]
    );
    assert_eq!(groups("  > - a\n>   - b\n>  - c\n"), vec![vec![1, 3], vec![2]]);
    assert_eq!(groups(">- a\n> - b\n"), vec![vec![1, 2]]);
    assert_eq!(groups("> - a\n>- b\n"), vec![vec![1, 2]]);
    assert_eq!(groups("> - a\n>\t- b\n"), vec![vec![1], vec![2]]);
    assert_eq!(groups("- a\n  > - b\n   > - c\n- e\n"), vec![vec![1, 4], vec![2, 3]]);
}

#[test]
fn list_block_item_groups_close_a_nested_list_on_ancestor_content() {
    // Content that belongs to the parent item, sitting left of the nested
    // items' content column, ends the nested list; a later nested list under
    // the same parent is a different list. A paragraph line directly under
    // an item continues it lazily wherever it starts, so it closes nothing.
    let groups = |content: &str| -> Vec<Vec<usize>> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(ctx.list_blocks.len(), 1, "{content:?}: {:?}", ctx.list_blocks);
        ctx.list_blocks
            .iter()
            .flat_map(|block| ctx.list_block_item_groups(block))
            .collect()
    };
    // Paragraph at the parent's content column between two nested lists.
    assert_eq!(
        groups("- p\n  - a\n  - b\n\n  With:\n\n  - c\n\n  - d\n"),
        vec![vec![1], vec![2, 3], vec![7, 9]]
    );
    // Content indented to the nested items' content column continues them.
    assert_eq!(
        groups("- p\n  - a\n\n    More about a.\n\n  - b\n"),
        vec![vec![1], vec![2, 6]]
    );
    // A lazy continuation line starts wherever it likes.
    assert_eq!(groups("- p\n  - a\nlazy tail of a\n  - b\n"), vec![vec![1], vec![2, 4]]);
    // Ancestor content also closes every list deeper than the one it ends.
    assert_eq!(
        groups("- p\n  - a\n    - a1\n\n  With:\n\n  - b\n    - b1\n"),
        vec![vec![1], vec![2], vec![3], vec![7], vec![8]]
    );
    // Inside a blockquote the same columns are measured after the marker.
    assert_eq!(
        groups("> - p\n>   - a\n>   - b\n>\n>   With:\n>\n>   - c\n>   - d\n"),
        vec![vec![1], vec![2, 3], vec![7, 8]]
    );
    // What interrupts a paragraph needs no blank line before it to end the
    // nested list: an HTML comment or instruction opener, and a blockquote
    // (a `>` with nothing after it opens one under an unquoted list; it is
    // a blank line only to a list at its own quote depth).
    assert_eq!(
        groups("- p\n  - a\n  - b\n  <!-- parent comment -->\n  - c\n\n  - d\n"),
        vec![vec![1], vec![2, 3], vec![5, 7]]
    );
    assert_eq!(
        groups("- p\n  - a\n  - b\n  <?php echo 1; ?>\n  - c\n\n  - d\n"),
        vec![vec![1], vec![2, 3], vec![5, 7]]
    );
    assert_eq!(
        groups("- p\n  - a\n  - b\n  >\n  - c\n\n  - d\n"),
        vec![vec![1], vec![2, 3], vec![5, 7]]
    );
    assert_eq!(
        groups("> - p\n>   - a\n>   - b\n>   >\n>   - c\n>\n>   - d\n"),
        vec![vec![1], vec![2, 3], vec![5, 7]]
    );
    // A comment that opens inside an item's paragraph continues that
    // paragraph across the lines it covers; only an opener at line start
    // is a block.
    assert_eq!(
        groups("- p\n  - a\n  - b <!-- start\n  continues\n  -->\n  - c\n\n  - d\n"),
        vec![vec![1], vec![2, 3, 6, 8]]
    );
    // Lazy continuation needs an open paragraph. A bare `>` inside the
    // nested item and an empty item leave none, so the dedented text after
    // them is the parent's and ends the nested list; a quoted paragraph is
    // one, and the text continues it.
    assert_eq!(
        groups("- p\n  - a\n    >\n  parent\n  - c\n\n  - d\n"),
        vec![vec![1], vec![2], vec![5, 7]]
    );
    assert_eq!(
        groups("- p\n  - a\n  -\n  text\n  - c\n\n  - d\n"),
        vec![vec![1], vec![2, 3], vec![5, 7]]
    );
    assert_eq!(
        groups("- p\n  - a\n    > quote\n  parent\n  - c\n\n  - d\n"),
        vec![vec![1], vec![2, 5, 7]]
    );
    // An item whose own text opens a block (a fence, a heading, a thematic
    // break, an HTML block, indented code) opens no paragraph either, so
    // dedented text after it ends the nested list; a nested item's text is a
    // paragraph and continues.
    for (content, second_list) in [
        ("- p\n  - a\n  - ```\n  more\n  - c\n\n  - d\n", vec![5, 7]),
        ("- p\n  - a\n  - # h\n  more\n  - c\n\n  - d\n", vec![5, 7]),
        ("- p\n  - a\n  - ***\n  more\n  - c\n\n  - d\n", vec![5, 7]),
        ("- p\n  - a\n  -     code\n  more\n  - c\n\n  - d\n", vec![5, 7]),
        ("- p\n  - a\n  - <div>\n  more\n  - c\n\n  - d\n", vec![5, 7]),
        ("- p\n  - a\n  - - # h\n  more\n  - c\n\n  - d\n", vec![5, 7]),
        ("- p\n  - a\n  - -\n  more\n  - c\n\n  - d\n", vec![5, 7]),
        ("- p\n  - a\n  - >\n  more\n  - c\n\n  - d\n", vec![5, 7]),
        ("- p\n  - a\n  - >     code\n  more\n  - c\n\n  - d\n", vec![5, 7]),
        ("- p\n  - a\n  - 1. ```\n  more\n  - c\n\n  - d\n", vec![5, 7]),
        (
            "- p\n  - a\n  - | h |\n    | --- |\n  more\n  - c\n\n  - d\n",
            vec![6, 8],
        ),
        ("- p\n  - a\n  - <!-- c\n  more\n  -->\n  - c\n\n  - d\n", vec![6, 8]),
    ] {
        assert_eq!(groups(content), vec![vec![1], vec![2, 3], second_list], "{content:?}");
    }
    for content in [
        "- p\n  - a\n  - - b1\n  more\n  - c\n\n  - d\n",
        "- p\n  - a\n  -    text\n  more\n  - c\n\n  - d\n",
        "- p\n  - a\n  - #tag\n  more\n  - c\n\n  - d\n",
        "- p\n  - a\n  - ```lang`bad\n  more\n  - c\n\n  - d\n",
        "- p\n  - a\n  - > text\n  more\n  - c\n\n  - d\n",
        "- p\n  - a\n  - - text\n  more\n  - c\n\n  - d\n",
        "- p\n  - a\n  - 1234567890. x\n  more\n  - c\n\n  - d\n",
    ] {
        assert_eq!(groups(content), vec![vec![1], vec![2, 3, 5, 7]], "{content:?}");
    }
}

#[test]
fn list_block_item_groups_end_with_the_item_that_holds_their_blockquote() {
    // A list inside a blockquote inside an item lives in that item: a line
    // whose `>` sits left of the item's content is not in the item, so it
    // ends the item and every list nested in it, however the columns inside
    // the quote compare. A line with fewer quote markers than the list ends
    // its blockquote unless it lazily continues a paragraph, a blank line
    // included, and items in the next blockquote are another list. A blank
    // line or bare `>` at the list's own depth is only the spacing between
    // its items.
    let groups = |content: &str| -> Vec<Vec<usize>> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(ctx.list_blocks.len(), 1, "{content:?}: {:?}", ctx.list_blocks);
        ctx.list_blocks
            .iter()
            .flat_map(|block| ctx.list_block_item_groups(block))
            .collect()
    };
    // The `>` at column 0 leaves `p`; the quote inside `p` measured its
    // items from column 0 too, so the columns alone would read `c` as a
    // sibling of `b2`.
    assert_eq!(
        groups("- p\n  >- b1\n  >\n  >- b2\n>- c\n>- d\n"),
        vec![vec![1], vec![2, 4], vec![5, 6]]
    );
    assert_eq!(groups("- p\n  >- b   x\n>- c   x\n"), vec![vec![1], vec![2], vec![3]]);
    assert_eq!(groups("- p\n  > - b1\n>\n  > - b2\n"), vec![vec![1], vec![2], vec![4]]);
    // A blank line ends the blockquote; a bare `>` at its depth does not.
    assert_eq!(groups("- p\n  > - b1\n\n  > - b2\n"), vec![vec![1], vec![2], vec![4]]);
    assert_eq!(
        groups("- p\n  > - b1\n  > - b2\n\n  > - b3\n"),
        vec![vec![1], vec![2, 3], vec![5]]
    );
    assert_eq!(
        groups("> - a\n>   - b\n>   - c\n\n>   - d\n"),
        vec![vec![1], vec![2, 3], vec![5]]
    );
    assert_eq!(groups("> - a\n>   - b\n>\n>   - c\n"), vec![vec![1], vec![2, 4]]);
    assert_eq!(
        groups("- p\n  > > - x\n  >\n  > > - y\n"),
        vec![vec![1], vec![2], vec![4]]
    );
    assert_eq!(groups("- p\n  > > - x\n  > >\n  > > - y\n"), vec![vec![1], vec![2, 4]]);
    // Paragraph text with fewer markers continues the quoted item lazily.
    assert_eq!(groups("- p\n  > - b1\n  lazy\n  > - b2\n"), vec![vec![1], vec![2, 4]]);
    assert_eq!(groups("- p\n  >- b1\ntext\n  >- b2\n"), vec![vec![1], vec![2, 4]]);
    // After the blockquote, an item at the holding item's content column
    // starts a new list in that item; a deeper `>` left of the quoted item's
    // content starts a blockquote beside it, and its list is nested in `p`.
    assert_eq!(groups("- p\n  > - b1\n  - q\n"), vec![vec![1], vec![2], vec![3]]);
    assert_eq!(groups("- p\n  > - a\n  >> - b\n"), vec![vec![1], vec![2], vec![3]]);
}

#[test]
fn list_block_item_groups_close_the_list_at_the_block_level_too() {
    // A fence, an HTML block or a blank line that ends a blockquote closes
    // the list at the block's own level the way it closes a nested one; the
    // tracker keeps the items on both sides in one block, so a later item
    // starts a new list at that level instead of nesting in the item the
    // line ended, and an item that would nest in the ended item is a
    // sibling of the item after it. A lazy paragraph line and content at
    // the item's content column continue the item.
    let groups = |content: &str| -> Vec<Vec<usize>> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(ctx.list_blocks.len(), 1, "{content:?}: {:?}", ctx.list_blocks);
        ctx.list_blocks
            .iter()
            .flat_map(|block| ctx.list_block_item_groups(block))
            .collect()
    };
    assert_eq!(groups("- p\n```\n```\n- q\n"), vec![vec![1], vec![4]]);
    assert_eq!(groups("- p\n<!-- x -->\n- q\n"), vec![vec![1], vec![3]]);
    assert_eq!(groups("- p\n```\n```\n  - b\n- c\n"), vec![vec![1], vec![4, 5]]);
    assert_eq!(groups("- p\n<!-- x -->\n  - b\n- c\n"), vec![vec![1], vec![3, 4]]);
    assert_eq!(
        groups("- p\n```\n```\n  > - b\n  lazy\n> - c\n"),
        vec![vec![1], vec![4, 6]]
    );
    assert_eq!(groups("> - a\n> - b\n\n> - c\n"), vec![vec![1, 2], vec![4]]);
    // Controls: the item continues through a lazy line and indented content.
    assert_eq!(groups("- p\nlazy\n- q\n"), vec![vec![1, 3]]);
    assert_eq!(groups("- p\n\n  more\n- q\n"), vec![vec![1, 4]]);
    assert_eq!(groups("- p\n\n  ```\n  ```\n- q\n"), vec![vec![1, 5]]);
}

#[test]
fn commonmark_ordered_lists_preserve_raw_membership_and_start_values() {
    let fixtures = [
        (
            MarkdownFlavor::Standard,
            "11. outer\n     1. nested\n     2. nested\n12. outer\n\nparagraph\n\n1. restart\n2. restart\n\n> 7. quoted\n> 8. quoted\n",
        ),
        (
            MarkdownFlavor::MkDocs,
            "1. outer\n    9. nested\n    10. nested\n2. outer\n\n!!! note\n    4. inside\n    5. inside\n",
        ),
        (
            MarkdownFlavor::Pandoc,
            "(@example) example marker\n\n3) parenthesized\n4) parenthesized\n\n1. ordinary\n",
        ),
    ];

    for (flavor, content) in fixtures {
        let ctx = LintContext::new(content, flavor, None);
        assert!(ctx.commonmark_ordered_lists_cache.get().is_none());
        let raw = crate::utils::code_block_utils::CodeBlockUtils::detect_code_blocks_and_spans(content);
        let mut groups: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();

        for line_num in 1..=ctx.lines.len() {
            if ctx.list_item_on_line(line_num).is_some_and(ParsedListItem::is_ordered)
                && let Some(&list_id) = raw.line_to_list.get(&line_num)
            {
                groups.entry(list_id).or_default().push(line_num);
            }
        }

        let mut expected: Vec<_> = groups
            .into_iter()
            .map(|(list_id, mut item_lines)| {
                item_lines.sort_unstable();
                (raw.list_start_values.get(&list_id).copied().unwrap_or(1), item_lines)
            })
            .collect();
        expected.sort_by_key(|(_, lines)| lines.first().copied().unwrap_or(0));

        let lists = ctx.commonmark_ordered_lists();
        assert!(ctx.commonmark_ordered_lists_cache.get().is_some());
        let actual: Vec<_> = lists
            .into_iter()
            .map(|list| {
                (
                    list.start_value(),
                    list.items().map(ParsedListItem::line_num).collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(actual, expected, "CommonMark grouping drifted for {flavor:?}");
        assert_eq!(lists.len(), expected.len());
        assert_eq!(lists.is_empty(), expected.is_empty());
        for (index, expected_list) in expected.iter().enumerate() {
            let list = lists.get(index).expect("index comes from expected list collection");
            assert_eq!(list.start_value(), expected_list.0);
        }
        assert!(lists.get(lists.len()).is_none());
    }
}

#[test]
fn commonmark_ordered_lists_keep_nested_groups_source_ordered() {
    let content = "11. outer\n     1. nested\n     2. nested\n12. outer\n\ntext\n\n1. restart\n2. restart\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let groups: Vec<_> = ctx
        .commonmark_ordered_lists()
        .into_iter()
        .map(|list| {
            (
                list.start_value(),
                list.items().map(ParsedListItem::line_num).collect::<Vec<_>>(),
            )
        })
        .collect();

    assert_eq!(groups, vec![(11, vec![1, 4]), (1, vec![2, 3]), (1, vec![8, 9])]);
}

#[test]
fn test_offset_to_line_col_edge_cases() {
    let content = "a\nb\nc";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    // line_offsets: [0, 2, 4]
    assert_eq!(ctx.offset_to_line_col(0), (1, 1)); // 'a'
    assert_eq!(ctx.offset_to_line_col(1), (1, 2)); // after 'a'
    assert_eq!(ctx.offset_to_line_col(2), (2, 1)); // 'b'
    assert_eq!(ctx.offset_to_line_col(3), (2, 2)); // after 'b'
    assert_eq!(ctx.offset_to_line_col(4), (3, 1)); // 'c'
    assert_eq!(ctx.offset_to_line_col(5), (3, 2)); // after 'c'
}

#[test]
fn test_offset_to_line_col_non_ascii() {
    // Issue #670: the column is a character offset, not a byte offset.
    // "你好x": 你=bytes 0-2, 好=bytes 3-5, x=byte 6.
    let content = "你好x";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.offset_to_line_col(0), (1, 1)); // 你 -> char column 1
    assert_eq!(ctx.offset_to_line_col(3), (1, 2)); // 好 -> char column 2
    assert_eq!(ctx.offset_to_line_col(6), (1, 3)); // x  -> char column 3 (not byte 7)
}

#[test]
fn source_location_queries_define_utf8_crlf_and_eof_boundaries() {
    let content = "éx\r\n中\nlast";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.line_start_byte(1), Some(0));
    assert_eq!(ctx.line_start_byte(2), Some(5));
    assert_eq!(ctx.line_start_byte(3), Some(9));
    assert_eq!(ctx.line_start_byte(0), None);
    assert_eq!(ctx.line_start_byte(4), None);

    assert_eq!(ctx.line_column_byte_range(1, 2), 2..2);
    assert_eq!(ctx.line_column_byte_range_with_length(1, 1, 2), 0..3);
    assert_eq!(ctx.line_text_byte_range(2, 1, 2), 5..8);

    assert_eq!(ctx.line_content_byte_range(1), 0..3);
    assert_eq!(ctx.whole_line_byte_range(1), 0..5);
    assert_eq!(ctx.line_span_byte_range(1, 2), 0..9);
    assert_eq!(ctx.line_content_byte_range(3), 9..13);
    assert_eq!(ctx.whole_line_byte_range(3), 9..13);

    let eof = content.len();
    assert_eq!(ctx.line_column_byte_range(99, 99), eof..eof);
    assert_eq!(ctx.line_content_byte_range(99), eof..eof);
}

#[test]
fn test_mdx_esm_blocks() {
    let content = r##"import {Chart} from './snowfall.js'
export const year = 2023

# Last year's snowfall

In {year}, the snowfall was above average.
It was followed by a warm spring which caused
flood conditions in many of the nearby rivers.

<Chart color="#fcb32c" year={year} />
"##;

    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // Check that lines 1 and 2 are marked as ESM blocks
    assert_eq!(ctx.lines.len(), 10);
    assert!(ctx.lines[0].in_esm_block, "Line 1 (import) should be in_esm_block");
    assert!(ctx.lines[1].in_esm_block, "Line 2 (export) should be in_esm_block");
    assert!(!ctx.lines[2].in_esm_block, "Line 3 (blank) should NOT be in_esm_block");
    assert!(
        !ctx.lines[3].in_esm_block,
        "Line 4 (heading) should NOT be in_esm_block"
    );
    assert!(!ctx.lines[4].in_esm_block, "Line 5 (blank) should NOT be in_esm_block");
    assert!(!ctx.lines[5].in_esm_block, "Line 6 (text) should NOT be in_esm_block");
}

#[test]
fn test_mdx_esm_blocks_not_detected_in_standard_flavor() {
    let content = r#"import {Chart} from './snowfall.js'
export const year = 2023

# Last year's snowfall
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // ESM blocks should NOT be detected in Standard flavor
    assert!(
        !ctx.lines[0].in_esm_block,
        "Line 1 should NOT be in_esm_block in Standard flavor"
    );
    assert!(
        !ctx.lines[1].in_esm_block,
        "Line 2 should NOT be in_esm_block in Standard flavor"
    );
}

#[test]
fn test_blockquote_with_indented_content() {
    // Lines with `>` followed by heavily-indented content should be detected as blockquotes.
    // The content inside the blockquote may also be detected as a code block (which is correct),
    // but for MD046 purposes, we need to know the line is inside a blockquote.
    let content = r#"# Heading

>      -S socket-path
>                    More text
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Line 3 (index 2) should be detected as blockquote
    assert!(
        ctx.lines.get(2).is_some_and(|l| l.blockquote.is_some()),
        "Line 3 should be a blockquote"
    );
    // Line 4 (index 3) should also be blockquote
    assert!(
        ctx.lines.get(3).is_some_and(|l| l.blockquote.is_some()),
        "Line 4 should be a blockquote"
    );

    // Verify blockquote content is correctly parsed
    // Note: spaces_after includes the spaces between `>` and content
    let bq3 = ctx.lines.get(2).unwrap().blockquote.as_ref().unwrap();
    assert_eq!(bq3.content, "-S socket-path");
    assert_eq!(bq3.nesting_level, 1);
    // 6 spaces after the `>` marker
    assert!(bq3.has_multiple_spaces_after_marker);

    let bq4 = ctx.lines.get(3).unwrap().blockquote.as_ref().unwrap();
    assert_eq!(bq4.content, "More text");
    assert_eq!(bq4.nesting_level, 1);
}

#[test]
fn test_blockquote_spaced_nested_markers_are_detected() {
    let content = r#"> > Nested quote content
> > Additional line
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let bq1 = ctx.lines.first().unwrap().blockquote.as_ref().unwrap();
    assert_eq!(bq1.nesting_level, 2);
    assert_eq!(bq1.prefix, "> > ");
    assert_eq!(bq1.content, "Nested quote content");

    let bq2 = ctx.lines.get(1).unwrap().blockquote.as_ref().unwrap();
    assert_eq!(bq2.nesting_level, 2);
    assert_eq!(bq2.prefix, "> > ");
    assert_eq!(bq2.content, "Additional line");
}

#[test]
fn test_ref_def_with_angle_bracket_destination_containing_space() {
    // CommonMark §6.6 admits <...>-form destinations that contain spaces.
    // Without this, the auto-fix output `[id]: <./has space.md>` (which
    // `format_url_destination` chooses for whitespace-bearing URLs) silently
    // disappears from `ctx.reference_definitions()` on the next parse, breaking
    // dedup in MD054 and ref-def discovery in MD053/MD057.
    let content = "[docs]: <./has space.md>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(
        ctx.reference_definitions().len(),
        1,
        "angle-bracket destination must parse"
    );
    assert_eq!(ctx.reference_definitions()[0].id, "docs");
    assert_eq!(
        ctx.reference_definitions()[0].url,
        "./has space.md",
        "URL should be the destination content, not the angle-bracketed form"
    );
    assert_eq!(ctx.reference_definitions()[0].title, None);
}

#[test]
fn test_ref_def_with_angle_bracket_destination_and_title() {
    // The optional title still parses after an angle-bracket destination.
    let content = "[docs]: <./has space.md> \"Help me\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(ctx.reference_definitions()[0].url, "./has space.md");
    assert_eq!(ctx.reference_definitions()[0].title.as_deref(), Some("Help me"));
}

#[test]
fn test_ref_def_multiline_title_on_next_line() {
    // CommonMark §4.7: a reference definition's title may sit on the line
    // immediately after the destination. The whole definition - including the
    // title line - is one reference definition.
    let content = "[ref]: https://example.com\n  \"the title\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(ctx.reference_definitions()[0].title.as_deref(), Some("the title"));
    // The byte range and is_in_reference_def cover the continuation title line.
    let quote = content.find('"').unwrap();
    assert!(
        ctx.is_in_reference_def(quote),
        "title line should be inside the ref def"
    );
    // The title byte range spans the delimiters exactly, and byte_end reaches
    // the end of the continuation line (the byte before its newline).
    let def = &ctx.reference_definitions()[0];
    assert_eq!(
        def.title_byte_start,
        Some(quote),
        "title_byte_start is the opening quote"
    );
    assert_eq!(
        def.title_byte_end,
        Some(quote + "\"the title\"".len()),
        "title_byte_end is one past the closing quote"
    );
    assert_eq!(
        def.byte_end,
        content.trim_end_matches('\n').len(),
        "byte_end reaches end of title line"
    );
}

#[test]
fn test_ref_def_single_quote_and_paren_title_on_next_line() {
    for content in [
        "[ref]: https://example.com\n  'the title'\n",
        "[ref]: https://example.com\n  (the title)\n",
    ] {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(ctx.reference_definitions().len(), 1, "input: {content:?}");
        assert_eq!(
            ctx.reference_definitions()[0].title.as_deref(),
            Some("the title"),
            "input: {content:?}"
        );
    }
}

#[test]
fn test_ref_def_blank_line_breaks_multiline_title() {
    // A blank line between the destination and a quoted line means the quoted
    // line is a separate paragraph, not the definition's title.
    let content = "[ref]: https://example.com\n\n\"not a title\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(ctx.reference_definitions()[0].title, None);
    let quote = content.find('"').unwrap();
    assert!(
        !ctx.is_in_reference_def(quote),
        "a blank-separated quoted line is not part of the ref def"
    );
}

#[test]
fn test_ref_def_non_title_next_line_not_consumed() {
    // The line after the destination is only a title if it is *only* a quoted
    // or parenthesised title; ordinary prose is a separate paragraph.
    let content = "[ref]: https://example.com\nordinary paragraph text\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(ctx.reference_definitions()[0].title, None);
    let para = content.find("ordinary").unwrap();
    assert!(
        !ctx.is_in_reference_def(para),
        "a following paragraph is not part of the ref def"
    );
}

#[test]
fn test_ref_def_inline_title_unaffected_by_lookahead() {
    // A definition that already has its title on the destination line is
    // unchanged; the next line is not pulled in.
    let content = "[ref]: https://example.com \"inline\"\n\"separate\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(ctx.reference_definitions()[0].title.as_deref(), Some("inline"));
    let sep = content.find("\"separate\"").unwrap();
    assert!(
        !ctx.is_in_reference_def(sep),
        "the inline-title def must not absorb the next line"
    );
}

#[test]
fn test_ref_def_multiline_title_in_blockquote() {
    // A reference definition inside a blockquote may also carry its title on
    // the following (still blockquoted) line.
    let content = "> [ref]: https://example.com\n>  \"the title\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(ctx.reference_definitions()[0].title.as_deref(), Some("the title"));
}

#[test]
fn test_ref_def_paren_title_with_escaped_parens() {
    // CommonMark §4.7 paren-form titles may contain `(`/`)` only when
    // backslash-escaped. Both pulldown-cmark and the rumdl ref-def regex
    // unescape the captured title (per CommonMark §6.1) so downstream rules
    // see the same value regardless of which path produced it.
    let content = "[docs]: https://example.com (title \\(x\\))\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(ctx.reference_definitions()[0].url, "https://example.com");
    assert_eq!(
        ctx.reference_definitions()[0].title.as_deref(),
        Some("title (x)"),
        "title must be unescaped to match pulldown-cmark's parsed value"
    );
}

#[test]
fn test_mkdocs_admonition_link_with_paren_title() {
    // pulldown-cmark treats indented MkDocs admonition content as a code block,
    // so the inline link is recovered by the regex fallback in
    // `parse_links_images_pulldown`. The fallback must recognize all three
    // CommonMark §6.7 title delimiter forms — including `(...)` — otherwise
    // a link like `[doc](url (title))` is parsed with title=None and MD054
    // auto-fix silently strips the title when rewriting the link.
    let content = "!!! note\n    See [doc](https://example.com (paren title)) here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let link = ctx
        .links()
        .iter()
        .find(|l| l.url == "https://example.com")
        .expect("MkDocs fallback must surface the link");
    assert_eq!(
        link.title.as_deref(),
        Some("paren title"),
        "paren-form title must be captured by the MkDocs link fallback"
    );
}

#[test]
fn test_mkdocs_admonition_image_with_paren_title() {
    // Mirror of the link test for images.
    let content = "!!! note\n    See ![alt](https://example.com/x.png (paren title)) here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let img = ctx
        .images()
        .iter()
        .find(|i| i.url == "https://example.com/x.png")
        .expect("MkDocs fallback must surface the image");
    assert_eq!(
        img.title.as_deref(),
        Some("paren title"),
        "paren-form title must be captured by the MkDocs image fallback"
    );
}

#[test]
fn parsed_link_queries_preserve_document_order_and_half_open_ranges() {
    let content = "[one](a)\n![pic](b) and [two](c)\n\n[Docs]: ./guide.md\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(
        ctx.links_on_line(1)
            .iter()
            .map(|link| link.text.as_ref())
            .collect::<Vec<_>>(),
        ["one"]
    );
    assert_eq!(
        ctx.links_on_line(2)
            .iter()
            .map(|link| link.text.as_ref())
            .collect::<Vec<_>>(),
        ["two"]
    );
    assert!(ctx.links_on_line(3).is_empty());
    assert_eq!(
        ctx.images_on_line(2)
            .iter()
            .map(|image| image.alt_text.as_ref())
            .collect::<Vec<_>>(),
        ["pic"]
    );

    let first_link_start = content.find("[one]").unwrap();
    let second_link_start = content.find("[two]").unwrap();
    let image_start = content.find("![pic]").unwrap();

    let first_link = ctx.link_starting_at(first_link_start).unwrap();
    assert!(std::ptr::eq(ctx.link_containing(first_link_start).unwrap(), first_link));
    assert!(std::ptr::eq(
        ctx.link_containing(first_link.byte_end - 1).unwrap(),
        first_link
    ));
    assert!(ctx.link_containing(first_link.byte_end).is_none());
    assert_eq!(ctx.links_starting_before_or_at(second_link_start).len(), 2);

    let image = ctx.image_starting_at(image_start).unwrap();
    assert!(std::ptr::eq(ctx.image_containing(image_start).unwrap(), image));
    assert!(std::ptr::eq(ctx.image_containing(image.byte_end - 1).unwrap(), image));
    assert!(ctx.image_containing(image.byte_end).is_none());

    let definition = ctx.reference_definition("DOCS").unwrap();
    assert_eq!(definition.url, "./guide.md");
    assert_eq!(ctx.get_reference_url("docs"), Some("./guide.md"));
}

#[test]
fn parsed_links_remain_ordered_after_regex_fallbacks() {
    // Undefined reference links are appended after the pulldown-cmark pass.
    // Queries rely on the final collections being restored to document order.
    let content = "\
# Document

See [undefined-ref] for details.

Some text with [another-undef-ref] here.

Short text [link](https://example.com/a-very-long-destination).
";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(
        ctx.links()
            .windows(2)
            .all(|pair| { (pair[0].line, pair[0].byte_offset) <= (pair[1].line, pair[1].byte_offset) })
    );
    assert!(
        ctx.images()
            .windows(2)
            .all(|pair| { (pair[0].line, pair[0].byte_offset) <= (pair[1].line, pair[1].byte_offset) })
    );
}

#[test]
fn test_wiki_embed_has_no_alt_text() {
    // A wiki embed has no alt-text slot: `![[note]]` transcludes the target and
    // the pipe in `![[image.png|300]]` sets the rendered dimensions. Reading the
    // pipe portion as alt text made MD045 accept `![[a.png|300]]` and report
    // `![[a.png]]`, neither of which the author can act on.
    for (content, url) in [
        ("![[image.png]]\n", "image.png"),
        ("![[image.png|300]]\n", "image.png"),
        ("![[image.png|Some description]]\n", "image.png"),
        ("![[subfolder/image.png|640x480]]\n", "subfolder/image.png"),
    ] {
        for flavor in [MarkdownFlavor::Obsidian, MarkdownFlavor::Standard] {
            let ctx = LintContext::new(content, flavor, None);
            assert_eq!(ctx.images().len(), 1, "{flavor:?}: {content:?}");
            assert_eq!(ctx.images()[0].url, url, "{flavor:?}: {content:?}");
            assert_eq!(
                ctx.images()[0].alt_text,
                "",
                "{flavor:?}: {content:?} has no alt-text slot"
            );
        }
    }
}

#[test]
fn test_wikilink_text_survives_a_nested_image() {
    // A wikilink is the only link whose text comes from the parse events rather
    // than from a source byte scan, so the nested image's events must leave the
    // text accumulated so far in place.
    let content = "[[Target|Display ![alt](x.png) more]]\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Obsidian, None);
    let link = ctx
        .links()
        .iter()
        .find(|l| l.url == "Target")
        .expect("the wikilink must be parsed");
    assert_eq!(link.text, "Display alt more");
}

#[test]
fn test_ref_def_angle_bracket_destination_with_escaped_brackets() {
    // CommonMark §6.6 angle-bracket destinations admit `\<` and `\>` so the
    // round-trip from `format_url_destination` (which emits `<a\<b\>c>` when
    // a URL contains `<` or `>`) is recovered on the next parse instead of
    // silently dropping the def out of `ctx.reference_definitions()`.
    let content = "[id]: <a\\<b\\>c>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(
        ctx.reference_definitions().len(),
        1,
        "escaped angle-bracket destination must round-trip through the regex"
    );
    assert_eq!(ctx.reference_definitions()[0].id, "id");
    assert_eq!(ctx.reference_definitions()[0].title, None);
}

#[test]
fn test_ref_def_label_with_escaped_closing_bracket() {
    // CommonMark ends a link label at the first right bracket that is *not*
    // backslash-escaped, so `ref1\[\]` and `ref2\]` are single labels. A label
    // scan that stops at any `]` drops these definitions entirely, which makes
    // MD034 lint the destination as prose and hides them from MD053.
    //
    // The stored id keeps its backslashes: label normalization case-folds and
    // collapses whitespace but does not unescape, and pulldown-cmark reports
    // the raw id, so both sides must agree for reference matching to work.
    let content = "[ref1\\[\\]]: https://example.com/ref1\n[ref2\\]]: https://example.com/ref2\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let ids: Vec<&str> = ctx.reference_definitions().iter().map(|d| d.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["ref1\\[\\]", "ref2\\]"],
        "an escaped `]` must not terminate the label scan"
    );
    assert_eq!(ctx.reference_definitions()[0].url, "https://example.com/ref1");
    assert_eq!(ctx.reference_definitions()[1].url, "https://example.com/ref2");
}

#[test]
fn test_ref_def_double_quoted_title_with_escaped_quote() {
    // Title delimiter `"` may appear inside the title only when backslash-escaped;
    // `format_title` produces this form whenever the unescaped title contains `"`,
    // so the regex must accept it or the freshly generated def disappears from
    // the next pass and MD053/MD057/dedup all break. The captured title is
    // unescaped (CommonMark §6.1) so it matches pulldown-cmark's parsed value.
    let content = "[id]: https://example.com \"he said \\\"hi\\\"\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(ctx.reference_definitions()[0].url, "https://example.com");
    assert_eq!(
        ctx.reference_definitions()[0].title.as_deref(),
        Some("he said \"hi\""),
        "title must be unescaped to match pulldown-cmark's parsed value"
    );
}

#[test]
fn test_ref_def_single_quoted_title_with_escaped_quote() {
    let content = "[id]: https://example.com 'it\\'s fine'\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(ctx.reference_definitions()[0].url, "https://example.com");
    assert_eq!(
        ctx.reference_definitions()[0].title.as_deref(),
        Some("it's fine"),
        "title must be unescaped to match pulldown-cmark's parsed value"
    );
}

#[test]
fn test_ref_def_url_unescapes_backslash_escapes() {
    // CommonMark §6.1: a backslash before any ASCII punctuation character
    // produces the literal character; the backslash itself is removed. The
    // rumdl regex fallback must apply this transform so `ctx.reference_definitions()[i].url`
    // matches what pulldown-cmark exposes via `Tag::Link`/`Tag::Image`. Without
    // this, MD053/MD054/MD057 would see `https://e.com/path\(1\)` while the
    // parser sees `https://e.com/path(1)`, and any rule that copies the value
    // back into the document would corrupt it.
    let content = "[id]: https://e.com/path\\(1\\)\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(
        ctx.reference_definitions()[0].url,
        "https://e.com/path(1)",
        "URL must be unescaped per CommonMark §6.1"
    );
}

#[test]
fn test_ref_def_unescape_preserves_non_punctuation_backslash() {
    // CommonMark §6.1 explicitly limits the escape to ASCII punctuation. A
    // backslash followed by a letter, digit, or whitespace is preserved
    // verbatim (the backslash stays in the output). Verifying this guards
    // against an over-eager unescape that would silently drop backslashes
    // from URL paths and titles.
    let content = "[id]: https://e.com/p\\ath \"a\\b c\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(
        ctx.reference_definitions()[0].url,
        "https://e.com/p\\ath",
        "backslash before non-punctuation must remain in URL"
    );
    assert_eq!(
        ctx.reference_definitions()[0].title.as_deref(),
        Some("a\\b c"),
        "backslash before non-punctuation must remain in title"
    );
}

#[test]
fn test_footnote_definitions_not_parsed_as_reference_defs() {
    // Footnote definitions use [^id]: syntax and should NOT be parsed as reference definitions
    let content = r#"# Title

A footnote[^1].

[^1]: This is the footnote content.

[^note]: Another footnote with [link](https://example.com).

[regular]: ./path.md "A real reference definition"
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should only have one reference definition (the regular one)
    assert_eq!(
        ctx.reference_definitions().len(),
        1,
        "Footnotes should not be parsed as reference definitions"
    );

    // The only reference def should be the regular one
    assert_eq!(ctx.reference_definitions()[0].id, "regular");
    assert_eq!(ctx.reference_definitions()[0].url, "./path.md");
    assert_eq!(
        ctx.reference_definitions()[0].title,
        Some("A real reference definition".to_string())
    );
}

#[test]
fn test_footnote_with_inline_link_not_misidentified() {
    // Regression test for issue #286: footnote containing an inline link
    // was incorrectly parsed as a reference definition with URL "[link](url)"
    let content = r#"# Title

A footnote[^1].

[^1]: [link](https://www.google.com).
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should have no reference definitions
    assert!(
        ctx.reference_definitions().is_empty(),
        "Footnote with inline link should not create a reference definition"
    );
}

#[test]
fn test_various_footnote_formats_excluded() {
    // Test various footnote ID formats are all excluded
    let content = r#"[^1]: Numeric footnote
[^note]: Named footnote
[^a]: Single char footnote
[^long-footnote-name]: Long named footnote
[^123abc]: Mixed alphanumeric

[ref1]: ./file1.md
[ref2]: ./file2.md
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should only have the two regular reference definitions
    assert_eq!(
        ctx.reference_definitions().len(),
        2,
        "Only regular reference definitions should be parsed"
    );

    let ids: Vec<&str> = ctx.reference_definitions().iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"ref1"));
    assert!(ids.contains(&"ref2"));
    assert!(!ids.iter().any(|id| id.starts_with('^')));
}

// =========================================================================
// Tests for has_char and char_count methods
// =========================================================================

#[test]
fn test_has_char_tracked_characters() {
    // Test all 12 tracked characters
    let content =
        "# Heading\n* list item\n_emphasis_ and -hyphen-\n+ plus\n> quote\n| table |\n[link]\n`code`\n<html>\n!image";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // All tracked characters should be detected
    assert!(ctx.has_char('#'), "Should detect hash");
    assert!(ctx.has_char('*'), "Should detect asterisk");
    assert!(ctx.has_char('_'), "Should detect underscore");
    assert!(ctx.has_char('-'), "Should detect hyphen");
    assert!(ctx.has_char('+'), "Should detect plus");
    assert!(ctx.has_char('>'), "Should detect gt");
    assert!(ctx.has_char('|'), "Should detect pipe");
    assert!(ctx.has_char('['), "Should detect bracket");
    assert!(ctx.has_char('`'), "Should detect backtick");
    assert!(ctx.has_char('<'), "Should detect lt");
    assert!(ctx.has_char('!'), "Should detect exclamation");
    assert!(ctx.has_char('\n'), "Should detect newline");
}

#[test]
fn test_has_char_absent_characters() {
    let content = "Simple text without special chars";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // None of the tracked characters should be present
    assert!(!ctx.has_char('#'), "Should not detect hash");
    assert!(!ctx.has_char('*'), "Should not detect asterisk");
    assert!(!ctx.has_char('_'), "Should not detect underscore");
    assert!(!ctx.has_char('-'), "Should not detect hyphen");
    assert!(!ctx.has_char('+'), "Should not detect plus");
    assert!(!ctx.has_char('>'), "Should not detect gt");
    assert!(!ctx.has_char('|'), "Should not detect pipe");
    assert!(!ctx.has_char('['), "Should not detect bracket");
    assert!(!ctx.has_char('`'), "Should not detect backtick");
    assert!(!ctx.has_char('<'), "Should not detect lt");
    assert!(!ctx.has_char('!'), "Should not detect exclamation");
    // Note: single line content has no newlines
    assert!(!ctx.has_char('\n'), "Should not detect newline in single line");
}

#[test]
fn test_has_char_fallback_for_untracked() {
    let content = "Text with @mention and $dollar and %percent";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Untracked characters should fall back to content.contains()
    assert!(ctx.has_char('@'), "Should detect @ via fallback");
    assert!(ctx.has_char('$'), "Should detect $ via fallback");
    assert!(ctx.has_char('%'), "Should detect % via fallback");
    assert!(!ctx.has_char('^'), "Should not detect absent ^ via fallback");
}

#[test]
fn test_char_count_tracked_characters() {
    let content =
        "## Heading ##\n***bold***\n__emphasis__\n---\n+++\n>> nested\n|| table ||\n[[link]]\n``code``\n<<html>>\n!!";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Count each tracked character
    assert_eq!(ctx.char_count('#'), 4, "Should count 4 hashes");
    assert_eq!(ctx.char_count('*'), 6, "Should count 6 asterisks");
    assert_eq!(ctx.char_count('_'), 4, "Should count 4 underscores");
    assert_eq!(ctx.char_count('-'), 3, "Should count 3 hyphens");
    assert_eq!(ctx.char_count('+'), 3, "Should count 3 pluses");
    assert_eq!(ctx.char_count('>'), 4, "Should count 4 gt (2 nested + 2 in <<html>>)");
    assert_eq!(ctx.char_count('|'), 4, "Should count 4 pipes");
    assert_eq!(ctx.char_count('['), 2, "Should count 2 brackets");
    assert_eq!(ctx.char_count('`'), 4, "Should count 4 backticks");
    assert_eq!(ctx.char_count('<'), 2, "Should count 2 lt");
    assert_eq!(ctx.char_count('!'), 2, "Should count 2 exclamations");
    assert_eq!(ctx.char_count('\n'), 10, "Should count 10 newlines");
}

#[test]
fn test_char_count_zero_for_absent() {
    let content = "Plain text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.char_count('#'), 0);
    assert_eq!(ctx.char_count('*'), 0);
    assert_eq!(ctx.char_count('_'), 0);
    assert_eq!(ctx.char_count('\n'), 0);
}

#[test]
fn test_char_count_fallback_for_untracked() {
    let content = "@@@ $$ %%%";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.char_count('@'), 3, "Should count 3 @ via fallback");
    assert_eq!(ctx.char_count('$'), 2, "Should count 2 $ via fallback");
    assert_eq!(ctx.char_count('%'), 3, "Should count 3 % via fallback");
    assert_eq!(ctx.char_count('^'), 0, "Should count 0 for absent char");
}

#[test]
fn test_char_count_empty_content() {
    let ctx = LintContext::new("", MarkdownFlavor::Standard, None);

    assert_eq!(ctx.char_count('#'), 0);
    assert_eq!(ctx.char_count('*'), 0);
    assert_eq!(ctx.char_count('@'), 0);
    assert!(!ctx.has_char('#'));
    assert!(!ctx.has_char('@'));
}

// =========================================================================
// Tests for is_in_html_tag method
// =========================================================================

#[test]
fn test_is_in_html_tag_simple() {
    let content = "<div>content</div>";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Inside opening tag
    assert!(ctx.is_in_html_tag(0), "Position 0 (<) should be in tag");
    assert!(ctx.is_in_html_tag(1), "Position 1 (d) should be in tag");
    assert!(ctx.is_in_html_tag(4), "Position 4 (>) should be in tag");

    // Outside tag (in content)
    assert!(!ctx.is_in_html_tag(5), "Position 5 (c) should not be in tag");
    assert!(!ctx.is_in_html_tag(10), "Position 10 (t) should not be in tag");

    // Inside closing tag
    assert!(ctx.is_in_html_tag(12), "Position 12 (<) should be in tag");
    assert!(ctx.is_in_html_tag(17), "Position 17 (>) should be in tag");
}

#[test]
fn test_is_in_html_tag_self_closing() {
    let content = "Text <br/> more text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Before tag
    assert!(!ctx.is_in_html_tag(0), "Position 0 should not be in tag");
    assert!(!ctx.is_in_html_tag(4), "Position 4 (space) should not be in tag");

    // Inside self-closing tag
    assert!(ctx.is_in_html_tag(5), "Position 5 (<) should be in tag");
    assert!(ctx.is_in_html_tag(8), "Position 8 (/) should be in tag");
    assert!(ctx.is_in_html_tag(9), "Position 9 (>) should be in tag");

    // After tag
    assert!(!ctx.is_in_html_tag(10), "Position 10 (space) should not be in tag");
}

#[test]
fn test_is_in_html_tag_with_attributes() {
    let content = r#"<a href="url" class="link">text</a>"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // All positions inside opening tag with attributes
    assert!(ctx.is_in_html_tag(0), "Start of tag");
    assert!(ctx.is_in_html_tag(10), "Inside href attribute");
    assert!(ctx.is_in_html_tag(20), "Inside class attribute");
    assert!(ctx.is_in_html_tag(26), "End of opening tag");

    // Content between tags
    assert!(!ctx.is_in_html_tag(27), "Start of content");
    assert!(!ctx.is_in_html_tag(30), "End of content");

    // Closing tag
    assert!(ctx.is_in_html_tag(31), "Start of closing tag");
}

#[test]
fn test_is_in_html_tag_multiline() {
    let content = "<div\n  class=\"test\"\n>\ncontent\n</div>";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Opening tag spans multiple lines
    assert!(ctx.is_in_html_tag(0), "Start of multiline tag");
    assert!(ctx.is_in_html_tag(5), "After first newline in tag");
    assert!(ctx.is_in_html_tag(15), "Inside attribute");

    // After closing > of opening tag
    let closing_bracket_pos = content.find(">\n").unwrap();
    assert!(!ctx.is_in_html_tag(closing_bracket_pos + 2), "Content after tag");
}

#[test]
fn test_is_in_html_tag_with_url_attributes() {
    // Tags with URLs in attributes contain '/' which must not be treated as self-closing
    let content = r#"<input name="fields[url]" value="https://www.example.com">"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let tags = ctx.html_tags();

    assert_eq!(tags.len(), 1, "Should detect one HTML tag");
    assert_eq!(tags[0].tag_name, "input");
    assert!(!tags[0].is_self_closing);
    assert!(ctx.is_in_html_tag(35), "URL position should be inside HTML tag");
}

#[test]
fn test_is_in_html_tag_self_closing_with_slash() {
    let content = "<br />";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let tags = ctx.html_tags();

    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].tag_name, "br");
    assert!(tags[0].is_self_closing);
}

#[test]
fn test_html_tags_respect_backslash_escaped_openers() {
    // CommonMark §6.1: an odd number of backslashes escapes the `<`, while an
    // even number escapes in pairs and leaves it active as markup.
    for content in [r"\<x-keyboard>", r"\\\<x-keyboard>"] {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert!(ctx.html_tags().is_empty(), "escaped opener in {content:?}");
    }

    for content in [r"<x-keyboard>", r"\\<x-keyboard>", r"\\\\<x-keyboard>"] {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let tags = ctx.html_tags();
        assert_eq!(tags.len(), 1, "active opener in {content:?}");
        assert_eq!(tags[0].tag_name, "x-keyboard");
    }
}

#[test]
fn test_is_in_html_tag_nested_angle_brackets() {
    // Hugo shortcodes: <a href="{{< ref ... >}}"> contain nested '<'
    let content = r#"<a href="{{< ref "../common-parameters" >}}">"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let tags = ctx.html_tags();

    // The regex handles nested '<' by matching the shortest valid tag
    assert!(!tags.is_empty(), "Should detect at least one tag fragment");
}

#[test]
fn test_html_tag_window_ends_inside_multibyte_char() {
    // Issue #757: an unterminated HTML-like tag (`<a...` with no closing `>`) whose
    // 4096-byte search window ends inside a multi-byte UTF-8 character must not panic.
    // `<a` (2 bytes) + 4093 * 'a' (4093 bytes) puts byte 4096 inside the following
    // '的' (bytes 4095..4098), which previously panicked when slicing the window.
    let content = format!("<a{}的", "a".repeat(4093));
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    let _ = ctx.html_tags(); // must not panic

    // A closing '>' beyond the window boundary, still landing inside a multi-byte char.
    let content2 = format!("<a {}的>", "b".repeat(5000));
    let ctx2 = LintContext::new(&content2, MarkdownFlavor::Standard, None);
    let _ = ctx2.html_tags(); // must not panic
}

#[test]
fn test_is_in_html_tag_no_tags() {
    let content = "Plain text without any HTML";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // No position should be in an HTML tag
    for i in 0..content.len() {
        assert!(!ctx.is_in_html_tag(i), "Position {i} should not be in tag");
    }
}

// =========================================================================
// Tests for is_in_jinja_range method
// =========================================================================

#[test]
fn test_is_in_jinja_range_expression() {
    let content = "Hello {{ name }}!";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Before Jinja
    assert!(!ctx.is_in_jinja_range(0), "H should not be in Jinja");
    assert!(!ctx.is_in_jinja_range(5), "Space before Jinja should not be in Jinja");

    // Inside Jinja expression (positions 6-15 for "{{ name }}")
    assert!(ctx.is_in_jinja_range(6), "First brace should be in Jinja");
    assert!(ctx.is_in_jinja_range(7), "Second brace should be in Jinja");
    assert!(ctx.is_in_jinja_range(10), "name should be in Jinja");
    assert!(ctx.is_in_jinja_range(14), "Closing brace should be in Jinja");
    assert!(ctx.is_in_jinja_range(15), "Second closing brace should be in Jinja");

    // After Jinja
    assert!(!ctx.is_in_jinja_range(16), "! should not be in Jinja");
}

#[test]
fn test_is_in_jinja_range_statement() {
    let content = "{% if condition %}content{% endif %}";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Inside opening statement
    assert!(ctx.is_in_jinja_range(0), "Start of Jinja statement");
    assert!(ctx.is_in_jinja_range(5), "condition should be in Jinja");
    assert!(ctx.is_in_jinja_range(17), "End of opening statement");

    // Content between
    assert!(!ctx.is_in_jinja_range(18), "content should not be in Jinja");

    // Inside closing statement
    assert!(ctx.is_in_jinja_range(25), "Start of endif");
    assert!(ctx.is_in_jinja_range(32), "endif should be in Jinja");
}

#[test]
fn test_is_in_jinja_range_multiple() {
    let content = "{{ a }} and {{ b }}";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // First Jinja expression
    assert!(ctx.is_in_jinja_range(0));
    assert!(ctx.is_in_jinja_range(3));
    assert!(ctx.is_in_jinja_range(6));

    // Between expressions
    assert!(!ctx.is_in_jinja_range(8));
    assert!(!ctx.is_in_jinja_range(11));

    // Second Jinja expression
    assert!(ctx.is_in_jinja_range(12));
    assert!(ctx.is_in_jinja_range(15));
    assert!(ctx.is_in_jinja_range(18));
}

#[test]
fn test_is_in_jinja_range_no_jinja() {
    let content = "Plain text with single braces but not Jinja";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // No position should be in Jinja
    for i in 0..content.len() {
        assert!(!ctx.is_in_jinja_range(i), "Position {i} should not be in Jinja");
    }
}

// =========================================================================
// Tests for is_in_link_title method
// =========================================================================

#[test]
fn test_is_in_link_title_with_title() {
    let content = r#"[ref]: https://example.com "Title text"

Some content."#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Verify we have a reference def with title
    assert_eq!(ctx.reference_definitions().len(), 1);
    let def = &ctx.reference_definitions()[0];
    assert!(def.title_byte_start.is_some());
    assert!(def.title_byte_end.is_some());

    let title_start = def.title_byte_start.unwrap();
    let title_end = def.title_byte_end.unwrap();

    // Before title (in URL)
    assert!(!ctx.is_in_link_title(10), "URL should not be in title");

    // Inside title
    assert!(ctx.is_in_link_title(title_start), "Title start should be in title");
    assert!(
        ctx.is_in_link_title(title_start + 5),
        "Middle of title should be in title"
    );
    assert!(ctx.is_in_link_title(title_end - 1), "End of title should be in title");

    // After title
    assert!(
        !ctx.is_in_link_title(title_end),
        "After title end should not be in title"
    );
}

#[test]
fn test_is_in_link_title_without_title() {
    let content = "[ref]: https://example.com\n\nSome content.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Reference def without title
    assert_eq!(ctx.reference_definitions().len(), 1);
    let def = &ctx.reference_definitions()[0];
    assert!(def.title_byte_start.is_none());
    assert!(def.title_byte_end.is_none());

    // No position should be in a title
    for i in 0..content.len() {
        assert!(!ctx.is_in_link_title(i), "Position {i} should not be in title");
    }
}

#[test]
fn test_is_in_link_title_multiple_refs() {
    let content = r#"[ref1]: /url1 "Title One"
[ref2]: /url2
[ref3]: /url3 "Title Three"
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should have 3 reference defs
    assert_eq!(ctx.reference_definitions().len(), 3);

    // ref1 has title
    let ref1 = ctx.reference_definitions().iter().find(|r| r.id == "ref1").unwrap();
    assert!(ref1.title_byte_start.is_some());

    // ref2 has no title
    let ref2 = ctx.reference_definitions().iter().find(|r| r.id == "ref2").unwrap();
    assert!(ref2.title_byte_start.is_none());

    // ref3 has title
    let ref3 = ctx.reference_definitions().iter().find(|r| r.id == "ref3").unwrap();
    assert!(ref3.title_byte_start.is_some());

    // Check positions in ref1's title
    if let (Some(start), Some(end)) = (ref1.title_byte_start, ref1.title_byte_end) {
        assert!(ctx.is_in_link_title(start + 1));
        assert!(!ctx.is_in_link_title(end + 5));
    }

    // Check positions in ref3's title
    if let (Some(start), Some(_end)) = (ref3.title_byte_start, ref3.title_byte_end) {
        assert!(ctx.is_in_link_title(start + 1));
    }
}

#[test]
fn test_is_in_link_title_single_quotes() {
    let content = "[ref]: /url 'Single quoted title'\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.reference_definitions().len(), 1);
    let def = &ctx.reference_definitions()[0];

    if let (Some(start), Some(end)) = (def.title_byte_start, def.title_byte_end) {
        assert!(ctx.is_in_link_title(start));
        assert!(ctx.is_in_link_title(start + 5));
        assert!(!ctx.is_in_link_title(end));
    }
}

#[test]
fn test_is_in_link_title_parentheses() {
    // Note: The reference def parser may not support parenthesized titles
    // This test verifies the is_in_link_title method works when titles exist
    let content = "[ref]: /url (Parenthesized title)\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Parser behavior: may or may not parse parenthesized titles
    // We test that is_in_link_title correctly reflects whatever was parsed
    if ctx.reference_definitions().is_empty() {
        // Parser didn't recognize this as a reference def
        for i in 0..content.len() {
            assert!(!ctx.is_in_link_title(i));
        }
    } else {
        let def = &ctx.reference_definitions()[0];
        if let (Some(start), Some(end)) = (def.title_byte_start, def.title_byte_end) {
            assert!(ctx.is_in_link_title(start));
            assert!(ctx.is_in_link_title(start + 5));
            assert!(!ctx.is_in_link_title(end));
        } else {
            // Title wasn't parsed, so no position should be in title
            for i in 0..content.len() {
                assert!(!ctx.is_in_link_title(i));
            }
        }
    }
}

#[test]
fn test_is_in_link_title_no_refs() {
    let content = "Just plain text without any reference definitions.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.reference_definitions().is_empty());

    for i in 0..content.len() {
        assert!(!ctx.is_in_link_title(i));
    }
}

// =========================================================================
// Math span tests (Issue #289)
// =========================================================================

#[test]
fn test_math_spans_inline() {
    let content = "Text with inline math $[f](x)$ in it.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1, "Should detect one inline math span");

    let span = &math_spans[0];
    assert!(!span.is_display, "Should be inline math, not display");
    assert_eq!(span.content, "[f](x)", "Content should be extracted correctly");
}

#[test]
fn test_math_spans_display_single_line() {
    let content = "$$X(\\zeta) = \\mathcal Z [x](\\zeta)$$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1, "Should detect one display math span");

    let span = &math_spans[0];
    assert!(span.is_display, "Should be display math");
    assert!(
        span.content.contains("[x](\\zeta)"),
        "Content should contain the link-like pattern"
    );
}

#[test]
fn test_math_spans_display_multiline() {
    let content = "Before\n\n$$\n[x](\\zeta) = \\sum_k x(k)\n$$\n\nAfter";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1, "Should detect one display math span");

    let span = &math_spans[0];
    assert!(span.is_display, "Should be display math");
}

#[test]
fn test_is_in_math_span() {
    let content = "Text $[f](x)$ more text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Position inside the math span
    let math_start = content.find('$').unwrap();
    let math_end = content.rfind('$').unwrap() + 1;

    assert!(
        ctx.is_in_math_span(math_start + 1),
        "Position inside math span should return true"
    );
    assert!(
        ctx.is_in_math_span(math_start + 3),
        "Position inside math span should return true"
    );

    // Position outside the math span
    assert!(!ctx.is_in_math_span(0), "Position before math span should return false");
    assert!(
        !ctx.is_in_math_span(math_end + 1),
        "Position after math span should return false"
    );
}

#[test]
fn test_math_spans_mixed_with_code() {
    let content = "Math $[f](x)$ and code `[g](y)` mixed";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    let code_spans = ctx.code_spans();

    assert_eq!(math_spans.len(), 1, "Should have one math span");
    assert_eq!(code_spans.len(), 1, "Should have one code span");

    // Verify math span content
    assert_eq!(math_spans[0].content, "[f](x)");
    // Verify code span content
    assert_eq!(code_spans[0].content, "[g](y)");
}

#[test]
fn test_math_spans_no_math() {
    let content = "Regular text without any math at all.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert!(math_spans.is_empty(), "Should have no math spans");
}

#[test]
fn test_math_spans_multiple() {
    let content = "First $a$ and second $b$ and display $$c$$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 3, "Should detect three math spans");

    // Two inline, one display
    let inline_count = math_spans.iter().filter(|s| !s.is_display).count();
    let display_count = math_spans.iter().filter(|s| s.is_display).count();

    assert_eq!(inline_count, 2, "Should have two inline math spans");
    assert_eq!(display_count, 1, "Should have one display math span");
}

#[test]
fn test_is_in_math_span_boundary_positions() {
    // Test exact boundary positions: $[f](x)$
    // Byte positions:                0123456789
    let content = "$[f](x)$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1, "Should have one math span");

    let span = &math_spans[0];

    // Position at opening $ should be in span (byte 0)
    assert!(
        ctx.is_in_math_span(span.byte_offset),
        "Start position should be in span"
    );

    // Position just inside should be in span
    assert!(
        ctx.is_in_math_span(span.byte_offset + 1),
        "Position after start should be in span"
    );

    // Position at closing $ should be in span (exclusive end means we check byte_end - 1)
    assert!(
        ctx.is_in_math_span(span.byte_end - 1),
        "Position at end-1 should be in span"
    );

    // Position at byte_end should NOT be in span (exclusive end)
    assert!(
        !ctx.is_in_math_span(span.byte_end),
        "Position at byte_end should NOT be in span (exclusive)"
    );
}

#[test]
fn test_math_spans_at_document_start() {
    let content = "$x$ text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1);
    assert_eq!(math_spans[0].byte_offset, 0, "Math should start at byte 0");
}

#[test]
fn test_math_spans_at_document_end() {
    let content = "text $x$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1);
    assert_eq!(math_spans[0].byte_end, content.len(), "Math should end at document end");
}

#[test]
fn test_math_spans_consecutive() {
    let content = "$a$$b$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    // pulldown-cmark should parse these as separate spans
    assert!(!math_spans.is_empty(), "Should detect at least one math span");

    // All positions should be in some math span
    for i in 0..content.len() {
        assert!(ctx.is_in_math_span(i), "Position {i} should be in a math span");
    }
}

#[test]
fn test_math_spans_currency_not_math() {
    // Unbalanced $ should not create math spans
    let content = "Price is $100";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    // pulldown-cmark requires balanced delimiters for math
    // $100 alone is not math
    assert!(
        math_spans.is_empty() || !math_spans.iter().any(|s| s.content.contains("100")),
        "Unbalanced $ should not create math span containing 100"
    );
}

// =========================================================================
// Tests for O(1) reference definition lookups via HashMap
// =========================================================================

#[test]
fn test_reference_lookup_o1_basic() {
    let content = r#"[ref1]: /url1
[REF2]: /url2 "Title"
[Ref3]: /url3

Use [link][ref1] and [link][REF2]."#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Verify we have 3 reference defs
    assert_eq!(ctx.reference_definitions().len(), 3);

    // Test get_reference_url with various cases
    assert_eq!(ctx.get_reference_url("ref1"), Some("/url1"));
    assert_eq!(ctx.get_reference_url("REF1"), Some("/url1")); // case insensitive
    assert_eq!(ctx.get_reference_url("Ref1"), Some("/url1")); // case insensitive
    assert_eq!(ctx.get_reference_url("ref2"), Some("/url2"));
    assert_eq!(ctx.get_reference_url("REF2"), Some("/url2"));
    assert_eq!(ctx.get_reference_url("ref3"), Some("/url3"));
    assert_eq!(ctx.get_reference_url("nonexistent"), None);
}

#[test]
fn test_reference_lookup_o1_empty_content() {
    let content = "No references here.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.reference_definitions().is_empty());
    assert_eq!(ctx.get_reference_url("anything"), None);
}

#[test]
fn test_reference_lookup_o1_special_characters_in_id() {
    let content = r#"[ref-with-dash]: /url1
[ref_with_underscore]: /url2
[ref.with.dots]: /url3
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.get_reference_url("ref-with-dash"), Some("/url1"));
    assert_eq!(ctx.get_reference_url("ref_with_underscore"), Some("/url2"));
    assert_eq!(ctx.get_reference_url("ref.with.dots"), Some("/url3"));
}

#[test]
fn test_reference_lookup_o1_unicode_id() {
    let content = r#"[日本語]: /japanese
[émoji]: /emoji
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.get_reference_url("日本語"), Some("/japanese"));
    assert_eq!(ctx.get_reference_url("émoji"), Some("/emoji"));
    assert_eq!(ctx.get_reference_url("ÉMOJI"), Some("/emoji")); // uppercase
}

#[test]
fn test_is_in_link_title_multiple_ranges_binary_search() {
    // Three reference defs with titles — verifies binary search works across all three
    let content = "[a]: /url1 \"Title A\"\n[b]: /url2 \"Title B\"\n[c]: /url3 \"Title C\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 3, "Should have 3 reference defs");

    // Position inside first title should return true
    if let (Some(start), Some(end)) = (
        ctx.reference_definitions()[0].title_byte_start,
        ctx.reference_definitions()[0].title_byte_end,
    ) {
        assert!(ctx.is_in_link_title(start + 1), "Inside first title should return true");
        // Position at exclusive end should return false
        assert!(!ctx.is_in_link_title(end), "At exclusive end should return false");
    }

    // Position between titles (in URL area of def B, before its title) should return false
    if let (Some(end_a), Some(start_b)) = (
        ctx.reference_definitions()[0].title_byte_end,
        ctx.reference_definitions()[1].title_byte_start,
    ) && end_a + 1 < start_b
    {
        assert!(!ctx.is_in_link_title(end_a + 1), "Between titles should return false");
    }

    // Position inside third title should return true
    if let Some(start) = ctx.reference_definitions()[2].title_byte_start {
        assert!(ctx.is_in_link_title(start + 1), "Inside third title should return true");
    }
}

#[test]
fn test_is_in_math_span_between_two_spans() {
    // Position in text between two math spans should return false
    let content = "$a$ text $b$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let math_spans = ctx.math_spans();
    if math_spans.len() >= 2 {
        let between = math_spans[0].byte_end + 1;
        assert!(
            !ctx.is_in_math_span(between),
            "Position between math spans should return false"
        );
    }
}

// =========================================================================
// Tests for code span and HTML tag detection at boundaries
// =========================================================================

#[test]
fn test_code_span_at_line_start() {
    let content = "Line one\n`code` end\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let spans = ctx.code_spans();
    let line2_spans: Vec<_> = spans.iter().filter(|s| s.line == 2).collect();
    assert!(!line2_spans.is_empty(), "Should detect code span on line 2");
    assert_eq!(line2_spans[0].start_col, 0, "Code span should start at column 0");
}

#[test]
fn test_html_tag_at_byte_zero() {
    let content = "<br/> text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let tags = ctx.html_tags();
    assert!(!tags.is_empty(), "Should detect HTML tag at byte 0");
    assert_eq!(tags[0].line, 1, "Tag at byte 0 should be on line 1");
}

// =========================================================================
// HTML block detection: CommonMark Type-1 blank-line handling
// =========================================================================
//
// Per CommonMark §4.6, Type-1 HTML blocks open with <pre, <script, <style,
// or <textarea and run until the matching end tag (or EOF). Blank lines do
// not terminate these blocks. Type 6/7 blocks (e.g. <div>, <p>) terminate
// at the first blank line.

#[test]
fn test_html_block_pre_with_blank_line_marks_all_inner_lines() {
    // Reproduces issue #578: a <pre> containing a blank line.
    let content = "# Heading\n\n<pre>\n\nhello  world\n</pre>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.is_in_html_block(3), "line 3 (`<pre>`) should be in html block");
    assert!(
        ctx.is_in_html_block(4),
        "line 4 (blank inside pre) should be in html block"
    );
    assert!(
        ctx.is_in_html_block(5),
        "line 5 (`hello  world`) should be in html block"
    );
    assert!(ctx.is_in_html_block(6), "line 6 (`</pre>`) should be in html block");
}

#[test]
fn test_html_block_textarea_with_blank_line_marks_all_inner_lines() {
    let content = "<textarea>\n\ninner  content\n</textarea>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.is_in_html_block(1), "line 1 (`<textarea>`) should be in html block");
    assert!(ctx.is_in_html_block(2), "line 2 (blank) should be in html block");
    assert!(
        ctx.is_in_html_block(3),
        "line 3 (inner content) should be in html block"
    );
    assert!(
        ctx.is_in_html_block(4),
        "line 4 (`</textarea>`) should be in html block"
    );
}

#[test]
fn test_html_block_long_pre_exceeds_arbitrary_line_cap() {
    // A <pre> with 120 inner lines (no blanks) must mark every inner line,
    // regardless of any internal line cap.
    let mut content = String::from("<pre>\n");
    for i in 0..120 {
        content.push_str(&format!("inner line {i}\n"));
    }
    content.push_str("</pre>\n");

    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);

    // 1-indexed: line 1 = <pre>, lines 2..=121 = inner, line 122 = </pre>.
    for line_num in 1..=122 {
        assert!(
            ctx.is_in_html_block(line_num),
            "line {line_num} of a 122-line <pre> block should be marked in_html_block",
        );
    }
}

#[test]
fn test_html_block_div_still_terminates_on_blank_line() {
    // Type-6 guardrail: <div> is not Type-1 and must terminate at blank line.
    let content = "<div>\ninner\n\nafter blank\n</div>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.is_in_html_block(1), "line 1 (`<div>`) should be in html block");
    assert!(ctx.is_in_html_block(2), "line 2 (inner) should be in html block");
    assert!(
        !ctx.is_in_html_block(4),
        "line 4 (`after blank`) must NOT be in html block"
    );
}

#[test]
fn test_html_block_unclosed_pre_extends_to_eof() {
    // Per CommonMark, an unclosed Type-1 block extends to end of document.
    let content = "<pre>\nline a\n\nline b\nline c\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    for line_num in 1..=5 {
        assert!(
            ctx.is_in_html_block(line_num),
            "line {line_num} of an unclosed <pre> should extend to EOF",
        );
    }
}

#[test]
fn test_html_block_nested_pre_with_split_closing_tag_ends_with_outer_block() {
    // Prettier wraps a long closing tag as `</pre\n>`, which no single-line
    // search for `</pre>` can match. Inside an already-open <table> block that
    // must not matter: the <pre> is block content, not a second block, so the
    // whole thing ends at the blank line that ends the <table>.
    let content = "<table>\n<tr><td>\n<pre>\nx</pre\n>\n</td></tr>\n</table>\n\nafter\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(
        ctx.is_in_html_block(3),
        "line 3 (`<pre>`) is content of the <table> block"
    );
    assert!(ctx.is_in_html_block(7), "line 7 (`</table>`) is still in the block");
    assert!(
        !ctx.is_in_html_block(9),
        "line 9 (`after`) must be ordinary markdown: the <table> block ended at the blank line"
    );
}

#[test]
fn test_html_block_top_level_pre_with_split_closing_tag_still_extends_to_eof() {
    // Guardrail for the case above. A Type-1 block that is NOT nested has
    // nothing else to close it, so CommonMark really does run it to EOF when
    // the end tag is split across lines. Narrowing that would be a regression.
    let content = "<pre>\nx</pre\n>\n\nafter\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    for line_num in 1..=5 {
        assert!(
            ctx.is_in_html_block(line_num),
            "line {line_num} of a top-level <pre> with a split end tag should reach EOF",
        );
    }
}

#[test]
fn test_html_block_nested_pre_does_not_outlive_a_type_6_parent() {
    // The same shape without a split tag: a <div> ends at its blank line even
    // though the <pre> it contains was never closed at all.
    let content = "<div>\n<pre>\nx\n</div>\n\nafter\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(
        ctx.is_in_html_block(2),
        "line 2 (`<pre>`) is content of the <div> block"
    );
    assert!(
        !ctx.is_in_html_block(6),
        "line 6 (`after`) must be ordinary markdown: the <div> block ended at the blank line"
    );
}

// ---------------------------------------------------------------------------
// Pulldown-cmark gives the same empty CowStr for both "no title" and "explicit
// empty title" (`""`/`''`/`()`). The link parser now rescans the source span
// to recover the distinction so MD054's auto-fix can't silently drop the
// delimiters when converting `[t](url "")` to autolink.
// ---------------------------------------------------------------------------

#[test]
fn test_link_no_title_yields_none() {
    let ctx = LintContext::new("[t](https://x.com)\n", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links().len(), 1);
    assert!(ctx.links()[0].title.is_none(), "no title delimiter must be None");
}

#[test]
fn test_link_explicit_empty_double_quote_title_yields_some_empty() {
    let ctx = LintContext::new(r#"[t](https://x.com "")"#, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links().len(), 1);
    assert_eq!(
        ctx.links()[0].title.as_deref(),
        Some(""),
        "`\"\"` must be preserved as Some(\"\"), not collapsed to None"
    );
}

#[test]
fn test_link_explicit_empty_single_quote_title_yields_some_empty() {
    let ctx = LintContext::new("[t](https://x.com '')\n", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links().len(), 1);
    assert_eq!(ctx.links()[0].title.as_deref(), Some(""));
}

#[test]
fn test_link_explicit_empty_paren_title_yields_some_empty() {
    let ctx = LintContext::new("[t](https://x.com ())\n", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links().len(), 1);
    assert_eq!(ctx.links()[0].title.as_deref(), Some(""));
}

#[test]
fn test_image_explicit_empty_title_yields_some_empty() {
    let ctx = LintContext::new(r#"![alt](https://x.com/img.png "")"#, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.images().len(), 1);
    assert_eq!(ctx.images()[0].title.as_deref(), Some(""));
}

#[test]
fn test_link_non_empty_title_is_unaffected() {
    let ctx = LintContext::new(r#"[t](https://x.com "real")"#, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links().len(), 1);
    assert_eq!(ctx.links()[0].title.as_deref(), Some("real"));
}

#[test]
fn test_link_title_with_trailing_whitespace_inside_parens() {
    // CommonMark allows whitespace between the closing title delimiter and
    // the link's closing `)`. The detector must skip that whitespace so it
    // still recognizes the explicit-empty-title pair.
    let ctx = LintContext::new(r#"[t](https://x.com ""    )"#, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links().len(), 1);
    assert_eq!(ctx.links()[0].title.as_deref(), Some(""));
}

#[test]
fn test_reference_link_empty_title_in_definition() {
    // Reference links carry their title in the *definition*, parsed by the
    // REF_DEF_PATTERN regex (which already distinguishes `Some("")` from
    // `None` via `cap.get(...)`); make sure that path keeps working.
    let content = "[t][r]\n\n[r]: https://x.com \"\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_definitions().len(), 1);
    assert_eq!(ctx.reference_definitions()[0].title.as_deref(), Some(""));
}

// ---------------------------------------------------------------------------
// Markdown inside an HTML block is not markdown: CommonMark renders it as
// literal text. Pulldown-cmark reports no links there, but the regex fallback
// used to re-add reference links (and reference images) while dropping inline
// ones, so the same construct was visible or invisible depending on which
// syntax the author picked.
// ---------------------------------------------------------------------------

#[test]
fn test_reference_link_in_an_html_block_is_not_parsed() {
    let content = "<details>\n<summary>[link][ref]</summary>\n</details>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(
        ctx.links().is_empty(),
        "a reference link inside an HTML block renders literally, so it is not a link: {:?}",
        ctx.links()
    );
}

#[test]
fn test_inline_link_in_an_html_block_is_not_parsed() {
    // The matched control for the test above: same position, same rendering,
    // and this half was already correct.
    let content = "<details>\n<summary>[link](https://example.com)</summary>\n</details>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(ctx.links().is_empty(), "unexpected links: {:?}", ctx.links());
}

#[test]
fn test_reference_image_in_an_html_block_is_not_parsed() {
    let content = "<details>\n<summary>![alt][img]</summary>\n</details>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(
        ctx.images().is_empty(),
        "a reference image inside an HTML block renders literally: {:?}",
        ctx.images()
    );
}

#[test]
fn test_reference_link_in_a_markdown_attribute_block_is_still_parsed() {
    // The `markdown` attribute is what MkDocs, kramdown and Jekyll use to say
    // the body IS markdown. Those lines carry `in_html_block` as well, so
    // suppressing on that flag alone would take real links with it.
    let content = "<div class=\"note\" markdown=\"1\">\n[link][ref]\n</div>\n\n[ref]: https://example.com\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(
        ctx.links().len(),
        1,
        "the body of a markdown=\"1\" block holds real markdown: {:?}",
        ctx.links()
    );
    assert_eq!(ctx.links()[0].line, 2);
    assert!(ctx.links()[0].is_reference);
}

#[test]
fn test_reference_image_in_a_markdown_attribute_block_is_still_parsed() {
    let content = "<div class=\"note\" markdown=\"1\">\n![alt][img]\n</div>\n\n[img]: https://example.com/i.png\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(
        ctx.images().len(),
        1,
        "the body of a markdown=\"1\" block holds real markdown: {:?}",
        ctx.images()
    );
    assert_eq!(ctx.images()[0].line, 2);
}

#[test]
fn test_reference_link_after_an_html_block_is_still_parsed() {
    // The suppression is per line, not "everything below the first tag".
    let content = "<details>\n<summary>[dead][ref]</summary>\n</details>\n\n[live][ref]\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links().len(), 1, "unexpected links: {:?}", ctx.links());
    assert_eq!(ctx.links()[0].text, "live");
}

#[test]
fn test_pandoc_flavor_detects_div_blocks() {
    let content = "::: {.callout-note}\nA note.\n:::\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    assert!(
        ctx.is_in_div_block(content.find(":::").unwrap()),
        "Pandoc flavor should detect div block ranges"
    );
}

#[test]
fn test_pandoc_flavor_detects_citations() {
    let content = "See [@smith2020] for details.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("[@smith2020]").unwrap() + 1;
    assert!(ctx.is_in_citation(pos), "Pandoc flavor should detect citation ranges");
}

#[test]
fn test_pandoc_flavor_detects_inline_footnotes() {
    let content = "Text ^[note here] more.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("^[").unwrap() + 1;
    assert!(
        ctx.is_in_inline_footnote(pos),
        "Pandoc flavor should detect inline footnote ranges"
    );
}

#[test]
fn test_standard_flavor_skips_inline_footnotes() {
    let content = "Text ^[note here] more.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("^[").unwrap() + 1;
    assert!(
        !ctx.is_in_inline_footnote(pos),
        "Standard flavor should not detect inline footnote ranges"
    );
}

#[test]
fn test_pandoc_flavor_resolves_implicit_header_reference() {
    let content = "# My Section\n\nSee [My Section] for details.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    assert!(ctx.matches_implicit_header_reference("My Section"));
    assert!(!ctx.matches_implicit_header_reference("Nonexistent"));
}

#[test]
fn test_standard_flavor_does_not_resolve_implicit_header_reference() {
    let content = "# My Section\n\nSee [My Section] for details.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(!ctx.matches_implicit_header_reference("My Section"));
}

#[test]
fn test_pandoc_flavor_detects_example_list_markers() {
    use crate::config::MarkdownFlavor;
    let content = "(@) First item.\n(@good) Second item.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("(@)").unwrap();
    assert!(ctx.is_in_example_list_marker(pos));
    let pos2 = content.find("(@good)").unwrap();
    assert!(ctx.is_in_example_list_marker(pos2));
}

#[test]
fn test_pandoc_flavor_detects_example_references() {
    use crate::config::MarkdownFlavor;
    let content = "(@good) First.\n\nAs shown in (@good), it works.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let ref_pos = content.rfind("(@good)").unwrap();
    assert!(ctx.is_in_example_reference(ref_pos));
    // The line-start marker is NOT a reference (filtered out).
    let marker_pos = content.find("(@good)").unwrap();
    assert!(!ctx.is_in_example_reference(marker_pos));
}

#[test]
fn test_standard_flavor_skips_example_lists() {
    use crate::config::MarkdownFlavor;
    let content = "(@) First.\nAs shown in (@good), it works.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("(@)").unwrap();
    assert!(!ctx.is_in_example_list_marker(pos));
    let ref_pos = content.find("(@good)").unwrap();
    assert!(!ctx.is_in_example_reference(ref_pos));
}

#[test]
fn test_pandoc_flavor_detects_subscript() {
    use crate::config::MarkdownFlavor;
    let content = "H~2~O is water.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("~2~").unwrap() + 1;
    assert!(ctx.is_in_subscript_or_superscript(pos));
}

#[test]
fn test_pandoc_flavor_detects_superscript() {
    use crate::config::MarkdownFlavor;
    let content = "2^10^ is 1024.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("^10^").unwrap() + 1;
    assert!(ctx.is_in_subscript_or_superscript(pos));
}

#[test]
fn test_pandoc_flavor_does_not_match_strikethrough() {
    use crate::config::MarkdownFlavor;
    let content = "This is ~~struck~~.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("~~struck~~").unwrap() + 2;
    assert!(!ctx.is_in_subscript_or_superscript(pos));
}

#[test]
fn test_standard_flavor_skips_sub_super() {
    use crate::config::MarkdownFlavor;
    let content = "H~2~O and 2^10^.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("~2~").unwrap() + 1;
    assert!(!ctx.is_in_subscript_or_superscript(pos));
}

#[test]
fn test_pandoc_flavor_detects_inline_code_attribute() {
    use crate::config::MarkdownFlavor;
    let content = "Use `print()`{.python} for output.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("{.python}").unwrap() + 1;
    assert!(ctx.is_in_inline_code_attr(pos));
}

#[test]
fn test_pandoc_flavor_skips_bare_brace_block() {
    use crate::config::MarkdownFlavor;
    // A `{...}` not preceded by `` `code` `` is not an inline-code attribute.
    let content = "Use {.example} for the class.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("{.example}").unwrap() + 1;
    assert!(!ctx.is_in_inline_code_attr(pos));
}

#[test]
fn test_standard_flavor_skips_inline_code_attribute() {
    use crate::config::MarkdownFlavor;
    let content = "Use `print()`{.python} for output.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("{.python}").unwrap() + 1;
    assert!(!ctx.is_in_inline_code_attr(pos));
}

#[test]
fn test_pandoc_flavor_detects_bracketed_span() {
    use crate::config::MarkdownFlavor;
    let content = "This is [some text]{.smallcaps} here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("[some text]").unwrap();
    assert!(ctx.is_in_bracketed_span(pos));
}

#[test]
fn test_pandoc_flavor_skips_link() {
    use crate::config::MarkdownFlavor;
    let content = "A [link](http://example.com) here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("[link]").unwrap();
    assert!(!ctx.is_in_bracketed_span(pos));
}

#[test]
fn test_standard_flavor_skips_bracketed_span() {
    use crate::config::MarkdownFlavor;
    let content = "This is [some text]{.smallcaps} here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("[some text]").unwrap();
    assert!(!ctx.is_in_bracketed_span(pos));
}

#[test]
fn test_pandoc_flavor_detects_line_block() {
    use crate::config::MarkdownFlavor;
    let content = "| The Lord of the Rings\n| by J.R.R. Tolkien\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("Lord").unwrap();
    assert!(ctx.is_in_line_block(pos));
}

#[test]
fn test_pandoc_flavor_line_block_does_not_match_pipe_table() {
    use crate::config::MarkdownFlavor;
    let content = "| col1 | col2 |\n|------|------|\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("col1").unwrap();
    assert!(!ctx.is_in_line_block(pos));
}

#[test]
fn test_standard_flavor_skips_line_block() {
    use crate::config::MarkdownFlavor;
    let content = "| The Lord of the Rings\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("Lord").unwrap();
    assert!(!ctx.is_in_line_block(pos));
}

#[test]
fn test_pandoc_flavor_line_block_continuation_is_in_block() {
    // The continuation line (whitespace-indented, no leading pipe) belongs
    // to the active block, so a position inside it must report true.
    use crate::config::MarkdownFlavor;
    let content = "| First line\n  continuation here\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("continuation").unwrap();
    assert!(ctx.is_in_line_block(pos));
}

#[test]
fn test_pandoc_flavor_detects_pipe_table_caption_below() {
    use crate::config::MarkdownFlavor;
    let content = "\
| col1 | col2 |
|------|------|
| a    | b    |

: My caption
";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("My caption").unwrap();
    assert!(ctx.is_in_pipe_table_caption(pos));
}

#[test]
fn test_pandoc_flavor_definition_term_is_not_pipe_table_caption() {
    use crate::config::MarkdownFlavor;
    let content = "Term\n: definition\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("definition").unwrap();
    assert!(!ctx.is_in_pipe_table_caption(pos));
}

#[test]
fn test_standard_flavor_skips_pipe_table_caption() {
    use crate::config::MarkdownFlavor;
    let content = "\
| col1 |
|------|
| a    |

: Caption
";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("Caption").unwrap();
    assert!(!ctx.is_in_pipe_table_caption(pos));
}

#[test]
fn test_pandoc_flavor_detects_pipe_table_caption_above() {
    use crate::config::MarkdownFlavor;
    let content = "\
: Caption first

| col1 | col2 |
|------|------|
| a    | b    |
";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("Caption first").unwrap();
    assert!(ctx.is_in_pipe_table_caption(pos));
}

#[test]
fn test_pandoc_flavor_detects_metadata_block_at_start() {
    use crate::config::MarkdownFlavor;
    let content = "---\ntitle: Doc\n---\n\nBody.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("title").unwrap();
    assert!(ctx.is_in_pandoc_metadata(pos));
    let body_pos = content.find("Body").unwrap();
    assert!(!ctx.is_in_pandoc_metadata(body_pos));
}

#[test]
fn test_pandoc_flavor_detects_mid_document_metadata() {
    use crate::config::MarkdownFlavor;
    let content = "Intro.\n\n---\nauthor: X\n---\n\nBody.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("author").unwrap();
    assert!(ctx.is_in_pandoc_metadata(pos));
}

#[test]
fn test_standard_flavor_skips_pandoc_metadata() {
    use crate::config::MarkdownFlavor;
    let content = "---\ntitle: Doc\n---\n\nBody.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("title").unwrap();
    assert!(!ctx.is_in_pandoc_metadata(pos));
}

#[test]
fn test_pandoc_flavor_detects_grid_table() {
    use crate::config::MarkdownFlavor;
    let content = "\
+---+---+
| a | b |
+---+---+
| 1 | 2 |
+---+---+
";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find('a').unwrap();
    assert!(ctx.is_in_grid_table(pos));
}

#[test]
fn test_pandoc_flavor_grid_table_excludes_surrounding_text() {
    use crate::config::MarkdownFlavor;
    let content = "Before.\n\n+---+---+\n| a | b |\n+---+---+\n\nAfter.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let before_pos = content.find("Before").unwrap();
    let after_pos = content.find("After").unwrap();
    assert!(!ctx.is_in_grid_table(before_pos));
    assert!(!ctx.is_in_grid_table(after_pos));
}

#[test]
fn test_standard_flavor_skips_grid_table() {
    use crate::config::MarkdownFlavor;
    let content = "+---+---+\n| a | b |\n+---+---+\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find('a').unwrap();
    assert!(!ctx.is_in_grid_table(pos));
}

#[test]
fn test_pandoc_flavor_detects_multi_line_table() {
    use crate::config::MarkdownFlavor;
    let content = "\
-------------------------------------------------------------
 Centered   Default           Right Left
  Header    Aligned         Aligned Aligned
----------- ------- --------------- -------------------------
   First    row                12.0 Example of a row that
                                    spans multiple lines.

  Second    row                 5.0 Here's another one. Note
                                    the blank line between
                                    rows.
-------------------------------------------------------------
";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    // The entire content should be detected as a single multi-line table.
    let first_pos = content.find("First").unwrap();
    let second_pos = content.find("Second").unwrap();
    assert!(ctx.is_in_multi_line_table(first_pos));
    assert!(ctx.is_in_multi_line_table(second_pos));
    // The detection covers byte 0 (the top border) through content.len().
    assert!(ctx.is_in_multi_line_table(0));
}

#[test]
fn test_pandoc_flavor_multi_line_table_excludes_surrounding_text() {
    use crate::config::MarkdownFlavor;
    let content = "\
Before text.

-------------------------------------------------------------
 Centered   Default           Right Left
  Header    Aligned         Aligned Aligned
----------- ------- --------------- -------------------------
   First    row                12.0 Example.
-------------------------------------------------------------

After text.
";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let before_pos = content.find("Before").unwrap();
    let after_pos = content.find("After").unwrap();
    let inside_pos = content.find("First").unwrap();
    assert!(!ctx.is_in_multi_line_table(before_pos));
    assert!(!ctx.is_in_multi_line_table(after_pos));
    assert!(ctx.is_in_multi_line_table(inside_pos));
}

#[test]
fn test_standard_flavor_skips_multi_line_table() {
    use crate::config::MarkdownFlavor;
    let content = "\
-------------------------------------------------------------
 Centered   Default           Right Left
  Header    Aligned         Aligned Aligned
----------- ------- --------------- -------------------------
   First    row                12.0 Example.
-------------------------------------------------------------
";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("First").unwrap();
    assert!(!ctx.is_in_multi_line_table(pos));
}

#[test]
fn test_front_matter_is_not_scanned_for_html_comments() {
    // A `<!--` in a YAML value is data. Pairing it with a `-->` in the body
    // would mark the heading between them as commented out, hiding it from
    // every rule.
    let content = "---\nauthor: \"a <!-- b\"\n---\n\n# Title\n\nText --> text\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(
        ctx.html_comment_ranges().is_empty(),
        "got: {:?}",
        ctx.html_comment_ranges()
    );
    assert!(ctx.unterminated_html_comment().is_none());
}

#[test]
fn test_html_comment_in_the_body_is_found_after_front_matter() {
    let content = "---\nauthor: \"a <!-- b\"\n---\n\n<!-- a body comment -->\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let ranges = ctx.html_comment_ranges();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start, content.find("<!-- a body").unwrap());
}

#[test]
fn test_unterminated_html_comment_offset() {
    let content = "# Title\n\n<!-- never closed\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let opener = content.find("<!--").unwrap();
    assert_eq!(ctx.unterminated_html_comment(), Some(opener));
    // The opener starts an HTML block, so it comments out the rest of it. The
    // range is what keeps the comment-aware rules agreeing with the parser,
    // which reports no headings or lists inside that block either.
    let ranges = ctx.html_comment_ranges();
    assert_eq!(ranges.len(), 1, "got: {ranges:?}");
    assert_eq!((ranges[0].start, ranges[0].end), (opener, content.len()));
}

#[test]
fn test_unterminated_inline_html_comment_has_no_range() {
    // Mid-paragraph there is no comment for `<!--` to open: CommonMark renders
    // it literally, so the text after it is published and stays visible to
    // every rule.
    let content = "# Title\n\nSome prose <!-- never closed\n\nMore text.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.unterminated_html_comment(), Some(content.find("<!--").unwrap()));
    assert!(
        ctx.html_comment_ranges().is_empty(),
        "an inline opener hides nothing, got: {:?}",
        ctx.html_comment_ranges()
    );
}

#[test]
fn test_unterminated_html_comment_range_stops_at_its_container() {
    // The block ends with the blockquote, not at the end of the document, so
    // the paragraph below it is ordinary content.
    let content = "> <!-- never closed\n> inside\n\nVisible text.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let ranges = ctx.html_comment_ranges();
    assert_eq!(ranges.len(), 1, "got: {ranges:?}");
    assert_eq!(ranges[0].start, content.find("<!--").unwrap());
    assert!(
        ranges[0].end < content.find("Visible").unwrap(),
        "range {:?} should stop before the text after the quote",
        ranges[0]
    );
}

#[test]
fn test_html_opener_a_closed_obsidian_pair_hides_gets_no_range() {
    use crate::config::MarkdownFlavor;
    // The `<!--` sits between a pair of `%%`, so Obsidian removes it before
    // anything parses it as an opener. Giving it a block range would close the
    // Obsidian comment nowhere and hide the rest of the note from every rule.
    let content = "%% note\n<!-- hidden\n%%\n\nVisible text.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Obsidian, None);
    assert_eq!(ctx.unterminated_html_comment(), None);
    assert_eq!(ctx.unterminated_obsidian_comment(), None);
    assert!(
        ctx.html_comment_ranges().is_empty(),
        "got: {:?}",
        ctx.html_comment_ranges()
    );
    let visible = content.find("Visible").unwrap();
    assert!(!ctx.is_in_html_comment(visible));
    assert!(!ctx.is_in_obsidian_comment(visible));
}

#[test]
fn test_obsidian_delimiters_inside_an_unclosed_html_block_are_comment_text() {
    use crate::config::MarkdownFlavor;
    // The block covers the `%%`, so it opens no Obsidian comment. Both scans
    // have to agree here, or the document reports a second unclosed comment
    // for a delimiter that renders as nothing.
    let content = "<!-- an aside\n\n%% a note\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Obsidian, None);
    assert_eq!(ctx.unterminated_html_comment(), Some(0));
    assert_eq!(ctx.unterminated_obsidian_comment(), None);
    assert!(ctx.is_in_html_comment(content.find("%%").unwrap()));
    assert!(!ctx.is_in_obsidian_comment(content.find("%%").unwrap()));
}

#[test]
fn test_the_comment_syntax_that_opens_first_hides_the_other() {
    use crate::config::MarkdownFlavor;
    // The HTML scan cannot see the `%%` delimiters, so it reports a comment
    // running from the hidden `<!--` to the `-->` below, over the closing `%%`.
    // The `%%` opens first, which makes the `<!--` comment text.
    let content = "%% note <!-- hidden %%\n\n<!-- closed -->\n\nVisible text.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Obsidian, None);
    assert_eq!(ctx.unterminated_obsidian_comment(), None);
    assert!(ctx.is_in_obsidian_comment(content.find("hidden").unwrap()));
    let visible = content.find("Visible").unwrap();
    assert!(!ctx.is_in_obsidian_comment(visible));
}

#[test]
fn test_obsidian_delimiter_behind_a_comment_on_the_same_line_is_comment_text() {
    use crate::config::MarkdownFlavor;
    // A comment can start and end partway along a line, so whether a `%%` is
    // hidden is a question about bytes, not about whole lines.
    let content = "text <!-- %% --> tail\n\n%% a note\n\nBelow.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Obsidian, None);
    let hidden = content.find("%%").unwrap();
    let opener = content.rfind("%%").unwrap();
    assert!(!ctx.is_in_obsidian_comment(hidden));
    assert_eq!(ctx.unterminated_obsidian_comment(), Some(opener));
    assert!(ctx.is_in_obsidian_comment(content.find("Below.").unwrap()));
}

#[test]
fn test_obsidian_comments_are_repaired_around_an_unclosed_html_block() {
    use crate::config::MarkdownFlavor;
    // The `%%` the block hides looks like the closer for the one below it. It
    // is comment text, so the one below opens a comment that nothing closes,
    // and everything after it is hidden.
    let content = "> <!-- an aside\n> %% hidden\n\n%% a note\n\nBelow.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Obsidian, None);
    let opener = content.rfind("%%").unwrap();
    assert_eq!(ctx.unterminated_html_comment(), Some(content.find("<!--").unwrap()));
    assert_eq!(ctx.unterminated_obsidian_comment(), Some(opener));
    assert!(ctx.is_in_html_comment(content.find("%% hidden").unwrap()));
    assert!(ctx.is_in_obsidian_comment(content.find("Below.").unwrap()));
}

#[test]
fn test_unterminated_obsidian_comment_offset() {
    let content = "# Title\n\n%% never closed\n";
    let obsidian = LintContext::new(content, MarkdownFlavor::Obsidian, None);
    assert_eq!(
        obsidian.unterminated_obsidian_comment(),
        Some(content.find("%%").unwrap())
    );

    let standard = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(
        standard.unterminated_obsidian_comment(),
        None,
        "%% is ordinary text outside the Obsidian flavor"
    );
}

#[test]
fn test_front_matter_is_not_scanned_for_obsidian_comments() {
    // A `%%` in a YAML value is part of the value. Treating it as a delimiter
    // opened a comment that ran to the end of the note, hiding the body from
    // every rule.
    let content = "---\ntitle: \"50%% off\"\n---\n\n# Title\n\nText\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Obsidian, None);
    assert_eq!(ctx.unterminated_obsidian_comment(), None);
    assert!(!ctx.is_in_obsidian_comment(content.find("# Title").unwrap()));
    assert!(!ctx.is_in_obsidian_comment(content.find("Text").unwrap()));
}

#[test]
fn test_obsidian_comment_in_the_body_is_found_after_front_matter() {
    let content = "---\ntitle: \"50%% off\"\n---\n\nText %% a note %% more\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Obsidian, None);
    assert!(ctx.is_in_obsidian_comment(content.find("a note").unwrap()));
    assert!(!ctx.is_in_obsidian_comment(content.find("more").unwrap()));
}

#[test]
fn test_delimiters_in_an_indented_code_block_are_not_comment_delimiters() {
    // The parser reports a real indented code block here, so both delimiters are
    // sample text. Pairing them would hide the paragraph between them from every
    // rule while the page shows it.
    let content = "Intro text.\n\n    <!-- open\n\nMiddle text.\n\n    --> close\n\nEnd.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(
        ctx.html_comment_ranges().is_empty(),
        "got: {:?}",
        ctx.html_comment_ranges()
    );
    assert!(!ctx.is_in_html_comment(content.find("Middle").unwrap()));
    assert_eq!(ctx.unterminated_html_comment(), None);
}

#[test]
fn test_unclosed_comment_in_an_admonition_hides_the_rest_of_its_body() {
    // The admonition body is markdown in its own right, so the `<!--` opens a
    // block there. The parser has no admonitions and reports none, which left
    // the line below the opener visible to every rule.
    let content = "!!! note\n    <!-- hidden\n    visit https://example.com\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    assert_eq!(ctx.unterminated_html_comment(), Some(content.find("<!--").unwrap()));
    assert!(ctx.is_in_html_comment(content.find("https://example.com").unwrap()));
}

#[test]
fn test_unclosed_comment_in_an_admonition_stops_at_the_next_admonition() {
    // A marker at the same indent opens a sibling container, so it is not part
    // of the body the comment hides.
    let content = "!!! note\n    <!-- hidden\n    hidden text\n\n!!! warning\n    visible text\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    assert!(ctx.is_in_html_comment(content.find("hidden text").unwrap()));
    assert!(!ctx.is_in_html_comment(content.find("visible text").unwrap()));
}

#[test]
fn test_unclosed_comment_in_a_markdown_html_block_hides_the_rest_of_it() {
    // The `markdown` attribute makes the body markdown in every flavor, so the
    // opener behaves the same as it does in an admonition.
    let content = "<div markdown>\n\n    <!-- hidden\n    visit https://example.com\n\n</div>\n\nAfter.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(ctx.is_in_html_comment(content.find("https://example.com").unwrap()));
    assert!(!ctx.is_in_html_comment(content.find("After.").unwrap()));
}

#[test]
fn test_an_admonition_comment_is_read_across_a_blank_line_after_the_marker() {
    // With a blank line after the marker the parser calls the body an indented
    // code block, which is the same container content by another misreading.
    let content = "!!! note\n\n    <!-- hidden\n    visit https://example.com\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    assert!(ctx.is_in_html_comment(content.find("https://example.com").unwrap()));
}

#[test]
fn test_a_comment_indented_inside_an_admonition_is_still_a_comment() {
    // The exemption the container gets is what keeps a closed comment written
    // at the body indent from being read as code.
    let content = "!!! note\n\n    <!-- a note -->\n    visit https://example.com\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    assert!(ctx.is_in_html_comment(content.find("a note").unwrap()));
    assert!(!ctx.is_in_html_comment(content.find("https://example.com").unwrap()));
}

#[test]
fn test_an_opener_partway_through_an_admonition_line_opens_nothing() {
    // Inline HTML renders literally, in a container as anywhere else, so the
    // text after it is published rather than hidden.
    let content = "!!! note\n    Some text <!-- never closed\n    visit https://example.com\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    assert_eq!(ctx.unterminated_html_comment(), Some(content.find("<!--").unwrap()));
    assert!(!ctx.is_in_html_comment(content.find("https://example.com").unwrap()));
}

#[test]
fn test_delimiters_in_a_fence_inside_an_admonition_are_not_comment_delimiters() {
    // The body is markdown, but a fence written in it is a real code block, so
    // the sample opener it holds neither opens a comment nor is left dangling.
    let content = "!!! note\n\n    ```\n    <!-- a sample opener\n    ```\n\n    visit https://example.com\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    assert_eq!(ctx.unterminated_html_comment(), None);
    assert!(!ctx.is_in_html_comment(content.find("https://example.com").unwrap()));
}

#[test]
fn test_an_unclosed_comment_in_an_admonition_hides_a_fence_below_it() {
    // Once the comment opens, the fence markers are raw HTML rather than a code
    // block, so the block runs on to the end of the body as it does at the top
    // level.
    let content = "!!! note\n    <!-- hidden\n    ```\n    code\n    ```\n    visit https://example.com\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    assert!(ctx.is_in_html_comment(content.find("code").unwrap()));
    assert!(ctx.is_in_html_comment(content.find("https://example.com").unwrap()));
}

/// Line number (1-indexed) of the only line carrying `is_horizontal_rule`, or
/// `None` when no line does.
fn horizontal_rule_line(ctx: &LintContext) -> Option<usize> {
    let marked: Vec<usize> = ctx
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.is_horizontal_rule)
        .map(|(i, _)| i + 1)
        .collect();
    assert!(marked.len() <= 1, "expected at most one marked line, got {marked:?}");
    marked.first().copied()
}

#[test]
fn test_a_break_in_a_block_that_hides_its_content_is_not_a_horizontal_rule() {
    // The markers are computed from the line text, before the passes that know
    // which block the line belongs to have run.
    for (content, flavor) in [
        ("Text.\n\n<!--\n***\n-->\n\nMore.\n", MarkdownFlavor::Standard),
        ("Text.\n\n<div>\n***\n</div>\n\nMore.\n", MarkdownFlavor::Standard),
        ("Text.\n\n$$\n***\n$$\n\nMore.\n", MarkdownFlavor::Standard),
        ("Text.\n\n::: mermaid\n***\n:::\n\nMore.\n", MarkdownFlavor::AzureDevOps),
        ("Text.\n\n{/*\n***\n*/}\n\nMore.\n", MarkdownFlavor::MDX),
        ("Text.\n\n%%\n***\n%%\n\nMore.\n", MarkdownFlavor::Obsidian),
    ] {
        let ctx = LintContext::new(content, flavor, None);
        assert_eq!(
            horizontal_rule_line(&ctx),
            None,
            "line 4 was still a horizontal rule in {content:?} ({flavor:?})"
        );
    }
}

#[test]
fn test_a_break_in_a_container_whose_body_is_markdown_stays_a_horizontal_rule() {
    // These containers render their body as markdown, so a break written in one
    // is a real break and the sweep above must leave it alone.
    for (content, flavor) in [
        ("Text.\n\n***\n\nMore.\n", MarkdownFlavor::Standard),
        ("::: note\nText.\n***\nMore.\n:::\n", MarkdownFlavor::Pandoc),
        ("<Tabs>\nText.\n***\nMore.\n</Tabs>\n", MarkdownFlavor::MDX),
        ("/// note\nText.\n***\nMore.\n///\n", MarkdownFlavor::MkDocs),
        ("> Text.\n> ***\n> More.\n", MarkdownFlavor::Standard),
        // A `markdown` attribute takes effect for content a blank line separates
        // from the tag, which is also where rumdl starts linting the body.
        (
            "<div markdown>\n\nText.\n***\nMore.\n\n</div>\n",
            MarkdownFlavor::Standard,
        ),
        (
            "<div markdown>\n\nText.\n***\nMore.\n\n</div>\n",
            MarkdownFlavor::MkDocs,
        ),
    ] {
        let ctx = LintContext::new(content, flavor, None);
        assert!(
            horizontal_rule_line(&ctx).is_some(),
            "the break stopped being a horizontal rule in {content:?} ({flavor:?})"
        );
    }
}

#[test]
fn test_a_break_written_flush_against_an_html_tag_is_raw_html() {
    // Without the blank line the `markdown` attribute needs, the body is a plain
    // HTML block: no other rule reads it as markdown (a heading there draws no
    // MD022, mixed list markers no MD004), so a break there is not one either.
    for (content, flavor) in [
        ("<div markdown>\nText.\n***\nMore.\n</div>\n", MarkdownFlavor::Standard),
        (
            "<div markdown=\"1\">\nText.\n***\nMore.\n</div>\n",
            MarkdownFlavor::MkDocs,
        ),
    ] {
        let ctx = LintContext::new(content, flavor, None);
        assert_eq!(
            horizontal_rule_line(&ctx),
            None,
            "the raw HTML was still a horizontal rule in {content:?} ({flavor:?})"
        );
    }
}
