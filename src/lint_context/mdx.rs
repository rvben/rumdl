//! MDX syntax, shared by context classification and link consumers.
//!
//! CommonMark's HTML events cannot describe JSX children: blank lines have no
//! bearing on whether those children are Markdown. Parse MDX once and use its
//! source positions, including for expressions and attributes on mixed lines.

use super::{FootnoteRef, LineInfo, LintContext, ParsedImage, ParsedLink, ReferenceDef};
use markdown::mdast::{AttributeContent, AttributeValue, Node, ReferenceKind};
use pulldown_cmark::LinkType;
use std::borrow::Cow;
use std::collections::HashMap;

pub(super) struct MdxContext {
    root: Node,
    pub code_blocks: Vec<(usize, usize)>,
    pub code_spans: Vec<(usize, usize)>,
    pub expressions: Vec<(usize, usize)>,
    pub comments: Vec<(usize, usize)>,
    text: Vec<(usize, usize)>,
    jsx: Vec<(usize, usize)>,
    esm: Vec<(usize, usize)>,
}

fn nodes(root: &Node) -> impl Iterator<Item = &Node> {
    let mut stack = vec![root];
    std::iter::from_fn(move || {
        let node = stack.pop()?;
        if let Some(children) = node.children() {
            stack.extend(children.iter().rev());
        }
        Some(node)
    })
}

impl MdxContext {
    pub(super) fn parse(content: &str, lines: &[LineInfo]) -> Option<Self> {
        // Front matter is metadata, not MDX. Mask it without changing source
        // offsets or line endings.
        let mut input = content.as_bytes().to_vec();
        for line in lines {
            if line.in_front_matter {
                for byte in &mut input[line.byte_offset..line.byte_offset + line.byte_len] {
                    if !matches!(*byte, b'\n' | b'\r') {
                        *byte = b' ';
                    }
                }
            }
        }
        let mut options = markdown::ParseOptions::mdx();
        options.mdx_expression_parse = Some(Box::new(parse_expression));
        options.mdx_esm_parse = Some(Box::new(parse_esm));
        // Heading structure comes from the primary Markdown parser. This AST
        // only supplies MDX regions and inline content; it does not consume
        // heading nodes. markdown-rs 1.0.0 can panic while closing a Setext
        // underline with an unclosed JSX element on its stack. Parse those
        // lines as ordinary Markdown here so malformed JSX returns an error
        // and uses the recovery context instead, including on panic=abort targets.
        options.constructs.heading_setext = false;
        options.constructs.gfm_table = true;
        options.constructs.gfm_strikethrough = true;
        options.constructs.gfm_task_list_item = true;
        options.constructs.gfm_footnote_definition = true;
        options.constructs.gfm_label_start_footnote = true;
        let root = match markdown::to_mdast(std::str::from_utf8(&input).expect("masked UTF-8"), &options) {
            Ok(root) => root,
            Err(error) => {
                log::debug!("MDX syntax unavailable; retaining recovery context: {error}");
                return None;
            }
        };
        let mut ctx = Self {
            root,
            code_blocks: Vec::new(),
            code_spans: Vec::new(),
            expressions: Vec::new(),
            comments: Vec::new(),
            text: Vec::new(),
            jsx: Vec::new(),
            esm: Vec::new(),
        };
        for node in nodes(&ctx.root) {
            let Some(pos) = node.position() else { continue };
            let range = (pos.start.offset, pos.end.offset);
            match node {
                Node::Code(_) => ctx.code_blocks.push(range),
                Node::InlineCode(_) => ctx.code_spans.push(range),
                Node::Text(_) => ctx.text.push(range),
                Node::MdxjsEsm(_) => ctx.esm.push(range),
                Node::MdxJsxFlowElement(element) => {
                    if ctx.jsx.last().is_none_or(|&(_, end)| range.0 >= end) {
                        ctx.jsx.push(range);
                    }
                    attribute_expressions(&element.attributes, &mut ctx.expressions);
                }
                Node::MdxJsxTextElement(element) => {
                    if ctx.jsx.last().is_none_or(|&(_, end)| range.0 >= end) {
                        ctx.jsx.push(range);
                    }
                    attribute_expressions(&element.attributes, &mut ctx.expressions);
                }
                Node::MdxFlowExpression(_) | Node::MdxTextExpression(_) => {
                    if content[range.0..range.1].starts_with("{/*") {
                        ctx.comments.push(range);
                    } else {
                        ctx.expressions.push(range);
                    }
                }
                _ => {}
            }
        }
        ctx.expressions.sort_unstable();
        Some(ctx)
    }

