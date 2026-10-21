//! Turn events into a syntax tree.

use crate::mdx_parser::event::{Event, Kind, Name};
use crate::mdx_parser::mdast::{
    AttributeContent, AttributeValue, AttributeValueExpression, Blockquote, Break, Code, Definition, Delete, Emphasis,
    FootnoteDefinition, FootnoteReference, Heading, Html, Image, ImageReference, InlineCode, InlineMath, Link,
    LinkReference, List, ListItem, Math, MdxFlowExpression, MdxJsxAttribute, MdxJsxExpressionAttribute,
    MdxJsxFlowElement, MdxJsxTextElement, MdxTextExpression, MdxjsEsm, Node, Paragraph, ReferenceKind, Root, Strong,
    Table, TableCell, TableRow, Text, ThematicBreak, Toml, Yaml,
};
use crate::mdx_parser::message;
use crate::mdx_parser::unist::{Point, Position};
use crate::mdx_parser::util::{
    character_reference::{decode as decode_character_reference, parse as parse_character_reference},
    infer::{ListSpread, gfm_table_align},
    mdx_collect::{Result as CollectResult, collect},
    normalize_identifier::normalize_identifier,
    slice::{Position as SlicePosition, Slice},
};
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::str;

/// A reference to something.
#[derive(Debug)]
struct Reference {
    #[allow(clippy::struct_field_names)]
    reference_kind: Option<ReferenceKind>,
    identifier: String,
    label: String,
    /// Exit of the label text, if any; its identifier is made only if needed.
    label_text_exit: Option<usize>,
}

/// Info on a tag.
///
/// JSX tags are parsed on their own.
/// They’re matched together here.
#[derive(Debug, Clone)]
struct JsxTag {
    /// Optional tag name.
    ///
    /// `None` means that it’s a fragment.
    name: Option<String>,
    /// List of attributes.
    attributes: Vec<AttributeContent>,
    /// Whether this is a closing tag.
    ///
    /// ```markdown
    /// > | </a>
    ///      ^
    /// ```
    close: bool,
    /// Whether this is a self-closing tag.
    ///
    /// ```markdown
    /// > | <a/>
    ///       ^
    /// ```
    self_closing: bool,
    /// Starting point.
    start: Point,
    /// Ending point.
    end: Point,
}

impl Reference {
    fn new() -> Reference {
        Reference {
            // Assume shortcut: removed on a resource, changed on a reference.
            reference_kind: Some(ReferenceKind::Shortcut),
            identifier: String::new(),
            label: String::new(),
            label_text_exit: None,
        }
    }
}

/// Context used to compile markdown.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
struct CompileContext<'a> {
    // Static info.
    /// List of events.
    events: &'a [Event],
    /// List of bytes.
    bytes: &'a [u8],
    // Fields used by handlers to track the things they need to track to
    // compile markdown.
    character_reference_marker: u8,
    gfm_table_inside: bool,
    hard_break_after: bool,
    heading_setext_text_after: bool,
    jsx_tag_stack: Vec<JsxTag>,
    jsx_tag: Option<JsxTag>,
    media_reference_stack: Vec<Reference>,
    raw_flow_fence_seen: bool,
    /// Spread of lists and items, found when the first list starts.
    list_spread: Option<ListSpread>,
    // Intermediate results.
    /// Primary tree and buffers: each a root, and its open nodes with their
    /// enter index, deepest last.
    trees: Vec<(Node, Vec<(Node, usize)>)>,
    /// Current event index.
    index: usize,
}

impl<'a> CompileContext<'a> {
    /// Create a new compile context.
    fn new(events: &'a [Event], bytes: &'a [u8]) -> CompileContext<'a> {
        let tree = Node::Root(Root {
            children: vec![],
            position: Some(Position {
                start: if events.is_empty() {
                    Point::new(1, 1, 0)
                } else {
                    events[0].point.to_unist()
                },
                end: if events.is_empty() {
                    Point::new(1, 1, 0)
                } else {
                    events[events.len() - 1].point.to_unist()
                },
            }),
        });

        CompileContext {
            events,
            bytes,
            character_reference_marker: 0,
            gfm_table_inside: false,
            hard_break_after: false,
            heading_setext_text_after: false,
            jsx_tag_stack: vec![],
            jsx_tag: None,
            media_reference_stack: vec![],
            raw_flow_fence_seen: false,
            list_spread: None,
            trees: vec![(tree, vec![])],
            index: 0,
        }
    }

    /// Push a buffer.
    fn buffer(&mut self) {
        self.trees.push((
            Node::Paragraph(Paragraph {
                children: vec![],
                position: None,
            }),
            vec![],
        ));
    }

    /// Pop a buffer, returning its value.
    fn resume(&mut self) -> Node {
        if let Some((mut root, mut open)) = self.trees.pop() {
            debug_assert_eq!(open.len(), 0, "expected open nodes to be closed");
            // Without debug assertions, keep unclosed nodes, such as an
            // unclosed JSX element in a label, where they are in the tree.
            while let Some((node, _)) = open.pop() {
                let parent = match open.last_mut() {
                    Some((parent, _)) => parent,
                    None => &mut root,
                };
                parent.children_mut().expect("Cannot push to non-parent").push(node);
            }
            root
        } else {
            unreachable!("Cannot resume w/o buffer")
        }
    }

    fn tail_mut(&mut self) -> &mut Node {
        let (root, open) = self.trees.last_mut().expect("Cannot get tail w/o tree");
        match open.last_mut() {
            Some((node, _)) => node,
            None => root,
        }
    }

    /// The parent of the tail, whose children do not include the tail yet.
    fn tail_penultimate_mut(&mut self) -> &mut Node {
        let (root, open) = self.trees.last_mut().expect("Cannot get tail w/o tree");
        let len = open.len();
        debug_assert!(len > 0, "expected an open node");
        if len > 1 { &mut open[len - 2].0 } else { root }
    }

    /// Open `child` as the new tail; it is added to its parent when it closes.
    fn tail_push(&mut self, mut child: Node) {
        if child.position().is_none() {
            child.position_set(Some(position_from_event(&self.events[self.index])));
        }

        let (_, open) = self.trees.last_mut().expect("Cannot get tail w/o tree");
        open.push((child, self.index));
    }

    /// Reopen the last child of the tail.
    fn tail_push_again(&mut self) {
        let child = self
            .tail_mut()
            .children_mut()
            .expect("Cannot push to non-parent")
            .pop()
            .expect("Cannot reopen w/o child");
        let (_, open) = self.trees.last_mut().expect("Cannot get tail w/o tree");
        open.push((child, self.index));
    }

    fn tail_pop(&mut self) -> Result<(), message::Message> {
        let ev = &self.events[self.index];
        let mut end = ev.point.to_unist();
        let (root, open) = self.trees.last_mut().expect("Cannot get tail w/o tree");
        let (mut node, left_index) = open.pop().expect("Cannot pop w/o open node");

        // Containers end before trailing line endings, as in `mdast-util-from-markdown`.
        if matches!(
            ev.name,
            Name::GfmFootnoteDefinition | Name::ListItem | Name::ListOrdered | Name::ListUnordered
        ) {
            let mut index = self.index;
            while index > 0 {
                let event = &self.events[index - 1];
                match event.name {
                    Name::LineEnding | Name::BlankLineEnding => {
                        if event.kind == Kind::Enter {
                            end = event.point.to_unist();
                        }
                    }
                    Name::SpaceOrTab => {}
                    Name::GfmFootnoteDefinition | Name::ListItem | Name::ListOrdered | Name::ListUnordered
                        if event.kind == Kind::Exit =>
                    {
                        // That container is the last child, its end already moved.
                        if let Some(child) = node.children().and_then(|children| children.last()) {
                            let child_end = &child.position().unwrap().end;
                            if child_end.offset < event.point.index {
                                end = child_end.clone();
                            }
                        }
                        break;
                    }
                    _ => break,
                }
                index -= 1;
            }
        }

        // An unclosed fence can include the line ending where its container
        // already closed. That ending belongs outside the code range (GH-205).
        if matches!(ev.name, Name::ListItem | Name::BlockQuote | Name::GfmFootnoteDefinition) {
            if let Some(child @ (Node::Code(_) | Node::Math(_))) =
                node.children_mut().and_then(|children| children.last_mut())
            {
                let position = child.position_mut().unwrap();
                if position.end.offset > end.offset
                    && self.bytes[end.offset..position.end.offset]
                        .iter()
                        .all(|byte| matches!(byte, b'\r' | b'\n'))
                {
                    position.end = end.clone();
                }
            }
        }
        let pos = node.position_mut().expect("Cannot pop manually added node");
        pos.end = end;

        let parent = match open.last_mut() {
            Some((parent, _)) => parent,
            None => root,
        };
        parent.children_mut().expect("Cannot push to non-parent").push(node);

        let left = &self.events[left_index];
        if left.name != ev.name {
            on_mismatch_error(self, Some(ev), left)?;
        }

        Ok(())
    }
}

