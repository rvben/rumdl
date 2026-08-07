#![cfg(feature = "html-fmt")]

use crate::config::{Config, HtmlConfig};
use crate::lint_context::LintContext;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use markup_fmt::{Language, config::FormatOptions, format_text};
use std::sync::Mutex;

/// Rule MD089: Embedded HTML blocks should be formatted
///
/// This rule integrates `markup_fmt` to format HTML blocks and optionally
/// formats JavaScript/TypeScript inside `<script>` tags and Markdown inside HTML comments.
pub struct MD089EmbeddedHtmlFmt {
    html_config: HtmlConfig,
    config: Config,
    // Use Mutex for lazy initialization to avoid recursion during from_config
    comment_rules: Mutex<Option<Vec<Box<dyn Rule>>>>,
}

impl std::fmt::Debug for MD089EmbeddedHtmlFmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MD089EmbeddedHtmlFmt")
            .field("html_config", &self.html_config)
            .finish_non_exhaustive()
    }
}

impl Clone for MD089EmbeddedHtmlFmt {
    fn clone(&self) -> Self {
        let comment_rules = self.comment_rules.lock().unwrap();
        Self {
            html_config: self.html_config.clone(),
            config: self.config.clone(),
            comment_rules: Mutex::new(comment_rules.clone()),
        }
    }
}

impl MD089EmbeddedHtmlFmt {
    fn get_comment_rules(&self) -> Vec<Box<dyn Rule>> {
        let mut guard = self.comment_rules.lock().unwrap();
        if guard.is_none() {
            let all_rules = crate::rules::all_rules(&self.config);
            let mut rules = crate::rules::filter_rules(&all_rules, &self.config.global);
            // Retain only rules that are not MD089 to avoid potential recursion
            rules.retain(|r| r.name() != self.name());
            *guard = Some(rules);
        }
        guard.as_ref().unwrap().clone()
    }

    fn format_html_block(
        &self,
        html: &str,
        flavor: crate::config::MarkdownFlavor,
        file_path: Option<&std::path::Path>,
    ) -> Option<String> {
        let options = FormatOptions {
            layout: markup_fmt::config::LayoutOptions {
                print_width: self.html_config.print_width,
                use_tabs: self.html_config.use_tabs,
                indent_width: self.html_config.indent_width,
                ..Default::default()
            },
            language: markup_fmt::config::LanguageOptions {
                quotes: match self.html_config.quotes.as_str() {
                    "single" => markup_fmt::config::Quotes::Single,
                    _ => markup_fmt::config::Quotes::Double,
                },
                ..Default::default()
            },
        };

        let typescript_config = if self.html_config.script.enabled {
            let mut config_dummy =
                dprint_plugin_typescript::configuration::resolve_config(indexmap::IndexMap::new(), &Default::default())
                    .config;

            config_dummy.line_width = self.html_config.print_width as u32;
            config_dummy.indent_width = self.html_config.indent_width as u8;
            config_dummy.use_tabs = self.html_config.use_tabs;

            config_dummy.semi_colons = match self.html_config.script.semi_colons.as_str() {
                "always" => dprint_plugin_typescript::configuration::SemiColons::Always,
                "prefer" => dprint_plugin_typescript::configuration::SemiColons::Prefer,
                "asi" => dprint_plugin_typescript::configuration::SemiColons::Asi,
                _ => dprint_plugin_typescript::configuration::SemiColons::Always,
            };

            config_dummy.quote_style = match self.html_config.script.quote_style.as_str() {
                "always-double" | "always_double" | "double" => {
                    dprint_plugin_typescript::configuration::QuoteStyle::AlwaysDouble
                }
                "always-single" | "always_single" | "single" => {
                    dprint_plugin_typescript::configuration::QuoteStyle::AlwaysSingle
                }
                "prefer-double" | "prefer_double" => dprint_plugin_typescript::configuration::QuoteStyle::PreferDouble,
                "prefer-single" | "prefer_single" => dprint_plugin_typescript::configuration::QuoteStyle::PreferSingle,
                _ => dprint_plugin_typescript::configuration::QuoteStyle::PreferDouble,
            };
            Some(config_dummy)
        } else {
            None
        };

        let formatted = format_text(html, Language::Html, &options, |code, hints| {
            if let Some(ref config_dummy) = typescript_config
                && (hints.ext == "js" || hints.ext == "ts" || hints.ext == "jsx" || hints.ext == "tsx")
            {
                let opts = dprint_plugin_typescript::FormatTextOptions {
                    path: std::path::Path::new("script.ts"),
                    extension: Some("ts"),
                    text: code.to_string(),
                    config: config_dummy,
                    external_formatter: None,
                };

                match dprint_plugin_typescript::format_text(opts) {
                    Ok(Some(formatted)) => Ok(std::borrow::Cow::Owned(formatted)),
                    Ok(None) => Ok(std::borrow::Cow::Borrowed(code)),
                    Err(e) => Err(anyhow::anyhow!(e)),
                }
            } else {
                Ok(std::borrow::Cow::Borrowed(code))
            }
        });

        formatted
            .map(|f| self.fix_comment_indentation(&f, html, flavor, file_path))
            .ok()
    }

