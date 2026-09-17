use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD003HeadingStyle;
use rumdl_lib::rules::heading_utils::HeadingStyle;

#[test]
fn test_consistent_atx() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_atx_closed() {
    let rule = MD003HeadingStyle::new(HeadingStyle::AtxClosed);
    let content = "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_styles() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n## Heading 2 ##\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_fix_mixed_styles() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n## Heading 2 ##\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1\n## Heading 2\n### Heading 3");
}

#[test]
fn test_fix_to_atx_closed() {
    let rule = MD003HeadingStyle::new(HeadingStyle::AtxClosed);
    let content = "# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_indented_headings() {
    let rule = MD003HeadingStyle::default();
    let content = "  # Heading 1\n  ## Heading 2\n  ### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_indentation() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n  ## Heading 2\n    ### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_preserve_content() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading with *emphasis* and **bold**\n## Another heading with [link](url)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, content);
}

#[test]
fn test_empty_headings() {
    let rule = MD003HeadingStyle::default();
    let content = "#\n##\n###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_heading_with_trailing_space() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1  \n## Heading 2  \n### Heading 3  ";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_setext() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading 1\n=========\n\nHeading 2\n---------";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_setext_atx() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading 1\n=========\n\n## Heading 2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4);
}

#[test]
fn test_fix_to_setext() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "# Heading 1\n## Heading 2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Heading 1\n=========\nHeading 2\n---------");
}

#[test]
fn test_setext_with_formatting() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading with *emphasis*\n====================\n\nHeading with **bold**\n--------------------";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_mixed_setext_atx() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading 1\n=========\n\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Heading 1\n=========\n\nHeading 2\n---------\n### Heading 3");
}

#[test]
fn test_fix_setext_to_atx_removes_the_underline() {
    // Converting a setext heading to ATX has to consume the underline as well.
    // Leaving it behind turns it into a thematic break, adding a horizontal
    // rule the document never had.
    let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
    let content = "Heading 1\n=========\n\nHeading 2\n---------\n\nBody text.\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1\n\n## Heading 2\n\nBody text.\n");
}

#[test]
fn test_fix_setext_to_atx_closed_removes_the_underline() {
    let rule = MD003HeadingStyle::new(HeadingStyle::AtxClosed);
    let content = "Heading 1\n=========\n\nBody text.\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1 #\n\nBody text.\n");
}

#[test]
fn test_fix_setext_to_atx_preserves_indentation() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
    let content = "  Heading 1\n  =========\n\nBody text.\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "  # Heading 1\n\nBody text.\n");
}

#[test]
fn test_underline_inside_an_html_block_is_html() {
    // `<span>` alone on its line opens an HTML block running to the blank
    // line, so the lines below it are raw HTML with no heading to rewrite.
    let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
    for content in ["# Intro\n\n<span>\nTitle\n===\n", "# Intro\n\n<span>\n===\n"] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty(), "{content:?}");
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }
    // A blank line ends the block, so the heading below it is rewritten.
    let content = "# Intro\n\n<span>\n\nTitle\n===\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(rule.fix(&ctx).unwrap(), "# Intro\n\n<span>\n\n# Title\n");
    // A tag inside front matter opens no block over the document below it.
    let content = "---\nhtml: |\n\n  <span>\n---\nTitle\n===\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(rule.fix(&ctx).unwrap(), "---\nhtml: |\n\n  <span>\n---\n# Title\n");
}

#[test]
fn test_setext_heading_with_a_short_dash_underline_is_checked() {
    // A single `-` or `--` under prose underlines it, so the document holds a
    // heading for the rule to check however few hyphens it contains.
    let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
    for (content, fixed) in [
        ("Title\n-\n", "## Title\n"),
        ("Title\n--\n", "## Title\n"),
        ("Title\n- \n\nText\n", "## Title\n\nText\n"),
    ] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(!rule.should_skip(&ctx), "{content:?}");
        let lines: Vec<_> = rule.check(&ctx).unwrap().iter().map(|warning| warning.line).collect();
        assert_eq!(lines, [1], "{content:?}");
        assert_eq!(rule.fix(&ctx).unwrap(), fixed, "{content:?}");
    }
}