/// Turn events and bytes into a syntax tree.
pub fn compile(events: &[Event], bytes: &[u8]) -> Result<Node, message::Message> {
    let mut context = CompileContext::new(events, bytes);

    let mut index = 0;
    while index < events.len() {
        handle(&mut context, index)?;
        index += 1;
    }

    debug_assert_eq!(context.trees.len(), 1, "expected 1 final tree");
    let (tree, open) = context.trees.pop().unwrap();

    if let Some((_, index)) = open.last() {
        let event = &events[*index];
        on_mismatch_error(&mut context, None, event)?;
    }

    Ok(tree)
}

/// Handle the event at `index`.
fn handle(context: &mut CompileContext, index: usize) -> Result<(), message::Message> {
    context.index = index;

    if context.events[index].kind == Kind::Enter {
        enter(context)?;
    } else {
        exit(context)?;
    }

    Ok(())
}

/// Handle [`Enter`][Kind::Enter].
fn enter(context: &mut CompileContext) -> Result<(), message::Message> {
    match context.events[context.index].name {
        Name::AutolinkEmail
        | Name::AutolinkProtocol
        | Name::CharacterEscapeValue
        | Name::CharacterReference
        | Name::CodeFlowChunk
        | Name::CodeTextData
        | Name::Data
        | Name::FrontmatterChunk
        | Name::HtmlFlowData
        | Name::HtmlTextData
        | Name::MathFlowChunk
        | Name::MathTextData
        | Name::MdxJsxTagAttributeValueLiteralValue => on_enter_data(context),
        Name::CodeFencedFenceInfo
        | Name::CodeFencedFenceMeta
        | Name::DefinitionDestinationString
        | Name::DefinitionLabelString
        | Name::DefinitionTitleString
        | Name::GfmFootnoteDefinitionLabelString
        | Name::LabelText
        | Name::MathFlowFenceMeta
        | Name::MdxJsxTagAttributeValueLiteral
        | Name::ReferenceString
        | Name::ResourceDestinationString
        | Name::ResourceTitleString => on_enter_buffer(context),
        Name::Autolink => on_enter_autolink(context),
        Name::BlockQuote => on_enter_block_quote(context),
        Name::CodeFenced => on_enter_code_fenced(context),
        Name::CodeIndented => on_enter_code_indented(context),
        Name::CodeText => on_enter_code_text(context),
        Name::Definition => on_enter_definition(context),
        Name::Emphasis => on_enter_emphasis(context),
        Name::Frontmatter => on_enter_frontmatter(context),
        Name::GfmAutolinkLiteralEmail
        | Name::GfmAutolinkLiteralMailto
        | Name::GfmAutolinkLiteralProtocol
        | Name::GfmAutolinkLiteralWww
        | Name::GfmAutolinkLiteralXmpp => on_enter_gfm_autolink_literal(context),
        Name::GfmFootnoteCall => on_enter_gfm_footnote_call(context),
        Name::GfmFootnoteDefinition => on_enter_gfm_footnote_definition(context),
        Name::GfmStrikethrough => on_enter_gfm_strikethrough(context),
        Name::GfmTable => on_enter_gfm_table(context),
        Name::GfmTableRow => on_enter_gfm_table_row(context),
        Name::GfmTableCell => on_enter_gfm_table_cell(context),
        Name::HardBreakEscape | Name::HardBreakTrailing => on_enter_hard_break(context),
        Name::HeadingAtx | Name::HeadingSetext => on_enter_heading(context),
        Name::HtmlFlow | Name::HtmlText => on_enter_html(context),
        Name::Image => on_enter_image(context),
        Name::Link => on_enter_link(context),
        Name::ListItem => on_enter_list_item(context),
        Name::ListOrdered | Name::ListUnordered => on_enter_list(context),
        Name::MathFlow => on_enter_math_flow(context),
        Name::MathText => on_enter_math_text(context),
        Name::MdxEsm => on_enter_mdx_esm(context),
        Name::MdxFlowExpression => on_enter_mdx_flow_expression(context),
        Name::MdxTextExpression => on_enter_mdx_text_expression(context),
        Name::MdxJsxFlowTag | Name::MdxJsxTextTag => on_enter_mdx_jsx_tag(context),
        Name::MdxJsxTagClosingMarker => on_enter_mdx_jsx_tag_closing_marker(context)?,
        Name::MdxJsxTagAttribute => on_enter_mdx_jsx_tag_attribute(context)?,
        Name::MdxJsxTagAttributeExpression => on_enter_mdx_jsx_tag_attribute_expression(context)?,
        Name::MdxJsxTagAttributeValueExpression => {
            on_enter_mdx_jsx_tag_attribute_value_expression(context);
        }
        Name::MdxJsxTagSelfClosingMarker => on_enter_mdx_jsx_tag_self_closing_marker(context)?,
        Name::Paragraph => on_enter_paragraph(context),
        Name::Reference => on_enter_reference(context),
        Name::Resource => on_enter_resource(context),
        Name::Strong => on_enter_strong(context),
        Name::ThematicBreak => on_enter_thematic_break(context),
        _ => {}
    }

    Ok(())
}