    pub(super) fn apply_lines(&self, lines: &mut [LineInfo]) {
        for line in lines.iter_mut() {
            // JSX is not literal HTML, including lowercase intrinsic elements.
            line.in_html_block = false;
            line.in_jsx_block = false;
            line.in_code_block = false;
            line.in_jsx_expression = false;
            line.in_mdx_comment = false;
            line.in_esm_block = false;
        }
        for &(start, end) in &self.esm {
            mark_lines(lines, start, end, |line| line.in_esm_block = true);
        }
        for &(start, end) in &self.jsx {
            mark_lines(lines, start, end, |line| line.in_jsx_block = true);
        }
        for &(start, end) in &self.code_blocks {
            mark_lines(lines, start, end, |line| line.in_code_block = true);
        }
        for &(start, end) in &self.expressions {
            mark_lines(lines, start, end, |line| line.in_jsx_expression = true);
        }
        for &(start, end) in &self.comments {
            mark_lines(lines, start, end, |line| line.in_mdx_comment = true);
        }
    }

    /// Only text nodes may contain unresolved reference syntax. In particular,
    /// regex fallbacks must never resurrect links from JSX attributes or JS.
    pub(super) fn contains_text(&self, start: usize, end: usize) -> bool {
        let idx = self.text.partition_point(|&(s, _)| s <= start);
        idx > 0 && end <= self.text[idx - 1].1
    }

    pub(super) fn reference_defs(&self, content: &str) -> Vec<ReferenceDef> {
        nodes(&self.root)
            .filter_map(|node| {
                let Node::Definition(def) = node else { return None };
                let pos = def.position.as_ref()?;
                let raw = &content[pos.start.offset..pos.end.offset];
                let title = def
                    .title
                    .clone()
                    .or_else(|| super::link_parser::has_explicit_empty_title_ending(raw).then(String::new));
                let title_range = title.as_ref().and_then(|_| title_range(raw));
                Some(ReferenceDef {
                    line: pos.start.line,
                    id: def.identifier.to_lowercase(),
                    url: def.url.clone(),
                    title,
                    byte_offset: pos.start.offset,
                    byte_end: pos.end.offset,
                    title_byte_start: title_range.map(|r| pos.start.offset + r.0),
                    title_byte_end: title_range.map(|r| pos.start.offset + r.1),
                })
            })
            .collect()
    }

    pub(super) fn footnote_refs(&self) -> Vec<FootnoteRef> {
        nodes(&self.root)
            .filter_map(|node| {
                let Node::FootnoteReference(reference) = node else {
                    return None;
                };
                let pos = reference.position.as_ref()?;
                Some(FootnoteRef {
                    id: reference.identifier.clone(),
                    line: pos.start.line,
                    byte_offset: pos.start.offset,
                })
            })
            .collect()
    }