    fn fix_comment_indentation(
        &self,
        formatted_html: &str,
        original_html: &str,
        _flavor: crate::config::MarkdownFlavor,
        file_path: Option<&std::path::Path>,
    ) -> String {
        use std::sync::LazyLock;
        static COMMENT_RE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"<!--(?s:.*?)-->").unwrap());

        let original_matches: Vec<_> = COMMENT_RE.find_iter(original_html).collect();
        let formatted_matches: Vec<_> = COMMENT_RE.find_iter(formatted_html).collect();

        if original_matches.len() != formatted_matches.len() {
            return formatted_html.to_string();
        }

        let mut result = String::with_capacity(formatted_html.len());
        let mut last_idx = 0;

        for (orig_mat, form_mat) in original_matches.iter().zip(formatted_matches.iter()) {
            result.push_str(&formatted_html[last_idx..form_mat.start()]);

            let orig_line_start = original_html[..orig_mat.start()].rfind('\n').map_or(0, |i| i + 1);
            let orig_prefix = &original_html[orig_line_start..orig_mat.start()];
            let i_start = if orig_prefix.chars().all(char::is_whitespace) {
                orig_prefix.len()
            } else {
                0
            };

            let form_line_start = formatted_html[..form_mat.start()].rfind('\n').map_or(0, |i| i + 1);
            let form_prefix = &formatted_html[form_line_start..form_mat.start()];
            let o_start = if form_prefix.chars().all(char::is_whitespace) {
                form_prefix.len()
            } else {
                0
            };

            let shift = o_start as isize - i_start as isize;

            let comment_str = form_mat.as_str();
            let raw_content = &comment_str[4..comment_str.len() - 3];

            let formatted_markdown = if self.html_config.format_comments_as_markdown {
                let orig_comment_str = orig_mat.as_str();
                let orig_raw_content = &orig_comment_str[4..orig_comment_str.len() - 3];
                let (stripped, comment_indent) = crate::embedded_lint::strip_common_indent(orig_raw_content);

                let mut content_to_format = stripped;
                let rules = self.get_comment_rules();
                let coordinator = crate::fix_coordinator::FixCoordinator::new();

                if let Ok(_res) =
                    coordinator.apply_fixes_iterative(&rules, &[], &mut content_to_format, &self.config, 3, file_path)
                {
                    let restored = restore_indent(&content_to_format, &comment_indent);
                    shift_lines(&restored, shift)
                } else {
                    shift_lines(raw_content, shift)
                }
            } else {
                shift_lines(raw_content, shift)
            };

            let mut comment_lines = formatted_markdown.lines();
            let mut reconstructed = String::new();

            reconstructed.push_str("<!--");
            if let Some(first) = comment_lines.next() {
                reconstructed.push_str(first);
            }

            for line in comment_lines {
                reconstructed.push('\n');
                reconstructed.push_str(line);
            }

            if raw_content.ends_with('\n') && !reconstructed.ends_with('\n') {
                reconstructed.push('\n');
                if shift > 0 {
                    reconstructed.push_str(&" ".repeat(shift as usize));
                }
            }
            reconstructed.push_str("-->");

            result.push_str(&reconstructed);
            last_idx = form_mat.end();
        }
        result.push_str(&formatted_html[last_idx..]);
        result
    }
}