#[test]
fn test_fmt_converts_a_setext_heading_with_a_one_character_underline() {
    // `fmt` rewrites a file only when its lint pass reports a warning, so the
    // heading has to survive every filter between the file and the rule.
    for (content, fixed) in [
        ("Title\n-\n", "## Title\n"),
        ("Title\n=\n", "# Title\n"),
        ("Title\n---\n", "## Title\n"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("doc.md");
        std::fs::write(&file, content).unwrap();
        let config = dir.path().join("rumdl.toml");
        std::fs::write(&config, "[global]\nenable = [\"MD003\"]\n\n[MD003]\nstyle = \"atx\"\n").unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .current_dir(dir.path())
            .args(["fmt", "--color", "never", "--no-cache", "--config"])
            .arg(&config)
            .arg("doc.md")
            .output()
            .expect("rumdl runs");
        assert!(
            output.status.code().is_some(),
            "fmt did not exit normally: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), fixed, "{content:?}");
    }
}

#[test]
fn test_fix_is_idempotent_when_style_counts_tie() {
    // `style = consistent` picks the most prevalent style, so rewriting one
    // heading can flip the tiebreaker and make the next pass rewrite a
    // different heading. Fixing must reach a fixpoint in one pass.
    let rule = MD003HeadingStyle::default();
    let content = "``\n---\n### #";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let once = rule.fix(&ctx).unwrap();
    let ctx2 = LintContext::new(&once, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let twice = rule.fix(&ctx2).unwrap();
    assert_eq!(once, twice, "MD003 fix is not idempotent");
}

#[test]
fn test_setext_with_indentation() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "  Heading 1\n  =========\n\n  Heading 2\n  ---------";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_with_front_matter() {
    let rule = MD003HeadingStyle::default();
    let content = "---\ntitle: \"Test Document\"\nauthor: \"Test Author\"\ndate: \"2024-04-03\"\n---\n\n# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings with front matter followed by ATX headings, but got {} warnings",
        result.len()
    );
}

#[test]
fn test_yaml_like_content_detected_as_setext_heading() {
    // Per CommonMark and markdownlint-cli: `---` in mid-document is a Setext underline,
    // not frontmatter. This content creates a Setext heading "config: value" with the `---`.
    // markdownlint-cli flags this as MD003 (heading style mismatch).
    let rule = MD003HeadingStyle::default();
    let content = "# Real Heading\n\n---\nconfig: value\nsetting: another value\n---\n\nMore content.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // The `---` after "setting: another value" creates a Setext h2 heading,
    // which conflicts with the ATX style used for "# Real Heading"
    assert!(
        !result.is_empty(),
        "Expected warning for Setext heading style mismatch, but got none"
    );
}

#[test]
fn test_legitimate_setext_headings_still_work() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Main Title\n==========\n\nSubtitle\n--------\n\nContent here.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Legitimate Setext headings should still work, but got {} warnings: {:?}",
        result.len(),
        result
    );
}

#[test]
fn test_setext_heading_starting_with_emphasis_is_checked() {
    // `**Practice**` is paragraph text, not a list item, so the underline makes
    // it a setext heading that breaks the ATX style the document opened with.
    let rule = MD003HeadingStyle::default();
    let content = "# Intro\n\n**Practice**\n---\n\nText.\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "setext heading must be checked: {result:?}");
    assert_eq!(result[0].line, 3);
    assert_eq!(rule.fix(&ctx).unwrap(), "# Intro\n\n## **Practice**\n\nText.\n");
}

#[test]
fn test_setext_heading_in_footnote_body_is_checked() {
    // A footnote body holds Markdown blocks, so an underline indented to the
    // body's edge makes a heading there, rewritten in place.
    let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
    let content = "# Intro\n\nRef[^a].\n\n[^a]: Note.\n\n    Heading\n    ===\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let lines: Vec<_> = rule.check(&ctx).unwrap().iter().map(|warning| warning.line).collect();
    assert_eq!(lines, [7]);
    assert_eq!(
        rule.fix(&ctx).unwrap(),
        "# Intro\n\nRef[^a].\n\n[^a]: Note.\n\n    # Heading\n"
    );

    // Text on the label line sits inside the body the label opens, and a
    // marker written before the label would end the definition.
    let content = "# Intro\n\nRef[^a].\n\n[^a]: Heading\n    ===\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert!(rule.check(&ctx).unwrap().is_empty());
    assert_eq!(rule.fix(&ctx).unwrap(), content);
}

#[test]
fn test_mdx_underline_below_a_nested_closing_tag_is_paragraph_text() {
    // `===` is a paragraph among `<Outer>`'s children and `text` one inside
    // `<Inner>`, so neither line is a heading to rewrite.
    let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
    let content = "<Outer>\n<Inner>\ntext\n</Inner>\n===\n</Outer>\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MDX, None);
    assert!(rule.check(&ctx).unwrap().is_empty());
    assert_eq!(rule.fix(&ctx).unwrap(), content);

    // The same underline directly below a child paragraph makes a heading.
    let content = "<Card>\ntext\n===\n</Card>\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MDX, None);
    let lines: Vec<_> = rule.check(&ctx).unwrap().iter().map(|warning| warning.line).collect();
    assert_eq!(lines, [2]);
    assert_eq!(rule.fix(&ctx).unwrap(), "<Card>\n# text\n</Card>\n");
}

#[test]
fn test_mdx_underline_below_a_flow_expression_is_paragraph_text() {
    // `{x}` alone on its line is a block of its own, so `===` below it is a
    // paragraph and there is no heading to rewrite.
    let rule = MD003HeadingStyle::new(HeadingStyle::Atx);
    let content = "# Intro\n\n{x}\n===\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MDX, None);
    assert!(rule.check(&ctx).unwrap().is_empty());
    assert_eq!(rule.fix(&ctx).unwrap(), content);

    // Text between the expression and the underline makes a heading.
    let content = "# Intro\n\n{x}\ntext\n===\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MDX, None);
    let lines: Vec<_> = rule.check(&ctx).unwrap().iter().map(|warning| warning.line).collect();
    assert_eq!(lines, [4]);
    assert_eq!(rule.fix(&ctx).unwrap(), "# Intro\n\n{x}\n# text\n");
}