    pub(super) fn links_and_images<'a>(
        &self,
        content: &'a str,
        lines: &[LineInfo],
    ) -> (Vec<ParsedLink<'a>>, Vec<ParsedImage<'a>>) {
        let mut definitions = HashMap::new();
        for node in nodes(&self.root) {
            if let Node::Definition(def) = node {
                definitions.entry(def.identifier.clone()).or_insert(def);
            }
        }
        let mut links = Vec::new();
        let mut images = Vec::new();
        for node in nodes(&self.root) {
            let (url, title, reference_id, link_type, image) = match node {
                Node::Link(link) => (link.url.as_str(), link.title.as_deref(), None, LinkType::Inline, false),
                Node::Image(image) => (image.url.as_str(), image.title.as_deref(), None, LinkType::Inline, true),
                Node::LinkReference(link) => {
                    let Some(def) = definitions.get(&link.identifier) else {
                        continue;
                    };
                    (
                        def.url.as_str(),
                        def.title.as_deref(),
                        Some(link.identifier.as_str()),
                        reference_type(&link.reference_kind),
                        false,
                    )
                }
                Node::ImageReference(image) => {
                    let Some(def) = definitions.get(&image.identifier) else {
                        continue;
                    };
                    (
                        def.url.as_str(),
                        def.title.as_deref(),
                        Some(image.identifier.as_str()),
                        reference_type(&image.reference_kind),
                        true,
                    )
                }
                _ => continue,
            };
            let pos = node.position().expect("parsed node position");
            let start = pos.start.offset;
            let end = pos.end.offset;
            let (_, line, start_col) = LintContext::find_line_for_offset(lines, content, start);
            let (_, end_line, end_col) = LintContext::find_line_for_offset(lines, content, end);
            let text = if let Some(children) = node.children() {
                // MDX labels may contain JSX with brackets in its attributes.
                // Child positions identify the label boundary without treating
                // those attribute brackets as Markdown delimiters.
                let label_end = children
                    .last()
                    .and_then(Node::position)
                    .map_or(start + 1, |p| p.end.offset);
                Cow::Borrowed(&content[start + 1..label_end])
            } else {
                Cow::Borrowed(image_label(&content[start..end], link_type, title.is_some()))
            };
            let url = Cow::Owned(url.to_owned());
            let title = title.or_else(|| {
                let def = definitions.get(reference_id?)?;
                let position = def.position.as_ref()?;
                super::link_parser::has_explicit_empty_title_ending(
                    &content[position.start.offset..position.end.offset],
                )
                .then_some("")
            });
            let title = title.map(|s| Cow::Owned(s.to_owned())).or_else(|| {
                (matches!(link_type, LinkType::Inline)
                    && super::link_parser::has_explicit_empty_inline_title(&content[start..end]))
                .then_some(Cow::Borrowed(""))
            });
            let is_reference = reference_id.is_some();
            let reference_id = reference_id.map(|s| Cow::Owned(s.to_lowercase()));
            if image {
                images.push(ParsedImage {
                    line,
                    end_line,
                    start_col,
                    end_col,
                    byte_offset: start,
                    byte_end: end,
                    alt_text: text,
                    url,
                    title,
                    is_reference,
                    reference_id,
                    link_type,
                });
            } else {
                links.push(ParsedLink {
                    line,
                    end_line,
                    start_col,
                    end_col,
                    byte_offset: start,
                    byte_end: end,
                    text,
                    url,
                    title,
                    is_reference,
                    reference_id,
                    link_type,
                });
            }
        }
        (links, images)
    }
}

/// A parsed definition's title is the final delimited string. Locate the raw
/// delimiters without comparing its decoded value with escaped source text.
fn title_range(source: &str) -> Option<(usize, usize)> {
    let bytes = source.trim_end().as_bytes();
    let end = bytes.len();
    let open = match bytes.last()? {
        b')' => b'(',
        b'"' => b'"',
        b'\'' => b'\'',
        _ => return None,
    };
    let mut escaped = false;
    let mut candidate = None;
    for (i, &byte) in bytes[..end - 1].iter().enumerate() {
        if byte == open && !escaped && i > 0 && bytes[i - 1].is_ascii_whitespace() {
            candidate = Some(i);
        }
        escaped = byte == b'\\' && !escaped;
    }
    candidate.map(|start| (start, end))
}

