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