/// Handle [`Exit`][Kind::Exit].
fn exit(context: &mut CompileContext) -> Result<(), message::Message> {
    match context.events[context.index].name {
        Name::Autolink
        | Name::BlockQuote
        | Name::CharacterReference
        | Name::Definition
        | Name::Emphasis
        | Name::GfmFootnoteDefinition
        | Name::GfmStrikethrough
        | Name::GfmTableRow
        | Name::GfmTableCell
        | Name::HeadingAtx
        | Name::ListOrdered
        | Name::ListUnordered
        | Name::Paragraph
        | Name::Strong
        | Name::ThematicBreak => {
            on_exit(context)?;
        }
        Name::CharacterEscapeValue
        | Name::CodeFlowChunk
        | Name::CodeTextData
        | Name::Data
        | Name::FrontmatterChunk
        | Name::HtmlFlowData
        | Name::HtmlTextData
        | Name::MathFlowChunk
        | Name::MathTextData
        | Name::MdxJsxTagAttributeValueLiteralValue => {
            on_exit_data(context)?;
        }
        Name::MdxJsxTagAttributeExpression | Name::MdxJsxTagAttributeValueExpression => {
            on_exit_drop(context);
        }
        Name::AutolinkProtocol => on_exit_autolink_protocol(context)?,
        Name::AutolinkEmail => on_exit_autolink_email(context)?,
        Name::CharacterReferenceMarker => on_exit_character_reference_marker(context),
        Name::CharacterReferenceMarkerNumeric => {
            on_exit_character_reference_marker_numeric(context);
        }
        Name::CharacterReferenceMarkerHexadecimal => {
            on_exit_character_reference_marker_hexadecimal(context);
        }
        Name::CharacterReferenceValue => on_exit_character_reference_value(context),
        Name::CodeFencedFenceInfo => on_exit_code_fenced_fence_info(context),
        Name::CodeFencedFenceMeta | Name::MathFlowFenceMeta => on_exit_raw_flow_fence_meta(context),
        Name::CodeFencedFence | Name::MathFlowFence => on_exit_raw_flow_fence(context),
        Name::CodeFenced | Name::MathFlow => on_exit_raw_flow(context)?,
        Name::CodeIndented => on_exit_code_indented(context)?,
        Name::CodeText | Name::MathText => on_exit_raw_text(context)?,
        Name::DefinitionDestinationString => on_exit_definition_destination_string(context),
        Name::DefinitionLabelString | Name::GfmFootnoteDefinitionLabelString => {
            on_exit_definition_id(context);
        }
        Name::DefinitionTitleString => on_exit_definition_title_string(context),
        Name::Frontmatter => on_exit_frontmatter(context)?,
        Name::GfmAutolinkLiteralEmail
        | Name::GfmAutolinkLiteralMailto
        | Name::GfmAutolinkLiteralProtocol
        | Name::GfmAutolinkLiteralWww
        | Name::GfmAutolinkLiteralXmpp => on_exit_gfm_autolink_literal(context)?,
        Name::GfmFootnoteCall | Name::Image | Name::Link => on_exit_media(context)?,
        Name::GfmTable => on_exit_gfm_table(context)?,
        Name::GfmTaskListItemValueUnchecked | Name::GfmTaskListItemValueChecked => {
            on_exit_gfm_task_list_item_value(context);
        }
        Name::HardBreakEscape | Name::HardBreakTrailing => on_exit_hard_break(context)?,
        Name::HeadingAtxSequence => on_exit_heading_atx_sequence(context),
        Name::HeadingSetext => on_exit_heading_setext(context)?,
        Name::HeadingSetextUnderlineSequence => on_exit_heading_setext_underline_sequence(context),
        Name::HeadingSetextText => on_exit_heading_setext_text(context),
        Name::HtmlFlow | Name::HtmlText => on_exit_html(context)?,
        Name::LabelText => on_exit_label_text(context)?,
        Name::LineEnding => on_exit_line_ending(context)?,
        Name::ListItem => on_exit_list_item(context)?,
        Name::ListItemValue => on_exit_list_item_value(context),
        Name::MdxEsm | Name::MdxFlowExpression | Name::MdxTextExpression => {
            on_exit_mdx_esm_or_expression(context)?;
        }
        Name::MdxJsxFlowTag | Name::MdxJsxTextTag => on_exit_mdx_jsx_tag(context)?,
        Name::MdxJsxTagClosingMarker => on_exit_mdx_jsx_tag_closing_marker(context),
        Name::MdxJsxTagNamePrimary => on_exit_mdx_jsx_tag_name_primary(context),
        Name::MdxJsxTagNameMember => on_exit_mdx_jsx_tag_name_member(context),
        Name::MdxJsxTagNameLocal => on_exit_mdx_jsx_tag_name_local(context),
        Name::MdxJsxTagAttributePrimaryName => on_exit_mdx_jsx_tag_attribute_primary_name(context),
        Name::MdxJsxTagAttributeNameLocal => on_exit_mdx_jsx_tag_attribute_name_local(context),
        Name::MdxJsxTagAttributeValueLiteral => {
            on_exit_mdx_jsx_tag_attribute_value_literal(context);
        }
        Name::MdxJsxTagSelfClosingMarker => on_exit_mdx_jsx_tag_self_closing_marker(context),

        Name::ReferenceString => on_exit_reference_string(context),
        Name::ResourceDestinationString => on_exit_resource_destination_string(context),
        Name::ResourceTitleString => on_exit_resource_title_string(context),
        _ => {}
    }

    Ok(())
}

/// Handle [`Enter`][Kind::Enter]:`*`.
fn on_enter_buffer(context: &mut CompileContext) {
    context.buffer();
}

/// Handle [`Enter`][Kind::Enter]:[`Data`][Name::Data] (and many text things).
fn on_enter_data(context: &mut CompileContext) {
    let parent = context.tail_mut();
    let children = parent.children_mut().expect("expected parent");

    // Add to stack again.
    if let Some(Node::Text(_)) = children.last_mut() {
        context.tail_push_again();
    } else {
        context.tail_push(Node::Text(Text {
            value: String::new(),
            position: None,
        }));
    }
}