fn attribute_expressions(attributes: &[AttributeContent], ranges: &mut Vec<(usize, usize)>) {
    for attribute in attributes {
        let (value, stops) = match attribute {
            AttributeContent::Expression(expr) => (&expr.value, &expr.stops),
            AttributeContent::Property(prop) => match &prop.value {
                Some(AttributeValue::Expression(expr)) => (&expr.value, &expr.stops),
                _ => continue,
            },
        };
        if let (Some(first), Some(last)) = (stops.first(), stops.last()) {
            ranges.push((first.1.saturating_sub(1), last.1 + value.len() - last.0 + 1));
        }
    }
}

/// Let a JavaScript parser decide whether a candidate `}` closes the MDX
/// expression. Counting braces cannot distinguish strings, regex literals,
/// comments, templates, or JSX nested in an expression.
fn parse_expression(value: &str, kind: &markdown::MdxExpressionKind) -> markdown::MdxSignal {
    let allocator = oxc_allocator::Allocator::default();
    let source_type = oxc_span::SourceType::jsx();
    let empty = oxc_parser::Parser::new(&allocator, value, source_type).parse();
    if empty.diagnostics.is_empty() && empty.program.body.is_empty() && empty.program.directives.is_empty() {
        return markdown::MdxSignal::Ok;
    }
    let wrapped = if matches!(kind, markdown::MdxExpressionKind::AttributeExpression) {
        format!("const mdx = ({{{value}\n}});")
    } else {
        format!("const mdx = (\n{value}\n);")
    };
    parse_esm(&wrapped)
}

fn parse_esm(value: &str) -> markdown::MdxSignal {
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, value, oxc_span::SourceType::jsx()).parse();
    if parsed.diagnostics.is_empty() && !parsed.panicked {
        markdown::MdxSignal::Ok
    } else {
        // A candidate ending may lie inside JS. Ask markdown-rs to continue to
        // the next candidate; at EOF it returns a syntax error for recovery.
        markdown::MdxSignal::Eof(
            "Incomplete JavaScript".into(),
            Box::new("rumdl".into()),
            Box::new("mdx".into()),
        )
    }
}

fn reference_type(kind: &ReferenceKind) -> LinkType {
    match kind {
        ReferenceKind::Full => LinkType::Reference,
        ReferenceKind::Collapsed => LinkType::Collapsed,
        ReferenceKind::Shortcut => LinkType::Shortcut,
    }
}

/// Image nodes store decoded alt text rather than children. Find the label
/// from the resource/reference suffix so brackets inside JSX and code in the
/// alt text cannot truncate a link-style fix.
fn image_label(source: &str, kind: LinkType, has_title: bool) -> &str {
    let bytes = source.as_bytes();
    let label_end = match kind {
        LinkType::Shortcut => Some(bytes.len() - 1),
        LinkType::Collapsed => Some(bytes.len() - 3),
        LinkType::Reference => (2..bytes.len() - 1)
            .rev()
            .find(|&i| bytes[i] == b'[' && !escaped_at(bytes, i))
            .and_then(|i| i.checked_sub(1)),
        LinkType::Inline => {
            let mut end = bytes.len() - 1; // outer closing parenthesis
            if (has_title || super::link_parser::has_explicit_empty_inline_title(source))
                && let Some((start, _)) = title_range(&source[..end])
            {
                end = start;
            }
            while end > 0 && bytes[end - 1].is_ascii_whitespace() {
                end -= 1;
            }
            // Angle destinations may themselves contain unbalanced parentheses.
            if end > 0
                && bytes[end - 1] == b'>'
                && !escaped_at(bytes, end - 1)
                && let Some(open) = (2..end - 1).rev().find(|&i| bytes[i] == b'<' && !escaped_at(bytes, i))
            {
                let before = source[..open].trim_end();
                if before.ends_with("](")
                    && !(open + 1..end - 1).any(|i| matches!(bytes[i], b'<' | b'>') && !escaped_at(bytes, i))
                {
                    end = open;
                }
            }
            let mut depth = 0usize;
            let mut opener = None;
            for i in (2..end).rev() {
                if !matches!(bytes[i], b'(' | b')') || escaped_at(bytes, i) {
                    continue;
                }
                match bytes[i] {
                    b')' => depth += 1,
                    b'(' if depth == 0 => {
                        opener = i.checked_sub(1);
                        break;
                    }
                    b'(' => depth -= 1,
                    _ => {}
                }
            }
            opener
        }
        _ => None,
    };
    let end = label_end
        .filter(|&end| end >= 2 && bytes.get(end) == Some(&b']'))
        .expect("MDX image has a parsed label and resource/reference suffix");
    &source[2..end]
}

