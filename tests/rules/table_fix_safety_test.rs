//! Table-fixing rules (MD055, MD058, MD060) must only rewrite what the Markdown
//! parser reads as a table. Structure is judged by comrak (GFM, as GitHub renders it).

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD055TablePipeStyle, MD058BlanksAroundTables, MD060TableFormat};

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
        assert_eq!(ctx.table_blocks.len(), 1, "{doc:?}");
        assert!(!ctx.table_blocks[0].parser_confirmed, "{doc:?}");
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
