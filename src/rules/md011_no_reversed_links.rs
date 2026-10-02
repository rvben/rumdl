/// Rule MD011: No reversed link syntax
///
/// See [docs/md011.md](../../docs/md011.md) for full documentation, configuration, and examples.
use crate::filtered_lines::FilteredLinesExt;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_match_range;
use crate::utils::skip_context::is_in_math_context;
use regex::Regex;
use std::sync::LazyLock;

// Reversed link detection pattern
const REVERSED_LINK_REGEX_STR: &str = r"\(([^()]+)\)\[((?:\\.|[^\]\\])+)\]";
static REVERSED_LINK_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(REVERSED_LINK_REGEX_STR).unwrap());

/// Classification of a link component
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkComponent {
    /// Clear URL: has protocol, www., mailto:, or path prefix
    ClearUrl,
    /// A complete native relative destination with a title
    TitledRelativeUrl,
    /// Multiple words or sentence-like (likely link text, not URL)
    MultiWord,
    /// Single word - could be either URL or text
    Ambiguous,
}

/// Information about a detected reversed link pattern
#[derive(Debug, Clone)]
struct ReversedLinkInfo {
    /// Content found in parentheses
    paren_content: String,
    /// Content found in square brackets
    bracket_content: String,
    /// Classification of parentheses content
    paren_type: LinkComponent,
    /// Classification of bracket content
    bracket_type: LinkComponent,
}

impl ReversedLinkInfo {
    /// Determine the correct order: returns (text, url)
    fn correct_order(&self) -> (&str, &str) {
        use LinkComponent::{Ambiguous, ClearUrl, MultiWord, TitledRelativeUrl};

        match (self.paren_type, self.bracket_type) {
            // One side is clearly a URL - that's the URL
            (ClearUrl, _) => (&self.bracket_content, &self.paren_content),
            (_, ClearUrl) => (&self.paren_content, &self.bracket_content),

            // A native title belongs to its relative destination, not the text.
            (TitledRelativeUrl, _) => (&self.bracket_content, &self.paren_content),
            (_, TitledRelativeUrl) => (&self.paren_content, &self.bracket_content),

            // One side is multi-word - that's the text, other is URL
            (MultiWord, _) => (&self.paren_content, &self.bracket_content),
            (_, MultiWord) => (&self.bracket_content, &self.paren_content),

            // Both ambiguous: assume standard reversed pattern (url)[text]
            (Ambiguous, Ambiguous) => (&self.bracket_content, &self.paren_content),
        }
    }
}

#[derive(Clone)]
pub struct MD011NoReversedLinks;

