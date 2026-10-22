//! MDX syntax, shared by context classification and link consumers.
//!
//! CommonMark's HTML events cannot describe JSX children: blank lines have no
//! bearing on whether those children are Markdown. Parse MDX once and use its
//! source positions, including for expressions and attributes on mixed lines.

use super::line_computation::spanned_lines;
use super::{FootnoteRef, HtmlTag, LineInfo, ParsedImage, ParsedLink, ReferenceDef, SourcePositions};
use crate::mdx_parser as markdown;
use markdown::mdast::{AttributeContent, AttributeValue, Node, ReferenceKind};
use pulldown_cmark::LinkType;
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::Once;

pub(super) struct MdxContext {
    root: Node,
    pub code_blocks: Vec<(usize, usize)>,
    pub code_spans: Vec<(usize, usize)>,
    pub expressions: Vec<(usize, usize)>,
    pub comments: Vec<(usize, usize)>,
    text: Vec<(usize, usize)>,
    jsx: Vec<(usize, usize)>,
    pub(super) esm: Vec<(usize, usize)>,
    /// Byte ranges of the tables markdown-rs reads, JSX-wrapped ones included
    pub(super) tables: Vec<(usize, usize)>,
}

impl Drop for MdxContext {
    fn drop(&mut self) {
        // Traversal is iterative, and releasing a deeply nested JSX tree must
        // be too. Empty each node before its ordinary destructor runs. The
        // children accessor covers every field that owns further nodes.
        let Some(children) = self.root.children_mut() else {
            return;
        };
        let mut pending = std::mem::take(children);
        while let Some(mut node) = pending.pop() {
            if let Some(children) = node.children_mut() {
                pending.append(children);
            }
        }
    }
}

thread_local! {
    /// Set only while `parse_without_crashing` is inside the MDX parser.
    static RECOVERING_FROM_MDX_PARSE: Cell<bool> = const { Cell::new(false) };
}

/// Run the MDX parser, answering a parser panic the same way as a parse error.
///
/// Known malformed-JSX failures return errors in the bundled parser. Retain
/// this boundary as a last resort for other upstream parser assertions, so
/// incomplete editor input can still use the recovery context.
///
/// The panic report is suppressed for this call alone. It names a crash that
/// did not happen, and printing it also captures a backtrace, which costs more
/// than the parse. The flag is thread-local and set only around the parser, so
/// panics from anywhere else still report in full.
fn parse_without_crashing<T, E: ToString>(parse: impl FnOnce() -> Result<T, E>) -> Result<T, String> {
    static INSTALL_HOOK: Once = Once::new();
    INSTALL_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !RECOVERING_FROM_MDX_PARSE.get() {
                previous(info);
            }
        }));
    });

    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            RECOVERING_FROM_MDX_PARSE.set(false);
        }
    }

    RECOVERING_FROM_MDX_PARSE.set(true);
    let guard = Guard;
    let parsed = std::panic::catch_unwind(AssertUnwindSafe(parse));
    drop(guard);

    match parsed {
        Ok(result) => result.map_err(|error| error.to_string()),
        Err(_) => Err("MDX parser panicked on malformed syntax".to_string()),
    }
}

/// The MDX syntax tree of `content` with every source position removed, as text:
/// two documents with the same signature parse to the same structure and
/// content. `None` when the document does not parse as MDX.
pub(crate) fn structure_signature(content: &str) -> Option<String> {
    let mut root = parse_for_structure(content)?;
    strip_positions(&mut root);
    Some(format!("{root:?}"))
}

/// The byte range of each innermost block of `content` in source order, each
/// widened to whole lines: containers (block quotes, lists, list items) are
/// replaced by the blocks they hold. `None` when the document does not parse as
/// MDX.
pub(crate) fn leaf_block_ranges(content: &str) -> Option<Vec<std::ops::Range<usize>>> {
    fn collect(node: &Node, content: &str, ranges: &mut Vec<std::ops::Range<usize>>) {
        for child in node.children().into_iter().flatten() {
            let container = matches!(child, Node::Blockquote(_) | Node::List(_) | Node::ListItem(_));
            if container && child.children().is_some_and(|c| !c.is_empty()) {
                collect(child, content, ranges);
                continue;
            }
            let Some(position) = child.position() else { continue };
            let start = content[..position.start.offset].rfind('\n').map_or(0, |i| i + 1);
            let end = content[position.end.offset..]
                .find('\n')
                .map_or(content.len(), |i| position.end.offset + i);
            match ranges.last_mut() {
                // Blocks that share a line are one range, so ranges never overlap.
                Some(last) if start < last.end => last.end = last.end.max(end),
                _ => ranges.push(start..end),
            }
        }
    }
    let root = parse_for_structure(content)?;
    let mut ranges = Vec::new();
    collect(&root, content, &mut ranges);
    Some(ranges)
}

