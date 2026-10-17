use super::*;
use crate::config::Config;
use crate::utils::fix_utils::apply_warning_fixes;

fn rule(order: SortOrder) -> MD096TableSort {
    MD096TableSort::from_config_struct(MD096Config {
        enabled: true,
        column: Some("Name".into()),
        order,
    })
}

fn assert_sorted(input: &str, expected: &str, order: SortOrder, flavor: MarkdownFlavor) {
    let rule = rule(order);
    let ctx = LintContext::new(input, flavor, None);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1, "expected one table diagnostic: {input:?}");
    assert_eq!(rule.fix(&ctx).unwrap(), expected);
    assert_eq!(apply_warning_fixes(input, &warnings).unwrap(), expected);
    let fixed_ctx = LintContext::new(expected, flavor, None);
    assert!(rule.check(&fixed_ctx).unwrap().is_empty(), "not sorted: {expected:?}");
    assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
}

#[test]
fn sorts_rows_without_changing_other_cells_or_surrounding_text() {
    assert_sorted(
        "Before\n\n| Name  | Value |\n| :---- | ----: |\n| Zebra |  2    |\n| Alpha | 1 |\n\nAfter\n",
        "Before\n\n| Name  | Value |\n| :---- | ----: |\n| Alpha | 1 |\n| Zebra |  2    |\n\nAfter\n",
        SortOrder::Asc,
        MarkdownFlavor::Standard,
    );
}

#[test]
fn stable_ties_in_both_directions() {
    let input = "| Name | ID |\n| --- | --- |\n| beta | b1 |\n| ALPHA | a1 |\n| Alpha | a2 |\n| Beta | b2 |\n";
    for (order, body) in [
        (
            SortOrder::Asc,
            "| ALPHA | a1 |\n| Alpha | a2 |\n| beta | b1 |\n| Beta | b2 |\n",
        ),
        (
            SortOrder::Desc,
            "| beta | b1 |\n| Beta | b2 |\n| ALPHA | a1 |\n| Alpha | a2 |\n",
        ),
    ] {
        assert_sorted(
            input,
            &format!("| Name | ID |\n| --- | --- |\n{body}"),
            order,
            MarkdownFlavor::Standard,
        );
    }
}

#[test]
fn visible_text_handles_inline_full_collapsed_and_shortcut_links() {
    assert_sorted(
        "| **NAME** |\n| --- |\n| [Zulu](https://a.test) |\n| [Bravo][two words] |\n| [Alpha][] |\n| [Charlie] |\n\n[two words]: /b\n[Alpha]: /a\n[Charlie]: /c\n",
        "| **NAME** |\n| --- |\n| [Alpha][] |\n| [Bravo][two words] |\n| [Charlie] |\n| [Zulu](https://a.test) |\n\n[two words]: /b\n[Alpha]: /a\n[Charlie]: /c\n",
        SortOrder::Asc,
        MarkdownFlavor::Standard,
    );
}

#[test]
fn keys_parse_inline_content_not_block_constructs() {
    let ctx = LintContext::new("", MarkdownFlavor::Standard, None);
    for (source, key) in [
        ("", ""),
        ("   ", ""),
        ("<span></span>", ""),
        ("<!-- hidden -->", ""),
        ("# Heading", "# heading"),
        ("- Item", "- item"),
        ("> Quote", "> quote"),
        ("[ref]: /url", "[ref]: /url"),
        ("  **Alpha** and `Beta`  ", "alpha and beta"),
        ("![Zebra](z.png)", "zebra"),
        ("<b>Alpha</b>", "alpha"),
        ("A&amp;B", "a&b"),
        ("$Alpha$", "alpha"),
        ("~~Alpha~~", "alpha"),
        ("`A\\|B`", "a|b"),
        ("[Unknown][undefined]", "[unknown][undefined]"),
        ("\"Alpha\" --", "\"alpha\" --"),
    ] {
        assert_eq!(MD096TableSort::sort_key(source, &ctx), key, "{source}");
    }
}

#[test]
fn preserves_escaped_pipes_and_code() {
    assert_sorted(
        "| Name | Extra |\n| --- | --- |\n| `Z\\|a` | a\\|b |\n| **Alpha** | `x\\|y` |\n",
        "| Name | Extra |\n| --- | --- |\n| **Alpha** | `x\\|y` |\n| `Z\\|a` | a\\|b |\n",
        SortOrder::Asc,
        MarkdownFlavor::Standard,
    );
}