impl MD011NoReversedLinks {
    /// A complete inline link, rather than a valid prefix followed by text.
    fn is_complete_inline_link(candidate: &str) -> bool {
        pulldown_cmark::Parser::new(candidate)
            .into_offset_iter()
            .any(|(event, range)| {
                matches!(
                    event,
                    pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link {
                        link_type: pulldown_cmark::LinkType::Inline,
                        ..
                    })
                ) && range == (0..candidate.len())
            })
    }

    /// Complete a label containing nested brackets. Escapes and brackets in
    /// literal Markdown/template contexts do not change the nesting depth.
    fn nested_label_end(
        ctx: &crate::lint_context::LintContext,
        line: &str,
        line_byte_pos: usize,
        open: usize,
    ) -> Option<usize> {
        let mut depth = 1;
        let mut escaped = false;
        for (relative, ch) in line[open + 1..].char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if !matches!(ch, '[' | ']') {
                continue;
            }
            let pos = line_byte_pos + open + 1 + relative;
            if ctx.is_in_code_block_or_span(pos)
                || ctx.is_in_html_tag(pos)
                || ctx.is_in_jsx_component_tag(pos)
                || ctx.is_in_html_comment(pos)
                || ctx.is_in_jinja_range(pos)
                || ctx.is_in_jsx_expression(pos)
                || ctx.is_in_mdx_comment(pos)
                || is_in_math_context(ctx, pos)
            {
                continue;
            }
            if ch == '[' {
                depth += 1;
            } else {
                depth -= 1;
                if depth == 0 {
                    return Some(open + relative + 2);
                }
            }
        }
        None
    }

    /// Space-containing destinations need angle brackets. Keep valid native
    /// titles intact and withhold a fix when a title cannot be distinguished
    /// from the destination, or when the result would still not be a link.
    fn replacement(text: &str, url: &str) -> Option<String> {
        let candidate = format!("[{text}]({url})");
        if !url.contains([' ', '\t']) {
            return (!text.contains(['[', ']']) || Self::is_complete_inline_link(&candidate)).then_some(candidate);
        }
        if Self::is_complete_inline_link(&candidate) {
            return Some(candidate);
        }
        let destination = url.trim_matches([' ', '\t']);
        if destination.bytes().any(|byte| byte.is_ascii_control()) {
            return None;
        }
        if let Some(quote @ (b'\'' | b'"')) = destination.as_bytes().last().copied()
            && destination
                .as_bytes()
                .windows(2)
                .any(|pair| pair[0] == b' ' && pair[1] == quote)
        {
            return None;
        }
        // The source is already Markdown-escaped: retain its backslashes and
        // escape only angle delimiters that are not already escaped.
        let mut escaped = String::with_capacity(destination.len());
        let mut backslashes = 0;
        for ch in destination.chars() {
            if matches!(ch, '<' | '>') && backslashes % 2 == 0 {
                escaped.push('\\');
            }
            escaped.push(ch);
            backslashes = if ch == '\\' { backslashes + 1 } else { 0 };
        }
        let candidate = format!("[{text}](<{escaped}>)");
        Self::is_complete_inline_link(&candidate).then_some(candidate)
    }

    fn has_clear_url_prefix(trimmed: &str) -> bool {
        trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("ftp://")
            || trimmed.starts_with("www.")
            || (trimmed.starts_with("mailto:") && trimmed.contains('@'))
            || (trimmed.starts_with('/') && trimmed.len() > 1)
            || (trimmed.starts_with("./") || trimmed.starts_with("../"))
            || (trimmed.starts_with('#') && trimmed.len() > 1 && !trimmed[1..].contains(' '))
    }

    /// Classify a link component as URL, multi-word text, or ambiguous
    fn classify_component(s: &str) -> LinkComponent {
        let trimmed = s.trim();
        // Angle-wrapped destinations can contain spaces and an optional title.
        // Decode only complete native link syntax; ordinary angle-bracket prose
        // and malformed destinations retain their existing classification.
        let angle_destination = trimmed
            .starts_with('<')
            .then(|| {
                let candidate = format!("[label]({trimmed})");
                pulldown_cmark::Parser::new(&candidate)
                    .into_offset_iter()
                    .find_map(|(event, range)| match event {
                        pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link {
                            link_type: pulldown_cmark::LinkType::Inline,
                            dest_url,
                            ..
                        }) if range == (0..candidate.len()) => Some(dest_url.into_string()),
                        _ => None,
                    })
            })
            .flatten();
        let trimmed = angle_destination
            .as_deref()
            .filter(|url| Self::has_clear_url_prefix(url))
            .unwrap_or(trimmed);

        if Self::has_clear_url_prefix(trimmed) {
            return LinkComponent::ClearUrl;
        }

        // Only promote a relative path when the whole component is a native
        // destination with a title. Slashes or quoted prose alone are ambiguous.
        if trimmed.contains('/') && trimmed.contains(['\'', '"']) {
            let candidate = format!("[label]({trimmed})");
            if pulldown_cmark::Parser::new(&candidate)
                .into_offset_iter()
                .any(|(event, range)| {
                    matches!(event, pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link {
                        link_type: pulldown_cmark::LinkType::Inline,
                        dest_url,
                        title,
                        ..
                    }) if range == (0..candidate.len()) && dest_url.contains('/') && !title.is_empty())
                })
            {
                return LinkComponent::TitledRelativeUrl;
            }
        }

        // Multi-word text is likely a description, not a URL
        if trimmed.contains(' ') {
            return LinkComponent::MultiWord;
        }

        // Single word - could be either
        LinkComponent::Ambiguous
    }
}