fn restore_indent(content: &str, indent: &str) -> String {
    let has_trailing_newline = content.ends_with('\n');

    let mut result: String = content
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if has_trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

fn shift_lines(content: &str, shift: isize) -> String {
    if shift == 0 {
        return content.to_string();
    }
    let has_trailing_newline = content.ends_with('\n');

    let mut result: String = content
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else if shift > 0 {
                let indent = " ".repeat(shift as usize);
                format!("{indent}{line}")
            } else {
                let strip_len = (-shift) as usize;
                let leading_spaces = line.chars().take_while(|c| *c == ' ').count();
                let actual_strip = leading_spaces.min(strip_len);
                line[actual_strip..].to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if has_trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

impl Rule for MD089EmbeddedHtmlFmt {
    fn name(&self) -> &'static str {
        "MD089"
    }

    fn description(&self) -> &'static str {
        "Embedded HTML blocks should be formatted"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Html
    }

    fn should_skip(&self, _ctx: &LintContext) -> bool {
        !self.html_config.enabled
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();

        for &(start, end) in &ctx.html_block_ranges {
            let raw_html = &ctx.content[start..end];

            // Skip JSX if flavor supports it and tag starts with uppercase
            if ctx.flavor.supports_jsx() && crate::utils::code_block_utils::CodeBlockUtils::is_jsx_block(raw_html) {
                continue;
            }

            if let Some(formatted) = self.format_html_block(raw_html, ctx.flavor, ctx.source_file.as_deref())
                && formatted != raw_html
            {
                let (start_line, start_col) = ctx.line_index.byte_to_line_col(start);
                let (end_line, end_col) = ctx.line_index.byte_to_line_col(end);

                warnings.push(LintWarning {
                    message: "HTML block is not formatted".to_string(),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix::new(start..end, formatted)),
                    rule_name: Some(self.name().to_string()),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings).map_err(LintError::InvalidInput)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD089EmbeddedHtmlFmt {
            html_config: config.html.clone(),
            config: config.clone(),
            comment_rules: Mutex::new(None),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, HtmlConfig, MarkdownFlavor, ScriptConfig};
    use crate::lint_context::LintContext;

    fn make_test_config(html_enabled: bool, script_enabled: bool) -> Config {
        let mut config = Config::default();
        config.html = HtmlConfig {
            enabled: html_enabled,
            print_width: 80,
            use_tabs: false,
            indent_width: 2,
            quotes: "double".to_string(),
            script: ScriptConfig {
                enabled: script_enabled,
                semi_colons: "always".to_string(),
                quote_style: "single".to_string(),
            },
            format_comments_as_markdown: false,
        };
        config
    }

    #[test]
    fn test_formatted_html_no_warnings() {
        let content = "Some text\n\n<div class=\"container\">\n  <p>Hello World</p>\n</div>\n";
        let config = make_test_config(true, false);
        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty(), "Expected no warnings, got: {warnings:?}");
    }

    #[test]
    fn test_unformatted_html_warning() {
        let content = "Some text\n\n<div class=\"container\">\n<p>Hello World</p>\n  </div>\n";
        let config = make_test_config(true, false);
        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "HTML block is not formatted");
        assert_eq!(warnings[0].line, 3);
        assert_eq!(warnings[0].column, 1);
    }

    #[test]
    fn test_disabled_rule() {
        let content = "Some text\n\n<div class=\"container\">\n<p>Hello World</p>\n  </div>\n";
        let config = make_test_config(false, false);
        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert!(rule.should_skip(&ctx));
    }

    #[test]
    fn test_inline_html_ignored() {
        let content = "This is a <span style=\"color: red;\">red</span> word.\n";
        let config = make_test_config(true, false);
        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_jsx_skips() {
        let content = "Some text\n\n<MyComponent prop={value}>\n  <Child />\n</MyComponent>\n";
        let config = make_test_config(true, false);
        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty(), "Should skip JSX component formatting");
    }

    #[test]
    fn test_script_formatting() {
        let content = r#"<script>
const a=1;
  const b = "double";
</script>
"#;
        let mut config = make_test_config(true, true);
        config.html.script.semi_colons = "always".to_string();
        config.html.script.quote_style = "single".to_string();

        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);

        let formatted = rule.fix(&ctx).unwrap();
        assert!(formatted.contains("const a = 1;"));
        assert!(formatted.contains("const b = 'double';"));
    }

    #[test]
    fn test_html_comment_nested_formatting() {
        let content = r#"<a href="docs/images/snippets.md#c">
<!--
Edit snippet in docs/images/snippets.md and:
https://drive.google.com/drive/folders/1QrBXiy_X74YsOueeC0IYlgyolWIhvusB
-->
<img src="docs/images/cpp_snippet.svg" width="600"
     alt="A snippet of C++ code. Follow the link to read it.">
</a>
"#;
        let config = make_test_config(true, false);
        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let formatted = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = formatted.lines().collect();
        assert_eq!(lines[0], r#"<a href="docs/images/snippets.md#c">"#);
        assert_eq!(lines[1], "  <!--");
        assert_eq!(lines[2], "  Edit snippet in docs/images/snippets.md and:");
        assert_eq!(
            lines[3],
            "  https://drive.google.com/drive/folders/1QrBXiy_X74YsOueeC0IYlgyolWIhvusB"
        );
        assert_eq!(lines[4], "  -->");
    }

    #[test]
    fn test_html_comment_format_as_markdown() {
        let content = r#"<div>
<!--
#  Heading with spaces
Some text.
-->
</div>
"#;
        let mut config = make_test_config(true, false);
        config.html.format_comments_as_markdown = true;

        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let formatted = rule.fix(&ctx).unwrap();

        assert!(formatted.contains("# Heading with spaces"));
        assert!(formatted.contains("  # Heading with spaces"));
    }

    #[test]
    fn test_html_comment_already_indented_no_change() {
        let content = r#"<div class="outer">
  <!--
  Already indented comment.
  This should not be over-indented.
  -->
</div>
"#;
        let config = make_test_config(true, false);
        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let formatted = rule.fix(&ctx).unwrap();
        assert_eq!(content, formatted);
    }

    #[test]
    fn test_invalid_script_formatting_is_skipped() {
        let content = r#"<script>
const a =; // Syntax error
</script>
"#;
        let config = make_test_config(true, true);
        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let formatted = rule.fix(&ctx).unwrap();
        assert_eq!(
            formatted, content,
            "Expected invalid script to skip formatting (return original content)"
        );
    }

    #[test]
    fn test_malformed_html_is_skipped() {
        let content = r#"<div class="container" <p>Hello</p> </div>"#;
        let config = make_test_config(true, false);
        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let formatted = rule.fix(&ctx).unwrap();
        assert_eq!(
            formatted, content,
            "Expected malformed HTML to skip formatting (return original content)"
        );
    }

    #[test]
    fn test_format_embedded_html_blocks_print_width() {
        let mut config = make_test_config(true, false);
        config.html.print_width = 40; // small print width to trigger wrapping

        let content = r#"# Document

<div>
  <p>This is a very long line of text inside an HTML block that should be wrapped to 40 columns by the HTML formatter because we configured it to do so.</p>
</div>
"#;
        let rule = MD089EmbeddedHtmlFmt::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let formatted = rule.fix(&ctx).unwrap();

        assert!(
            formatted.contains("<p>\n    This is a very long line"),
            "HTML paragraph content should be wrapped, got:\n{formatted}"
        );
    }
}