fn escaped_at(bytes: &[u8], offset: usize) -> bool {
    bytes[..offset].iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 1
}

fn mark_lines(lines: &mut [LineInfo], start: usize, end: usize, mark: impl Fn(&mut LineInfo)) {
    let first = lines
        .partition_point(|line| line.byte_offset <= start)
        .saturating_sub(1);
    for line in lines[first..].iter_mut().take_while(|line| line.byte_offset < end) {
        mark(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::rule::Rule;
    use crate::rules::{MD042NoEmptyLinks, MD059LinkText, MD091NoMarkdownInHtml};

    fn context(content: &str) -> LintContext<'_> {
        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
        assert!(
            MdxContext::parse(content, &ctx.lines).is_some(),
            "MDX parse failed: {content}"
        );
        ctx
    }

    #[test]
    fn unclosed_jsx_before_setext_underline_uses_recovery_context() {
        for content in [
            "<span>\\</span>\n``\n- ",
            "<span>\\</span>\ntext\n---",
            "<span>\\</span>\r\ntext\r\n===",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
            assert!(MdxContext::parse(content, &ctx.lines).is_none());
            assert_eq!(ctx.content, content);
        }
    }

    #[test]
    fn setext_headings_keep_mdx_links_and_code_positions() {
        for underline in ["-", "---", "==="] {
            for ending in ["\n", "\r\n"] {
                let content = format!(
                    "Heading <span>é [Visible](/visible) `[Hidden](/hidden)`</span>{ending}{underline}{ending}"
                );
                let ctx = context(&content);
                assert_eq!(ctx.links.len(), 1);
                assert_eq!(ctx.links[0].url, "/visible");
                assert_eq!(
                    &content[ctx.links[0].byte_offset..ctx.links[0].byte_end],
                    "[Visible](/visible)"
                );
                assert_eq!(ctx.code_spans().len(), 1);
                assert!(ctx.lines[0].heading.is_some());
            }
        }
    }

    #[test]
    fn generated_table_blank_lines_do_not_change_link_semantics() {
        // Shape emitted by typedoc-plugin-markdown's htmlTable renderer.
        let expanded = "<table>\n<thead>\n<tr><th>Function</th></tr>\n</thead>\n<tbody>\n<tr>\n<td>\n\n[Documentation](functions/example.md)\n\n</td>\n</tr>\n</tbody>\n</table>\n";
        for content in [expanded.to_owned(), expanded.replace("\n\n", "\n")] {
            let ctx = context(&content);
            assert_eq!(ctx.links.len(), 1);
            assert_eq!(ctx.links[0].url, "functions/example.md");
            assert!(ctx.lines.iter().all(|line| !line.in_html_block));
            assert!(MD091NoMarkdownInHtml::new().check(&ctx).unwrap().is_empty());
        }
        let compact = expanded.replace("\n\n", "\n");
        let standard = LintContext::new(&compact, MarkdownFlavor::Standard, None);
        assert_eq!(MD091NoMarkdownInHtml::new().check(&standard).unwrap().len(), 1);
        assert!(standard.links.is_empty());
    }

    #[test]
    fn table_links_reach_other_rules_with_original_unicode_positions() {
        let content = "<table>\n<tr><td>é [click here](/docs) [Empty]()</td></tr>\n</table>\n";
        let ctx = context(content);
        let empty = MD042NoEmptyLinks::new().check(&ctx).unwrap();
        let vague = MD059LinkText::default().check(&ctx).unwrap();
        assert_eq!(empty.len(), 1);
        assert_eq!(vague.len(), 1);
        assert_eq!(empty[0].line, 2);
        assert_eq!(vague[0].column, 11);
        for link in &ctx.links {
            assert!(content[link.byte_offset..link.byte_end].starts_with('['));
        }
    }

    #[test]
    fn expressions_attributes_and_comments_are_not_links() {
        let content = "<div title=\"[Empty]()\" data-text={'[click here](/docs)'}>\n{<table><tbody><tr><td>[click here](/docs) [Empty]()</td></tr></tbody></table>}\n{/* [Empty]() */}\n[Actual](/actual)\n</div>\n";
        let ctx = context(content);
        assert_eq!(ctx.links.len(), 1);
        assert_eq!(ctx.links[0].text, "Actual");
        assert!(MD042NoEmptyLinks::new().check(&ctx).unwrap().is_empty());
        assert!(MD059LinkText::default().check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn code_inside_intrinsic_and_custom_elements_stays_code() {
        let content = "<section>\n<Card>\n    [Visible](/visible)\n    `[Empty]()`\n    ```md\n    [Empty]()\n    ```\n</Card>\n</section>\n";
        let ctx = context(content);
        assert_eq!(ctx.links.len(), 1);
        assert_eq!(ctx.code_spans().len(), 1);
        assert!(ctx.lines[5].in_code_block);
        assert!(MD042NoEmptyLinks::new().check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn reference_links_images_and_titles_resolve_inside_jsx() {
        let content = "<div>\n[Named][target] [target][] [target] ![alt][target]\n![Inline](image.png \"title\")\n</div>\n\n[target]: path.md \"Reference title\"\n";
        let ctx = context(content);
        assert_eq!(ctx.links.len(), 3);
        assert_eq!(ctx.images.len(), 2);
        for link in &ctx.links {
            assert_eq!(link.url, "path.md");
            assert_eq!(link.title.as_deref(), Some("Reference title"));
        }
        assert_eq!(ctx.images[1].title.as_deref(), Some("title"));
        assert_eq!(ctx.reference_defs.len(), 1);
    }

    #[test]
    fn unresolved_references_only_come_from_markdown_text() {
        let content = "<div title=\"[Fake][missing]\">\n{'[Fake][missing]'}\n[Actual][missing]\n</div>\n";
        let ctx = context(content);
        assert_eq!(ctx.links.len(), 1);
        assert_eq!(ctx.links[0].text, "Actual");
    }

    #[test]
    fn inline_jsx_fragments_and_multiline_attributes() {
        let content =
            "<>\n<ui.Card\n title=\"a > [Empty]()\"\n>\n<span>[Actual](a(b).md \"\")</span>\n</ui.Card>\n</>\n";
        let ctx = context(content);
        assert_eq!(ctx.links.len(), 1);
        assert_eq!(ctx.links[0].url, "a(b).md");
        assert_eq!(ctx.links[0].title.as_deref(), Some(""));
    }

    #[test]
    fn source_label_retains_nested_markup_and_escapes() {
        let content = "<div>[a `]` and **b**](target.md) ![a\\]b](image.png)</div>\n";
        let ctx = context(content);
        assert_eq!(ctx.links[0].text, "a `]` and **b**");
        assert_eq!(ctx.images[0].alt_text, "a\\]b");
    }

    #[test]
    fn braces_in_javascript_strings_do_not_end_the_expression() {
        for content in [
            "<div>{'} [Empty]()'} [Actual](/docs)</div>\n",
            "<div>{/}/.test('}') ? '[Empty]()' : ''} [Actual](/docs)</div>\n",
            "<div>{`text } ${\"[Empty]()\"}`} [Actual](/docs)</div>\n",
            "<div>{/* } [Empty]() */} [Actual](/docs)</div>\n",
        ] {
            let ctx = context(content);
            assert_eq!(ctx.links.len(), 1, "{content}");
            assert_eq!(ctx.links[0].text, "Actual");
        }
    }
    #[test]
    fn esm_frontmatter_and_indented_reference_definitions() {
        let content = "---\ntitle: '[Fake]()'\n---\nimport Component from './component.js'\n\nexport const example = {\n  text: '[Fake]()',\n}\n\n<Component>\n    [Actual][target]\n\n    [target]: path.md \"title\"\n</Component>\n";
        let ctx = context(content);
        assert_eq!(ctx.links.len(), 1);
        assert_eq!(ctx.reference_defs.len(), 1);
        assert_eq!(ctx.reference_defs[0].url, "path.md");
        let def = &ctx.reference_defs[0];
        assert_eq!(
            &content[def.title_byte_start.unwrap()..def.title_byte_end.unwrap()],
            "\"title\""
        );
    }

    #[test]
    fn code_and_attribute_expression_ranges_remain_available_to_other_rules() {
        let content = "<div value={{text: '[Fake]()'}}>é `[Code]()` [Actual](/docs)</div>\n";
        let ctx = context(content);
        assert!(ctx.is_in_jsx_expression(content.find("[Fake]").unwrap()));
        assert!(ctx.is_in_code_span_byte(content.find("[Code]").unwrap()));
        assert_eq!(ctx.links.len(), 1);
    }

    #[test]
    fn unmatched_backticks_in_link_labels_remain_literal() {
        let ctx = context("<div>[a ` b](url)</div>\n");
        assert_eq!(ctx.links[0].text, "a ` b");
    }
    #[test]
    fn jsx_brackets_in_a_link_label_do_not_truncate_its_source() {
        let content = "<div>[a <Badge title=\"]\" /> b](url) [ spaced ](url) [](url)</div>\n";
        let ctx = context(content);
        assert_eq!(ctx.links[0].text, "a <Badge title=\"]\" /> b");
        assert_eq!(ctx.links[1].text, " spaced ");
        assert_eq!(ctx.links[2].text, "");
    }
    #[test]
    fn image_alt_source_is_preserved_even_with_jsx_brackets() {
        for (source, expected) in [
            ("![a <Badge title=\"]\" /> b](url)", "a <Badge title=\"]\" /> b"),
            ("![a `]` b](url)", "a `]` b"),
            ("![a <Badge />](url>)", "a <Badge />"),
            ("![a](<unbalanced(.png> \"title\")", "a"),
            ("![a](url(with\\)escape).png)", "a"),
            ("![a][ref]\n\n[ref]: url", "a"),
        ] {
            let ctx = context(source);
            assert_eq!(ctx.images.len(), 1, "{source}");
            assert_eq!(ctx.images[0].alt_text, expected, "{source}");
        }
    }
    #[test]
    fn image_suffixes_do_not_confuse_label_brackets_with_destinations() {
        for alt in ["plain", "a `]` b", "a <Badge title=\"]\" /> b", "a [nested](url) b"] {
            for destination in [
                "",
                "file.png",
                "a(b)c.png",
                r"a\)b.png",
                "url>",
                "<a(b.png>",
                "<a)b.png>",
            ] {
                for title in ["", " \"\"", " 'title'", r" (title \(escaped\))"] {
                    // With no destination, a leading parenthesis is parsed as
                    // the destination rather than a parenthesized title.
                    if destination.is_empty() && title.starts_with(" (") {
                        continue;
                    }
                    let source = format!("![{alt}]({destination}{title})");
                    let ctx = context(&source);
                    assert_eq!(ctx.images.len(), 1, "{source}");
                    assert_eq!(ctx.images[0].alt_text, alt, "{source}");
                }
            }
        }
    }
}