#[test]
fn renderer_cell_boundaries_determine_whether_sorting_is_safe() {
    // GFM splits this code span at its unescaped pipe. The utility's general
    // formatter splitter masks code pipes, so use the renderer's cells instead.
    let input = "| Code | Name |\n| --- | --- |\n| `a|b` | Zulu |\n| value | Alpha |\n";
    let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
    assert!(ctx.table_blocks.iter().any(|table| table.parser_confirmed));
    assert!(rule(SortOrder::Asc).check(&ctx).unwrap().is_empty());
    assert_eq!(rule(SortOrder::Asc).fix(&ctx).unwrap(), input);
    // Python-Markdown masks code-span pipes before splitting the table.
    assert_sorted(
        input,
        "| Code | Name |\n| --- | --- |\n| value | Alpha |\n| `a|b` | Zulu |\n",
        SortOrder::Asc,
        MarkdownFlavor::MkDocs,
    );
}

#[test]
fn never_sorts_a_subset_of_a_table_with_pipe_less_continuations() {
    let input = "| Name |\n| --- |\n| Zulu |\n| Alpha |\nMike\n";
    let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
    assert!(ctx.table_blocks.iter().any(|table| table.parser_confirmed));
    assert!(rule(SortOrder::Asc).check(&ctx).unwrap().is_empty());
    assert_eq!(rule(SortOrder::Asc).fix(&ctx).unwrap(), input);
}

#[test]
fn trailing_whitespace_does_not_become_a_container_prefix() {
    assert_sorted(
        "| Name |\n| --- |\n| Zulu |  \n| Alpha |\t\n",
        "| Name |\n| --- |\n| Alpha |\t\n| Zulu |  \n",
        SortOrder::Asc,
        MarkdownFlavor::Standard,
    );
    let input = "| Name |\n| --- |\n| Éclair |\u{3000}\n| Alpha |\n";
    let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
    assert_eq!(rule(SortOrder::Asc).fix(&ctx).unwrap(), input);
}

#[test]
fn empty_header_does_not_match_column_x() {
    let rule = MD096TableSort::from_config_struct(MD096Config {
        column: Some("x".into()),
        ..Default::default()
    });
    let input = "| x | |\n| --- | --- |\n| 2 | b |\n| 1 | a |\n";
    let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
    assert_eq!(rule.check(&ctx).unwrap().len(), 1);
    assert_eq!(
        rule.fix(&ctx).unwrap(),
        "| x | |\n| --- | --- |\n| 1 | a |\n| 2 | b |\n"
    );
}

#[test]
fn obsidian_wikilinks_sort_by_alias_and_keep_the_target() {
    assert_sorted(
        "| Name |\n| --- |\n| [[Aardvark\\|Zulu]] |\n| [[Zebra\\|Alpha]] |\n",
        "| Name |\n| --- |\n| [[Zebra\\|Alpha]] |\n| [[Aardvark\\|Zulu]] |\n",
        SortOrder::Asc,
        MarkdownFlavor::Obsidian,
    );
}

#[test]
fn empty_cells_and_unicode_have_documented_lexical_order() {
    assert_sorted(
        "| Name |\n| --- |\n| Éclair |\n| zebra |\n|  |\n| Äpfel |\n| alpha |\n",
        "| Name |\n| --- |\n|  |\n| alpha |\n| zebra |\n| Äpfel |\n| Éclair |\n",
        SortOrder::Asc,
        MarkdownFlavor::Standard,
    );
}

#[test]
fn selects_a_nonfirst_column() {
    assert_sorted(
        "ID | Name\n--- | ---\n1 | Zulu\n2 | Alpha\n",
        "ID | Name\n--- | ---\n2 | Alpha\n1 | Zulu\n",
        SortOrder::Asc,
        MarkdownFlavor::Standard,
    );
}

#[test]
fn preserves_list_and_blockquote_containers() {
    for (header_prefix, row_prefix) in [("- ", "  "), ("> ", "> "), ("- > ", "  > "), ("> - ", ">   ")] {
        let input =
            format!("{header_prefix}| Name |\n{row_prefix}| --- |\n{row_prefix}| Zulu |\n{row_prefix}| Alpha |\n");
        let expected =
            format!("{header_prefix}| Name |\n{row_prefix}| --- |\n{row_prefix}| Alpha |\n{row_prefix}| Zulu |\n");
        assert_sorted(&input, &expected, SortOrder::Asc, MarkdownFlavor::Standard);
    }
}