fn parse_for_structure(content: &str) -> Option<Node> {
    let options = mdx_parse_options();
    parse_without_crashing(|| markdown::to_mdast(content, &options)).ok()
}

fn mdx_parse_options() -> markdown::ParseOptions {
    let mut options = markdown::ParseOptions::mdx();
    options.mdx_expression_parse = Some(Box::new(parse_expression));
    options.mdx_esm_parse = Some(Box::new(parse_esm));
    options.constructs.gfm_table = true;
    options.constructs.gfm_strikethrough = true;
    options.constructs.gfm_task_list_item = true;
    options.constructs.gfm_footnote_definition = true;
    options.constructs.gfm_label_start_footnote = true;
    options
}

fn strip_positions(node: &mut Node) {
    if let Some(position) = node.position_mut() {
        *position = markdown::unist::Position::new(1, 1, 0, 1, 1, 0);
    }
    // The stops of an expression say where each slice of it sits in the source,
    // which is a position too.
    match node {
        Node::MdxFlowExpression(expression) => expression.stops.clear(),
        Node::MdxTextExpression(expression) => expression.stops.clear(),
        Node::MdxjsEsm(esm) => esm.stops.clear(),
        Node::MdxJsxFlowElement(element) => strip_attribute_stops(&mut element.attributes),
        Node::MdxJsxTextElement(element) => strip_attribute_stops(&mut element.attributes),
        _ => {}
    }
    if let Some(children) = node.children_mut() {
        children.iter_mut().for_each(strip_positions);
    }
}