impl Rule for MD011NoReversedLinks {
    fn name(&self) -> &'static str {
        "MD011"
    }

    fn description(&self) -> &'static str {
        "Reversed link syntax"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Use filtered_lines() to automatically skip front-matter and Obsidian comments
        for filtered_line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_esm_blocks()
            .skip_obsidian_comments()
        {
            if filtered_line.line_info.is_myst_comment {
                continue;
            }
            // Raw HTML block contents are literal. MDX JSX children and
            // containers opted into Markdown remain lintable.
            if filtered_line.line_info.in_html_block
                && !ctx.flavor.supports_jsx()
                && !filtered_line.line_info.in_mkdocs_html_markdown
            {
                continue;
            }
            let line_num = filtered_line.line_num;
            let line = filtered_line.content;

            let byte_pos = ctx.line_start_byte(line_num).unwrap_or(0);

            let mut last_end = 0;

            while let Some(cap) = REVERSED_LINK_REGEX.captures(&line[last_end..]) {
                let match_obj = cap.get(0).unwrap();
                let match_start = last_end + match_obj.start();
                // An opening parenthesis is escaped only by an odd run of
                // backslashes. Even runs leave a literal backslash before it.
                if line.as_bytes()[..match_start]
                    .iter()
                    .rev()
                    .take_while(|&&byte| byte == b'\\')
                    .count()
                    % 2
                    == 1
                {
                    last_end += match_obj.end();
                    continue;
                }
                let paren_content = cap[1].to_string();
                let bracket_start = last_end + cap.get(2).unwrap().start() - 1;
                let end_pos = if cap[2].contains('[') {
                    Self::nested_label_end(ctx, line, byte_pos, bracket_start).unwrap_or(last_end + match_obj.end())
                } else {
                    last_end + match_obj.end()
                };
                let bracket_content = line[bracket_start + 1..end_pos - 1].to_string();

                // Skip wiki-link patterns whose bracket content starts with [.
                // This handles cases like (url)[[wiki-link]] being misdetected
                if bracket_content.starts_with('[') {
                    last_end = end_pos;
                    continue;
                }

                // Skip footnote references: [^footnote]
                // This prevents false positives like [link](url)[^footnote]
                if bracket_content.starts_with('^') {
                    last_end = end_pos;
                    continue;
                }

                // Skip Dataview inline fields in Obsidian flavor
                // Pattern: (field:: value)[text] is valid Obsidian syntax, not a reversed link
                if ctx.flavor == crate::config::MarkdownFlavor::Obsidian && paren_content.contains("::") {
                    last_end = end_pos;
                    continue;
                }

                // Manual negative lookahead: skip if followed by (
                // This prevents matching (text)[ref](url) patterns
                if end_pos < line.len() && line[end_pos..].starts_with('(') {
                    last_end = end_pos;
                    continue;
                }

                // Calculate the actual position
                let match_byte_pos = byte_pos + match_start;
                let bracket_byte_pos = byte_pos + bracket_start;
                let closing_byte_pos = byte_pos + end_pos - 1;

                // A closing bracket inside a literal context cannot complete
                // reversed link syntax that started outside that context.
                let in_literal_context = [match_byte_pos, closing_byte_pos].into_iter().any(|pos| {
                    ctx.is_in_code_block_or_span(pos)
                        || ctx.is_in_html_tag(pos)
                        || ctx.is_in_jsx_component_tag(pos)
                        || ctx.is_in_html_comment(pos)
                        || ctx.is_in_mdx_comment(pos)
                        || is_in_math_context(ctx, pos)
                        || ctx.is_in_jinja_range(pos)
                });

                // Link labels, destinations, and reference definitions already
                // belong to another construct. Rewriting their contents can
                // create a nested link and destroy the original outer link.
                if in_literal_context
                    || ctx.is_in_link(match_byte_pos)
                    // Parenthesized prose can precede a valid reference link;
                    // its destination must not be replaced by the parentheses.
                    || ctx.has_parsed_link_start_in(bracket_byte_pos, bracket_byte_pos + 1)
                    // Protect MDX code by range so prose beside it remains lintable.
                    || ctx.overlaps_mdx_inline_code(match_byte_pos, closing_byte_pos + 1)
                {
                    last_end = end_pos;
                    continue;
                }

                // Classify both components and determine correct order
                let paren_type = Self::classify_component(&paren_content);
                let bracket_type = Self::classify_component(&bracket_content);

                let info = ReversedLinkInfo {
                    paren_content,
                    bracket_content,
                    paren_type,
                    bracket_type,
                };

                let (text, url) = info.correct_order();

                // A standalone candidate parse cannot resolve the document's
                // references. Preserve actual inner links, including empty
                // reference destinations, instead of creating nested anchors.
                let replacement = if ctx.has_parsed_link_start_in(bracket_byte_pos + 1, closing_byte_pos + 1) {
                    None
                } else {
                    Self::replacement(text, url)
                };
                let actual_length = end_pos - match_start;
                let (start_line, start_col, end_line, end_col) =
                    calculate_match_range(line_num, line, match_start, actual_length);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: replacement.as_ref().map_or_else(
                        || "Reversed link syntax: reorder the link text and destination manually".to_string(),
                        |replacement| format!("Reversed link syntax: use {replacement} instead"),
                    ),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Error,
                    fix: replacement.map(|replacement| {
                        let match_start_byte = byte_pos + match_start;
                        let match_end_byte = match_start_byte + actual_length;
                        Fix::new(match_start_byte..match_end_byte, replacement)
                    }),
                });

                last_end = end_pos;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let warnings = self.check(ctx)?;
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        let mut content = ctx.content.to_string();
        // Apply fixes in reverse order to preserve byte offsets
        let mut fixes: Vec<_> = warnings.iter().filter_map(|w| w.fix.as_ref()).collect();
        fixes.sort_by_key(|f| std::cmp::Reverse(f.range.start));

        for fix in fixes {
            if fix.range.start < content.len() && fix.range.end <= content.len() {
                content.replace_range(fix.range.clone(), &fix.replacement);
            }
        }
        Ok(content)
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_links_or_images()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD011NoReversedLinks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_nested_reversed_labels_preserve_resolved_reference_links() {
        let rule = MD011NoReversedLinks;
        for destination in ["/existing", "<>"] {
            let source = format!("(https://example.org)[outer [inner] tail]\n\n[inner]: {destination}\n");
            let ctx = crate::lint_context::LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1);
            assert!(warnings[0].fix.is_none(), "existing nested reference: {source}");
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_reversed_patterns_preserve_empty_reference_destinations() {
        let rule = MD011NoReversedLinks;
        for source in [
            "(prose)[known]\n\n[known]: <>\n",
            "(prose)[display][known]\n\n[known]: <>\n",
            "(prose)[known][]\n\n[known]: <>\n",
            "(prose)[STRASSE]\n\n[Straße]: <>\n",
            r"(prose)[label\!]

[label\!]: <>
",
        ] {
            let ctx = crate::lint_context::LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert!(
                rule.check(&ctx).unwrap().is_empty(),
                "resolved empty reference: {source}"
            );
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_nested_reversed_labels_preserve_the_complete_link_text() {
        let rule = MD011NoReversedLinks;
        for label in [
            "outer [inner]",
            "outer [inner] tail",
            "outer [inner [deep]] tail",
            r"outer [inner\]] tail",
            "内容 [内部] 終わり",
        ] {
            let source = format!("Before (https://example.org)[{label}] after\n");
            let expected = format!("Before [{label}](https://example.org) after\n");
            let ctx = crate::lint_context::LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "complete nested label: {source}");
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
            assert_eq!(
                warnings[0].fix.as_ref().unwrap().range.end,
                source.find(" after").unwrap()
            );
            let fixed_ctx =
                crate::lint_context::LintContext::new(&expected, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        }
    }

    #[test]
    fn test_nested_reversed_labels_ignore_literal_brackets() {
        let rule = MD011NoReversedLinks;
        for label in [
            "outer [inner] `code ] [` tail",
            r#"outer [inner] <span title="] [">text</span> tail"#,
            "outer [inner] <!-- ] [ --> tail",
        ] {
            let source = format!("Before (https://example.org)[{label}] after\n");
            let expected = format!("Before [{label}](https://example.org) after\n");
            let ctx = crate::lint_context::LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "literal brackets: {source}");
        }
    }

    #[test]
    fn test_nested_reversed_label_does_not_consume_following_matches() {
        let rule = MD011NoReversedLinks;
        let source = "(https://example.org)[outer [inner] tail] and (https://other.test)[second [nested]]\n";
        let expected = "[outer [inner] tail](https://example.org) and [second [nested]](https://other.test)\n";
        let ctx = crate::lint_context::LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 2);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
    }

    #[test]
    fn test_nested_reversed_labels_with_invalid_link_structure_have_no_fix() {
        let rule = MD011NoReversedLinks;
        for source in [
            "(https://example.org)[outer [inner]\n",
            "(https://example.org)[outer [inner](https://other.test)]\n",
        ] {
            let ctx = crate::lint_context::LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1);
            assert!(warnings[0].fix.is_none(), "invalid nested link structure: {source}");
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_angle_wrapped_reversed_destinations_are_classified_as_urls() {
        let rule = MD011NoReversedLinks;
        for url in [
            "<https://example.org/a b>",
            "<https://example.org/a b> \"Title\"",
            "<./hello world>",
            "</hello world>",
            "<../hello world>",
            "<mailto:user@example.org>",
        ] {
            for label in ["label", "my label"] {
                let source = format!("({url})[{label}]\n");
                let expected = format!("[{label}]({url})\n");
                let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
                assert_eq!(rule.fix(&ctx).unwrap(), expected, "angle destination: {source}");
                let fixed_ctx = LintContext::new(&expected, crate::config::MarkdownFlavor::Standard, None);
                assert!(rule.check(&fixed_ctx).unwrap().is_empty());
                assert!(MD011NoReversedLinks::is_complete_inline_link(expected.trim_end()));
            }
        }
    }

    #[test]
    fn test_angle_destination_in_brackets_keeps_its_title() {
        let rule = MD011NoReversedLinks;
        let source = "(my label)[<https://example.org/a b> \"Title\"]\n";
        let expected = "[my label](<https://example.org/a b> \"Title\")\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
        assert!(pulldown_cmark::Parser::new(expected).any(|event| matches!(event,
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, title, .. })
            if dest_url.as_ref() == "https://example.org/a b" && title.as_ref() == "Title")));
    }

    #[test]
    fn test_angle_wrapping_only_promotes_valid_clear_url_destinations() {
        for (component, expected) in [
            ("<hello world>", LinkComponent::MultiWord),
            ("<hello>", LinkComponent::Ambiguous),
            ("<hello> \"Title\"", LinkComponent::MultiWord),
            ("<https://example.org/a b> stray text", LinkComponent::MultiWord),
            ("<https://example.org/a b", LinkComponent::MultiWord),
        ] {
            assert_eq!(MD011NoReversedLinks::classify_component(component), expected);
        }
    }

    #[test]
    fn test_space_containing_reversed_destinations_form_real_links() {
        let rule = MD011NoReversedLinks;
        for url in [
            "https://example.org/hello world",
            "https://example.org/a b?x=c d",
            "./hello world",
            "/hello world",
            "https://example.org/Bob's file",
        ] {
            let source = format!("({url})[label]\n");
            let expected = format!("[label](<{url}>)\n");
            let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
            let fixed_ctx = LintContext::new(&expected, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            let links: Vec<_> = pulldown_cmark::Parser::new(&expected)
                .filter_map(|event| match event {
                    pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, title, .. }) => {
                        Some((dest_url.to_string(), title.to_string()))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(links, vec![(url.to_string(), String::new())]);
        }
    }

    #[test]
    fn test_reversed_space_destinations_preserve_escapes_and_angle_characters() {
        let rule = MD011NoReversedLinks;
        for (source, expected, destination) in [
            (
                r"(https://example.org/a<b c>d)[label]",
                r"[label](<https://example.org/a\<b c\>d>)",
                "https://example.org/a<b c>d",
            ),
            (
                r"(https://example.org/a\<b c\>d)[a\]b]",
                r"[a\]b](<https://example.org/a\<b c\>d>)",
                "https://example.org/a<b c>d",
            ),
            (
                r"(https://example.org/a\\ b)[label]",
                r"[label](<https://example.org/a\\ b>)",
                r"https://example.org/a\ b",
            ),
            (
                "(my label)[https://example.org/a) b]",
                "[my label](<https://example.org/a) b>)",
                "https://example.org/a) b",
            ),
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
            assert!(pulldown_cmark::Parser::new(expected).any(|event| matches!(event,
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) if dest_url.as_ref() == destination)));
        }
    }

    #[test]
    fn test_reversed_native_titles_and_padding_keep_their_meaning() {
        let rule = MD011NoReversedLinks;
        for url in [
            r#"https://example.org "Title""#,
            "https://example.org 'Title'",
            " https://example.org ",
            "https://example.org/a<b>c",
        ] {
            let source = format!("({url})[label]\n");
            let expected = format!("[label]({url})\n");
            let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_ambiguous_or_control_character_space_destinations_need_manual_fix() {
        let rule = MD011NoReversedLinks;
        for url in [
            r#"https://example.org/a b "Title""#,
            "https://example.org/a b 'Title'",
            "https://example.org/a\tb",
            "https://example.org/a b\\",
        ] {
            let source = format!("({url})[label]\n");
            let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1);
            assert!(warnings[0].fix.is_none(), "unsafe destination: {url}");
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_myst_comment_contents_are_not_linted() {
        let rule = MD011NoReversedLinks;
        for prefix in ["% ", "%", "  %\t", "   % "] {
            let source = format!("{prefix}(url)[literal]\n\nBefore (url)[visible]\n");
            let expected = format!("{prefix}(url)[literal]\n\nBefore [visible](url)\n");
            let ctx = crate::lint_context::LintContext::new(&source, crate::config::MarkdownFlavor::MyST, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "comments are not visible prose: {source}");
            assert_eq!(warnings[0].line, 3);
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_even_backslashes_before_reversed_links_are_not_escapes() {
        let rule = MD011NoReversedLinks;
        for prefix in [r"\\", r"\\\\", r"日本語 \\"] {
            let source = format!("{prefix}(url)[label]\n");
            let expected = format!("{prefix}[label](url)\n");
            let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1, "even backslashes: {source}");
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_reversed_labels_with_escaped_brackets_or_backslashes_are_fixed() {
        let rule = MD011NoReversedLinks;
        for (source, expected) in [
            (r"(https://example.org)[a\]b]", r"[a\]b](https://example.org)"),
            (r"(url)[label\\]", r"[label\\](url)"),
            (r"(url)[label\]]", r"[label\]](url)"),
            (r"(url)[a\\\]b]", r"[a\\\]b](url)"),
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1, "escaped label: {source}");
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
            let fixed_ctx = LintContext::new(expected, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
            assert!(
                pulldown_cmark::Parser::new(expected)
                    .any(|event| matches!(event, pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { .. })))
            );
        }
    }

    #[test]
    fn test_escaped_openers_and_unterminated_escaped_labels_remain_ignored() {
        let rule = MD011NoReversedLinks;
        for source in [
            r"\(url)[label]",
            r"\\\(url)[label]",
            r"(url)[label\]",
            r"(url)[label\\\]",
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "escaped delimiter: {source}");
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_parenthetical_before_resolved_reference_is_not_reversed() {
        let rule = MD011NoReversedLinks;
        for source in [
            "(see this)[ref]\n\n[ref]: /target\n",
            "(https://other.test)[ref]\n\n[ref]: /target\n",
            "(url)[ref][]\n\n[ref]: /target\n",
            "(url)[text][ref]\n\n[ref]: /target\n",
            "(前置)[STRASSE]\n\n[straße]: /target\n",
            "(note)[two  words]\n\n[two words]: /target\n",
            "(note)[a\\!b]\n\n[a\\!b]: /target\n",
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert!(
                rule.check(&ctx).unwrap().is_empty(),
                "reference already resolves: {source}"
            );
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_real_reversed_link_beside_parenthetical_reference_is_fixed() {
        let rule = MD011NoReversedLinks;
        let source = "(see this)[ref] and (url)[real]\n\n[ref]: /target\n";
        let expected = "(see this)[ref] and [real](url)\n\n[ref]: /target\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
        let fixed_ctx = LintContext::new(expected, crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new(expected));
        assert_eq!(
            html,
            "<p>(see this)<a href=\"/target\">ref</a> and <a href=\"url\">real</a></p>\n"
        );
    }

    #[test]
    fn test_reversed_links_beside_mdx_expressions_are_checked() {
        let rule = MD011NoReversedLinks;
        for source in [
            "Before {\"(url)[literal]\"} and (url)[real]\n",
            "Before {/* (url)[comment] */} and (url)[real]\n",
            "Before {\n\"(url)[literal]\"\n} and (url)[real]\n",
            "Before {/*\n(url)[comment]\n*/} and (url)[real]\n",
            "Before (url)[text {\"value]\"} and (url)[real]\n",
            "日本語 {\"(url)[内容]\"} and (url)[real]\n",
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::MDX, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "only the prose link: {source}");
            assert_eq!(
                warnings[0].fix.as_ref().unwrap().range.start,
                source.rfind("(url)[real]").unwrap()
            );
            let expected = source.replace(" and (url)[real]", " and [real](url)");
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
            let fixed_ctx = LintContext::new(&expected, crate::config::MarkdownFlavor::MDX, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        }
    }

    #[test]
    fn test_reversed_patterns_touching_mdx_expressions_remain_ignored() {
        let rule = MD011NoReversedLinks;
        for source in [
            "Before {\"(url)[literal]\"}\n",
            "Before {/* (url)[comment] */}\n",
            "Before (url)[text {\"value]\"}\n",
            "Before (url)[text {\"value\"}]\n",
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::MDX, None);
            assert!(
                rule.check(&ctx).unwrap().is_empty(),
                "MDX code must be preserved: {source}"
            );
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_reversed_patterns_closing_inside_literal_contexts_are_preserved() {
        let rule = MD011NoReversedLinks;
        for source in [
            "Before (url)[label `code]\nend`\n",
            "Before (url `code)[label]\nend`\n",
            "Before (url)[text <!-- label] -->\n",
            "Before (url)[label <span title=\"close]\">visible</span>\n",
            "Before (url)[text {{ \"value]\" }}\n",
            "Before (url)[text $x]$\n",
            "日本語 (url)[内容 `コード]\n終了`\n",
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert!(
                rule.check(&ctx).unwrap().is_empty(),
                "closing bracket is literal: {source}"
            );
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_reversed_link_beside_partial_literal_pattern_still_gets_fixed() {
        let rule = MD011NoReversedLinks;
        let source = "Before (url)[label `code]\nend` and (https://example.org)[real]\n";
        let expected = "Before (url)[label `code]\nend` and [real](https://example.org)\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
        let fixed_ctx = LintContext::new(expected, crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        let code_values = |content: &str| {
            pulldown_cmark::Parser::new(content)
                .filter_map(|event| match event {
                    pulldown_cmark::Event::Code(value) => Some(value.into_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(code_values(source), vec!["code] end"]);
        assert_eq!(code_values(source), code_values(expected));
    }

    #[test]
    fn test_reversed_patterns_in_html_attributes_are_preserved() {
        let rule = MD011NoReversedLinks;
        for source in [
            "Before <span title=\"(url)[tooltip]\">visible</span>\n",
            "Before <span\n title=\"(url)[tooltip]\">visible</span>\n",
            "日本語 <span title=\"(url)[内容]\">表示</span>\n",
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "literal attribute: {source}");
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_reversed_patterns_in_raw_html_blocks_are_preserved() {
        let rule = MD011NoReversedLinks;
        for source in [
            "<pre>\n(url)[literal]\n</pre>\n",
            "<script>\nconst text = \"(url)[literal]\";\n</script>\n",
            "<div>\n(url)[literal]\n</div>\n",
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "raw HTML: {source}");
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_reversed_patterns_in_mdx_modules_are_preserved() {
        let rule = MD011NoReversedLinks;
        for source in [
            "export const text = \"(url)[literal]\"\n\nVisible prose\n",
            "export const text =\n \"(url)[literal]\"\n\nVisible prose\n",
            "import thing from \"(url)[module]\"\n\nVisible prose\n",
            "export const info = {text: \"(url)[literal]\"}\n\nVisible prose\n",
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::MDX, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "module string: {source}");
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_reversed_link_beside_html_literals_still_gets_fixed() {
        let rule = MD011NoReversedLinks;
        let source = "Before <span title=\"(url)[tooltip]\">visible</span> and (url)[real]\n\n<pre>\n(url)[literal]\n</pre>\n\n(url)[after]\n";
        let expected = "Before <span title=\"(url)[tooltip]\">visible</span> and [real](url)\n\n<pre>\n(url)[literal]\n</pre>\n\n[after](url)\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 2);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
    }

    #[test]
    fn test_reversed_link_after_mdx_module_still_gets_fixed() {
        let rule = MD011NoReversedLinks;
        let source = "export const text = \"(url)[literal]\"\n\n(url)[real]\n";
        let expected = "export const text = \"(url)[literal]\"\n\n[real](url)\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::MDX, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
    }

    #[test]
    fn test_reversed_links_in_markdown_html_remain_fixable() {
        let rule = MD011NoReversedLinks;
        for (flavor, source) in [
            (crate::config::MarkdownFlavor::MDX, "<div>\n(url)[visible]\n</div>\n"),
            (
                crate::config::MarkdownFlavor::MkDocs,
                "<div markdown=\"1\">\n(url)[visible]\n</div>\n",
            ),
            (
                crate::config::MarkdownFlavor::Kramdown,
                "<div markdown=\"1\">\n(url)[visible]\n</div>\n",
            ),
        ] {
            let ctx = LintContext::new(source, flavor, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1);
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                source.replace("(url)[visible]", "[visible](url)")
            );
        }
    }

    #[test]
    fn test_reversed_patterns_inside_existing_links_are_preserved() {
        let rule = MD011NoReversedLinks;
        for source in [
            "[Example (https://example.org)[label]](https://outer.test)\n",
            "![Example (https://example.org)[label]](https://outer.test/image.png)\n",
            "[Example (url)[label]][ref]\n\n[ref]: /target\n",
            "[Example (url)[label]][ref]\n\n[ref]: <>\n",
            "[go](https://example.org/(segment)[query])\n",
            "[reference]: https://example.org \"(url)[title]\"\n\n[reference]\n",
            "[reference]: https://example.org\n  \"(url)[title]\"\n\n[reference]\n",
            "日本語 [例 (https://example.org)[内容]](/target)\n",
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "existing link content: {source}");
            assert_eq!(rule.fix(&ctx).unwrap(), source);
        }
    }

    #[test]
    fn test_real_reversed_link_beside_existing_link_is_fixed() {
        let rule = MD011NoReversedLinks;
        let source = "[Example (url)[label]](/outer) and (https://example.org)[real]\n";
        let expected = "[Example (url)[label]](/outer) and [real](https://example.org)\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
        let fixed_ctx = LintContext::new(expected, crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new(expected));
        assert_eq!(
            html,
            "<p><a href=\"/outer\">Example (url)[label]</a> and <a href=\"https://example.org\">real</a></p>\n"
        );
    }

    #[test]
    fn test_fixed_link_is_not_reinterpreted_as_another_reversed_link() {
        let rule = MD011NoReversedLinks;
        let source = "(url)[label][missing]\n";
        let expected = "[label](url)[missing]\n";
        let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
        let fixed_ctx = LintContext::new(expected, crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new(expected));
        assert_eq!(html, "<p><a href=\"url\">label</a>[missing]</p>\n");
    }

    #[test]
    fn test_md011_basic() {
        let rule = MD011NoReversedLinks;

        // Should detect reversed links
        let content = "(http://example.com)[Example]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 1);

        // Should not detect correct links
        let content = "[Example](http://example.com)\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_md011_with_escaped_brackets() {
        let rule = MD011NoReversedLinks;

        // Should not detect if brackets are escaped
        let content = "(url)[text\\]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_md011_no_false_positive_with_reference_link() {
        let rule = MD011NoReversedLinks;

        // Should not detect (text)[ref](url) as reversed
        let content = "(text)[ref](url)\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_md011_fix() {
        let rule = MD011NoReversedLinks;

        let content = "(http://example.com)[Example]\n(another/url)[text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[Example](http://example.com)\n[text](another/url)\n");
    }

    #[test]
    fn test_md011_in_code_block() {
        let rule = MD011NoReversedLinks;

        let content = "```\n(url)[text]\n```\n(url)[text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 4);
    }

    #[test]
    fn test_md011_inline_code() {
        let rule = MD011NoReversedLinks;

        let content = "`(url)[text]` and (url)[text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].column, 19);
    }

    #[test]
    fn test_md011_no_false_positive_with_footnote() {
        let rule = MD011NoReversedLinks;

        // Should not detect [link](url)[^footnote] as reversed - this is valid markdown
        // The [^footnote] is a footnote reference, not part of a reversed link
        let content = "Some text with [a link](https://example.com/)[^ft].\n\n[^ft]: Note.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);

        // Also test with multiple footnotes
        let content = "[link1](url1)[^1] and [link2](url2)[^2]\n\n[^1]: First\n[^2]: Second\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);

        // But should still detect actual reversed links
        let content = "(url)[text] and [link](url)[^footnote]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 1);
        assert_eq!(warnings[0].column, 1);
    }

    #[test]
    fn test_md011_skip_dataview_inline_fields_obsidian() {
        let rule = MD011NoReversedLinks;

        // Dataview inline field pattern: (field:: value)[text]
        // In Obsidian flavor, this should NOT be flagged as a reversed link
        let content = "(status:: active)[link text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Obsidian, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            0,
            "Should not flag Dataview inline field in Obsidian flavor"
        );

        // Multiple inline fields
        let content = "(author:: John)[read more] and (date:: 2024-01-01)[link]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Obsidian, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "Should not flag multiple Dataview inline fields");

        // Mixed content: Dataview field and actual reversed link
        let content = "(status:: done)[info] (url)[text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Obsidian, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Should flag reversed link but not Dataview field");
        assert_eq!(warnings[0].column, 23);
    }

    #[test]
    fn test_md011_flag_dataview_in_standard_flavor() {
        let rule = MD011NoReversedLinks;

        // In Standard flavor, (field:: value)[text] is treated as a reversed link
        // because Dataview is Obsidian-specific
        let content = "(status:: active)[link text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            1,
            "Should flag Dataview-like pattern in Standard flavor"
        );
    }

    #[test]
    fn test_md011_reversed_link_in_jsx_nested_fence_not_flagged() {
        // A reversed link inside a fenced code block nested in a JSX component is
        // code, not prose. pulldown-cmark classifies the component as one HTML
        // block and emits no code-block range for the fence, so MD011's
        // byte-range code check would flag it (and `fmt` would corrupt the code)
        // unless the JSX fence range is added to ctx.code_blocks.
        let rule = MD011NoReversedLinks;
        let content =
            "<Steps>\n  <Step>\n```text\nsee (this)[https://example.com] reversed\n```\n  </Step>\n</Steps>\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "reversed link inside a JSX-nested fence must not be flagged: {warnings:?}"
        );
    }

    #[test]
    fn test_md011_dataview_bracket_syntax_obsidian() {
        let rule = MD011NoReversedLinks;

        // Dataview also supports [field:: value] syntax inside brackets
        // The pattern (field:: value)[text] should be skipped in Obsidian
        let content = "Task has (priority:: high)[see details]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Obsidian, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "Should skip Dataview field with spaces");

        // Field with no value (just key::)
        let content = "(completed::)[marker]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Obsidian, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "Should skip Dataview field with empty value");
    }

    #[test]
    fn test_md011_fix_skips_obsidian_comments() {
        let rule = MD011NoReversedLinks;

        // Reversed link inside Obsidian comment block should not be modified by fix()
        let content = "%%\n(http://example.com)[hidden link]\n%%\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Obsidian, None);

        // check() should produce no warnings (Obsidian comment is skipped)
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "check() should skip Obsidian comment content");

        // fix() should not modify content inside Obsidian comments
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, content,
            "fix() should not modify reversed links inside Obsidian comments"
        );
    }

    #[test]
    fn test_md011_fix_skips_obsidian_comments_with_surrounding_content() {
        let rule = MD011NoReversedLinks;

        // Mix of Obsidian comment and real reversed link
        let content = "%%\n(http://example.com)[hidden]\n%%\n\n(http://real.com)[visible]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Obsidian, None);

        // check() should only flag the visible one
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "check() should only flag visible reversed link");
        assert_eq!(warnings[0].line, 5);

        // fix() should only fix the visible one, leaving comment content untouched
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "%%\n(http://example.com)[hidden]\n%%\n\n[visible](http://real.com)\n",
            "fix() should only modify visible reversed links"
        );
    }

    #[test]
    fn test_md011_fix_skips_dataview_fields_obsidian() {
        let rule = MD011NoReversedLinks;

        // Dataview inline field should not be modified by fix()
        let content = "(status:: active)[link text]\n(http://example.com)[real link]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Obsidian, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "check() should only flag the real reversed link");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "(status:: active)[link text]\n[real link](http://example.com)\n",
            "fix() should not modify Dataview inline fields"
        );
    }
}

#[cfg(test)]
mod titled_relative_destination_tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn md011_titled_relative_paths_keep_destination_and_title() {
        for url in [
            r#"docs/guide.md "Guide""#,
            "relative/path.html 'Title'",
            r#"<docs/my guide.md> "Long guide""#,
            r#"docs/guide.md "Title with \"quotes\"""#,
            r#"docs/guide.md "&amp;copy;""#,
        ] {
            for label in ["label", "read the guide", "内容 guide"] {
                for source in [format!("({url})[{label}]"), format!("({label})[{url}]")] {
                    for ending in ["\n", "\r\n"] {
                        let source = format!("{source}{ending}");
                        let expected = format!("[{label}]({url}){ending}");
                        let ctx = LintContext::new(&source, crate::config::MarkdownFlavor::Standard, None);
                        assert_eq!(MD011NoReversedLinks.fix(&ctx).unwrap(), expected, "{source}");
                        let ctx = LintContext::new(&expected, crate::config::MarkdownFlavor::Standard, None);
                        assert!(MD011NoReversedLinks.check(&ctx).unwrap().is_empty());
                        assert_eq!(MD011NoReversedLinks.fix(&ctx).unwrap(), expected);
                    }
                }
            }
        }
    }

    #[test]
    fn md011_clear_urls_keep_priority_over_titled_relative_paths() {
        for (source, expected) in [
            (
                r#"(docs/guide.md "Guide")[https://example.org]"#,
                r#"[docs/guide.md "Guide"](https://example.org)"#,
            ),
            (
                r#"(https://example.org)[docs/guide.md "Guide"]"#,
                r#"[docs/guide.md "Guide"](https://example.org)"#,
            ),
            (
                r#"(say "hello")[https://example.org]"#,
                r#"[say "hello"](https://example.org)"#,
            ),
            (
                r#"(https://example.org "Title")[label]"#,
                r#"[label](https://example.org "Title")"#,
            ),
        ] {
            let ctx = LintContext::new(source, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(MD011NoReversedLinks.fix(&ctx).unwrap(), expected);
        }
    }

    #[test]
    fn md011_slashes_and_quotes_alone_do_not_promote_prose() {
        for component in [
            "read/write documentation",
            r#"say "hello""#,
            r#"docs/guide.md "unterminated"#,
            r#"docs/guide.md "Title" trailing"#,
            r#"docs/guide.md """#,
        ] {
            assert_eq!(
                MD011NoReversedLinks::classify_component(component),
                LinkComponent::MultiWord,
                "{component}"
            );
        }
    }
}