/// Handle [`Enter`][Kind::Enter]:[`Autolink`][Name::Autolink].
fn on_enter_autolink(context: &mut CompileContext) {
    context.tail_push(Node::Link(Link {
        url: String::new(),
        title: None,
        children: vec![],
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:[`BlockQuote`][Name::BlockQuote].
fn on_enter_block_quote(context: &mut CompileContext) {
    context.tail_push(Node::Blockquote(Blockquote {
        children: vec![],
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:[`CodeFenced`][Name::CodeFenced].
fn on_enter_code_fenced(context: &mut CompileContext) {
    context.tail_push(Node::Code(Code {
        lang: None,
        meta: None,
        value: String::new(),
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:[`CodeIndented`][Name::CodeIndented].
fn on_enter_code_indented(context: &mut CompileContext) {
    on_enter_code_fenced(context);
    on_enter_buffer(context);
}

/// Handle [`Enter`][Kind::Enter]:[`CodeText`][Name::CodeText].
fn on_enter_code_text(context: &mut CompileContext) {
    context.tail_push(Node::InlineCode(InlineCode {
        value: String::new(),
        position: None,
    }));
    context.buffer();
}

/// Handle [`Enter`][Kind::Enter]:[`MathText`][Name::MathText].
fn on_enter_math_text(context: &mut CompileContext) {
    context.tail_push(Node::InlineMath(InlineMath {
        value: String::new(),
        position: None,
    }));
    context.buffer();
}

/// Handle [`Enter`][Kind::Enter]:[`MdxEsm`][Name::MdxEsm].
fn on_enter_mdx_esm(context: &mut CompileContext) {
    let result = collect(
        context.events,
        context.bytes,
        context.index,
        &[Name::MdxEsmData, Name::LineEnding],
        &[Name::MdxEsm],
    );
    context.tail_push(Node::MdxjsEsm(MdxjsEsm {
        value: result.value,
        position: None,
        stops: result.stops,
    }));
    context.buffer();
}

/// Handle [`Enter`][Kind::Enter]:[`MdxFlowExpression`][Name::MdxFlowExpression].
fn on_enter_mdx_flow_expression(context: &mut CompileContext) {
    let result = collect(
        context.events,
        context.bytes,
        context.index,
        &[Name::MdxExpressionData, Name::LineEnding],
        &[Name::MdxFlowExpression],
    );
    context.tail_push(Node::MdxFlowExpression(MdxFlowExpression {
        value: result.value,
        position: None,
        stops: result.stops,
    }));
    context.buffer();
}

/// Handle [`Enter`][Kind::Enter]:[`MdxTextExpression`][Name::MdxTextExpression].
fn on_enter_mdx_text_expression(context: &mut CompileContext) {
    let result = collect(
        context.events,
        context.bytes,
        context.index,
        &[Name::MdxExpressionData, Name::LineEnding],
        &[Name::MdxTextExpression],
    );
    context.tail_push(Node::MdxTextExpression(MdxTextExpression {
        value: result.value,
        position: None,
        stops: result.stops,
    }));
    context.buffer();
}

/// Handle [`Enter`][Kind::Enter]:[`Definition`][Name::Definition].
fn on_enter_definition(context: &mut CompileContext) {
    context.tail_push(Node::Definition(Definition {
        url: String::new(),
        identifier: String::new(),
        label: None,
        title: None,
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:[`Emphasis`][Name::Emphasis].
fn on_enter_emphasis(context: &mut CompileContext) {
    context.tail_push(Node::Emphasis(Emphasis {
        children: vec![],
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:{[`GfmAutolinkLiteralEmail`][Name::GfmAutolinkLiteralEmail],[`GfmAutolinkLiteralMailto`][Name::GfmAutolinkLiteralMailto],[`GfmAutolinkLiteralProtocol`][Name::GfmAutolinkLiteralProtocol],[`GfmAutolinkLiteralWww`][Name::GfmAutolinkLiteralWww],[`GfmAutolinkLiteralXmpp`][Name::GfmAutolinkLiteralXmpp]}.
fn on_enter_gfm_autolink_literal(context: &mut CompileContext) {
    on_enter_autolink(context);
    on_enter_data(context);
}

/// Handle [`Enter`][Kind::Enter]:[`GfmFootnoteCall`][Name::GfmFootnoteCall].
fn on_enter_gfm_footnote_call(context: &mut CompileContext) {
    context.tail_push(Node::FootnoteReference(FootnoteReference {
        identifier: String::new(),
        label: None,
        position: None,
    }));
    context.media_reference_stack.push(Reference::new());
}

/// Handle [`Enter`][Kind::Enter]:[`GfmFootnoteDefinition`][Name::GfmFootnoteDefinition].
fn on_enter_gfm_footnote_definition(context: &mut CompileContext) {
    context.tail_push(Node::FootnoteDefinition(FootnoteDefinition {
        identifier: String::new(),
        label: None,
        children: vec![],
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:[`GfmStrikethrough`][Name::GfmStrikethrough].
fn on_enter_gfm_strikethrough(context: &mut CompileContext) {
    context.tail_push(Node::Delete(Delete {
        children: vec![],
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:[`GfmTable`][Name::GfmTable].
fn on_enter_gfm_table(context: &mut CompileContext) {
    let align = gfm_table_align(context.events, context.index);
    context.tail_push(Node::Table(Table {
        align,
        children: vec![],
        position: None,
    }));
    context.gfm_table_inside = true;
}

/// Handle [`Enter`][Kind::Enter]:[`GfmTableRow`][Name::GfmTableRow].
fn on_enter_gfm_table_row(context: &mut CompileContext) {
    context.tail_push(Node::TableRow(TableRow {
        children: vec![],
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:[`GfmTableCell`][Name::GfmTableCell].
fn on_enter_gfm_table_cell(context: &mut CompileContext) {
    context.tail_push(Node::TableCell(TableCell {
        children: vec![],
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:[`HardBreakEscape`][Name::HardBreakEscape].
fn on_enter_hard_break(context: &mut CompileContext) {
    context.tail_push(Node::Break(Break { position: None }));
}

/// Handle [`Enter`][Kind::Enter]:[`Frontmatter`][Name::Frontmatter].
fn on_enter_frontmatter(context: &mut CompileContext) {
    let index = context.events[context.index].point.index;
    let byte = context.bytes[index];
    let node = if byte == b'+' {
        Node::Toml(Toml {
            value: String::new(),
            position: None,
        })
    } else {
        Node::Yaml(Yaml {
            value: String::new(),
            position: None,
        })
    };

    context.tail_push(node);
    context.buffer();
}

/// Handle [`Enter`][Kind::Enter]:[`Reference`][Name::Reference].
fn on_enter_reference(context: &mut CompileContext) {
    let reference = context
        .media_reference_stack
        .last_mut()
        .expect("expected reference on media stack");
    // Assume collapsed.
    // If there’s a string after it, we set `Full`.
    reference.reference_kind = Some(ReferenceKind::Collapsed);
}

/// Handle [`Enter`][Kind::Enter]:[`Resource`][Name::Resource].
fn on_enter_resource(context: &mut CompileContext) {
    let reference = context
        .media_reference_stack
        .last_mut()
        .expect("expected reference on media stack");
    // It’s not a reference.
    reference.reference_kind = None;
}

/// Handle [`Enter`][Kind::Enter]:[`Strong`][Name::Strong].
fn on_enter_strong(context: &mut CompileContext) {
    context.tail_push(Node::Strong(Strong {
        children: vec![],
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:[`ThematicBreak`][Name::ThematicBreak].
fn on_enter_thematic_break(context: &mut CompileContext) {
    context.tail_push(Node::ThematicBreak(ThematicBreak { position: None }));
}

/// Handle [`Enter`][Kind::Enter]:[`HeadingAtx`][Name::HeadingAtx].
fn on_enter_heading(context: &mut CompileContext) {
    context.tail_push(Node::Heading(Heading {
        depth: 0, // Will be set later.
        children: vec![],
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:{[`HtmlFlow`][Name::HtmlFlow],[`HtmlText`][Name::HtmlText]}.
fn on_enter_html(context: &mut CompileContext) {
    context.tail_push(Node::Html(Html {
        value: String::new(),
        position: None,
    }));
    context.buffer();
}

/// Handle [`Enter`][Kind::Enter]:[`Image`][Name::Image].
fn on_enter_image(context: &mut CompileContext) {
    context.tail_push(Node::Image(Image {
        url: String::new(),
        title: None,
        alt: String::new(),
        position: None,
    }));
    context.media_reference_stack.push(Reference::new());
}

/// Handle [`Enter`][Kind::Enter]:[`Link`][Name::Link].
fn on_enter_link(context: &mut CompileContext) {
    context.tail_push(Node::Link(Link {
        url: String::new(),
        title: None,
        children: vec![],
        position: None,
    }));
    context.media_reference_stack.push(Reference::new());
}

/// Handle [`Enter`][Kind::Enter]:{[`ListOrdered`][Name::ListOrdered],[`ListUnordered`][Name::ListUnordered]}.
fn on_enter_list(context: &mut CompileContext) {
    let ordered = context.events[context.index].name == Name::ListOrdered;
    let events = context.events;
    let spread = context
        .list_spread
        .get_or_insert_with(|| ListSpread::new(events))
        .spread(context.index);

    context.tail_push(Node::List(List {
        ordered,
        spread,
        start: None,
        children: vec![],
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:[`ListItem`][Name::ListItem].
fn on_enter_list_item(context: &mut CompileContext) {
    let events = context.events;
    let spread = context
        .list_spread
        .get_or_insert_with(|| ListSpread::new(events))
        .spread(context.index);

    context.tail_push(Node::ListItem(ListItem {
        spread,
        checked: None,
        children: vec![],
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:[`MathFlow`][Name::MathFlow].
fn on_enter_math_flow(context: &mut CompileContext) {
    context.tail_push(Node::Math(Math {
        meta: None,
        value: String::new(),
        position: None,
    }));
}

/// Handle [`Enter`][Kind::Enter]:{[`MdxJsxFlowTag`][Name::MdxJsxFlowTag],[`MdxJsxTextTag`][Name::MdxJsxTextTag]}.
fn on_enter_mdx_jsx_tag(context: &mut CompileContext) {
    let point = context.events[context.index].point.to_unist();
    context.jsx_tag = Some(JsxTag {
        name: None,
        attributes: vec![],
        start: point.clone(),
        end: point,
        close: false,
        self_closing: false,
    });
    context.buffer();
}

/// Handle [`Enter`][Kind::Enter]:[`MdxJsxTagClosingMarker`][Name::MdxJsxTagClosingMarker].
fn on_enter_mdx_jsx_tag_closing_marker(context: &mut CompileContext) -> Result<(), message::Message> {
    if context.jsx_tag_stack.is_empty() {
        let event = &context.events[context.index];
        Err(message::Message {
            place: Some(Box::new(message::Place::Point(event.point.to_unist()))),
            reason: "Unexpected closing slash `/` in tag, expected an open tag first".into(),
            rule_id: Box::new("unexpected-closing-slash".into()),
            source: Box::new("markdown-rs".into()),
        })
    } else {
        Ok(())
    }
}

/// Handle [`Enter`][Kind::Enter]:{[`MdxJsxTagAttribute`][Name::MdxJsxTagAttribute],[`MdxJsxTagAttributeExpression`][Name::MdxJsxTagAttributeExpression]}.
fn on_enter_mdx_jsx_tag_any_attribute(context: &mut CompileContext) -> Result<(), message::Message> {
    if context.jsx_tag.as_ref().expect("expected tag").close {
        let event = &context.events[context.index];
        Err(message::Message {
            place: Some(Box::new(message::Place::Point(event.point.to_unist()))),
            reason: "Unexpected attribute in closing tag, expected the end of the tag".into(),
            rule_id: Box::new("unexpected-attribute".into()),
            source: Box::new("markdown-rs".into()),
        })
    } else {
        Ok(())
    }
}

/// Handle [`Enter`][Kind::Enter]:[`MdxJsxTagAttribute`][Name::MdxJsxTagAttribute].
fn on_enter_mdx_jsx_tag_attribute(context: &mut CompileContext) -> Result<(), message::Message> {
    on_enter_mdx_jsx_tag_any_attribute(context)?;

    context
        .jsx_tag
        .as_mut()
        .expect("expected tag")
        .attributes
        .push(AttributeContent::Property(MdxJsxAttribute {
            name: String::new(),
            value: None,
        }));

    Ok(())
}

/// Handle [`Enter`][Kind::Enter]:[`MdxJsxTagAttributeExpression`][Name::MdxJsxTagAttributeExpression].
fn on_enter_mdx_jsx_tag_attribute_expression(context: &mut CompileContext) -> Result<(), message::Message> {
    on_enter_mdx_jsx_tag_any_attribute(context)?;

    let CollectResult { value, stops } = collect(
        context.events,
        context.bytes,
        context.index,
        &[Name::MdxExpressionData, Name::LineEnding],
        &[Name::MdxJsxTagAttributeExpression],
    );
    context
        .jsx_tag
        .as_mut()
        .expect("expected tag")
        .attributes
        .push(AttributeContent::Expression(MdxJsxExpressionAttribute { value, stops }));

    context.buffer();

    Ok(())
}

/// Handle [`Enter`][Kind::Enter]:[`MdxJsxTagAttributeValueExpression`][Name::MdxJsxTagAttributeValueExpression].
fn on_enter_mdx_jsx_tag_attribute_value_expression(context: &mut CompileContext) {
    let CollectResult { value, stops } = collect(
        context.events,
        context.bytes,
        context.index,
        &[Name::MdxExpressionData, Name::LineEnding],
        &[Name::MdxJsxTagAttributeValueExpression],
    );

    if let Some(AttributeContent::Property(node)) =
        context.jsx_tag.as_mut().expect("expected tag").attributes.last_mut()
    {
        node.value = Some(AttributeValue::Expression(AttributeValueExpression { value, stops }));
    } else {
        unreachable!("expected property")
    }

    context.buffer();
}

/// Handle [`Enter`][Kind::Enter]:[`MdxJsxTagSelfClosingMarker`][Name::MdxJsxTagSelfClosingMarker].
fn on_enter_mdx_jsx_tag_self_closing_marker(context: &mut CompileContext) -> Result<(), message::Message> {
    let tag = context.jsx_tag.as_ref().expect("expected tag");
    if tag.close {
        let event = &context.events[context.index];
        Err(message::Message {
            place: Some(Box::new(message::Place::Point(event.point.to_unist()))),
            reason: "Unexpected self-closing slash `/` in closing tag, expected the end of the tag".into(),
            rule_id: Box::new("unexpected-self-closing-slash".into()),
            source: Box::new("markdown-rs".into()),
        })
    } else {
        Ok(())
    }
}

/// Handle [`Enter`][Kind::Enter]:[`Paragraph`][Name::Paragraph].
fn on_enter_paragraph(context: &mut CompileContext) {
    context.tail_push(Node::Paragraph(Paragraph {
        children: vec![],
        position: None,
    }));
}

/// Handle [`Exit`][Kind::Exit]:`*`.
fn on_exit(context: &mut CompileContext) -> Result<(), message::Message> {
    context.tail_pop()?;
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`AutolinkProtocol`][Name::AutolinkProtocol].
fn on_exit_autolink_protocol(context: &mut CompileContext) -> Result<(), message::Message> {
    on_exit_data(context)?;
    let value = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );
    if let Node::Link(link) = context.tail_mut() {
        link.url.push_str(value.as_str());
    } else {
        unreachable!("expected link on stack");
    }
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`AutolinkEmail`][Name::AutolinkEmail].
fn on_exit_autolink_email(context: &mut CompileContext) -> Result<(), message::Message> {
    on_exit_data(context)?;
    let value = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );
    if let Node::Link(link) = context.tail_mut() {
        link.url.push_str("mailto:");
        link.url.push_str(value.as_str());
    } else {
        unreachable!("expected link on stack");
    }
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`CharacterReferenceMarker`][Name::CharacterReferenceMarker].
fn on_exit_character_reference_marker(context: &mut CompileContext) {
    context.character_reference_marker = b'&';
}

/// Handle [`Exit`][Kind::Exit]:[`CharacterReferenceMarkerHexadecimal`][Name::CharacterReferenceMarkerHexadecimal].
fn on_exit_character_reference_marker_hexadecimal(context: &mut CompileContext) {
    context.character_reference_marker = b'x';
}

/// Handle [`Exit`][Kind::Exit]:[`CharacterReferenceMarkerNumeric`][Name::CharacterReferenceMarkerNumeric].
fn on_exit_character_reference_marker_numeric(context: &mut CompileContext) {
    context.character_reference_marker = b'#';
}

/// Handle [`Exit`][Kind::Exit]:[`CharacterReferenceValue`][Name::CharacterReferenceValue].
fn on_exit_character_reference_value(context: &mut CompileContext) {
    let slice = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );
    let value = decode_character_reference(slice.as_str(), context.character_reference_marker, true)
        .expect("expected to parse only valid named references");

    if let Node::Text(node) = context.tail_mut() {
        node.value.push_str(value.as_str());
    } else {
        unreachable!("expected text on stack");
    }

    context.character_reference_marker = 0;
}

/// Handle [`Exit`][Kind::Exit]:[`CodeFencedFenceInfo`][Name::CodeFencedFenceInfo].
fn on_exit_code_fenced_fence_info(context: &mut CompileContext) {
    let value = context.resume().to_string();
    if let Node::Code(node) = context.tail_mut() {
        node.lang = Some(value);
    } else {
        unreachable!("expected code on stack");
    }
}

/// Handle [`Exit`][Kind::Exit]:{[`CodeFencedFenceMeta`][Name::CodeFencedFenceMeta],[`MathFlowFenceMeta`][Name::MathFlowFenceMeta]}.
fn on_exit_raw_flow_fence_meta(context: &mut CompileContext) {
    let value = context.resume().to_string();
    match context.tail_mut() {
        Node::Code(node) => node.meta = Some(value),
        Node::Math(node) => node.meta = Some(value),
        _ => {
            unreachable!("expected code or math on stack");
        }
    }
}

/// Handle [`Exit`][Kind::Exit]:{[`CodeFencedFence`][Name::CodeFencedFence],[`MathFlowFence`][Name::MathFlowFence]}.
fn on_exit_raw_flow_fence(context: &mut CompileContext) {
    if context.raw_flow_fence_seen {
        // Second fence, ignore.
    } else {
        context.buffer();
        context.raw_flow_fence_seen = true;
    }
}

/// Handle [`Exit`][Kind::Exit]:{[`CodeFenced`][Name::CodeFenced],[`MathFlow`][Name::MathFlow]}.
fn on_exit_raw_flow(context: &mut CompileContext) -> Result<(), message::Message> {
    let value = trim_eol(context.resume().to_string(), true, true);

    match context.tail_mut() {
        Node::Code(node) => node.value = value,
        Node::Math(node) => node.value = value,
        _ => unreachable!("expected code or math on stack for value"),
    }

    on_exit(context)?;
    context.raw_flow_fence_seen = false;
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`CodeIndented`][Name::CodeIndented].
fn on_exit_code_indented(context: &mut CompileContext) -> Result<(), message::Message> {
    let value = context.resume().to_string();

    if let Node::Code(node) = context.tail_mut() {
        node.value = trim_eol(value, false, true);
    } else {
        unreachable!("expected code on stack for value");
    }
    on_exit(context)?;
    context.raw_flow_fence_seen = false;
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:{[`CodeText`][Name::CodeText],[`MathText`][Name::MathText]}.
fn on_exit_raw_text(context: &mut CompileContext) -> Result<(), message::Message> {
    let mut value = context.resume().to_string();

    // To do: share with `to_html`.
    // If we are in a GFM table, we need to decode escaped pipes.
    // This is a rather weird GFM feature.
    if context.gfm_table_inside && value.contains("\\|") {
        value = value.replace("\\|", "|");
    }

    let value_bytes = value.as_bytes();
    if value.len() > 2
        && value_bytes[0] == b' '
        && value_bytes[value.len() - 1] == b' '
        && !value_bytes.iter().all(|b| *b == b' ')
    {
        value.remove(0);
        value.pop();
    }

    match context.tail_mut() {
        Node::InlineCode(node) => node.value = value,
        Node::InlineMath(node) => node.value = value,
        _ => unreachable!("expected inline code or math on stack for value"),
    }

    on_exit(context)?;
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`Data`][Name::Data] (and many text things).
fn on_exit_data(context: &mut CompileContext) -> Result<(), message::Message> {
    let value = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );
    if let Node::Text(text) = context.tail_mut() {
        text.value.push_str(value.as_str());
    } else {
        unreachable!("expected text on stack");
    }
    on_exit(context)?;
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`DefinitionDestinationString`][Name::DefinitionDestinationString].
fn on_exit_definition_destination_string(context: &mut CompileContext) {
    let value = context.resume().to_string();
    if let Node::Definition(node) = context.tail_mut() {
        node.url = value;
    } else {
        unreachable!("expected definition on stack");
    }
}

/// Handle [`Exit`][Kind::Exit]:{[`DefinitionLabelString`][Name::DefinitionLabelString],[`GfmFootnoteDefinitionLabelString`][Name::GfmFootnoteDefinitionLabelString]}.
fn on_exit_definition_id(context: &mut CompileContext) {
    let label = context.resume().to_string();
    let slice = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );
    let identifier = normalize_identifier(slice.as_str()).to_lowercase();

    match context.tail_mut() {
        Node::Definition(node) => {
            node.label = Some(label);
            node.identifier = identifier;
        }
        Node::FootnoteDefinition(node) => {
            node.label = Some(label);
            node.identifier = identifier;
        }
        _ => unreachable!("expected definition or footnote definition on stack"),
    }
}

/// Handle [`Exit`][Kind::Exit]:[`DefinitionTitleString`][Name::DefinitionTitleString].
fn on_exit_definition_title_string(context: &mut CompileContext) {
    let value = context.resume().to_string();
    if let Node::Definition(node) = context.tail_mut() {
        node.title = Some(value);
    } else {
        unreachable!("expected definition on stack");
    }
}

/// Handle [`Exit`][Kind::Exit]:*, by dropping the current buffer.
fn on_exit_drop(context: &mut CompileContext) {
    context.resume();
}

/// Handle [`Exit`][Kind::Exit]:[`Frontmatter`][Name::Frontmatter].
fn on_exit_frontmatter(context: &mut CompileContext) -> Result<(), message::Message> {
    let value = trim_eol(context.resume().to_string(), true, true);

    match context.tail_mut() {
        Node::Yaml(node) => node.value = value,
        Node::Toml(node) => node.value = value,
        _ => unreachable!("expected yaml/toml on stack for value"),
    }

    on_exit(context)?;
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:{[`GfmAutolinkLiteralEmail`][Name::GfmAutolinkLiteralEmail],[`GfmAutolinkLiteralMailto`][Name::GfmAutolinkLiteralMailto],[`GfmAutolinkLiteralProtocol`][Name::GfmAutolinkLiteralProtocol],[`GfmAutolinkLiteralWww`][Name::GfmAutolinkLiteralWww],[`GfmAutolinkLiteralXmpp`][Name::GfmAutolinkLiteralXmpp]}.
fn on_exit_gfm_autolink_literal(context: &mut CompileContext) -> Result<(), message::Message> {
    on_exit_data(context)?;

    let value = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );

    let prefix = match &context.events[context.index].name {
        Name::GfmAutolinkLiteralEmail => Some("mailto:"),
        Name::GfmAutolinkLiteralWww => Some("http://"),
        // `GfmAutolinkLiteralMailto`, `GfmAutolinkLiteralProtocol`, `GfmAutolinkLiteralXmpp`.
        _ => None,
    };

    if let Node::Link(link) = context.tail_mut() {
        if let Some(prefix) = prefix {
            link.url.push_str(prefix);
        }
        link.url.push_str(value.as_str());
    } else {
        unreachable!("expected link on stack");
    }

    on_exit(context)?;

    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`GfmTable`][Name::GfmTable].
fn on_exit_gfm_table(context: &mut CompileContext) -> Result<(), message::Message> {
    on_exit(context)?;
    context.gfm_table_inside = false;
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:{[`GfmTaskListItemValueChecked`][Name::GfmTaskListItemValueChecked],[`GfmTaskListItemValueUnchecked`][Name::GfmTaskListItemValueUnchecked]}.
fn on_exit_gfm_task_list_item_value(context: &mut CompileContext) {
    let checked = context.events[context.index].name == Name::GfmTaskListItemValueChecked;
    let ancestor = context.tail_penultimate_mut();

    if let Node::ListItem(node) = ancestor {
        node.checked = Some(checked);
    } else {
        unreachable!("expected list item on stack");
    }
}

/// Handle [`Exit`][Kind::Exit]:{[`HardBreakEscape`][Name::HardBreakEscape],[`HardBreakTrailing`][Name::HardBreakTrailing]}.
fn on_exit_hard_break(context: &mut CompileContext) -> Result<(), message::Message> {
    on_exit(context)?;
    context.hard_break_after = true;
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`HeadingAtxSequence`][Name::HeadingAtxSequence].
fn on_exit_heading_atx_sequence(context: &mut CompileContext) {
    let slice = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );

    let size = slice.len();
    // A closing sequence can follow unclosed JSX, just as a Setext underline can.
    // Set the heading ancestor's depth; closing it will report the JSX mismatch.
    let (_, open) = context.trees.last_mut().expect("expected heading tree");
    let heading = open
        .iter_mut()
        .rev()
        .find_map(|(node, _)| match node {
            Node::Heading(node) => Some(node),
            _ => None,
        })
        .expect("expected heading on stack");
    if heading.depth == 0 {
        #[allow(clippy::cast_possible_truncation)]
        let depth = size as u8;
        heading.depth = depth;
    }
}

/// Handle [`Exit`][Kind::Exit]:[`HeadingSetext`][Name::HeadingSetext].
fn on_exit_heading_setext(context: &mut CompileContext) -> Result<(), message::Message> {
    context.heading_setext_text_after = false;
    on_exit(context)?;
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`HeadingSetextText`][Name::HeadingSetextText].
fn on_exit_heading_setext_text(context: &mut CompileContext) {
    context.heading_setext_text_after = true;
}

/// Handle [`Exit`][Kind::Exit]:[`HeadingSetextUnderlineSequence`][Name::HeadingSetextUnderlineSequence].
fn on_exit_heading_setext_underline_sequence(context: &mut CompileContext) {
    let position = SlicePosition::from_exit_event(context.events, context.index);
    let depth = if context.bytes[position.start.index] == b'-' {
        2
    } else {
        1
    };
    // An unclosed JSX element can sit above the heading. Closing the heading
    // will report that mismatch after its ancestor's depth is set.
    let (_, open) = context.trees.last_mut().expect("expected heading tree");
    let heading = open
        .iter_mut()
        .rev()
        .find_map(|(node, _)| match node {
            Node::Heading(node) => Some(node),
            _ => None,
        })
        .expect("expected heading on stack");
    heading.depth = depth;
}

/// Handle [`Exit`][Kind::Exit]:[`LabelText`][Name::LabelText].
fn on_exit_label_text(context: &mut CompileContext) -> Result<(), message::Message> {
    // A label buffer must not silently absorb an unclosed JSX element. Check
    // before `resume`: its assertions disappear in release builds, which used
    // to accept an invalid tree rather than returning the same syntax error.
    if let Some(&(_, index)) = context.trees.last().and_then(|(_, open)| open.last()) {
        let opening = &context.events[index];
        let closing = &context.events[context.index];
        on_mismatch_error(context, Some(closing), opening)?;
    }
    let mut fragment = context.resume();
    let label = fragment.to_string();
    let children = fragment.children_mut().unwrap().split_off(0);
    let reference = context
        .media_reference_stack
        .last_mut()
        .expect("expected reference on media stack");
    reference.label.clone_from(&label);
    reference.label_text_exit = Some(context.index);

    match context.tail_mut() {
        Node::Link(node) => node.children = children,
        Node::Image(node) => node.alt = label,
        Node::FootnoteReference(_) => {}
        _ => unreachable!("expected footnote refereence, image, or link on stack"),
    }

    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`LineEnding`][Name::LineEnding].
fn on_exit_line_ending(context: &mut CompileContext) -> Result<(), message::Message> {
    if context.heading_setext_text_after {
        // Ignore.
    }
    // Line ending position after hard break is part of it.
    else if context.hard_break_after {
        let end = context.events[context.index].point.to_unist();
        let node = context.tail_mut();
        let tail = node
            .children_mut()
            .expect("expected parent")
            .last_mut()
            .expect("expected tail (break)");
        tail.position_mut().unwrap().end = end;
        context.hard_break_after = false;
    }
    // Line ending is a part of nodes that accept phrasing.
    else if matches!(
        context.tail_mut(),
        Node::Emphasis(_) | Node::Heading(_) | Node::Paragraph(_) | Node::Strong(_) | Node::Delete(_)
    ) {
        context.index -= 1;
        on_enter_data(context);
        context.index += 1;
        on_exit_data(context)?;
    }

    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:{[`HtmlFlow`][Name::HtmlFlow],[`HtmlText`][Name::HtmlText]}.
fn on_exit_html(context: &mut CompileContext) -> Result<(), message::Message> {
    let value = context.resume().to_string();

    match context.tail_mut() {
        Node::Html(node) => node.value = value,
        _ => unreachable!("expected html on stack for value"),
    }

    on_exit(context)?;
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:{[`GfmFootnoteCall`][Name::GfmFootnoteCall],[`Image`][Name::Image],[`Link`][Name::Link]}.
fn on_exit_media(context: &mut CompileContext) -> Result<(), message::Message> {
    let mut reference = context
        .media_reference_stack
        .pop()
        .expect("expected reference on media stack");
    on_exit(context)?;

    // It’s a reference.
    if let Some(kind) = reference.reference_kind {
        // Full references got their identifier from the reference string.
        if kind != ReferenceKind::Full {
            if let Some(exit) = reference.label_text_exit {
                let slice = Slice::from_position(context.bytes, &SlicePosition::from_exit_event(context.events, exit));
                reference.identifier = normalize_identifier(slice.as_str()).to_lowercase();
            }
        }

        let parent = context.tail_mut();
        let siblings = parent.children_mut().unwrap();

        match siblings.last_mut().unwrap() {
            Node::FootnoteReference(node) => {
                node.identifier = reference.identifier;
                node.label = Some(reference.label);
            }
            Node::Image(_) => {
                // Need to swap it with a reference version of the node.
                if let Some(Node::Image(node)) = siblings.pop() {
                    siblings.push(Node::ImageReference(ImageReference {
                        reference_kind: kind,
                        identifier: reference.identifier,
                        label: Some(reference.label),
                        alt: node.alt,
                        position: node.position,
                    }));
                } else {
                    unreachable!("impossible: it’s an image")
                }
            }
            Node::Link(_) => {
                // Need to swap it with a reference version of the node.
                if let Some(Node::Link(node)) = siblings.pop() {
                    siblings.push(Node::LinkReference(LinkReference {
                        reference_kind: kind,
                        identifier: reference.identifier,
                        label: Some(reference.label),
                        children: node.children,
                        position: node.position,
                    }));
                } else {
                    unreachable!("impossible: it’s a link")
                }
            }
            _ => unreachable!("expected footnote reference, image, or link on stack"),
        }
    }

    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`ListItem`][Name::ListItem].
fn on_exit_list_item(context: &mut CompileContext) -> Result<(), message::Message> {
    if let Node::ListItem(item) = context.tail_mut() {
        if item.checked.is_some() {
            if let Some(Node::Paragraph(paragraph)) = item.children.first_mut() {
                if let Some(Node::Text(text)) = paragraph.children.first_mut() {
                    let mut point = text.position.as_ref().unwrap().start.clone();
                    let bytes = text.value.as_bytes();
                    let mut start = 0;

                    // Move past eol.
                    if matches!(bytes[0], b'\t' | b' ') {
                        point.offset += 1;
                        point.column += 1;
                        start += 1;
                    } else if matches!(bytes[0], b'\r' | b'\n') {
                        point.line += 1;
                        point.column = 1;
                        point.offset += 1;
                        start += 1;
                        // Move past the LF of CRLF.
                        if bytes.len() > 1 && bytes[0] == b'\r' && bytes[1] == b'\n' {
                            point.offset += 1;
                            start += 1;
                        }
                    }

                    // The whole text is whitespace: update the text.
                    if start == bytes.len() {
                        paragraph.children.remove(0);
                    } else {
                        text.value = str::from_utf8(&bytes[start..]).unwrap().into();
                        text.position.as_mut().unwrap().start = point.clone();
                    }
                    paragraph.position.as_mut().unwrap().start = point;
                }
            }
        }
    }

    on_exit(context)?;

    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`ListItemValue`][Name::ListItemValue].
fn on_exit_list_item_value(context: &mut CompileContext) {
    let start = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    )
    .as_str()
    .parse()
    .expect("expected list value up to u8");

    if let Node::List(node) = context.tail_penultimate_mut() {
        debug_assert!(node.ordered, "expected list to be ordered");
        if node.start.is_none() {
            node.start = Some(start);
        }
    } else {
        unreachable!("expected list on stack");
    }
}

/// Handle [`Exit`][Kind::Exit]:{[`MdxJsxFlowTag`][Name::MdxJsxFlowTag],[`MdxJsxTextTag`][Name::MdxJsxTextTag]}.
fn on_exit_mdx_jsx_tag(context: &mut CompileContext) -> Result<(), message::Message> {
    let mut tag = context.jsx_tag.as_ref().expect("expected tag").clone();

    // End of a tag, so drop the buffer.
    context.resume();
    // Set end point.
    tag.end = context.events[context.index].point.to_unist();

    let stack = &context.jsx_tag_stack;
    let tail = stack.last();

    if tag.close {
        // Unwrap: we crashed earlier if there’s nothing on the stack.
        let tail = tail.unwrap();

        if tail.name != tag.name {
            let label = serialize_abbreviated_tag(&tag);
            return Err(message::Message {
                place: Some(Box::new(message::Place::Position(Position {
                    start: tag.start,
                    end: tag.end,
                }))),
                reason: format!(
                    "Unexpected closing tag `{}`, expected corresponding closing tag for `{}` ({}:{})",
                    label,
                    serialize_abbreviated_tag(tail),
                    tail.start.line,
                    tail.start.column,
                ),
                rule_id: Box::new("end-tag-mismatch".into()),
                source: Box::new("markdown-rs".into()),
            });
        }

        // JSX opened outside a label cannot close inside that label's buffer.
        // The global JSX stack still contains the tag, but the buffer's node
        // stack is empty. Popping it would panic even in release builds.
        if context.trees.last().expect("expected tree").1.is_empty() {
            return Err(message::Message {
                place: Some(Box::new(message::Place::Position(Position {
                    start: tag.start.clone(),
                    end: tag.end.clone(),
                }))),
                reason: format!(
                    "Expected the closing tag `{}` after the end of this label, or another opening tag inside it",
                    serialize_abbreviated_tag(&tag),
                ),
                rule_id: Box::new("end-tag-mismatch".into()),
                source: Box::new("markdown-rs".into()),
            });
        }

        // Remove from our custom stack.
        // Note that this does not exit the node.
        context.jsx_tag_stack.pop();
    } else {
        let node = if context.events[context.index].name == Name::MdxJsxFlowTag {
            Node::MdxJsxFlowElement(MdxJsxFlowElement {
                name: tag.name.clone(),
                attributes: tag.attributes.clone(),
                children: vec![],
                position: Some(Position {
                    start: tag.start.clone(),
                    end: tag.end.clone(),
                }),
            })
        } else {
            Node::MdxJsxTextElement(MdxJsxTextElement {
                name: tag.name.clone(),
                attributes: tag.attributes.clone(),
                children: vec![],
                position: Some(Position {
                    start: tag.start.clone(),
                    end: tag.end.clone(),
                }),
            })
        };

        context.tail_push(node);
    }

    if tag.self_closing || tag.close {
        context.tail_pop()?;
    } else {
        context.jsx_tag_stack.push(tag);
    }

    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`MdxJsxTagClosingMarker`][Name::MdxJsxTagClosingMarker].
fn on_exit_mdx_jsx_tag_closing_marker(context: &mut CompileContext) {
    context.jsx_tag.as_mut().expect("expected tag").close = true;
}

/// Handle [`Exit`][Kind::Exit]:[`MdxJsxTagNamePrimary`][Name::MdxJsxTagNamePrimary].
fn on_exit_mdx_jsx_tag_name_primary(context: &mut CompileContext) {
    let slice = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );
    let value = slice.serialize();
    context.jsx_tag.as_mut().expect("expected tag").name = Some(value);
}

/// Handle [`Exit`][Kind::Exit]:[`MdxJsxTagNameMember`][Name::MdxJsxTagNameMember].
fn on_exit_mdx_jsx_tag_name_member(context: &mut CompileContext) {
    let slice = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );
    let name = context
        .jsx_tag
        .as_mut()
        .expect("expected tag")
        .name
        .as_mut()
        .expect("expected primary before member");
    name.push('.');
    name.push_str(slice.as_str());
}

/// Handle [`Exit`][Kind::Exit]:[`MdxJsxTagNameLocal`][Name::MdxJsxTagNameLocal].
fn on_exit_mdx_jsx_tag_name_local(context: &mut CompileContext) {
    let slice = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );
    let name = context
        .jsx_tag
        .as_mut()
        .expect("expected tag")
        .name
        .as_mut()
        .expect("expected primary before local");
    name.push(':');
    name.push_str(slice.as_str());
}

/// Handle [`Exit`][Kind::Exit]:{[`MdxEsm`][Name::MdxEsm],[`MdxFlowExpression`][Name::MdxFlowExpression],[`MdxTextExpression`][Name::MdxTextExpression]}.
fn on_exit_mdx_esm_or_expression(context: &mut CompileContext) -> Result<(), message::Message> {
    on_exit_drop(context);
    context.tail_pop()?;
    Ok(())
}

/// Handle [`Exit`][Kind::Exit]:[`MdxJsxTagAttributePrimaryName`][Name::MdxJsxTagAttributePrimaryName].
fn on_exit_mdx_jsx_tag_attribute_primary_name(context: &mut CompileContext) {
    let slice = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );
    let value = slice.serialize();

    if let Some(AttributeContent::Property(attribute)) =
        context.jsx_tag.as_mut().expect("expected tag").attributes.last_mut()
    {
        attribute.name = value;
    } else {
        unreachable!("expected property")
    }
}

/// Handle [`Exit`][Kind::Exit]:[`MdxJsxTagAttributeNameLocal`][Name::MdxJsxTagAttributeNameLocal].
fn on_exit_mdx_jsx_tag_attribute_name_local(context: &mut CompileContext) {
    let slice = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );
    if let Some(AttributeContent::Property(attribute)) =
        context.jsx_tag.as_mut().expect("expected tag").attributes.last_mut()
    {
        attribute.name.push(':');
        attribute.name.push_str(slice.as_str());
    } else {
        unreachable!("expected property")
    }
}

/// Handle [`Exit`][Kind::Exit]:[`MdxJsxTagAttributeValueLiteral`][Name::MdxJsxTagAttributeValueLiteral].
fn on_exit_mdx_jsx_tag_attribute_value_literal(context: &mut CompileContext) {
    let value = context.resume();

    if let Some(AttributeContent::Property(node)) =
        context.jsx_tag.as_mut().expect("expected tag").attributes.last_mut()
    {
        node.value = Some(AttributeValue::Literal(parse_character_reference(&value.to_string())));
    } else {
        unreachable!("expected property")
    }
}

/// Handle [`Exit`][Kind::Exit]:[`MdxJsxTagSelfClosingMarker`][Name::MdxJsxTagSelfClosingMarker].
fn on_exit_mdx_jsx_tag_self_closing_marker(context: &mut CompileContext) {
    context.jsx_tag.as_mut().expect("expected tag").self_closing = true;
}

/// Handle [`Exit`][Kind::Exit]:[`ReferenceString`][Name::ReferenceString].
fn on_exit_reference_string(context: &mut CompileContext) {
    let label = context.resume().to_string();
    let slice = Slice::from_position(
        context.bytes,
        &SlicePosition::from_exit_event(context.events, context.index),
    );
    let identifier = normalize_identifier(slice.as_str()).to_lowercase();
    let reference = context
        .media_reference_stack
        .last_mut()
        .expect("expected reference on media stack");
    reference.reference_kind = Some(ReferenceKind::Full);
    reference.label = label;
    reference.identifier = identifier;
}

/// Handle [`Exit`][Kind::Exit]:[`ResourceDestinationString`][Name::ResourceDestinationString].
fn on_exit_resource_destination_string(context: &mut CompileContext) {
    let value = context.resume().to_string();

    match context.tail_mut() {
        Node::Link(node) => node.url = value,
        Node::Image(node) => node.url = value,
        _ => unreachable!("expected link, image on stack"),
    }
}

/// Handle [`Exit`][Kind::Exit]:[`ResourceTitleString`][Name::ResourceTitleString].
fn on_exit_resource_title_string(context: &mut CompileContext) {
    let value = Some(context.resume().to_string());

    match context.tail_mut() {
        Node::Link(node) => node.title = value,
        Node::Image(node) => node.title = value,
        _ => unreachable!("expected link, image on stack"),
    }
}

/// Create a position from an event.
fn position_from_event(event: &Event) -> Position {
    let end = Point::new(event.point.line, event.point.column, event.point.index);
    Position {
        start: end.clone(),
        end,
    }
}

/// Remove initial/final EOLs.
fn trim_eol(value: String, at_start: bool, at_end: bool) -> String {
    let bytes = value.as_bytes();
    let mut start = 0;
    let mut end = bytes.len();

    if at_start && !bytes.is_empty() {
        if bytes[0] == b'\n' {
            start += 1;
        } else if bytes[0] == b'\r' {
            start += 1;
            if bytes.len() > 1 && bytes[1] == b'\n' {
                start += 1;
            }
        }
    }

    if at_end && end > start {
        if bytes[end - 1] == b'\n' {
            end -= 1;
            if end > start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
        } else if bytes[end - 1] == b'\r' {
            end -= 1;
        }
    }

    if start > 0 || end < bytes.len() {
        str::from_utf8(&bytes[start..end]).unwrap().into()
    } else {
        value
    }
}

/// Handle a mismatch.
///
/// Mismatches can occur with MDX JSX tags.
fn on_mismatch_error(
    context: &mut CompileContext,
    left: Option<&Event>,
    right: &Event,
) -> Result<(), message::Message> {
    if right.name == Name::MdxJsxFlowTag || right.name == Name::MdxJsxTextTag {
        let stack = &context.jsx_tag_stack;
        let tag = stack.last().unwrap();
        let point = if let Some(left) = left {
            &left.point
        } else {
            &context.events[context.events.len() - 1].point
        };

        return Err(message::Message {
            place: Some(Box::new(message::Place::Point(point.to_unist()))),
            reason: format!(
                "Expected a closing tag for `{}` ({}:{}){}",
                serialize_abbreviated_tag(tag),
                tag.start.line,
                tag.start.column,
                if let Some(left) = left {
                    format!(" before the end of `{:?}`", left.name)
                } else {
                    String::new()
                }
            ),
            rule_id: Box::new("end-tag-mismatch".into()),
            source: Box::new("markdown-rs".into()),
        });
    }

    if let Some(left) = left {
        if left.name == Name::MdxJsxFlowTag || left.name == Name::MdxJsxTextTag {
            let tag = context.jsx_tag.as_ref().unwrap();

            return Err(message::Message {
                place: Some(Box::new(message::Place::Point(tag.start.clone()))),
                reason: format!(
                    "Expected the closing tag `{}` either before the start of `{:?}` ({}:{}), or another opening tag after that start",
                    serialize_abbreviated_tag(tag),
                    &right.name,
                    &right.point.line,
                    &right.point.column,
                ),
                rule_id: Box::new("end-tag-mismatch".into()),
                source: Box::new("markdown-rs".into()),
            });
        }
        unreachable!("mismatched (non-jsx): {:?} / {:?}", left.name, right.name);
    } else {
        unreachable!("mismatched (non-jsx): document / {:?}", right.name);
    }
}

/// Format a JSX tag, ignoring its attributes.
fn serialize_abbreviated_tag(tag: &JsxTag) -> String {
    format!(
        "<{}{}>",
        if tag.close { "/" } else { "" },
        if let Some(name) = &tag.name { name } else { "" },
    )
}