#[test]
fn preserves_each_line_ending_and_missing_eof_newline() {
    for ending in ["\n", "\r\n"] {
        for eof in ["", ending] {
            let input = format!("| Name |{ending}| --- |{ending}| Zulu |{ending}| Alpha |{eof}");
            let expected = format!("| Name |{ending}| --- |{ending}| Alpha |{ending}| Zulu |{eof}");
            assert_sorted(&input, &expected, SortOrder::Asc, MarkdownFlavor::Standard);
        }
    }
    assert_sorted(
        "| Name |\r\n| --- |\n| Zulu |\r\n| Alpha |\n",
        // The shared fix utility normalizes a mixed-ending file to LF.
        "| Name |\n| --- |\n| Alpha |\n| Zulu |\n",
        SortOrder::Asc,
        MarkdownFlavor::Standard,
    );
}

#[test]
fn raw_quick_fix_preserves_destination_endings_even_in_a_mixed_file() {
    let input = "| Name |\r\n| --- |\n| Zulu |\r\n| Alpha |\n";
    let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
    let warning = rule(SortOrder::Asc).check(&ctx).unwrap().remove(0);
    let fix = warning.fix.unwrap();
    let mut fixed = input.to_string();
    fixed.replace_range(fix.range, &fix.replacement);
    assert_eq!(fixed, "| Name |\r\n| --- |\n| Alpha |\r\n| Zulu |\n");
}

#[test]
fn multiple_tables_are_independent_and_nonmatching_tables_are_preserved() {
    let input = "| Name |\n| --- |\n| Zulu |\n| Alpha |\n\n| Other |\n| --- |\n| Zulu |\n| Alpha |\n\n| Name |\n| --- |\n| Gamma |\n| Beta |\n";
    let expected = "| Name |\n| --- |\n| Alpha |\n| Zulu |\n\n| Other |\n| --- |\n| Zulu |\n| Alpha |\n\n| Name |\n| --- |\n| Beta |\n| Gamma |\n";
    let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
    assert_eq!(rule(SortOrder::Asc).check(&ctx).unwrap().len(), 2);
    assert_eq!(rule(SortOrder::Asc).fix(&ctx).unwrap(), expected);
}

#[test]
fn mixed_body_container_prefixes_are_not_moved() {
    let input = "> | Name |\n> | --- |\n> | Zulu |\n| Alpha |\n";
    let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
    // Make the test meaningful even if table recognition becomes stricter.
    assert!(ctx.table_blocks.iter().any(|table| table.parser_confirmed));
    assert!(rule(SortOrder::Asc).check(&ctx).unwrap().is_empty());
    assert_eq!(rule(SortOrder::Asc).fix(&ctx).unwrap(), input);
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(64))]
    #[test]
    fn sorting_preserves_every_record_and_stable_ties(
        keys in proptest::collection::vec(0u8..10, 2..40),
        descending in proptest::bool::ANY,
        crlf in proptest::bool::ANY,
        final_newline in proptest::bool::ANY,
    ) {
        let ending = if crlf { "\r\n" } else { "\n" };
        let header = format!("| Name | ID |{ending}| --- | --- |{ending}");
        let records: Vec<_> = keys.iter().enumerate().map(|(id, key)| (*key, id)).collect();
        let render = |rows: &[(u8, usize)]| {
            let mut text = header.clone();
            for (index, (key, id)) in rows.iter().enumerate() {
                text.push_str(&format!("| key{key} | {id} |"));
                if index + 1 < rows.len() || final_newline { text.push_str(ending); }
            }
            text
        };
        let mut expected = records.clone();
        expected.sort_by_key(|&(key, id)| (if descending { 9 - key } else { key }, id));
        let input = render(&records);
        let sorter = rule(if descending { SortOrder::Desc } else { SortOrder::Asc });
        let ctx = LintContext::new(&input, MarkdownFlavor::Standard, None);
        let fixed = sorter.fix(&ctx).unwrap();
        proptest::prop_assert_eq!(&fixed, &render(&expected));
        let fixed_ctx = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
        proptest::prop_assert!(sorter.check(&fixed_ctx).unwrap().is_empty());
        proptest::prop_assert_eq!(sorter.fix(&fixed_ctx).unwrap(), fixed);
    }
}

#[test]
fn skips_absent_ambiguous_malformed_and_non_table_content() {
    for input in [
        "| Other |\n| --- |\n| Zulu |\n| Alpha |\n",
        "| Name | **name** |\n| --- | --- |\n| Zulu | z |\n| Alpha | a |\n",
        "| Name | Value |\n| --- | --- |\n| Zulu | 1 |\n| Alpha |\n",
        "| Name |\n| --- |\n| Zulu | x |\n| Alpha |\n",
        "| Name |\n| --- | --- |\n| Zulu |\n| Alpha |\n",
        "```markdown\n| Name |\n| --- |\n| Zulu |\n| Alpha |\n```\n",
        "    | Name |\n    | --- |\n    | Zulu |\n    | Alpha |\n",
        "<pre>\n| Name |\n| --- |\n| Zulu |\n| Alpha |\n</pre>\n",
        "| Name |\n| --- |\n| Alpha |\n",
    ] {
        let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
        assert!(rule(SortOrder::Asc).check(&ctx).unwrap().is_empty(), "{input:?}");
        assert_eq!(rule(SortOrder::Asc).fix(&ctx).unwrap(), input);
    }
}

