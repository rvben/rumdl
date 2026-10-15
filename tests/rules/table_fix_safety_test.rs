//! Table-fixing rules (MD055, MD058, MD060) must only rewrite what the Markdown
//! parser reads as a table. Structure is judged by comrak (GFM, as GitHub renders it).

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD055TablePipeStyle, MD056TableColumnCount, MD058BlanksAroundTables, MD060TableFormat};

fn html(doc: &str) -> String {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    let out = comrak::markdown_to_html(doc, &options);
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

fn rules() -> Vec<(String, Box<dyn Rule>)> {
    let mut v: Vec<(String, Box<dyn Rule>)> = Vec::new();
    for style in [
        "leading_and_trailing",
        "no_leading_or_trailing",
        "leading_only",
        "trailing_only",
    ] {
        v.push((
            format!("MD055/{style}"),
            Box::new(MD055TablePipeStyle::new(style.to_string())),
        ));
    }
    v.push(("MD058".to_string(), Box::new(MD058BlanksAroundTables::default())));
    for style in ["aligned", "compact", "tight"] {
        v.push((
            format!("MD060/{style}"),
            Box::new(MD060TableFormat::new(true, style.to_string())),
        ));
    }
    v
}

/// Every table rule leaves `doc` byte-identical and reports nothing.
fn assert_untouched(doc: &str, flavor: MarkdownFlavor) {
    for (name, rule) in rules() {
        let ctx = LintContext::new(doc, flavor, None);
        let warnings = rule.check(&ctx).unwrap();
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, doc, "{name} rewrote a non-table: {doc:?}");
        assert!(
            warnings.is_empty(),
            "{name} warned on a non-table: {doc:?}: {warnings:?}"
        );
    }
}

/// Every table rule keeps the document's block structure intact.
fn assert_structure_kept(doc: &str, flavor: MarkdownFlavor) {
    let before = html(doc);
    for (name, rule) in rules() {
        let ctx = LintContext::new(doc, flavor, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            html(&fixed),
            before,
            "{name} changed the structure of {doc:?} into {fixed:?}"
        );
    }
}

