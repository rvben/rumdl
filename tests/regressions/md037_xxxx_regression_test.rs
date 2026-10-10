//! Spaced emphasis diagnostics must never replace original content with masking
//! placeholders or infer formatting that the author did not write.
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD037NoSpaceInEmphasis;

#[test]
fn test_regression_xxxx_content_replacement_bug() {
    let rule = MD037NoSpaceInEmphasis;
    for content in [
        "**simple emphasis with spaces** and `code` and **another emphasis**",
        "1. **Use `force_exclude` in your configuration file:**",
        "`code` and * bad emphasis * here",
        "Code `let x = 1;` with * spaces * and more",
        "Multiple `code` spans and * bad * and `more code` text",
        "Start with * bad * then `code` then * more bad *",
    ] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
        assert!(rule.check(&ctx).unwrap().iter().all(|warning| warning.fix.is_none()));
    }
}