#[test]
fn enabling_without_a_column_never_reorders_tables() {
    let input = "| Name |\n| --- |\n| Zulu |\n| Alpha |\n";
    let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
    for column in [None, Some("  ".into())] {
        let rule = MD096TableSort::from_config_struct(MD096Config {
            column,
            ..Default::default()
        });
        assert!(rule.should_skip(&ctx));
        assert!(rule.check(&ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), input);
    }
}

#[test]
fn comments_are_reported_without_moving_instructions() {
    let input = "| Name |\n| --- |\n| Zulu <!-- rumdl-disable MD013 --> |\n| Alpha |\n";
    let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
    let warnings = rule(SortOrder::Asc).check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].fix.is_none());
    assert_eq!(rule(SortOrder::Asc).fix(&ctx).unwrap(), input);
}

#[test]
fn literal_comment_markers_in_code_do_not_prevent_sorting() {
    assert_sorted(
        "| Name | Note |\n| --- | --- |\n| Zulu | `<!-- -->` |\n| Alpha | comment example |\n",
        "| Name | Note |\n| --- | --- |\n| Alpha | comment example |\n| Zulu | `<!-- -->` |\n",
        SortOrder::Asc,
        MarkdownFlavor::Standard,
    );
}

#[test]
fn disabled_table_stays_unchanged_and_md087_counts_the_suppression() {
    for input in [
        "<!-- rumdl-disable MD096 -->\n| Name |\n| --- |\n| Zulu |\n| Alpha |\n",
        "<!-- rumdl-disable-next-line table-sort -->\n| Name |\n| --- |\n| Zulu |\n| Alpha |\n",
        "| Name |\n| --- |\n| Zulu <!-- rumdl-disable-line MD096 --> |\n| Alpha |\n",
    ] {
        let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
        let sorter = rule(SortOrder::Asc);
        assert_eq!(sorter.check(&ctx).unwrap().len(), 1, "raw diagnostic for MD087");
        assert_eq!(sorter.fix(&ctx).unwrap(), input);
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(sorter), Box::new(crate::rules::MD087UnusedDisableComment)];
        let warnings = crate::lint(input, &rules, false, MarkdownFlavor::Standard, None, None).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
    }
}

#[test]
fn configuration_roundtrips_and_invalid_order_is_rejected() {
    for (value, expected) in [
        ("asc", SortOrder::Asc),
        ("ASC", SortOrder::Asc),
        ("desc", SortOrder::Desc),
        ("DESC", SortOrder::Desc),
    ] {
        let config: MD096Config = toml::from_str(&format!("column = 'Name'\norder = '{value}'")).unwrap();
        assert_eq!(config.order, expected);
    }
    assert!(toml::from_str::<MD096Config>("order = 'random'").is_err());
    let default = MD096TableSort::default();
    let (_, defaults) = default.default_config_section().unwrap();
    assert!(!defaults.as_table().unwrap().contains_key("column"));
    assert!(
        default
            .config_schema()
            .unwrap()
            .1
            .as_table()
            .unwrap()
            .contains_key("column")
    );
    let config = Config::default();
    assert!(crate::rules::opt_in_rules().contains("MD096"));
    assert!(
        crate::rules::all_rules(&config)
            .iter()
            .any(|rule| rule.name() == "MD096")
    );
}

#[test]
fn reenabled_table_sorts_and_md087_flags_the_unused_disable() {
    let input = "<!-- rumdl-disable MD096 -->\n<!-- rumdl-enable MD096 -->\n\n| Name |\n| --- |\n| Zulu |\n| Alpha |\n";
    let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
    let sorter = rule(SortOrder::Asc);
    assert!(sorter.fix(&ctx).unwrap().find("| Alpha |").unwrap() < sorter.fix(&ctx).unwrap().find("| Zulu |").unwrap());
    let rules: Vec<Box<dyn Rule>> = vec![Box::new(sorter), Box::new(crate::rules::MD087UnusedDisableComment)];
    let warnings = crate::lint(input, &rules, false, MarkdownFlavor::Standard, None, None).unwrap();
    assert_eq!(warnings.len(), 2);
    assert!(warnings.iter().any(|w| w.rule_name.as_deref() == Some("MD096")));
    assert!(warnings.iter().any(|w| w.rule_name.as_deref() == Some("MD087")));
}