fn strip_attribute_stops(attributes: &mut [AttributeContent]) {
    for attribute in attributes {
        match attribute {
            AttributeContent::Expression(expression) => expression.stops.clear(),
            AttributeContent::Property(property) => {
                if let Some(AttributeValue::Expression(expression)) = &mut property.value {
                    expression.stops.clear();
                }
            }
        }
    }
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
    /// Tag ranges from native JSX nodes exclude their child Markdown. Attribute
    /// expressions already have JavaScript-aware source ranges, so an arrow,
    /// regex or nested JSX element in a prop cannot end the outer opening tag.
    pub(super) fn html_tags(&self, content: &str, lines: &[LineInfo]) -> (Vec<HtmlTag>, Vec<HtmlTag>) {
        let mut html = Vec::new();
        let mut components = Vec::new();
        let mut positions = SourcePositions::new(content, lines);
        for node in nodes(&self.root) {
            let (name, position) = match node {
                Node::MdxJsxFlowElement(element) => (&element.name, &element.position),
                Node::MdxJsxTextElement(element) => (&element.name, &element.position),
                _ => continue,
            };
            let (Some(name), Some(position)) = (name, position) else {
                // Fragments have no attributes or tag name to classify.
                continue;
            };
            let start = position.start.offset;
            let end = position.end.offset;
            let Some(open_end) = jsx_tag_end(content, start, end, &self.expressions) else {
                continue;
            };
            let self_closing = content[start..open_end - 1].trim_end().ends_with('/');
            let closing = if self_closing {
                None
            } else {
                content[open_end..end].rfind("</").and_then(|relative| {
                    let close_start = open_end + relative;
                    content[close_start..end]
                        .find('>')
                        .map(|relative_end| (close_start, close_start + relative_end + 1, true, false))
                })
            };
            let is_component =
                name.contains('.') || (!name.contains(':') && name.chars().next().is_some_and(char::is_uppercase));
            let tags = if is_component { &mut components } else { &mut html };
            for (tag_start, tag_end, is_closing, is_self_closing) in
                std::iter::once((start, open_end, false, self_closing)).chain(closing)
            {
                let (index, line, start_col) = positions.position(tag_start);
                let end_col = positions.column_on_line(index, tag_end);
                tags.push(HtmlTag {
                    line,
                    start_col,
                    end_col,
                    byte_offset: tag_start,
                    byte_end: tag_end,
                    tag_name: name.to_lowercase(),
                    is_closing,
                    is_self_closing,
                });
            }
        }
        // Walking the AST visits an outer closing tag before its children.
        html.sort_unstable_by_key(|tag| tag.byte_offset);
        components.sort_unstable_by_key(|tag| tag.byte_offset);
        (html, components)
    }

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
        let options = mdx_parse_options();
        let source = std::str::from_utf8(&input).expect("masked UTF-8");
        let (root, attribute_ranges) =
            match parse_without_crashing(|| markdown::to_mdast_with_attribute_ranges(source, &options)) {
                Ok(parsed) => parsed,
                Err(error) => {
                    log::debug!("MDX syntax unavailable; retaining recovery context: {error}");
                    return None;
                }
            };
        let mut ctx = Self {
            root,
            code_blocks: Vec::new(),
            code_spans: Vec::new(),
            expressions: attribute_ranges,
            comments: Vec::new(),
            text: Vec::new(),
            jsx: Vec::new(),
            esm: Vec::new(),
            tables: Vec::new(),
        };
        for node in nodes(&ctx.root) {
            let Some(pos) = node.position() else { continue };
            let range = (pos.start.offset, pos.end.offset);
            match node {
                Node::Code(_) => ctx.code_blocks.push(range),
                Node::InlineCode(_) => ctx.code_spans.push(range),
                Node::Text(_) => ctx.text.push(range),
                Node::MdxjsEsm(_) => ctx.esm.push(range),
                Node::Table(_) => ctx.tables.push(range),
                Node::MdxJsxFlowElement(_) => {
                    if ctx.jsx.last().is_none_or(|&(_, end)| range.0 >= end) {
                        ctx.jsx.push(range);
                    }
                }
                Node::MdxJsxTextElement(_) => {
                    if ctx.jsx.last().is_none_or(|&(_, end)| range.0 >= end) {
                        ctx.jsx.push(range);
                    }
                }
                Node::MdxFlowExpression(_) | Node::MdxTextExpression(_) => {
                    if crate::utils::mdx_comments::is_comment_only_expression(&content[range.0..range.1]) {
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

    /// Whether each line holds MDX flow syntax, the lines a paragraph cannot run
    /// across: a flow expression, which is a block of its own, and a JSX flow
    /// element's own tags.
    ///
    /// An element's tags sit on the lines of its span that none of its children
    /// covers, such as the middle of a tag written across several lines. The
    /// children are Markdown blocks of their own, so their lines stay unmarked
    /// and a paragraph among them reads as one, while a flow element or
    /// expression among them marks its own lines. Only those two can share a
    /// line with a tag; any other block there is a parse error, which leaves no
    /// MDX parse to read. `in_jsx_block` cannot say this: it marks every line
    /// of the outermost element, and text elements too. Nor can
    /// `in_jsx_expression`, which also marks an expression inside a line of text.
    pub(super) fn flow_lines(&self, lines: &[LineInfo]) -> Vec<bool> {
        let mut flow = vec![false; lines.len()];
        for node in nodes(&self.root) {
            match node {
                Node::MdxFlowExpression(expression) => {
                    let Some(pos) = &expression.position else { continue };
                    flow[spanned_lines(lines, pos.start.offset, pos.end.offset)].fill(true);
                }
                Node::MdxJsxFlowElement(element) => {
                    let Some(pos) = &element.position else { continue };
                    let children: Vec<_> = element
                        .children
                        .iter()
                        .filter_map(Node::position)
                        .map(|child| spanned_lines(lines, child.start.offset, child.end.offset))
                        .collect();
                    for index in spanned_lines(lines, pos.start.offset, pos.end.offset) {
                        if !children.iter().any(|child| child.contains(&index)) {
                            flow[index] = true;
                        }
                    }
                }
                _ => {}
            }
        }
        flow
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
        let mut positions = SourcePositions::new(content, lines);
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
            let (_, line, start_col) = positions.position(start);
            let (_, end_line, end_col) = positions.position(end);
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
                let Some(label) = image_label(&content[start..end], link_type, title.is_some()) else {
                    log::debug!("MDX image label unavailable at {start}..{end}");
                    continue;
                };
                Cow::Borrowed(label)
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

fn jsx_tag_end(content: &str, start: usize, limit: usize, expressions: &[(usize, usize)]) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut pos = start + 1;
    let mut quote = None;
    while pos < limit {
        let byte = bytes[pos];
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
        } else if byte == b'{' {
            let index = expressions.partition_point(|&(_, end)| end <= pos);
            if let Some(&(expression_start, expression_end)) = expressions.get(index)
                && expression_start <= pos
            {
                pos = expression_end;
                continue;
            }
        } else if matches!(byte, b'"' | b'\'') {
            quote = Some(byte);
        } else if byte == b'>' {
            return Some(pos + 1);
        }
        pos += 1;
    }
    None
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
fn image_label(source: &str, kind: LinkType, has_title: bool) -> Option<&str> {
    let bytes = source.as_bytes();
    let suffix = bytes.len().checked_sub(1)?;
    let label_end = match kind {
        LinkType::Shortcut => Some(suffix),
        LinkType::Collapsed => bytes.len().checked_sub(3),
        LinkType::Reference => (2..suffix)
            .rev()
            .find(|&i| bytes[i] == b'[' && !escaped_at(bytes, i))
            .and_then(|i| i.checked_sub(1)),
        LinkType::Inline => {
            let mut end = suffix; // outer closing parenthesis
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
    let end = label_end.filter(|&end| end >= 2 && bytes.get(end) == Some(&b']'))?;
    source.get(2..end)
}

fn escaped_at(bytes: &[u8], offset: usize) -> bool {
    bytes[..offset].iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 1
}

fn mark_lines(lines: &mut [LineInfo], start: usize, end: usize, mark: impl Fn(&mut LineInfo)) {
    let span = spanned_lines(lines, start, end);
    for line in &mut lines[span] {
        mark(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;
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
    fn attribute_ranges_preserve_indentation_tabs_and_container_prefixes() {
        for source in [
            "<A b={x\n  } />",
            "- <A b={\n\tx}/>",
            "- <A b={\n\t x}/>",
            "> <A b={x\n>   } />",
            "<A {...props\n  } />",
            "<A b={/* } */} />",
            "<A b={x > y} />",
            "<A b={<B title='}' />} />",
        ] {
            for newline in ["\n", "\r\n"] {
                let content = source.replace('\n', newline);
                let ctx = context(&content);
                let mdx = MdxContext::parse(&content, &ctx.lines).unwrap();
                assert_eq!(
                    mdx.expressions,
                    vec![(content.find('{').unwrap(), content.rfind('}').unwrap() + 1)],
                    "{content:?}",
                );
                let (_, components) = mdx.html_tags(&content, &ctx.lines);
                assert_eq!(components.len(), 1, "{content:?}");
                assert_eq!(components[0].byte_offset, content.find('<').unwrap());
                assert_eq!(components[0].byte_end, content.len());
            }
        }
        let content = "{before}\n\n<A b={after} />\n\n{tail}";
        let ctx = context(content);
        let mdx = MdxContext::parse(content, &ctx.lines).unwrap();
        let slices: Vec<_> = mdx
            .expressions
            .iter()
            .map(|&(start, end)| &content[start..end])
            .collect();
        assert_eq!(slices, ["{before}", "{after}", "{tail}"]);
    }

    #[test]
    fn native_positions_match_source_for_nested_and_multiline_elements() {
        let prefix = "é日本🙂e\u{301}\t".repeat(80);
        let source = format!(
            "{prefix}<Card><span title=\"日本\">text <b>bold</b></span></Card> [![图](image.png)](target)\n\n\
             <Card\n  title=\"multiline\">\n\n{prefix}[multi\nline](target) ![multi\nimage](image.png)\n\n</Card>\n\n\
             {prefix}[ref][] ![ref][] [full][ref] ![full][ref] [ref] ![ref]\n\n[ref]: target"
        );
        for newline in ["\n", "\r\n"] {
            let content = source.replace('\n', newline);
            let ctx = context(&content);
            let mdx = MdxContext::parse(&content, &ctx.lines).unwrap();
            let (links, images) = mdx.links_and_images(&content, &ctx.lines);
            assert_eq!(links.len(), 5);
            assert_eq!(images.len(), 5);
            for (start, end, line, col, end_line, end_col) in links
                .iter()
                .map(|s| (s.byte_offset, s.byte_end, s.line, s.start_col, s.end_line, s.end_col))
                .chain(
                    images
                        .iter()
                        .map(|s| (s.byte_offset, s.byte_end, s.line, s.start_col, s.end_line, s.end_col)),
                )
            {
                let (_, expected_line, expected_col) = LintContext::find_line_for_offset(&ctx.lines, &content, start);
                let (_, expected_end_line, expected_end_col) =
                    LintContext::find_line_for_offset(&ctx.lines, &content, end);
                assert_eq!(
                    (line, col, end_line, end_col),
                    (expected_line, expected_col, expected_end_line, expected_end_col)
                );
            }
            let (html, components) = mdx.html_tags(&content, &ctx.lines);
            assert_eq!(html.len(), 4);
            assert_eq!(components.len(), 4);
            for tag in html.iter().chain(&components) {
                let (index, line, col) = LintContext::find_line_for_offset(&ctx.lines, &content, tag.byte_offset);
                let info = &ctx.lines[index];
                let end_col = crate::utils::range_utils::byte_to_char_count(
                    info.content(&content),
                    tag.byte_end - info.byte_offset,
                ) - 1;
                assert_eq!((tag.line, tag.start_col, tag.end_col), (line, col, end_col));
            }
        }
    }

    #[test]
    fn native_parser_matches_upstream_with_javascript_callbacks() {
        use ::markdown as upstream;

        fn signal(value: markdown::MdxSignal) -> upstream::MdxSignal {
            match value {
                markdown::MdxSignal::Ok => upstream::MdxSignal::Ok,
                markdown::MdxSignal::Eof(message, source, rule) => upstream::MdxSignal::Eof(message, source, rule),
                markdown::MdxSignal::Error(message, offset, source, rule) => {
                    upstream::MdxSignal::Error(message, offset, source, rule)
                }
            }
        }
        let mut options = upstream::ParseOptions::mdx();
        options.constructs.gfm_table = true;
        options.constructs.gfm_strikethrough = true;
        options.constructs.gfm_task_list_item = true;
        options.constructs.gfm_footnote_definition = true;
        options.constructs.gfm_label_start_footnote = true;
        options.mdx_esm_parse = Some(Box::new(|value| signal(parse_esm(value))));
        options.mdx_expression_parse = Some(Box::new(|value, kind| {
            let kind = match kind {
                upstream::MdxExpressionKind::Expression => markdown::MdxExpressionKind::Expression,
                upstream::MdxExpressionKind::AttributeExpression => markdown::MdxExpressionKind::AttributeExpression,
                upstream::MdxExpressionKind::AttributeValueExpression => {
                    markdown::MdxExpressionKind::AttributeValueExpression
                }
            };
            signal(parse_expression(value, &kind))
        }));
        for source in [
            "import Card from './card';\n\nexport const value = {nested: '日本'};\n\n<Card>{value.nested}</Card>\n",
            "<Card {...props} title={foo({a: 1})}>[![图](img)](url) {`hello ${name}`}</Card>\n",
            "<日本 属性={value}>[é *label*](target)</日本> {/* [hidden](url) */}\n",
            "[a <span>{value}</span> b](target) ![<Icon /> image](image.png)\n",
            "<Card>{(() => { return /[}]/.test('}'); })()}</Card>\n",
            "Heading <b>text</b>\n===\n",
            "# Heading <b>text</b> #\n",
            "<Card title={value +} />\n",
            "export const = broken;\n",
            "<Card>{unterminated",
        ] {
            for newline in ["\n", "\r\n"] {
                let content = source.replace('\n', newline);
                let ctx = LintContext::new(&content, MarkdownFlavor::MDX, None);
                let actual = MdxContext::parse(&content, &ctx.lines);
                let expected = parse_without_crashing(|| upstream::to_mdast(&content, &options)).ok();
                assert_eq!(
                    actual.as_ref().map(|mdx| format!("{:?}", mdx.root)),
                    expected.as_ref().map(|root| format!("{root:?}")),
                    "source: {content:?}",
                );
            }
        }
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
    fn unclosed_jsx_in_link_label_is_linted_without_crashing() {
        let rules = crate::rules::all_rules(&crate::config::Config::default());
        for content in [
            "[<span>text](/x)\n",
            "[<>]()\n",
            "[<>](x)\n",
            "[<Foo>]()\n",
            "[a<>b](/x)\n",
            "![<>]()\n",
            "[<div>](/x)\r\n",
            "<b>[x</b>](/y)",
            "<b>![x</b>](/y)",
            "> <b>[x</b>](/y)",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
            assert!(
                MdxContext::parse(content, &ctx.lines).is_none(),
                "expected recovery context for {content:?}"
            );
            assert_eq!(ctx.content, content);
            let linted = crate::lint(content, &rules, false, MarkdownFlavor::MDX, None, None);
            assert!(linted.is_ok(), "{content:?}: {linted:?}");
        }
    }

    #[test]
    fn a_parser_panic_reads_as_a_parse_error() {
        let panicked = parse_without_crashing(|| -> Result<(), String> { panic!("node stack not drained") });
        assert_eq!(panicked, Err("MDX parser panicked on malformed syntax".to_string()));

        let rejected = parse_without_crashing(|| Err::<(), _>("unexpected end of file"));
        assert_eq!(rejected, Err("unexpected end of file".to_string()));
    }

    #[test]
    fn a_recovered_parse_leaves_panic_reporting_on() {
        // The report is silenced only while the parser runs. If the flag
        // survived a panic, every later panic in this thread would vanish.
        let panicked = parse_without_crashing(|| -> Result<(), String> { panic!("node stack not drained") });
        assert!(panicked.is_err());
        assert!(!RECOVERING_FROM_MDX_PARSE.get());
    }

    #[test]
    fn well_formed_jsx_in_link_label_still_parses() {
        // The recovery path must not swallow labels that markdown-rs handles.
        for content in [
            "[<code>npm</code>](/install)\n",
            "[<Icon />](/docs)\n",
            "[<b>bold</b> link](/x)\n",
        ] {
            let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
            assert!(
                MdxContext::parse(content, &ctx.lines).is_some(),
                "expected an MDX parse for {content:?}"
            );
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
    fn comment_followed_by_template_code_does_not_create_literal_headings() {
        let source = "{/* c */ `literal\n# hidden heading\n`}\n\n# actual heading\n";
        let ctx = context(source);
        let headings: Vec<_> = ctx.headings().map(|h| h.heading.raw_text.as_str()).collect();
        assert_eq!(headings, vec!["actual heading"]);
    }

    #[test]
    fn comment_only_expressions_with_whitespace_have_comment_ranges() {
        for expression in [
            "{ /* hidden */ }",
            "{\n/*\n# hidden heading\n*/\n}",
            "{ // hidden\n}",
            "{/* first */ /* second */}",
        ] {
            let prefix = "export const unused = 1\n\n";
            let source = format!("{prefix}{expression}\n\n# visible title\n");
            let ctx = context(&source);
            assert_eq!(
                ctx.mdx_comment_ranges(),
                &[(prefix.len(), prefix.len() + expression.len())],
                "invisible comment: {source}"
            );
            assert!(ctx.is_in_mdx_comment(prefix.len()));
            let headings: Vec<_> = ctx.headings().map(|h| h.heading.raw_text.as_str()).collect();
            assert_eq!(headings, vec!["visible title"]);
            let rule = crate::MD041FirstLineHeading::default();
            assert!(rule.check(&ctx).unwrap().is_empty());
        }
    }

    #[test]
    fn a_comment_followed_by_expression_code_is_not_an_invisible_comment() {
        let source = "export const name = \"Visible prose\"\n\n{/* c */ name}\n\n# actual heading\n";
        let ctx = context(source);
        let start = source.find("{/*").unwrap();
        assert!(!ctx.is_in_mdx_comment(start));
        assert!(ctx.is_in_jsx_expression(start));
        let rule = crate::MD041FirstLineHeading::default();
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
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