#[test]
fn delimiter_row_with_fewer_cells_is_not_a_table() {
    for doc in [
        "a|b\n|-|\n",
        "a|b|\n|-\n",
        "m\nA|B\n|-|\n",
        "|a|b|c|\n|-|-|\n",
        "|a|b|\n|-|-|-|\n",
    ] {
        assert_untouched(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn indented_code_is_not_a_table() {
    for doc in [
        "\t||\n-|\n",
        "    | a |\n    |---|\n",
        "    |a|b|\n    |-|-|\n",
        "- x\n\n  ||\n-|\n",
        "- x\n\n      |a|b|\n      |-|-|\n",
        "> x\n>\n>     |a|b|\n>     |-|-|\n",
    ] {
        assert_untouched(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn indented_paragraph_continuation_is_not_a_table() {
    assert_untouched("m\n    |a|b|\n    |-|-|\n", MarkdownFlavor::Standard);
}

#[test]
fn ordered_marker_that_cannot_interrupt_a_paragraph_is_not_a_table() {
    assert_untouched("m\n2. |a|b|\n   |-|-|\n", MarkdownFlavor::Standard);
}

#[test]
fn blockquote_depth_mismatch_is_not_a_table() {
    for doc in ["o|b\n>|-|\n", "> |a|b|\n|-|-|\n", "|a|b|\n> |-|-|\n", "> a|b\n>> -|-\n"] {
        assert_untouched(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn lazy_continuation_is_not_a_table() {
    for doc in ["- x\n|a|b|\n|-|-|\n", "> x\n|a|b|\n|-|-|\n"] {
        assert_untouched(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn hard_break_row_is_not_a_table() {
    assert_untouched("o|r  \n|-\no|\n", MarkdownFlavor::Standard);
}

#[test]
fn invalid_delimiter_cells_are_not_a_table() {
    assert_untouched("|a|b|\n|- -|--|\n", MarkdownFlavor::Standard);
}

#[test]
fn code_span_pipe_counts_as_a_cell_separator() {
    assert_untouched("| Type | `x | y` |\n|---|---|\n", MarkdownFlavor::Standard);
}

#[test]
fn raw_html_block_content_is_not_a_table() {
    assert_untouched("<div>\n|a|b|\n|-|-|\n</div>\n", MarkdownFlavor::Standard);
}

#[test]
fn indented_code_after_a_table_is_not_a_table_row() {
    assert_structure_kept("a|b\n-|-\n    c|d\n", MarkdownFlavor::Standard);
    assert_structure_kept("|a|b|\n|-|-|\n    |x|y|\n", MarkdownFlavor::Standard);
}

#[test]
fn bare_list_marker_before_a_table_is_not_split_by_a_blank_line() {
    for doc in [
        "-\n  | a |\n  |---|\n",
        "1.\n   | a |\n   |---|\n",
        "> -\n>   | a |\n>   |---|\n",
        "- -\n    | a |\n    |---|\n",
        "-\t\n  | a |\n  |---|\n",
    ] {
        let rule = MD058BlanksAroundTables::default();
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), doc, "MD058 changed {doc:?}");
        assert!(rule.check(&ctx).unwrap().is_empty(), "MD058 warned on {doc:?}");
        assert_eq!(
            html(&rule.fix(&ctx).unwrap()),
            html(doc),
            "MD058 changed the structure of {doc:?}"
        );
    }
}

#[test]
fn thematic_break_before_a_table_still_needs_a_blank_line() {
    let doc = "- - -\n| a |\n|---|\n";
    let rule = MD058BlanksAroundTables::default();
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    assert_eq!(rule.fix(&ctx).unwrap(), "- - -\n\n| a |\n|---|\n");
}

#[test]
fn valid_tables_keep_their_structure_under_every_rule() {
    for doc in [
        "|a|b|\n|-|-|\n|x|y|\n",
        "a|b\n-|-\nc|d\n",
        "- x\n\n  |a|b|\n  |-|-|\n  |x|y|\n",
        "> |a|b|\n> |-|-|\n> |x|y|\n",
        "1. x\n\n   |a|b|\n   |-|-|\n",
    ] {
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn valid_table_is_still_reformatted() {
    let doc = "|a|b|\n|-|-|\n|x|y|\n";
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    assert_eq!(rule.fix(&ctx).unwrap(), "| a   | b   |\n| --- | --- |\n| x   | y   |\n");
}

#[test]
fn table_interrupting_a_paragraph_still_gets_a_blank_line_before_it() {
    let rule = MD058BlanksAroundTables::default();
    for (doc, expected) in [
        ("m\nA|B\n|-|-|\n", "m\n\nA|B\n|-|-|\n"),
        ("> m\n> A|B\n> |-|-|\n", "> m\n>\n> A|B\n> |-|-|\n"),
        ("- m\n  A|B\n  |-|-|\n", "- m\n\n  A|B\n  |-|-|\n"),
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), expected, "{doc:?}");
    }
}

#[test]
fn mkdocs_html_markdown_div_table_is_still_reformatted() {
    let doc = "<div markdown>\n\n|a|b|\n|-|-|\n|x|y|\n\n</div>\n";
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    assert_eq!(
        rule.fix(&ctx).unwrap(),
        "<div markdown>\n\n| a   | b   |\n| --- | --- |\n| x   | y   |\n\n</div>\n"
    );
}

#[test]
fn mkdocs_admonition_with_raw_html_before_the_table_is_left_alone() {
    let doc = "!!! note\n\n    <pre>\n\n    |a|b|\n    |-|-|\n\n    </pre>\n";
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    assert_eq!(rule.fix(&ctx).unwrap(), doc);
    let doc = "!!! note\n\n    - <pre>\n\n      |a|b|\n      |-|-|\n\n      </pre>\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    assert_eq!(rule.fix(&ctx).unwrap(), doc);
}

#[test]
fn mkdocs_html_markdown_div_with_raw_html_on_the_opener_line_is_left_alone() {
    let doc = "<div markdown><pre>\n\n|a|b|\n|-|-|\n|x|y|\n</pre>\n</div>\n";
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    assert_eq!(rule.fix(&ctx).unwrap(), doc);
}

// ---- Real binary -------------------------------------------------------------------

/// `rumdl check --fix --no-cache --enable rule` over `doc`; the file
/// content afterwards.
fn cli_fix(doc: &str, rule: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("doc.md");
    std::fs::write(&file, doc).unwrap();
    // MD060 is opt-in; the config turns it on without adding any other rule.
    let config = dir.path().join("rumdl.toml");
    std::fs::write(&config, "[MD060]\nenabled = true\nstyle = \"aligned\"\n").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--fix", "--no-cache", "--enable", rule, "--config"])
        .arg(&config)
        .arg(&file)
        .output()
        .unwrap();
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "rumdl failed on {doc:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(&file).unwrap()
}

#[test]
fn cli_fixes_keep_the_structure_of_every_reported_shape() {
    let bug_inputs = [
        "a|b\n|-|\n",
        "m\nA|B\n|-|\n",
        "-\n  | a |\n  |---|\n",
        "m\n    |a|b|\n    |-|-|\n",
        "m\n2. |a|b|\n   |-|-|\n",
        "|a|b|\n|-|-|\n    |x|y|\n",
        "<div>\n|a|b|\n|-|-|\n</div>\n",
        "|a|b|\n|- -|--|\n",
        "- x\n|a|b|\n|-|-|\n",
    ];
    for doc in bug_inputs {
        // Variants: in a list item, in a blockquote, nested.
        let variants = [
            doc.to_string(),
            doc.lines().map(|line| format!("- {line}\n")).collect::<String>(),
            doc.lines().map(|line| format!("> {line}\n")).collect::<String>(),
            doc.lines().map(|line| format!("> - {line}\n")).collect::<String>(),
        ];
        for variant in variants {
            for rule in ["MD055", "MD058", "MD060"] {
                let once = cli_fix(&variant, rule);
                assert_eq!(
                    html(&once),
                    html(&variant),
                    "{rule} changed the structure of {variant:?} into {once:?}"
                );
                assert_eq!(cli_fix(&once, rule), once, "{rule} is not idempotent on {variant:?}");
            }
        }
    }
}

#[test]
fn cli_still_reformats_a_valid_table() {
    assert_eq!(
        cli_fix("|a|b|\n|-|-|\n|x|y|\n", "MD060"),
        "| a   | b   |\n| --- | --- |\n| x   | y   |\n"
    );
    assert_eq!(cli_fix("m\nA|B\n|-|-|\n", "MD058"), "m\n\nA|B\n|-|-|\n");
}

// ---- Confirmation flag -------------------------------------------------------------

#[test]
fn mismatched_delimiter_row_stays_a_block_but_is_not_confirmed() {
    for doc in ["a|b\n|-|\n", "|a|b|c|\n|-|-|\n|1|2|3|\n"] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(ctx.table_blocks.iter().all(|block| !block.parser_confirmed), "{doc:?}");
    }
}

#[test]
fn well_formed_tables_are_confirmed() {
    for doc in [
        "|a|b|\n|-|-|\n",
        "a|b\n-|-\n",
        "- x\n\n  |a|b|\n  |-|-|\n",
        "> |a|b|\n> |-|-|\n",
        "m\nA|B\n|-|-|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert_eq!(ctx.table_blocks.len(), 1, "{doc:?}");
        assert!(ctx.table_blocks[0].parser_confirmed, "{doc:?}");
    }
}

#[test]
fn confirmed_body_stops_before_indented_code() {
    let ctx = LintContext::new("|a|b|\n|-|-|\n|1|2|\n    |x|y|\n", MarkdownFlavor::Standard, None);
    let block = &ctx.table_blocks[0];
    assert!(block.parser_confirmed);
    assert_eq!(block.end_line, 2);
    assert_eq!(block.content_lines, vec![2]);
}

#[test]
fn table_under_a_bare_list_marker_keeps_the_item_content_column() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    for (doc, expected) in [
        ("-\n  | a |\n  |---|\n", "-\n  | a   |\n  | --- |\n"),
        ("1.\n   | a |\n   |---|\n", "1.\n   | a   |\n   | --- |\n"),
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        assert_eq!(html(&fixed), html(doc), "structure of {doc:?}");
    }
}

#[test]
fn table_under_a_bare_list_marker_in_a_blockquote_keeps_its_structure() {
    let doc = "> -\n>   | a |\n>   |---|\n";
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    assert_eq!(html(&rule.fix(&ctx).unwrap()), html(doc));
}

#[test]
fn indented_code_after_a_rejected_definition_is_not_a_table() {
    for doc in [
        "term\n:warning:\n\n    | a | b |\n    |---|---|\n",
        "term\n:warning:\n\n    |a|b|\n    |-|-|\n",
    ] {
        assert_untouched(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn flavor_cell_counting_keeps_pipes_inside_code_spans_and_wikilinks_in_one_cell() {
    let cases = [
        (
            "| Type | `x | y` |\n|---|---|\n| 1 | 2 |\n",
            MarkdownFlavor::MkDocs,
            true,
        ),
        (
            "| Type | `x | y` |\n|---|---|\n| 1 | 2 |\n",
            MarkdownFlavor::Standard,
            false,
        ),
        (
            "| [[a|b]] | c |\n|---|---|\n| 1 | 2 |\n",
            MarkdownFlavor::Obsidian,
            true,
        ),
        (
            "| [[a|b]] | c |\n|---|---|\n| 1 | 2 |\n",
            MarkdownFlavor::Standard,
            false,
        ),
    ];
    for (doc, flavor, confirmed) in cases {
        let ctx = LintContext::new(doc, flavor, None);
        assert_eq!(ctx.table_blocks.len(), 1, "{flavor:?} {doc:?}");
        assert_eq!(ctx.table_blocks[0].parser_confirmed, confirmed, "{flavor:?} {doc:?}");
    }
}

#[test]
fn mdx_tables_the_native_parser_reads_are_confirmed() {
    for doc in [
        "<Foo>\n|a|b|\n|-|-|\n|1|2|\n</Foo>\n",
        "<Foo>\n  |a|b|\n  |-|-|\n  |1|2|\n</Foo>\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::MDX, None);
        assert_eq!(ctx.table_blocks.len(), 1, "{doc:?}");
        assert!(ctx.table_blocks[0].parser_confirmed, "{doc:?}");
    }
    // Standard Markdown reads the same lines as raw HTML.
    let ctx = LintContext::new("<Foo>\n|a|b|\n|-|-|\n|1|2|\n</Foo>\n", MarkdownFlavor::Standard, None);
    assert!(ctx.table_blocks.iter().all(|block| !block.parser_confirmed));
}

#[test]
fn a_non_table_beside_a_real_table_is_left_alone_by_every_fix() {
    let non_table = "m\nA|B\n|-|\n";
    let doc = format!("|a|b|\n|-|-|\n|1|2|\n\n{non_table}");
    assert_structure_kept(&doc, MarkdownFlavor::Standard);
    for (name, rule) in rules() {
        let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.ends_with(non_table), "{name} rewrote the non-table: {fixed:?}");
    }
}

#[test]
fn a_document_without_a_real_table_is_returned_byte_for_byte() {
    for doc in ["m\nA|B\n|-|\n\n\n", "m\r\nA|B\r\n|-|\r\n", "m\nA|B\n|-|"] {
        assert_untouched(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn a_row_indented_as_code_beside_a_table_is_not_a_table_row_to_md056() {
    use rumdl_lib::rules::MD056TableColumnCount;
    let doc = "|a|b|\n|-|-|\n    |x|y|z|\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    assert!(MD056TableColumnCount.check(&ctx).unwrap().is_empty());
}

#[test]
fn tab_indented_rows_under_a_header_on_the_list_marker_line_are_left_alone() {
    // The scanner counts a tab as one column, so it drops the list container of
    // the first and ends the table before the tab-indented row of the second.
    for doc in ["- |a|b|\n\t|-|-|\n", "- |a|b|\n  |-|-|\n\t|x|y|\n"] {
        assert_untouched(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn spaces_under_a_header_on_the_list_marker_line_are_still_formatted() {
    let ctx = LintContext::new("- |a|b|\n  |-|-|\n  |x|y|\n", MarkdownFlavor::Standard, None);
    assert!(ctx.table_blocks[0].parser_confirmed);
}

#[test]
fn table_in_an_item_with_a_wide_marker_gap_keeps_the_item_content_column() {
    for doc in [
        "-   x\n\n    |a|b|\n    |-|-|\n    |x|y|\n",
        "1.  x\n\n    |a|b|\n    |-|-|\n",
        "-   |a|b|\n    |-|-|\n    |x|y|\n",
    ] {
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn table_in_an_item_opened_with_a_tab_after_the_marker_keeps_the_item_content_column() {
    for doc in [
        "-\tx\n\n    |a|b|\n    |-|-|\n    |1|2|\n",
        "1.\tx\n\n    |a|b|\n    |-|-|\n",
    ] {
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn list_layouts_a_rewrite_cannot_reproduce_keep_the_document_structure() {
    for doc in [
        // Two markers on one line: the table belongs to the inner item.
        "- - x\n\n    |a|b|\n    |-|-|\n    |c|d|\n",
        // A wide gap after the marker on the header line.
        "-   |a|b|\n    |-|-|\n    |c|d|\n\n  tail\n",
        // A tab-indented table in an item.
        "- x\n\n\t|a|b|\n\t|-|-|\n\t|c|d|\n",
    ] {
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn a_thematic_break_inside_an_item_is_not_a_bare_list_marker_for_md058() {
    let rule = MD058BlanksAroundTables::default();
    for doc in [
        "- * * *\n  |a|b|\n  |-|-|\n",
        "- - -\n|a|b|\n|-|-|\n",
        "> ***\n> |a|b|\n> |-|-|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(!rule.check(&ctx).unwrap().is_empty(), "no warning for {doc:?}");
    }
    for doc in ["-\n  |a|b|\n  |-|-|\n", "- -\n    |a|b|\n    |-|-|\n"] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty(), "warning for {doc:?}");
    }
}

#[test]
fn a_table_outside_the_item_of_a_bare_marker_still_needs_a_blank_line_before_it() {
    let rule = MD058BlanksAroundTables::default();
    for doc in ["-\n|a|b|\n|-|-|\n", "> -\n> |a|b|\n> |-|-|\n", "1.\n|a|b|\n|-|-|\n"] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(!rule.check(&ctx).unwrap().is_empty(), "no warning for {doc:?}");
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn tables_in_list_items_are_confirmed_inside_a_blockquote() {
    for doc in [
        "> - |a|b|\n>   |-|-|\n",
        ">   - |a|b|\n>     |-|-|\n",
        "> - x\n>\n>   |a|b|\n>   |-|-|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(
            ctx.table_blocks.len() == 1 && ctx.table_blocks[0].parser_confirmed,
            "{doc:?}"
        );
    }
}

#[test]
fn flavor_divergent_tables_are_confirmed_when_indented() {
    for indent in ["", " ", "  ", "   "] {
        for (row, flavor) in [
            ("| Type | `x | y` |", MarkdownFlavor::MkDocs),
            ("| [[a|b]] | c |", MarkdownFlavor::Obsidian),
        ] {
            let doc = format!("{indent}{row}\n{indent}|---|---|\n{indent}| 1 | 2 |\n");
            let ctx = LintContext::new(&doc, flavor, None);
            assert!(
                ctx.table_blocks.len() == 1 && ctx.table_blocks[0].parser_confirmed,
                "{flavor:?} {doc:?}"
            );
        }
    }
}

#[test]
fn a_bare_marker_item_holds_its_content_one_column_past_the_marker() {
    // The first child is indented further than the item's content column, and
    // the table sits at the content column, so it belongs to the item.
    for doc in [
        "-\n   text\n\n  |a|b|\n  |-|-|\n",
        "1.\n    text\n\n   |a|b|\n   |-|-|\n",
        "> -\n>    text\n>\n>   |a|b|\n>   |-|-|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(
            ctx.table_blocks.iter().any(|block| block.parser_confirmed),
            "{doc:?} holds a table in the item"
        );
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn an_item_opened_with_indented_code_holds_its_content_at_the_marker_padding() {
    // Five spaces after the marker start indented code and leave the content
    // column one past the marker, so the table at column 2 belongs to the item.
    for doc in [
        "-     code\n\n  |a|b|\n  |-|-|\n",
        "1.     code\n\n   |a|b|\n   |-|-|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(
            ctx.table_blocks.iter().any(|block| block.parser_confirmed),
            "{doc:?} holds a table in the item"
        );
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn a_table_in_a_blockquote_inside_a_list_item_is_confirmed() {
    for doc in [
        "- x\n\n  > |a|b|\n  > |-|-|\n",
        "1. x\n\n   > |a|b|\n   > |-|-|\n   > |1|2|\n",
        "- x\n  - y\n\n    > |a|b|\n    > |-|-|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(
            ctx.table_blocks.iter().any(|block| block.parser_confirmed),
            "{doc:?} holds a table in the quote"
        );
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn myst_directive_bodies_keep_indented_code_as_code() {
    // Directive content is not indented, so four spaces make indented code
    // there, and a fix must not turn it into a table.
    for doc in [
        "```{note}\nSome text.\n\n    |a|b|\n    |-|-|\n    |c|d|\n```\n",
        "```{note}\n\n    |a|b|\n    |-|-|\n```\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::MyST, None);
        assert!(
            ctx.table_blocks.iter().all(|block| !block.parser_confirmed),
            "{doc:?} holds code, not a table"
        );
        assert_structure_kept(doc, MarkdownFlavor::MyST);
    }
}

#[test]
fn raw_html_before_a_nested_opener_keeps_table_lines_unconfirmed() {
    // The `<pre>` swallows everything after it, the admonition opener line
    // included, so the rows are preformatted text.
    let doc = "<div markdown>\n\n<pre>\n\n!!! note\n\n|a|b|\n|-|-|\n\n</pre>\n</div>\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    assert!(
        ctx.table_blocks.iter().all(|block| !block.parser_confirmed),
        "rows inside <pre> are not a table"
    );
}

#[test]
fn a_delimiter_indented_as_code_in_a_rejected_definition_is_not_a_table() {
    for doc in [
        "term\n:warning:\n\n |a|b|\n    |-|-|\n",
        "term\n:warning:\n\n  |a|b|\n    |-|-|\n    |1|2|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(
            ctx.table_blocks.iter().all(|block| !block.parser_confirmed),
            "{doc:?} holds no table"
        );
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn a_header_lazy_in_a_list_item_with_the_delimiter_inside_it_is_not_a_table() {
    for doc in [
        "- x\n |a|b|\n   |-|-|\n",
        "1. x\n |a|b|\n   |-|-|\n",
        "- x\n |a|b|\n  |-|-|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(
            ctx.table_blocks.iter().all(|block| !block.parser_confirmed),
            "{doc:?} holds no table"
        );
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn a_list_marker_row_after_a_definition_is_not_a_table() {
    let doc = "[x]: url\n2. |a|b|\n- |-|-|\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    assert!(
        ctx.table_blocks.iter().all(|block| !block.parser_confirmed),
        "{doc:?} holds no table"
    );
    assert_structure_kept(doc, MarkdownFlavor::Standard);
}

#[test]
fn text_around_a_rejected_definition_never_makes_a_confirmed_table() {
    for doc in [
        // Quoted code: four spaces, then a blockquote marker.
        "term\n:warning:\n\n    > |a|b|\n    > |-|-|\n",
        // A list item cannot interrupt the paragraph the rejected definition restores.
        "term\n:warning:\n2. |a|b|\n   |-|-|\n",
        "term\n:warning:\n|a|b|\n|-|-|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(
            ctx.table_blocks.iter().all(|block| !block.parser_confirmed),
            "{doc:?} holds no table"
        );
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

#[test]
fn a_lone_carriage_return_inside_a_table_line_leaves_the_block_unconfirmed() {
    // A lone CR ends a line for the parser but not for the line index.
    let doc = "[x]: /url\r|a|b|\n|-|-|\n\n[x]\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    assert!(
        ctx.table_blocks.iter().all(|block| !block.parser_confirmed),
        "{doc:?} holds no confirmed table"
    );
    for (name, rule) in rules() {
        let fixed = rule
            .fix(&LintContext::new(doc, MarkdownFlavor::Standard, None))
            .unwrap();
        assert_eq!(fixed, doc, "{name} rewrote a document with a lone CR");
    }
}

#[test]
fn a_gherkin_row_indented_past_the_header_stays_in_the_table() {
    let doc = "  |a|b|\n  |-|-|\n    |c|d|\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::MDG, None);
    let block = &ctx.table_blocks[0];
    assert!(block.parser_confirmed);
    assert_eq!(block.end_line, 2, "the four-space row belongs to the Gherkin table");
    let fixed = MD060TableFormat::new(true, "aligned".to_string()).fix(&ctx).unwrap();
    assert!(fixed.contains("|c") || fixed.contains("| c"), "{fixed:?}");
    assert_ne!(fixed, doc, "the row is formatted with the rest of the table");
    let spaced = MD058BlanksAroundTables::default().fix(&ctx).unwrap();
    assert_eq!(spaced, doc, "no blank line is inserted inside the table");
}

#[test]
fn a_list_table_inside_a_carved_container_is_formatted() {
    let doc = "!!! note\n\n    - item\n\n      |a|b|\n      |-|-|\n      |1|2|\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    assert!(
        ctx.table_blocks.iter().any(|block| block.parser_confirmed),
        "the table inside the admonition list item is confirmed"
    );
    let fixed = MD060TableFormat::new(true, "aligned".to_string()).fix(&ctx).unwrap();
    assert!(
        fixed.contains("      | a   | b   |"),
        "the table is formatted in place: {fixed:?}"
    );
}

#[test]
fn a_quote_opening_inside_an_item_of_a_quoted_list_is_a_table() {
    let doc = "> - > text\n>   >\n>   > |a|b|\n>   > |-|-|\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    assert!(
        ctx.table_blocks.iter().any(|block| block.parser_confirmed),
        "{doc:?} holds a confirmed table"
    );
    assert_structure_kept(doc, MarkdownFlavor::Standard);
}

#[test]
fn a_table_in_a_jsx_wrapped_list_item_is_confirmed_by_the_native_parser() {
    let doc = "<Foo>\n- |a|b|\n  |-|-|\n  |x|y|\n</Foo>\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::MDX, None);
    assert!(
        ctx.table_blocks.iter().any(|block| block.parser_confirmed),
        "{doc:?} holds a confirmed table"
    );
}

#[test]
fn a_quoted_list_table_keeps_its_indentation_when_reformatted() {
    for doc in [
        "> 10.\n>     |a|b|\n>     |-|-|\n",
        "> - x\n>\n>   |a|b|\n>   |-|-|\n",
        "> - x\n>\n>   |a|b|\n>   |-|-|\n>   |1|2|\n",
        "> 1. x\n>\n>    |a|b|\n>    |-|-|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        let fixed = MD060TableFormat::new(true, "aligned".to_string()).fix(&ctx).unwrap();
        assert_eq!(html(&fixed), html(doc), "{doc:?} -> {fixed:?}");
    }
}

#[test]
fn an_obsidian_wikilink_inside_a_code_span_does_not_hide_its_pipe() {
    // The table engine leaves a wikilink in a code span alone, so the pipe inside
    // it separates cells: three cells under a three-cell delimiter row.
    let doc = "| `[[a|b]]` | c |\n|---|---|---|\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::Obsidian, None);
    assert!(
        ctx.table_blocks.iter().any(|block| block.parser_confirmed),
        "{doc:?} holds a confirmed table"
    );
}

#[test]
fn inline_html_is_not_a_raw_html_block_above_a_carved_table() {
    for text in [
        "See <https://example.com>.",
        "See `<foo>`.",
        "Mail <me@example.com>.",
        "Text with <em>emphasis</em>.",
        "Ends with a tag <br>",
    ] {
        let doc = format!("!!! note\n\n    {text}\n\n    |a|b|\n    |-|-|\n");
        let ctx = LintContext::new(&doc, MarkdownFlavor::MkDocs, None);
        assert!(
            ctx.table_blocks.iter().any(|block| block.parser_confirmed),
            "{doc:?} holds a confirmed table"
        );
    }
    // A raw HTML block still open above the table leaves it unconfirmed.
    let doc = "!!! note\n\n    <pre>\n\n    |a|b|\n    |-|-|\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    assert!(ctx.table_blocks.iter().all(|block| !block.parser_confirmed));
}

#[test]
fn a_header_opening_a_new_item_is_not_lazy_text_of_the_previous_one() {
    let doc = "- first\n- |a|b|\n  |-|-|\n  |1|2|\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    assert!(
        ctx.table_blocks.iter().any(|block| block.parser_confirmed),
        "{doc:?} holds a confirmed table"
    );
}

#[test]
fn a_tab_after_a_blockquote_marker_in_front_of_a_list_does_not_panic() {
    let doc = ">\t-\n>\t  |a|b|\n>\t  |-|-|\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    assert!(
        ctx.table_blocks.iter().all(|block| !block.parser_confirmed),
        "{doc:?} holds no confirmed table"
    );
    assert_structure_kept(doc, MarkdownFlavor::Standard);
}

// ---- Tables inside nested containers ----------------------------------------------

/// (container, prefix of the first line, prefix of every later line). Each is a
/// container shape a table can sit in; a tab after a marker is part of the prefix.
const CONTAINERS: &[(&str, &str, &str)] = &[
    ("top level", "", ""),
    ("quote", "> ", "> "),
    ("item", "- ", "  "),
    ("item, wide gap", "-   ", "    "),
    ("ordered item", "1. ", "   "),
    ("item in quote", "> - ", ">   "),
    ("quote in item", "- > ", "  > "),
    ("quote in ordered item", "1. > ", "   > "),
    ("nested quotes", "> > ", "> > "),
    ("quote in item in item", "- - > ", "    > "),
    ("quote in item in quote", "> - > ", ">   > "),
    ("tab after quote", ">\t", ">\t"),
    ("tab after item marker", "-\t", "\t"),
    ("quote in item, tab after quote", "- >\t", "  >\t"),
    ("item in quote, tab after item marker", "> -\t", ">\t "),
];

/// `rows` written in a container: the first row after the first prefix, the rest after the second.
fn contained(first: &str, rest: &str, rows: &[&str]) -> String {
    rows.iter()
        .enumerate()
        .map(|(index, row)| format!("{}{row}\n", if index == 0 { first } else { rest }))
        .collect()
}

/// Whether a rewrite can reproduce the container's layout: a tab or a wide gap
/// after a list marker, or two list markers on the header line, leave the table
/// to the parser alone.
fn is_plain_layout(name: &str, first: &str) -> bool {
    !first.contains('\t') && !name.contains("wide") && !name.contains("item in item")
}

fn table_rule_names() -> Vec<&'static str> {
    vec!["MD055", "MD056", "MD058", "MD060"]
}

#[test]
fn well_formed_tables_in_every_container_are_not_corrupted_by_any_table_rule() {
    let tables: [&[&str]; 3] = [
        &["|a|b|", "|-|-|", "|x|y|"],
        &["a|b", "-|-", "x|y"],
        &["| a | b |", "|:-:|--:|", "| 1 | 2 |"],
    ];
    for &(name, first, rest) in CONTAINERS {
        for rows in tables {
            let doc = contained(first, rest, rows);
            let before = html(&doc);
            assert!(before.contains("<table>"), "{name}: comrak reads no table in {doc:?}");
            for rule in table_rule_names() {
                let once = cli_fix(&doc, rule);
                assert_eq!(
                    html(&once),
                    before,
                    "{rule} changed the structure of the table in {name}: {doc:?} became {once:?}"
                );
                if rule == "MD056" {
                    assert_eq!(
                        once, doc,
                        "MD056 rewrote a table with equal cell counts in {name}: {doc:?}"
                    );
                }
                assert_eq!(
                    cli_fix(&once, rule),
                    once,
                    "{rule} is not idempotent on the table in {name}: {doc:?}"
                );
            }
        }
    }
}

#[test]
fn md056_pads_and_trims_rows_in_every_container_without_moving_them() {
    for &(name, first, rest) in CONTAINERS {
        let doc = contained(first, rest, &["|a|b|", "|-|", "|x|y|z|"]);
        let once = cli_fix(&doc, "MD056");
        if !is_plain_layout(name, first) && once == doc {
            continue;
        }
        assert!(
            html(&once).contains("<table>"),
            "MD056 lost the table in {name}: {doc:?} became {once:?}"
        );
        for (index, line) in once.lines().enumerate() {
            let prefix = if index == 0 { first } else { rest };
            assert!(
                line.starts_with(prefix),
                "MD056 moved row {index} of the table in {name}: {line:?} does not start with {prefix:?}"
            );
        }
        assert_eq!(
            html(&once).matches("<th>").count(),
            html(&once).matches("<td>").count().min(2),
            "MD056 left rows with unequal cells in {name}: {once:?}"
        );
    }
}

#[test]
fn md055_restyles_rows_in_every_container_without_moving_them() {
    for &(name, first, rest) in CONTAINERS {
        let doc = contained(first, rest, &["a|b", "-|-", "x|y"]);
        let once = cli_fix(&doc, "MD055");
        assert_eq!(
            html(&once),
            html(&doc),
            "MD055 changed the structure of {name}: {doc:?} became {once:?}"
        );
        for (index, line) in once.lines().enumerate() {
            let prefix = if index == 0 { first } else { rest };
            assert!(
                line.starts_with(prefix),
                "MD055 moved row {index} of the table in {name}: {line:?} does not start with {prefix:?}"
            );
        }
    }
}

#[test]
fn md060_reformats_a_table_in_every_container_keeping_its_prefixes() {
    for &(name, first, rest) in CONTAINERS {
        let doc = contained(first, rest, &["|a|b|", "|-|-|", "|x|y|"]);
        let once = cli_fix(&doc, "MD060");
        assert_eq!(
            html(&once),
            html(&doc),
            "MD060 changed the structure of {name}: {doc:?} became {once:?}"
        );
        for (index, line) in once.lines().enumerate() {
            let prefix = if index == 0 { first } else { rest };
            assert!(
                line.starts_with(prefix),
                "MD060 moved row {index} of the table in {name}: {line:?} does not start with {prefix:?}"
            );
        }
        if is_plain_layout(name, first) {
            assert!(
                once.contains("| a   | b   |"),
                "MD060 did not align the table in {name}: {once:?}"
            );
        }
    }
}

#[test]
fn a_table_after_a_definition_keeps_its_structure_in_every_container() {
    // The header is indented four columns past its container, so the blank line
    // MD058 would add above it turns it into indented code.
    for &(name, first, rest) in CONTAINERS {
        let doc = contained(first, rest, &["[a]: /url", "    |a|b|", "|-|-|", "|x|y|"]);
        let before = html(&doc);
        for rule in table_rule_names() {
            let once = cli_fix(&doc, rule);
            assert_eq!(
                html(&once),
                before,
                "{rule} changed the structure after a definition in {name}: {doc:?} became {once:?}"
            );
        }
    }
}

#[test]
fn md058_still_separates_a_table_from_the_definition_above_it_in_every_container() {
    for &(name, first, rest) in CONTAINERS {
        let doc = contained(first, rest, &["[a]: /url", "|a|b|", "|-|-|", "|x|y|"]);
        let once = cli_fix(&doc, "MD058");
        if is_plain_layout(name, first) {
            assert_ne!(once, doc, "MD058 reported nothing for {name}: {doc:?}");
        }
        if once == doc {
            continue;
        }
        // The blank line also lets the definition take effect, so only the table is compared.
        let table = |html: String| html.find("<table>").map(|start| html[start..].to_string());
        assert!(table(html(&once)).is_some(), "MD058 lost the table in {name}: {once:?}");
        assert_eq!(
            table(html(&once)),
            table(html(&doc)),
            "MD058 changed the table in {name}: {doc:?} became {once:?}"
        );
        assert_eq!(once.lines().count(), doc.lines().count() + 1, "{name}: {once:?}");
    }
}

#[test]
fn a_header_two_columns_inside_a_quoted_list_item_is_still_aligned() {
    // The header sits two columns past the item's content, so it is a table of its
    // own and MD060 formats it; the list's indentation is not code indentation.
    let doc = "> - intro\n>\n>     |a|b|\n>   |-|-|\n>   |x|y|\n";
    assert_eq!(
        html(doc).matches("<table>").count(),
        1,
        "the oracle must read a table: {doc:?}"
    );
    let once = cli_fix(doc, "MD060");
    assert_ne!(once, doc, "MD060 left the table unformatted");
    assert_eq!(html(&once), html(doc), "MD060 changed the structure: {once:?}");
}

#[test]
fn a_header_that_starts_like_a_list_marker_keeps_all_its_cells() {
    // Ten digits are not an ordered-list marker, so `1234567890.` is the first
    // cell's text and the table has three columns.
    let doc = "1234567890. |a|b|\n---|---|---\nx|y|z\n";
    assert_eq!(
        html(doc).matches("<table>").count(),
        1,
        "the oracle must read a table: {doc:?}"
    );
    let once = cli_fix(doc, "MD056");
    assert_eq!(html(&once), html(doc), "MD056 changed the structure: {once:?}");
}

#[test]
fn a_header_starting_with_a_number_that_cannot_interrupt_a_paragraph_keeps_all_its_cells() {
    let doc = "intro\n2. |a|b|\n---|---|---\nx|y|z\n";
    assert_eq!(
        html(doc).matches("<table>").count(),
        1,
        "the oracle must read a table: {doc:?}"
    );
    let once = cli_fix(doc, "MD056");
    assert_eq!(html(&once), html(doc), "MD056 changed the structure: {once:?}");
}

#[test]
fn a_header_that_only_looks_like_it_opens_an_item_keeps_all_its_cells() {
    for doc in [
        "intro\n2. > |a|b|\n---|---|---\nx|y|z\n",
        "[a]: /url\n    - |a|b|\n---|---|---\nx|y|z\n",
    ] {
        assert_eq!(
            html(doc).matches("<table>").count(),
            1,
            "the oracle must read a table: {doc:?}"
        );
        let once = cli_fix(doc, "MD056");
        assert_eq!(
            html(&once),
            html(doc),
            "MD056 changed the structure: {doc:?} became {once:?}"
        );
    }
}

#[test]
fn md060_formats_a_gherkin_table_whose_rows_have_different_indentation() {
    let doc = "# Feature: F\n\n#### Examples:\n\n    |a|b|\n  |-|-|\n  |x|y|\n";
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    let ctx = LintContext::new(doc, MarkdownFlavor::MDG, None);
    assert_ne!(rule.fix(&ctx).unwrap(), doc, "MD060 left the Gherkin table unformatted");
}

/// Whether MD060 reports the unaligned table of `doc` as MkDocs. A table the
/// parser does not confirm is not reported.
fn mkdocs_reports(doc: &str) -> bool {
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    !rule.check(&ctx).unwrap().is_empty()
}

#[test]
fn mkdocs_a_closed_html_construct_before_the_table_does_not_hide_it() {
    for doc in [
        "<div markdown>\n\n<!-- comment -->\n\n|a|b|\n|-|-|\n\n</div>\n",
        "<div markdown>\n\n<em>Intro</em>\n\n|a|b|\n|-|-|\n\n</div>\n",
        "<div markdown>\n\n<div>x</div>\n\n|a|b|\n|-|-|\n\n</div>\n",
        "<div markdown>\n\n<span>\n\n|a|b|\n|-|-|\n\n</div>\n",
        "<div markdown>\n\n<div>\n\n|a|b|\n|-|-|\n\n</div>\n",
        "<div markdown>\n\n<em>Intro</em> text\n|a|b|\n|-|-|\n\n</div>\n",
        "!!! note\n\n    <!-- c -->\n\n    |a|b|\n    |-|-|\n",
        "!!! note\n\n    <!--\n    c\n    -->\n\n    |a|b|\n    |-|-|\n",
    ] {
        assert!(mkdocs_reports(doc), "the table is not reported: {doc:?}");
    }
}

#[test]
fn mkdocs_an_open_html_block_before_the_table_keeps_it_unconfirmed() {
    for doc in [
        "<div markdown>\n\n<div>\n|a|b|\n|-|-|\n</div>\n\n</div>\n",
        "<div markdown>\n\n<!-- c\n\n|a|b|\n|-|-|\n\n-->\n\n</div>\n",
        "!!! note\n\n    <div>\n    |a|b|\n    |-|-|\n    </div>\n",
        "!!! note\n\n    <pre>\n\n    |a|b|\n    |-|-|\n\n    </pre>\n",
    ] {
        assert!(!mkdocs_reports(doc), "a table inside raw HTML is reported: {doc:?}");
    }
}

#[test]
fn mkdocs_a_quoted_table_in_a_markdown_container_is_reported() {
    for doc in [
        "<div markdown>\n\n> |a|b|\n> |-|-|\n\n</div>\n",
        "!!! note\n\n    > |a|b|\n    > |-|-|\n",
        "!!! note\n\n    > |a|b|\n    > |-|-|\n    > |x|y|\n",
    ] {
        assert!(mkdocs_reports(doc), "the quoted table is not reported: {doc:?}");
    }
}

#[test]
fn mkdocs_a_quoted_table_in_a_markdown_div_is_formatted_keeping_its_quote() {
    let doc = "<div markdown>\n\n> |a|b|\n> |-|-|\n> |x|y|\n\n</div>\n";
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    assert_eq!(
        rule.fix(&ctx).unwrap(),
        "<div markdown>\n\n> | a   | b   |\n> | --- | --- |\n> | x   | y   |\n\n</div>\n"
    );
}

#[test]
fn an_escaped_backtick_in_a_mkdocs_header_does_not_open_a_code_span() {
    let doc = "| \\`a|b` |c|\n|-|-|-|\n|xxx|y|z|\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    assert!(ctx.table_blocks.len() == 1 && ctx.table_blocks[0].parser_confirmed);
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    assert_eq!(
        rule.fix(&ctx).unwrap(),
        "| \\`a | b`  | c   |\n| --- | --- | --- |\n| xxx | y   | z   |\n"
    );
}

#[test]
fn a_rejected_definition_does_not_reach_a_table_past_a_block_boundary() {
    for boundary in ["# Heading\n", "***\n", "```\ncode\n```\n", "<div>\nx\n</div>\n\n"] {
        let doc = format!("term\n:warning:\n{boundary}|a|b|\n|-|-|\n|xxx|y|\n");
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MkDocs] {
            let ctx = LintContext::new(&doc, flavor, None);
            assert!(
                ctx.table_blocks.iter().any(|block| block.parser_confirmed),
                "{doc:?} {flavor:?}"
            );
        }
    }
}

#[test]
fn a_table_after_a_list_marker_that_cannot_interrupt_a_paragraph_is_confirmed() {
    let doc = "intro\n2. text\n\n   |a|b|\n   |-|-|\n   |xxx|y|\n";
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MkDocs] {
        let ctx = LintContext::new(doc, flavor, None);
        assert!(
            ctx.table_blocks.iter().any(|block| block.parser_confirmed),
            "{flavor:?}"
        );
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "intro\n2. text\n\n   | a   | b   |\n   | --- | --- |\n   | xxx | y   |\n",
            "{flavor:?}"
        );
    }
}

#[test]
fn a_phantom_list_context_never_moves_a_table_that_is_not_at_its_content_column() {
    for doc in [
        "intro\n2. text\n\n  |a|b|\n  |-|-|\n  |xxx|y|\n",
        "intro\n2. text\n\n |a|b|\n |-|-|\n |xxx|y|\n",
    ] {
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MkDocs] {
            assert_structure_kept(doc, flavor);
        }
    }
}

/// The document holds exactly one table and the parser confirms it.
fn mkdocs_has_one_confirmed_table(doc: &str) -> bool {
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    ctx.table_blocks.len() == 1 && ctx.table_blocks[0].parser_confirmed
}

fn mkdocs_fix(rule: &dyn Rule, doc: &str) -> String {
    let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
    rule.fix(&ctx).unwrap()
}

/// Containers written as indentation: the fixed table must stay inside them.
const INDENTED_CONTAINERS: [(&str, &str); 6] = [
    ("!!! note\n\n", "    "),
    ("??? note\n\n", "    "),
    ("=== \"A\"\n\n", "    "),
    ("!!! a\n\n    !!! b\n\n", "        "),
    ("<div markdown>\n\n", "    "),
    ("!!! note\n\n    text\n", "    "),
];

#[test]
fn mkdocs_md060_keeps_the_indentation_of_a_table_in_an_indented_container() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    for (opener, indent) in INDENTED_CONTAINERS {
        let doc = format!("{opener}{indent}|a|b|\n{indent}|-|-|\n{indent}|x|y|\n");
        let expected = format!("{opener}{indent}| a   | b   |\n{indent}| --- | --- |\n{indent}| x   | y   |\n");
        assert_eq!(mkdocs_fix(&rule, &doc), expected, "{doc:?}");
        assert!(mkdocs_has_one_confirmed_table(&doc), "{doc:?}");
        assert!(
            mkdocs_has_one_confirmed_table(&expected),
            "the fixed table left its container: {expected:?}"
        );
    }
}

#[test]
fn mkdocs_md060_keeps_the_indentation_and_quote_of_a_quoted_table_in_a_container() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    for (opener, indent) in INDENTED_CONTAINERS {
        let doc = format!("{opener}{indent}> |a|b|\n{indent}> |-|-|\n{indent}> |x|y|\n");
        let expected = format!("{opener}{indent}> | a   | b   |\n{indent}> | --- | --- |\n{indent}> | x   | y   |\n");
        assert_eq!(mkdocs_fix(&rule, &doc), expected, "{doc:?}");
    }
}

#[test]
fn mkdocs_md055_keeps_the_indentation_of_every_row_in_an_indented_container() {
    let rule = MD055TablePipeStyle::new("leading_and_trailing".to_string());
    for (opener, indent) in INDENTED_CONTAINERS {
        let doc = format!("{opener}{indent}a|b\n{indent}-|-\n{indent}x|y\n");
        let expected = format!("{opener}{indent}| a|b |\n{indent}| -|- |\n{indent}| x|y |\n");
        assert_eq!(mkdocs_fix(&rule, &doc), expected, "{doc:?}");
        let quoted = format!("{opener}{indent}> a|b\n{indent}> -|-\n{indent}> x|y\n");
        let expected = format!("{opener}{indent}> | a|b |\n{indent}> | -|- |\n{indent}> | x|y |\n");
        assert_eq!(mkdocs_fix(&rule, &quoted), expected, "{quoted:?}");
    }
}

#[test]
fn mkdocs_md056_keeps_the_indentation_of_a_padded_row_in_an_indented_container() {
    use rumdl_lib::rules::MD056TableColumnCount;
    for (opener, indent) in INDENTED_CONTAINERS {
        let doc = format!("{opener}{indent}|a|b|\n{indent}|-|-|\n{indent}|x|\n");
        let expected = format!("{opener}{indent}|a|b|\n{indent}|-|-|\n{indent}| x |  |\n");
        assert_eq!(mkdocs_fix(&MD056TableColumnCount, &doc), expected, "{doc:?}");
        let quoted = format!("{opener}{indent}> |a|b|\n{indent}> |-|-|\n{indent}> |x|\n");
        let expected = format!("{opener}{indent}> |a|b|\n{indent}> |-|-|\n{indent}> | x |  |\n");
        assert_eq!(mkdocs_fix(&MD056TableColumnCount, &quoted), expected, "{quoted:?}");
    }
}

#[test]
fn mkdocs_md058_blank_line_before_a_table_keeps_it_inside_the_admonition() {
    let doc = "!!! note\n\n    text\n    |a|b|\n    |-|-|\n    |x|y|\n";
    let fixed = mkdocs_fix(&MD058BlanksAroundTables::default(), doc);
    assert_eq!(fixed, "!!! note\n\n    text\n\n    |a|b|\n    |-|-|\n    |x|y|\n");
    assert!(
        mkdocs_has_one_confirmed_table(&fixed),
        "the table left the admonition: {fixed:?}"
    );
}

#[test]
fn mkdocs_a_table_directly_after_the_container_opener_is_confirmed() {
    for doc in [
        "!!! note\n    |a|b|\n    |-|-|\n",
        "=== \"A\"\n    |a|b|\n    |-|-|\n",
        "!!! a\n    !!! b\n        |a|b|\n        |-|-|\n",
        "!!! note \"Title\"\n    |a|b|\n    |-|-|\n",
        "???+ note\n    |a|b|\n    |-|-|\n",
    ] {
        assert!(mkdocs_has_one_confirmed_table(doc), "{doc:?}");
    }
    let rule = MD060TableFormat::new(true, "aligned".to_string());
    assert_eq!(
        mkdocs_fix(&rule, "!!! note\n    |a|b|\n    |-|-|\n    |x|y|\n"),
        "!!! note\n    | a   | b   |\n    | --- | --- |\n    | x   | y   |\n"
    );
}

#[test]
fn mkdocs_a_table_continuing_the_opener_paragraph_is_not_confirmed() {
    for doc in [
        "!!! note\n    text\n    |a|b|\n    |-|-|\n    |x|y|\n",
        "??? note\n    text\n    more\n    |a|b|\n    |-|-|\n",
        "!!! note\n    <pre>\n    </pre>\n    |a|b|\n    |-|-|\n",
        "!!! note\n    <pre>\n        code\n    |a|b|\n    |-|-|\n",
        "!!! outer\n    !!! a\u{301}\n        |a|b|\n        |-|-|\n",
        "\u{a0}!!! note\n    |a|b|\n    |-|-|\n",
    ] {
        assert!(!mkdocs_has_one_confirmed_table(doc), "{doc:?}");
    }
}

#[test]
fn a_table_after_an_opener_like_line_outside_mkdocs_stays_a_paragraph_continuation() {
    let doc = "!!! note\n    |a|b|\n    |-|-|\n";
    for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::Quarto, MarkdownFlavor::MyST] {
        let ctx = LintContext::new(doc, flavor, None);
        assert!(
            ctx.table_blocks.iter().all(|block| !block.parser_confirmed),
            "{flavor:?}"
        );
    }
}

#[test]
fn mkdocs_indented_code_after_the_container_opener_is_not_a_table() {
    for doc in [
        "!!! note\n    ```\n    |a|b|\n    |-|-|\n    ```\n",
        "!!! note\n    text\n\n        |a|b|\n        |-|-|\n",
        "!!! note\n    text\nlazy\n    |a|b|\n    |-|-|\n",
        "!!! note\n        |a|b|\n        |-|-|\n",
        "!!! note\n        code\n    |a|b|\n    |-|-|\n",
        "!!! note\n    text\n        |a|b|\n        |-|-|\n",
        "!!! note\n    !!!\n        |a|b|\n        |-|-|\n",
        "!!! note\n    <https://markdown.org>\n        |a|b|\n        |-|-|\n",
        "!!! note\n    === ===\n        |a|b|\n        |-|-|\n",
        "!!! note\n        !!! tip\n            |a|b|\n            |-|-|\n",
        "<div>\n!!! note\n    |a|b|\n    |-|-|\n</div>\n",
        "!!! note\n<div>\n!!! tip\n    |a|b|\n    |-|-|\n</div>\n",
        "!!! note garbage!\n    |a|b|\n    |-|-|\n",
        "!!!  note\n    |a|b|\n    |-|-|\n",
        "???  note\n    |a|b|\n    |-|-|\n",
        "=== Label\n    |a|b|\n    |-|-|\n",
        " !!! note\n     |a|b|\n     |-|-|\n",
        "!!! a\n    text\n\n    !!! b\n      |a|b|\n      |-|-|\n",
        "!!! a\n     !!! b\n         |a|b|\n         |-|-|\n",
        "!!! note\n    <pre>\n    !!! tip\n        |a|b|\n        |-|-|\n    </pre>\n",
        "<pre>\n\n!!! note\n    |a|b|\n    |-|-|\n</pre>\n",
        "<script>\n\n!!! note\n    |a|b|\n    |-|-|\n</script>\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
        assert!(ctx.table_blocks.iter().all(|block| !block.parser_confirmed), "{doc:?}");
    }
}

/// Indented code that looks like a table with a short last row, one per container.
const CODE_LOOKING_LIKE_A_TABLE: &[(&str, MarkdownFlavor)] = &[
    ("Text\n\n    |a|b|\n    |-|-|\n    |x|\n", MarkdownFlavor::Standard),
    ("Text\n\n    |a|b|\n    |-|-|\n    |x|\n", MarkdownFlavor::MkDocs),
    (
        "> Text\n>\n>     |a|b|\n>     |-|-|\n>     |x|\n",
        MarkdownFlavor::Standard,
    ),
    (
        "- Text\n\n      |a|b|\n      |-|-|\n      |x|\n",
        MarkdownFlavor::Standard,
    ),
    (
        "1. Text\n\n       |a|b|\n       |-|-|\n       |x|\n",
        MarkdownFlavor::Standard,
    ),
    (
        "!!! note\n    Text\n\n        |a|b|\n        |-|-|\n        |x|\n",
        MarkdownFlavor::MkDocs,
    ),
    (
        "??? note\n    Text\n\n        |a|b|\n        |-|-|\n        |x|\n",
        MarkdownFlavor::MkDocs,
    ),
    (
        "=== \"Tab\"\n    Text\n\n        |a|b|\n        |-|-|\n        |x|\n",
        MarkdownFlavor::MkDocs,
    ),
    (
        "<div markdown>\n\n    Text\n\n        |a|b|\n        |-|-|\n        |x|\n\n</div>\n",
        MarkdownFlavor::MkDocs,
    ),
];

#[test]
fn md056_never_warns_on_or_edits_indented_code() {
    let md056 = MD056TableColumnCount;
    for &(doc, flavor) in CODE_LOOKING_LIKE_A_TABLE {
        let ctx = LintContext::new(doc, flavor, None);
        let warnings = md056.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "MD056 warned on code: {doc:?} {flavor:?}: {warnings:?}"
        );
        assert_eq!(md056.fix(&ctx).unwrap(), doc, "MD056 edited code: {doc:?} {flavor:?}");
        assert_eq!(cli_fix(doc, "MD056"), doc, "MD056 CLI edited code: {doc:?} {flavor:?}");
        assert_untouched(doc, flavor);
    }
}

#[test]
fn md056_still_fixes_a_short_row_where_four_spaces_are_content() {
    let md056 = MD056TableColumnCount;
    for (doc, expected) in [
        (
            "!!! note\n    |a|b|\n    |-|-|\n    |x|\n",
            "!!! note\n    |a|b|\n    |-|-|\n    | x |  |\n",
        ),
        (
            "<div markdown>\n\n    |a|b|\n    |-|-|\n    |x|\n\n</div>\n",
            "<div markdown>\n\n    |a|b|\n    |-|-|\n    | x |  |\n\n</div>\n",
        ),
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::MkDocs, None);
        assert_eq!(md056.fix(&ctx).unwrap(), expected, "{doc:?}");
    }
}

#[test]
fn md056_still_reports_a_delimiter_row_with_the_wrong_cell_count_in_every_container() {
    let md056 = MD056TableColumnCount;
    for (first, rest) in [("", ""), ("> ", "> "), ("- ", "  "), ("1. ", "   ")] {
        let doc = contained(first, rest, &["|a|b|", "|-|", "|x|y|"]);
        let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
        assert!(!md056.check(&ctx).unwrap().is_empty(), "no warning for {doc:?}");
    }
}

#[test]
fn md056_fixes_are_idempotent_in_every_container() {
    for &(name, first, rest) in CONTAINERS {
        let doc = contained(first, rest, &["|a|b|", "|-|", "|x|y|z|"]);
        let once = cli_fix(&doc, "MD056");
        assert_eq!(
            cli_fix(&once, "MD056"),
            once,
            "MD056 is not idempotent in {name}: {doc:?}"
        );
    }
}
/// A header written after an ordered marker other than `1.` is cell text when the
/// marker could not start an item (it would interrupt a paragraph) and a quoted
/// table in an item when it can. The parser decides, so the table is confirmed and
/// checked exactly when pandoc reads a table.
#[test]
fn a_quoted_table_after_a_numbered_marker_is_checked_when_the_parser_opens_an_item() {
    let rows = "> |a|b|\n   > |-|-|\n   > |x|\n";
    let cases: &[(&str, &str, bool)] = &[
        ("", "2. ", true),
        ("", "1. ", true),
        ("para\n\n", "2. ", true),
        ("# Heading\n", "2. ", true),
        ("- outer\n\n  ", "2. ", true),
        ("para\n", "2. ", false),
        ("- outer\n  ", "2. ", false),
        ("- outer\n  para\n  ", "2. ", false),
    ];
    for &(above, marker, is_table) in cases {
        let indent = " ".repeat(above.rsplit('\n').next().unwrap_or("").len() + marker.len() - 3);
        let table_rows = rows.replace("\n   ", &format!("\n{indent}   "));
        let doc = format!("{above}{marker}{table_rows}");
        let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
        let confirmed = ctx.table_blocks.iter().any(|block| block.parser_confirmed);
        assert_eq!(confirmed, is_table, "{doc:?}");
        let warnings = MD056TableColumnCount.check(&ctx).unwrap();
        assert_eq!(warnings.len(), usize::from(is_table), "{doc:?}: {warnings:?}");
    }
}

#[test]
fn md058_still_requires_a_blank_line_after_a_bare_marker_that_opens_no_item() {
    let md058 = MD058BlanksAroundTables::default();
    for doc in ["intro\n2.\n   |a|b|\n   |-|-|\n", "intro\n-\n  |a|b|\n  |-|-|\n"] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(!md058.check(&ctx).unwrap().is_empty(), "no warning for {doc:?}");
    }
    for doc in [
        "-\n  |a|b|\n  |-|-|\n",
        "- a\n-\n  |a|b|\n  |-|-|\n",
        "> -\n>   |a|b|\n>   |-|-|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(md058.check(&ctx).unwrap().is_empty(), "warned for {doc:?}");
    }
}

#[test]
fn md056_checks_a_table_in_a_nested_item_longer_than_the_confirmation_window() {
    let body = "      body words\n".repeat(3000);
    for doc in [
        format!("- outer\n  - inner\n    - deep\n\n{body}\n      |a|b|\n      |-|\n      |x|y|\n"),
        format!("1. outer\n   1. inner\n      1. deep\n\n{body}\n         |a|b|\n         |-|\n         |x|y|\n"),
    ] {
        let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
        let warnings = MD056TableColumnCount.check(&ctx).unwrap();
        assert!(
            warnings
                .iter()
                .any(|w| w.message.contains("delimiter") || w.message.contains("cells")),
            "the table in the nested item is not checked: {warnings:?}"
        );
    }
}

#[test]
fn md056_still_repairs_a_table_longer_than_the_confirmation_window() {
    let mut doc = String::from("intro\n|a|b|\n|-|\n");
    doc.push_str(&"|x|y|\n".repeat(6000));
    let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
    let warnings = MD056TableColumnCount.check(&ctx).unwrap();
    assert!(
        warnings.iter().any(|w| w.line == 3),
        "the short delimiter row of a 6000-row table is not reported"
    );
}

#[test]
fn md058_requires_a_blank_line_after_a_setext_underline_inside_a_list_item() {
    let md058 = MD058BlanksAroundTables::default();
    for doc in [
        "- paragraph\n  -\n  |a|b|\n  |-|-|\n",
        "1. paragraph\n   -\n   |a|b|\n   |-|-|\n",
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        assert!(!md058.check(&ctx).unwrap().is_empty(), "no warning for {doc:?}");
    }
    // A bare marker at the marker column of the item above is a sibling item.
    let doc = "- a\n-\n  |a|b|\n  |-|-|\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    assert!(md058.check(&ctx).unwrap().is_empty(), "warned for {doc:?}");
}

#[test]
fn md060_formats_a_list_table_followed_by_a_tab_indented_block() {
    let doc = "- |a|bbbb|\n  |-|-|\n\t# Heading\n";
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    let md060 = MD060TableFormat::new(true, "aligned".to_string());
    assert!(!md060.check(&ctx).unwrap().is_empty(), "table not formatted: {doc:?}");
}

#[test]
fn a_table_in_an_admonition_longer_than_the_slice_limit_is_still_formatted() {
    let md060 = MD060TableFormat::new(true, "aligned".to_string());
    for (rows, expected) in [(2, 4), (7000, 7002)] {
        let doc = format!(
            "# T\n\n!!! note\n\n    |a|bbbb|\n    |-|-|\n{}",
            "    |xx|y|\n".repeat(rows)
        );
        let ctx = LintContext::new(&doc, MarkdownFlavor::MkDocs, None);
        assert_eq!(md060.check(&ctx).unwrap().len(), expected, "{rows} rows");
    }
}

#[test]
fn a_repairable_table_after_a_long_list_item_body_is_still_reported() {
    let paragraph = format!("  {}\n\n", "word ".repeat(15));
    for repeats in [5, 500] {
        let doc = format!(
            "# T\n\n- item\n\n{}  intro\n  |a|b|\n  |-|\n  |x|y|\n",
            paragraph.repeat(repeats)
        );
        let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
        let warnings = MD056TableColumnCount.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{repeats} paragraphs: {warnings:?}");
    }
}

#[test]
fn a_row_far_below_the_confirmation_window_is_still_checked() {
    let rows = "|x|y|\n".repeat(6000);
    let indented = "    |x|y|\n".repeat(6000);
    for (doc, flavor, line) in [
        (format!("|a|b|\n|-|-|\n{rows}|x|y|z|\n"), MarkdownFlavor::Standard, 6003),
        (
            format!("!!! note\n\n    |a|b|\n    |-|-|\n{indented}    |x|y|z|\n"),
            MarkdownFlavor::MkDocs,
            6005,
        ),
    ] {
        let ctx = LintContext::new(&doc, flavor, None);
        let lines: Vec<usize> = MD056TableColumnCount
            .check(&ctx)
            .unwrap()
            .iter()
            .map(|w| w.line)
            .collect();
        assert_eq!(lines, vec![line], "{flavor:?}");
    }
}

#[test]
fn a_flavor_masked_header_in_a_long_list_item_keeps_its_container() {
    // The masked pipe sends the table through the bounded reparse; at column four
    // the rows are code unless the list item around them is part of the slice.
    let paragraph = format!("    {}\n\n", "word ".repeat(15));
    for repeats in [5, 500] {
        let doc = format!(
            "-   item\n\n{}    intro\n    |`a|b`|c|\n    |-|-|\n    |x|y|\n",
            paragraph.repeat(repeats)
        );
        let ctx = LintContext::new(&doc, MarkdownFlavor::MkDocs, None);
        let warnings = MD058BlanksAroundTables::default().check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{repeats} paragraphs: {warnings:?}");
    }
}

#[test]
fn a_flavor_masked_long_table_ends_where_the_scanner_says() {
    let doc = format!("intro\n\n|`a|b`|c|\n|-|-|\n{}# Heading\n", "|x|y|\n".repeat(6000));
    let ctx = LintContext::new(&doc, MarkdownFlavor::MkDocs, None);
    let lines: Vec<usize> = MD058BlanksAroundTables::default()
        .check(&ctx)
        .unwrap()
        .iter()
        .map(|w| w.line)
        .collect();
    assert_eq!(lines, vec![6004], "the heading follows the last row directly");
}

#[test]
fn md058_treats_inline_html_and_type_7_tags_as_paragraph_text_before_a_setext_underline() {
    let md058 = MD058BlanksAroundTables::default();
    // None of these can interrupt a paragraph or open an HTML block that swallows
    // the next line, so the `-` underlines a heading and the table follows it.
    for first in [
        "<em>Intro</em>",
        "<https://x.org>",
        "<mailto:a@b.org>",
        "<span>Intro</span>",
        "<a href=\"x\">Intro</a>",
        "<span>Intro",
        "< not a tag",
        "<3 Intro",
    ] {
        let doc = format!("{first}\n-\n  |a|b|\n  |-|-|\n");
        let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
        let warnings = md058.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{doc:?}: {warnings:?}");
    }
}

#[test]
fn md058_keeps_a_bare_marker_after_an_html_block_start_an_item() {
    let md058 = MD058BlanksAroundTables::default();
    // A line the parser reads as the start of an HTML block of types 1-6 is not
    // paragraph text; when it ends on its own line the `-` opens an empty item.
    for first in [
        "<div></div>",
        "</div>",
        "<table></table>",
        "<h1>Intro</h1>",
        "<!-- note -->",
        "<?php echo 1; ?>",
        "<!DOCTYPE html>",
        "<![CDATA[x]]>",
        "<script></script>",
        "<pre></pre>",
        "<style></style>",
        "<textarea></textarea>",
    ] {
        let doc = format!("{first}\n-\n  |a|b|\n  |-|-|\n");
        let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
        let warnings = md058.check(&ctx).unwrap();
        assert!(warnings.is_empty(), "{doc:?}: {warnings:?}");
    }
}

#[test]
fn md058_leaves_a_lone_type_7_tag_line_and_what_it_swallows_alone() {
    // A complete tag alone on its line opens an HTML block that runs to the next
    // blank line, so the lines after it are raw HTML and no table exists.
    for first in ["<span>", "</span>", "<custom-tag>", "<br/>"] {
        let doc = format!("{first}\n-\n  |a|b|\n  |-|-|\n");
        let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
        let warnings = MD058BlanksAroundTables::default().check(&ctx).unwrap();
        assert!(warnings.is_empty(), "{doc:?}: {warnings:?}");
    }
}

/// A bare `-` before an indented table either opens an empty list item that holds the
/// table (no blank line can go before it) or underlines the paragraph above as a
/// setext heading (the table follows the heading and needs the blank line). The
/// parser decides which, so every opener and container is checked against it.
#[test]
fn md058_follows_the_parser_on_whether_a_bare_marker_opens_an_item() {
    let md058 = MD058BlanksAroundTables::default();
    let table = "|a|b|\n|-|-|\n";
    let indent = |prefix: &str, pad: &str| -> String {
        table
            .lines()
            .map(|row| format!("{prefix}{pad}{row}\n"))
            .collect::<String>()
    };
    // (lines above the bare marker, expected warnings)
    let cases: &[(&str, usize)] = &[
        // A numbered line that cannot interrupt a paragraph is paragraph text.
        ("paragraph\n2. text\n", 1),
        ("paragraph\n1. text\n", 0),
        ("paragraph\n- text\n", 0),
        ("1. text\n", 0),
        ("2. text\n", 0),
        ("- text\n", 0),
        ("Intro\n", 1),
        ("<em>Intro</em>\n", 1),
        ("<https://x.org>\n", 1),
        // A paragraph-interrupting HTML block swallows the lines after it.
        ("<div>x</div>\n", 0),
    ];
    for &(above, expected) in cases {
        for quote in ["", "> "] {
            let quoted_above: String = above.lines().map(|line| format!("{quote}{line}\n")).collect();
            let doc = format!("{quoted_above}{quote}-\n{}", indent(quote, "  "));
            let ctx = LintContext::new(&doc, MarkdownFlavor::Standard, None);
            let warnings = md058.check(&ctx).unwrap();
            assert_eq!(warnings.len(), expected, "{doc:?}: {warnings:?}");
        }
    }
}

#[test]
fn md058_follows_the_parser_on_a_bare_marker_nested_in_a_list_item() {
    let md058 = MD058BlanksAroundTables::default();
    for (doc, expected) in [
        ("- outer\n  paragraph\n  2. text\n  -\n    |a|b|\n    |-|-|\n", 1),
        ("- outer\n  paragraph\n  1. text\n  -\n    |a|b|\n    |-|-|\n", 0),
        ("- outer\n  para\n  -\n    |a|b|\n    |-|-|\n", 1),
        ("- outer\n  - inner\n  -\n    |a|b|\n    |-|-|\n", 0),
    ] {
        let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
        let warnings = md058.check(&ctx).unwrap();
        assert_eq!(warnings.len(), expected, "{doc:?}: {warnings:?}");
    }
}

#[test]
fn rewrites_keep_the_structure_when_a_marker_the_parser_rejects_precedes_the_table() {
    for doc in [
        "paragraph\n2. text\n-\n  |a|b|\n  |-|-|\n",
        "> paragraph\n> 2. text\n> -\n>   |a|b|\n>   |-|-|\n",
        "> Intro\n> -\n>   |a|b|\n>   |-|-|\n",
        "- outer\n  paragraph\n  2. text\n  -\n    |a|b|\n    |-|-|\n",
        "- outer\n  para\n  -\n    |a|b|\n    |-|-|\n",
    ] {
        assert_structure_kept(doc, MarkdownFlavor::Standard);
    }
}

// ---- MD055 never changes what a table says ------------------------------------------

fn pulldown_html(doc: &str) -> String {
    let mut out = String::new();
    pulldown_cmark::html::push_html(
        &mut out,
        pulldown_cmark::Parser::new_ext(doc, pulldown_cmark::Options::ENABLE_TABLES),
    );
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

fn md055_styles() -> Vec<MD055TablePipeStyle> {
    [
        "leading_and_trailing",
        "no_leading_or_trailing",
        "leading_only",
        "trailing_only",
        "consistent",
    ]
    .iter()
    .map(|style| MD055TablePipeStyle::new((*style).to_string()))
    .collect()
}

/// `fix` under `rule`, asserting that both oracles read the same document after
/// it as before and that a second fix changes nothing.
fn assert_md055_keeps_meaning(rule: &MD055TablePipeStyle, doc: &str, comrak_before: &str, pulldown_before: &str) {
    let ctx = LintContext::new(doc, MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    if fixed == doc {
        return;
    }
    assert_eq!(html(&fixed), comrak_before, "comrak: {doc:?} became {fixed:?}");
    assert_eq!(
        pulldown_html(&fixed),
        pulldown_before,
        "pulldown: {doc:?} became {fixed:?}"
    );
    let again = rule
        .fix(&LintContext::new(&fixed, MarkdownFlavor::Standard, None))
        .unwrap();
    assert_eq!(again, fixed, "not idempotent: {doc:?}");
}

/// The four ways a row can carry its outer pipes, for cells `cells`.
fn row_shapes(cells: &[&str]) -> Vec<String> {
    let inner = cells.join(" | ");
    vec![
        format!("| {inner} |"),
        format!("{inner}"),
        format!("| {inner}"),
        format!("{inner} |"),
    ]
}

/// Every cell assignment of `cols` columns drawn from `alphabet`.
fn cell_rows<'a>(cols: usize, alphabet: &[&'a str]) -> Vec<Vec<&'a str>> {
    let mut rows: Vec<Vec<&str>> = vec![vec![]];
    for _ in 0..cols {
        rows = rows
            .into_iter()
            .flat_map(|row| {
                alphabet.iter().map(move |cell| {
                    let mut next = row.clone();
                    next.push(cell);
                    next
                })
            })
            .collect();
    }
    rows
}

fn assert_all_md055_styles_keep_meaning(doc: &str) {
    let comrak_before = html(doc);
    let pulldown_before = pulldown_html(doc);
    for rule in md055_styles() {
        assert_md055_keeps_meaning(&rule, doc, &comrak_before, &pulldown_before);
    }
}

/// Every table of one to three columns with empty and filled cells, each row
/// written in each of the four pipe shapes: no style rewrite may change how
/// either parser reads it, whatever it does (including nothing).
#[test]
fn md055_keeps_the_meaning_of_every_small_table() {
    for (cols, max_body) in [(1usize, 2usize), (2, 2), (3, 1)] {
        let cell_sets = cell_rows(cols, &["", "a"]);
        let delimiter_cells = vec!["-"; cols];
        let delimiters = row_shapes(&delimiter_cells);
        let rows: Vec<String> = cell_sets.iter().flat_map(|cells| row_shapes(cells)).collect();
        for header in &rows {
            for delimiter in &delimiters {
                let mut bodies: Vec<Vec<&String>> = vec![vec![]];
                for _ in 0..max_body {
                    let grown: Vec<Vec<&String>> = bodies
                        .iter()
                        .filter(|body| body.len() < max_body)
                        .flat_map(|body| {
                            rows.iter().map(move |row| {
                                let mut next = body.clone();
                                next.push(row);
                                next
                            })
                        })
                        .collect();
                    bodies.extend(grown);
                }
                for body in bodies {
                    let mut doc = format!("{header}\n{delimiter}\n");
                    for row in body {
                        doc.push_str(row);
                        doc.push('\n');
                    }
                    assert_all_md055_styles_keep_meaning(&doc);
                }
            }
        }
    }
}

/// A cell that reads as a block when it opens the row, or that carries text a
/// careless trim would eat: placed in every position of 1 and 2 column tables
/// in every pipe shape.
#[test]
fn md055_keeps_the_meaning_of_tables_with_special_cells() {
    let specials = [
        "# a",
        "- a",
        "+ a",
        "* a",
        "> a",
        "1. a",
        "1) a",
        "<b>",
        "[a]: /u",
        "```",
        "~~~",
        "$$",
        ": a",
        "\u{2003}a",
        "a\u{2003}",
        "a\\",
        "a\\\\",
        "-",
        "- -",
        ":-:",
    ];
    for special in specials {
        for cols in [1usize, 2] {
            for position in 0..cols {
                let mut cells = vec!["a"; cols];
                cells[position] = special;
                let plain = vec!["a"; cols];
                let delimiter = vec!["-"; cols];
                for header in row_shapes(&cells) {
                    for delim in row_shapes(&delimiter) {
                        for body in row_shapes(&cells) {
                            assert_all_md055_styles_keep_meaning(&format!("{header}\n{delim}\n{body}\n"));
                        }
                    }
                }
                for header in row_shapes(&plain) {
                    for delim in row_shapes(&delimiter) {
                        for body in row_shapes(&cells) {
                            assert_all_md055_styles_keep_meaning(&format!("{header}\n{delim}\n{body}\n"));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn md055_keeps_the_meaning_of_empty_one_column_tables() {
    for doc in ["||\n|-|\n||\n", "| |\n|-|\n| |\n| a |\n", "|\n|-|\n|\n"] {
        assert_all_md055_styles_keep_meaning(doc);
    }
}

/// A trailing pipe after a backslash is cell text for both parsers, whatever the
/// count of backslashes.
#[test]
fn md055_keeps_a_pipe_that_follows_backslashes() {
    for slashes in ["\\", "\\\\", "\\\\\\"] {
        let doc = format!("| h | j |\n| - | - |\n| a | b{slashes}|\n");
        assert_all_md055_styles_keep_meaning(&doc);
    }
}

/// `rumdl check --fix` with MD055 set to `style`: (file afterwards, stdout).
fn cli_md055(doc: &str, style: &str) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("doc.md");
    std::fs::write(&file, doc).unwrap();
    let config = dir.path().join("rumdl.toml");
    std::fs::write(&config, format!("[MD055]\nstyle = \"{style}\"\n")).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--fix", "--no-cache", "--enable", "MD055", "--config"])
        .arg(&config)
        .arg(&file)
        .output()
        .unwrap();
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "rumdl failed on {doc:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        std::fs::read_to_string(&file).unwrap(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// The reported repro: stripping the pipes of a one-column table made a setext
/// heading and a paragraph. Real binary, both oracles, plain and nested.
#[test]
fn cli_md055_leaves_one_column_tables_as_tables() {
    let table = "| a |\n| - |\n| x |\n| y |\n";
    let variants = [
        table.to_string(),
        table.lines().map(|line| format!("- {line}\n")).collect::<String>(),
        table.lines().map(|line| format!("> {line}\n")).collect::<String>(),
        table.lines().map(|line| format!("> - {line}\n")).collect::<String>(),
    ];
    for variant in variants {
        for style in [
            "no_leading_or_trailing",
            "trailing_only",
            "leading_only",
            "leading_and_trailing",
        ] {
            let (fixed, _) = cli_md055(&variant, style);
            assert_eq!(html(&fixed), html(&variant), "{style}: {variant:?} became {fixed:?}");
            assert_eq!(
                pulldown_html(&fixed),
                pulldown_html(&variant),
                "{style}: {variant:?} -> {fixed:?}"
            );
            assert_eq!(
                cli_md055(&fixed, style).0,
                fixed,
                "{style}: not idempotent on {variant:?}"
            );
        }
        // The two styles with no pipe-free form report nothing at all.
        for style in ["no_leading_or_trailing", "trailing_only"] {
            let (fixed, out) = cli_md055(&variant, style);
            assert_eq!(fixed, variant, "{style}");
            assert!(!out.contains("MD055"), "{style}: {out}");
        }
    }
    // Controls: the leading-pipe style is applied (the rule is live) and the same
    // document in two columns is restyled as before.
    assert_eq!(cli_md055(table, "leading_only").0, "| a\n| -\n| x\n| y\n");
    assert_eq!(
        cli_md055("| a | b |\n| - | - |\n| x | y |\n", "no_leading_or_trailing").0,
        "a | b\n| - | - |\nx | y\n"
    );
}
