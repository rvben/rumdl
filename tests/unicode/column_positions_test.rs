//! Regression guard for issue #670: diagnostic columns must be 1-indexed
//! CHARACTER offsets, never byte offsets. Each test lints content with a
//! multi-byte UTF-8 prefix before the flagged element and asserts the reported
//! column is the character position (a byte offset would over-count).
//!
//! When adding a rule that computes columns from byte offsets (regex matches,
//! `str::find`, parser byte offsets), add a case here. Convert through
//! `byte_to_char_count` / a char-based range helper before storing the column.

#[cfg(test)]
mod tests {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;

    fn ctx(content: &str) -> LintContext<'_> {
        LintContext::new(content, MarkdownFlavor::Standard, None)
    }

    #[test]
    fn md061_forbidden_term_column() {
        use rumdl_lib::rules::MD061ForbiddenTerms;
        let rule = MD061ForbiddenTerms::new(vec!["foobar".to_string()], false);
        // 1:你 2:好 3:(space) 4:f ...  "foobar" starts at character column 4.
        let content = "你好 foobar baz";
        let result = rule.check(&ctx(content)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 4, "MD061 column must be a character offset");
        assert_eq!(result[0].end_column, 10, "MD061 end_column must be character-based");
    }

    #[test]
    fn md044_proper_name_column() {
        use rumdl_lib::rules::MD044ProperNames;
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);
        // 1:你 2:好 3:(space) 4:j ...  "javascript" starts at character column 4.
        let content = "你好 javascript rocks";
        let result = rule.check(&ctx(content)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 4, "MD044 column must be a character offset");
        assert_eq!(result[0].end_column, 14, "MD044 end_column must be character-based");
    }

    #[test]
    fn md037_spaces_in_emphasis_column() {
        use rumdl_lib::rules::MD037NoSpaceInEmphasis;
        let rule = MD037NoSpaceInEmphasis;
        // 1:你 2:好 3:(space) 4:* ...  The emphasis "* text *" opens at column 4.
        let content = "你好 * text *";
        let result = rule.check(&ctx(content)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 4, "MD037 column must be a character offset");
        assert_eq!(result[0].end_column, 12, "MD037 end_column must be character-based");
    }

    #[test]
    fn md038_code_span_end_column() {
        use rumdl_lib::rules::MD038NoSpaceInCode;
        let rule = MD038NoSpaceInCode::new();
        // 1:你 2:好 3:(space) 4:` 5:a 6:(space) 7:` ...  The span ends at column 7,
        // so the exclusive end is 8. An end of 7 leaves the closing backtick out of
        // the highlight.
        let content = "你好 `a ` baz";
        let result = rule.check(&ctx(content)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 4, "MD038 column must be a character offset");
        assert_eq!(result[0].end_column, 8, "MD038 end_column must be a character offset");
    }

    #[test]
    fn md059_link_end_column() {
        use rumdl_lib::rules::MD059LinkText;
        let rule = MD059LinkText::default();
        // 1:你 2:好 3:(space) 4:[ ...  "[here](u.md)" is 12 characters starting at
        // column 4, so the exclusive end is 16. Storing the parser's 0-indexed end
        // left the closing paren out of the highlight.
        let content = "你好 [here](u.md) baz";
        let result = rule.check(&ctx(content)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 4, "MD059 column must be a character offset");
        assert_eq!(result[0].end_column, 16, "MD059 end_column must be a character offset");
    }

    /// A link may span lines, and then its end column belongs to the line it
    /// ends on. Reporting it against the line the link *starts* on produced a
    /// range running backwards whenever the link began to the right of where it
    /// ended, which is every link far enough into a line.
    #[test]
    fn multiline_link_ranges_end_on_the_line_the_link_ends_on() {
        use rumdl_lib::rules::{MD039NoSpaceInLinks, MD045NoAltText, MD051LinkFragments, MD059LinkText};

        // Each link opens 20 characters into its first line and closes early on
        // the next, so a range ending on the opening line runs backwards. The
        // expected end column is one past the closing `)`.
        let cases: Vec<(&str, &str, usize, Box<dyn Rule>)> = vec![
            (
                "MD059",
                "Some long text here [ here\n](u.md) tail\n",
                8,
                Box::new(MD059LinkText::default()),
            ),
            (
                "MD039",
                "Some long text here [ here\n](u.md) tail\n",
                8,
                Box::new(MD039NoSpaceInLinks),
            ),
            (
                "MD045",
                "Some long text here ![](\ni.png) tail\n",
                7,
                Box::new(MD045NoAltText),
            ),
            (
                "MD051",
                "# H\n\nSome long text here [text](\n#nope) tail\n",
                7,
                Box::new(MD051LinkFragments::new()),
            ),
        ];

        // Every row is checked before asserting, so a regression names each rule
        // it affects rather than stopping at the first.
        let mut wrong = Vec::new();
        for (name, content, expected_end_column, rule) in cases {
            let result = rule.check(&ctx(content)).unwrap();
            assert_eq!(result.len(), 1, "{name} must flag the multi-line link");
            let w = &result[0];
            let got = (w.line, w.column, w.end_line, w.end_column);
            let want = (w.line, 21, w.line + 1, expected_end_column);
            if got != want {
                wrong.push(format!("{name}: got {got:?}, want {want:?}"));
            }
        }
        assert!(
            wrong.is_empty(),
            "multi-line link ranges are wrong:\n  {}",
            wrong.join("\n  ")
        );
    }

    #[test]
    fn md033_html_tag_end_column() {
        use rumdl_lib::rules::MD033NoInlineHtml;
        let rule = MD033NoInlineHtml::default();
        // 1:你 2:好 3:< 4:b 5:r 6:> ...  "<br>" spans columns 3..7.
        let content = "你好<br>x";
        let result = rule.check(&ctx(content)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 3, "MD033 column must be a character offset");
        assert_eq!(result[0].end_column, 7, "MD033 end_column must be a character offset");
    }

    #[test]
    fn md049_emphasis_style_column() {
        use rumdl_lib::MD049EmphasisStyle;
        use rumdl_lib::rules::emphasis_style::EmphasisStyle;
        let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
        // 1:你 2:好 3:(space) 4:_ ...  "_word_" opens at column 4.
        let content = "你好 _word_ x";
        let result = rule.check(&ctx(content)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 4, "MD049 column must be a character offset");
        assert_eq!(result[0].end_column, 10, "MD049 end_column must be character-based");
    }

    #[test]
    fn md013_line_length_end_column() {
        use rumdl_lib::rules::MD013LineLength;
        // A long, breakable line ending in multi-byte text. The end column is the
        // line's character count + 1, not its byte count + 1.
        let rule = MD013LineLength::new(80, true, true, true, false);
        let content = format!("{}你好", "word ".repeat(20));
        let expected = content.chars().count() + 1;
        let byte_based = content.len() + 1;
        assert_ne!(expected, byte_based, "test content must contain multi-byte chars");
        let result = rule.check(&ctx(&content)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].end_column, expected,
            "MD013 end_column must be a character offset"
        );
    }

    #[test]
    fn md058_table_end_column() {
        use rumdl_lib::rules::MD058BlanksAroundTables;
        let rule = MD058BlanksAroundTables::default();
        // The final table row "| 你好 | x |" is 10 characters (14 bytes); the table
        // is not followed by a blank line, so MD058 flags it at the row's end.
        let content = "| a | b |\n|---|---|\n| 你好 | x |\ntext\n";
        let result = rule.check(&ctx(content)).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0].column, 11, "MD058 column must be a character offset");
        assert_eq!(result[0].end_column, 12, "MD058 end_column must be a character offset");
    }
}
