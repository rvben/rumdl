//! markdown syntax tree: [mdast][].
//!
//! [mdast]: https://github.com/syntax-tree/mdast

use crate::mdx_parser::unist::Position;
use alloc::{
    fmt,
    string::{String, ToString},
    vec::Vec,
};

/// MDX: relative byte index into a string, to an absolute byte index into the
/// whole document.
pub type Stop = (usize, usize);

/// Explicitness of a reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize), serde(rename_all = "lowercase"))]
pub enum ReferenceKind {
    /// The reference is implicit, its identifier inferred from its content.
    Shortcut,
    /// The reference is explicit, its identifier inferred from its content.
    Collapsed,
    /// The reference is explicit, its identifier explicitly set.
    Full,
}

/// GFM: alignment of phrasing content.
///
/// Used to align the contents of table cells within a table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlignKind {
    /// Left alignment.
    ///
    /// See the `left` value of the `text-align` CSS property.
    ///
    /// ```markdown
    ///   | | aaa |
    /// > | | :-- |
    ///       ^^^
    /// ```
    Left,
    /// Right alignment.
    ///
    /// See the `right` value of the `text-align` CSS property.
    ///
    /// ```markdown
    ///   | | aaa |
    /// > | | --: |
    ///       ^^^
    /// ```
    Right,
    /// Center alignment.
    ///
    /// See the `center` value of the `text-align` CSS property.
    ///
    /// ```markdown
    ///   | | aaa |
    /// > | | :-: |
    ///       ^^^
    /// ```
    Center,
    /// No alignment.
    ///
    /// Phrasing content is aligned as defined by the host environment.
    ///
    /// ```markdown
    ///   | | aaa |
    /// > | | --- |
    ///       ^^^
    /// ```
    None,
}

/// Implement serde according to <https://github.com/syntax-tree/mdast#aligntype>
#[cfg(any())]
impl serde::ser::Serialize for AlignKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self {
            AlignKind::Left => serializer.serialize_unit_variant("AlignKind", 0, "left"),
            AlignKind::Right => serializer.serialize_unit_variant("AlignKind", 1, "right"),
            AlignKind::Center => serializer.serialize_unit_variant("AlignKind", 2, "center"),
            AlignKind::None => serializer.serialize_none(),
        }
    }
}

#[cfg(any())]
struct AlignKindVisitor;

#[cfg(any())]
impl serde::de::Visitor<'_> for AlignKindVisitor {
    type Value = AlignKind;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("'left', 'right', 'center' or null")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match v {
            "left" => Ok(AlignKind::Left),
            "right" => Ok(AlignKind::Right),
            "center" => Ok(AlignKind::Center),
            &_ => Err(serde::de::Error::invalid_type(serde::de::Unexpected::Str(v), &self)),
        }
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match v {
            b"left" => Ok(AlignKind::Left),
            b"right" => Ok(AlignKind::Right),
            b"center" => Ok(AlignKind::Center),
            &_ => Err(serde::de::Error::invalid_type(serde::de::Unexpected::Bytes(v), &self)),
        }
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(AlignKind::None)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(AlignKind::None)
    }
}

#[cfg(any())]
impl<'de> serde::de::Deserialize<'de> for AlignKind {
    fn deserialize<D>(deserializer: D) -> Result<AlignKind, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_any(AlignKindVisitor)
    }
}

/// Nodes.
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(
    any(),
    derive(serde::Serialize, serde::Deserialize),
    serde(tag = "type", rename_all = "camelCase")
)]
pub enum Node {
    // Document:
    /// Root.
    Root(Root),

    // Container:
    /// Block quote.
    Blockquote(Blockquote),
    /// Footnote definition.
    FootnoteDefinition(FootnoteDefinition),
    /// MDX: JSX element (container).
    MdxJsxFlowElement(MdxJsxFlowElement),
    /// List.
    List(List),

    // Frontmatter:
    /// MDX.js ESM.
    MdxjsEsm(MdxjsEsm),
    /// Toml.
    Toml(Toml),
    /// Yaml.
    Yaml(Yaml),

    // Phrasing:
    /// Break.
    Break(Break),
    /// Code (phrasing).
    InlineCode(InlineCode),
    /// Math (phrasing).
    InlineMath(InlineMath),
    /// Delete.
    Delete(Delete),
    /// Emphasis.
    Emphasis(Emphasis),
    // MDX: expression (text).
    MdxTextExpression(MdxTextExpression),
    /// Footnote reference.
    FootnoteReference(FootnoteReference),
    /// Html (phrasing).
    Html(Html),
    /// Image.
    Image(Image),
    /// Image reference.
    ImageReference(ImageReference),
    // MDX: JSX element (text).
    MdxJsxTextElement(MdxJsxTextElement),
    /// Link.
    Link(Link),
    /// Link reference.
    LinkReference(LinkReference),
    /// Strong
    Strong(Strong),
    /// Text.
    Text(Text),

    // Flow:
    /// Code (flow).
    Code(Code),
    /// Math (flow).
    Math(Math),
    // MDX: expression (flow).
    MdxFlowExpression(MdxFlowExpression),
    /// Heading.
    Heading(Heading),
    /// Html (flow).
    // Html(Html),
    /// Table.
    Table(Table),
    /// Thematic break.
    ThematicBreak(ThematicBreak),

    // Table content.
    /// Table row.
    TableRow(TableRow),

    // Row content.
    /// Table cell.
    TableCell(TableCell),

    // List content.
    /// List item.
    ListItem(ListItem),

    // Content.
    /// Definition.
    Definition(Definition),
    /// Paragraph.
    Paragraph(Paragraph),
}

impl fmt::Debug for Node {
    // Debug the wrapped struct.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::Root(x) => x.fmt(f),
            Node::Blockquote(x) => x.fmt(f),
            Node::FootnoteDefinition(x) => x.fmt(f),
            Node::MdxJsxFlowElement(x) => x.fmt(f),
            Node::List(x) => x.fmt(f),
            Node::MdxjsEsm(x) => x.fmt(f),
            Node::Toml(x) => x.fmt(f),
            Node::Yaml(x) => x.fmt(f),
            Node::Break(x) => x.fmt(f),
            Node::InlineCode(x) => x.fmt(f),
            Node::InlineMath(x) => x.fmt(f),
            Node::Delete(x) => x.fmt(f),
            Node::Emphasis(x) => x.fmt(f),
            Node::MdxTextExpression(x) => x.fmt(f),
            Node::FootnoteReference(x) => x.fmt(f),
            Node::Html(x) => x.fmt(f),
            Node::Image(x) => x.fmt(f),
            Node::ImageReference(x) => x.fmt(f),
            Node::MdxJsxTextElement(x) => x.fmt(f),
            Node::Link(x) => x.fmt(f),
            Node::LinkReference(x) => x.fmt(f),
            Node::Strong(x) => x.fmt(f),
            Node::Text(x) => x.fmt(f),
            Node::Code(x) => x.fmt(f),
            Node::Math(x) => x.fmt(f),
            Node::MdxFlowExpression(x) => x.fmt(f),
            Node::Heading(x) => x.fmt(f),
            Node::Table(x) => x.fmt(f),
            Node::ThematicBreak(x) => x.fmt(f),
            Node::TableRow(x) => x.fmt(f),
            Node::TableCell(x) => x.fmt(f),
            Node::ListItem(x) => x.fmt(f),
            Node::Definition(x) => x.fmt(f),
            Node::Paragraph(x) => x.fmt(f),
        }
    }
}

// To do: clippy may be right but that’s a breaking change.
#[allow(clippy::to_string_trait_impl)]
impl ToString for Node {
    fn to_string(&self) -> String {
        // A stack instead of recursion, so deep trees do not overflow.
        let mut value = String::new();
        let mut stack = Vec::new();
        let mut siblings = match self.children() {
            Some(children) => children.iter(),
            None => core::slice::from_ref(self).iter(),
        };

        loop {
            if let Some(node) = siblings.next() {
                if let Some(children) = node.children() {
                    stack.push(core::mem::replace(&mut siblings, children.iter()));
                } else {
                    value.push_str(match node {
                        // Literals.
                        Node::MdxjsEsm(x) => &x.value,
                        Node::Toml(x) => &x.value,
                        Node::Yaml(x) => &x.value,
                        Node::InlineCode(x) => &x.value,
                        Node::InlineMath(x) => &x.value,
                        Node::MdxTextExpression(x) => &x.value,
                        Node::Html(x) => &x.value,
                        Node::Text(x) => &x.value,
                        Node::Code(x) => &x.value,
                        Node::Math(x) => &x.value,
                        Node::MdxFlowExpression(x) => &x.value,
                        // Voids, such as images, and parents, which have
                        // children.
                        _ => "",
                    });
                }
            } else if let Some(parent_siblings) = stack.pop() {
                siblings = parent_siblings;
            } else {
                break value;
            }
        }
    }
}

impl Node {
    #[must_use]
    pub fn children(&self) -> Option<&Vec<Node>> {
        match self {
            // Parent.
            Node::Root(x) => Some(&x.children),
            Node::Paragraph(x) => Some(&x.children),
            Node::Heading(x) => Some(&x.children),
            Node::Blockquote(x) => Some(&x.children),
            Node::List(x) => Some(&x.children),
            Node::ListItem(x) => Some(&x.children),
            Node::Emphasis(x) => Some(&x.children),
            Node::Strong(x) => Some(&x.children),
            Node::Link(x) => Some(&x.children),
            Node::LinkReference(x) => Some(&x.children),
            Node::FootnoteDefinition(x) => Some(&x.children),
            Node::Table(x) => Some(&x.children),
            Node::TableRow(x) => Some(&x.children),
            Node::TableCell(x) => Some(&x.children),
            Node::Delete(x) => Some(&x.children),
            Node::MdxJsxFlowElement(x) => Some(&x.children),
            Node::MdxJsxTextElement(x) => Some(&x.children),
            // Non-parent.
            _ => None,
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
            // Parent.
            Node::Root(x) => Some(&mut x.children),
            Node::Paragraph(x) => Some(&mut x.children),
            Node::Heading(x) => Some(&mut x.children),
            Node::Blockquote(x) => Some(&mut x.children),
            Node::List(x) => Some(&mut x.children),
            Node::ListItem(x) => Some(&mut x.children),
            Node::Emphasis(x) => Some(&mut x.children),
            Node::Strong(x) => Some(&mut x.children),
            Node::Link(x) => Some(&mut x.children),
            Node::LinkReference(x) => Some(&mut x.children),
            Node::FootnoteDefinition(x) => Some(&mut x.children),
            Node::Table(x) => Some(&mut x.children),
            Node::TableRow(x) => Some(&mut x.children),
            Node::TableCell(x) => Some(&mut x.children),
            Node::Delete(x) => Some(&mut x.children),
            Node::MdxJsxFlowElement(x) => Some(&mut x.children),
            Node::MdxJsxTextElement(x) => Some(&mut x.children),
            // Non-parent.
            _ => None,
        }
    }

    #[must_use]
    pub fn position(&self) -> Option<&Position> {
        match self {
            Node::Root(x) => x.position.as_ref(),
            Node::Blockquote(x) => x.position.as_ref(),
            Node::FootnoteDefinition(x) => x.position.as_ref(),
            Node::MdxJsxFlowElement(x) => x.position.as_ref(),
            Node::List(x) => x.position.as_ref(),
            Node::MdxjsEsm(x) => x.position.as_ref(),
            Node::Toml(x) => x.position.as_ref(),
            Node::Yaml(x) => x.position.as_ref(),
            Node::Break(x) => x.position.as_ref(),
            Node::InlineCode(x) => x.position.as_ref(),
            Node::InlineMath(x) => x.position.as_ref(),
            Node::Delete(x) => x.position.as_ref(),
            Node::Emphasis(x) => x.position.as_ref(),
            Node::MdxTextExpression(x) => x.position.as_ref(),
            Node::FootnoteReference(x) => x.position.as_ref(),
            Node::Html(x) => x.position.as_ref(),
            Node::Image(x) => x.position.as_ref(),
            Node::ImageReference(x) => x.position.as_ref(),
            Node::MdxJsxTextElement(x) => x.position.as_ref(),
            Node::Link(x) => x.position.as_ref(),
            Node::LinkReference(x) => x.position.as_ref(),
            Node::Strong(x) => x.position.as_ref(),
            Node::Text(x) => x.position.as_ref(),
            Node::Code(x) => x.position.as_ref(),
            Node::Math(x) => x.position.as_ref(),
            Node::MdxFlowExpression(x) => x.position.as_ref(),
            Node::Heading(x) => x.position.as_ref(),
            Node::Table(x) => x.position.as_ref(),
            Node::ThematicBreak(x) => x.position.as_ref(),
            Node::TableRow(x) => x.position.as_ref(),
            Node::TableCell(x) => x.position.as_ref(),
            Node::ListItem(x) => x.position.as_ref(),
            Node::Definition(x) => x.position.as_ref(),
            Node::Paragraph(x) => x.position.as_ref(),
        }
    }

    pub fn position_mut(&mut self) -> Option<&mut Position> {
        match self {
            Node::Root(x) => x.position.as_mut(),
            Node::Blockquote(x) => x.position.as_mut(),
            Node::FootnoteDefinition(x) => x.position.as_mut(),
            Node::MdxJsxFlowElement(x) => x.position.as_mut(),
            Node::List(x) => x.position.as_mut(),
            Node::MdxjsEsm(x) => x.position.as_mut(),
            Node::Toml(x) => x.position.as_mut(),
            Node::Yaml(x) => x.position.as_mut(),
            Node::Break(x) => x.position.as_mut(),
            Node::InlineCode(x) => x.position.as_mut(),
            Node::InlineMath(x) => x.position.as_mut(),
            Node::Delete(x) => x.position.as_mut(),
            Node::Emphasis(x) => x.position.as_mut(),
            Node::MdxTextExpression(x) => x.position.as_mut(),
            Node::FootnoteReference(x) => x.position.as_mut(),
            Node::Html(x) => x.position.as_mut(),
            Node::Image(x) => x.position.as_mut(),
            Node::ImageReference(x) => x.position.as_mut(),
            Node::MdxJsxTextElement(x) => x.position.as_mut(),
            Node::Link(x) => x.position.as_mut(),
            Node::LinkReference(x) => x.position.as_mut(),
            Node::Strong(x) => x.position.as_mut(),
            Node::Text(x) => x.position.as_mut(),
            Node::Code(x) => x.position.as_mut(),
            Node::Math(x) => x.position.as_mut(),
            Node::MdxFlowExpression(x) => x.position.as_mut(),
            Node::Heading(x) => x.position.as_mut(),
            Node::Table(x) => x.position.as_mut(),
            Node::ThematicBreak(x) => x.position.as_mut(),
            Node::TableRow(x) => x.position.as_mut(),
            Node::TableCell(x) => x.position.as_mut(),
            Node::ListItem(x) => x.position.as_mut(),
            Node::Definition(x) => x.position.as_mut(),
            Node::Paragraph(x) => x.position.as_mut(),
        }
    }

    pub fn position_set(&mut self, position: Option<Position>) {
        match self {
            Node::Root(x) => x.position = position,
            Node::Blockquote(x) => x.position = position,
            Node::FootnoteDefinition(x) => x.position = position,
            Node::MdxJsxFlowElement(x) => x.position = position,
            Node::List(x) => x.position = position,
            Node::MdxjsEsm(x) => x.position = position,
            Node::Toml(x) => x.position = position,
            Node::Yaml(x) => x.position = position,
            Node::Break(x) => x.position = position,
            Node::InlineCode(x) => x.position = position,
            Node::InlineMath(x) => x.position = position,
            Node::Delete(x) => x.position = position,
            Node::Emphasis(x) => x.position = position,
            Node::MdxTextExpression(x) => x.position = position,
            Node::FootnoteReference(x) => x.position = position,
            Node::Html(x) => x.position = position,
            Node::Image(x) => x.position = position,
            Node::ImageReference(x) => x.position = position,
            Node::MdxJsxTextElement(x) => x.position = position,
            Node::Link(x) => x.position = position,
            Node::LinkReference(x) => x.position = position,
            Node::Strong(x) => x.position = position,
            Node::Text(x) => x.position = position,
            Node::Code(x) => x.position = position,
            Node::Math(x) => x.position = position,
            Node::MdxFlowExpression(x) => x.position = position,
            Node::Heading(x) => x.position = position,
            Node::Table(x) => x.position = position,
            Node::ThematicBreak(x) => x.position = position,
            Node::TableRow(x) => x.position = position,
            Node::TableCell(x) => x.position = position,
            Node::ListItem(x) => x.position = position,
            Node::Definition(x) => x.position = position,
            Node::Paragraph(x) => x.position = position,
        }
    }
}

/// MDX: attribute content.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize), serde(untagged))]
pub enum AttributeContent {
    /// JSX expression.
    ///
    /// ```markdown
    /// > | <a {...b} />
    ///        ^^^^^^
    /// ```
    Expression(MdxJsxExpressionAttribute),
    /// JSX property.
    ///
    /// ```markdown
    /// > | <a b />
    ///        ^
    /// ```
    Property(MdxJsxAttribute),
}
//
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
    any(),
    derive(serde::Serialize, serde::Deserialize),
    serde(tag = "type", rename = "mdxJsxAttributeValueExpression")
)]
pub struct AttributeValueExpression {
    pub value: String,
    #[cfg_attr(any(), serde(rename = "_markdownRsStops"))]
    pub stops: Vec<Stop>,
}

/// MDX: attribute value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize), serde(untagged))]
pub enum AttributeValue {
    /// Expression value.
    ///
    /// ```markdown
    /// > | <a b={c} />
    ///          ^^^
    /// ```
    Expression(AttributeValueExpression),
    /// Static value.
    ///
    /// ```markdown
    /// > | <a b="c" />
    ///          ^^^
    /// ```
    Literal(String),
}

/// Document.
///
/// ```markdown
/// > | a
///     ^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Root {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Paragraph.
///
/// ```markdown
/// > | a
///     ^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Paragraph {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Heading.
///
/// ```markdown
/// > | # a
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Heading {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Extra.
    /// Rank (between `1` and `6`, both including).
    pub depth: u8,
}

/// Thematic break.
///
/// ```markdown
/// > | ***
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct ThematicBreak {
    // Void.
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Block quote.
///
/// ```markdown
/// > | > a
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Blockquote {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// List.
///
/// ```markdown
/// > | * a
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct List {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Extra.
    /// Ordered (`true`) or unordered (`false`).
    pub ordered: bool,
    /// Starting number of the list.
    /// `None` when unordered.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub start: Option<u32>,
    /// One or more of its children are separated with a blank line from its
    /// siblings (when `true`), or not (when `false`).
    pub spread: bool,
}

/// List item.
///
/// ```markdown
/// > | * a
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct ListItem {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Extra.
    /// The item contains two or more children separated by a blank line
    /// (when `true`), or not (when `false`).
    pub spread: bool,
    /// GFM: whether the item is done (when `true`), not done (when `false`),
    /// or indeterminate or not applicable (`None`).
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub checked: Option<bool>,
}

/// Html (flow or phrasing).
///
/// ```markdown
/// > | <a>
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Html {
    // Text.
    /// Content model.
    pub value: String,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Code (flow).
///
/// ```markdown
/// > | ~~~
///     ^^^
/// > | a
///     ^
/// > | ~~~
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Code {
    // Text.
    /// Content model.
    pub value: String,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Extra.
    /// The language of computer code being marked up.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub lang: Option<String>,
    /// Custom info relating to the node.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub meta: Option<String>,
}

/// Math (flow).
///
/// ```markdown
/// > | $$
///     ^^
/// > | a
///     ^
/// > | $$
///     ^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Math {
    // Text.
    /// Content model.
    pub value: String,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Extra.
    /// Custom info relating to the node.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub meta: Option<String>,
}

/// Definition.
///
/// ```markdown
/// > | [a]: b
///     ^^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Definition {
    // Void.
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Resource.
    /// URL to the referenced resource.
    pub url: String,
    /// Advisory info for the resource, such as something that would be
    /// appropriate for a tooltip.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub title: Option<String>,
    // Association.
    /// Value that can match another node.
    /// `identifier` is a source value: character escapes and character references
    /// are *not* parsed.
    /// Its value must be normalized.
    pub identifier: String,
    /// `label` is a string value: it works just like `title` on a link or a
    /// `lang` on code: character escapes and character references are parsed.
    ///
    /// To normalize a value, collapse markdown whitespace (`[\t\n\r ]+`) to a
    /// space, trim the optional initial and/or final space, and perform
    /// case-folding.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub label: Option<String>,
}

/// Text.
///
/// ```markdown
/// > | a
///     ^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Text {
    // Text.
    /// Content model.
    pub value: String,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Emphasis.
///
/// ```markdown
/// > | *a*
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Emphasis {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Strong.
///
/// ```markdown
/// > | **a**
///     ^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Strong {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Code (phrasing).
///
/// ```markdown
/// > | `a`
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct InlineCode {
    // Text.
    /// Content model.
    pub value: String,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Math (phrasing).
///
/// ```markdown
/// > | $a$
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct InlineMath {
    // Text.
    /// Content model.
    pub value: String,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Break.
///
/// ```markdown
/// > | a\
///      ^
///   | b
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Break {
    // Void.
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Link.
///
/// ```markdown
/// > | [a](b)
///     ^^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Link {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Resource.
    /// URL to the referenced resource.
    pub url: String,
    /// Advisory info for the resource, such as something that would be
    /// appropriate for a tooltip.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub title: Option<String>,
}

/// Image.
///
/// ```markdown
/// > | ![a](b)
///     ^^^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Image {
    // Void.
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Alternative.
    /// Equivalent content for environments that cannot represent the node as
    /// intended.
    pub alt: String,
    // Resource.
    /// URL to the referenced resource.
    pub url: String,
    /// Advisory info for the resource, such as something that would be
    /// appropriate for a tooltip.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub title: Option<String>,
}

/// Link reference.
///
/// ```markdown
/// > | [a]
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct LinkReference {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Reference.
    /// Explicitness of a reference.
    #[cfg_attr(any(), serde(rename = "referenceType"))]
    pub reference_kind: ReferenceKind,
    // Association.
    /// Value that can match another node.
    /// `identifier` is a source value: character escapes and character references
    /// are *not* parsed.
    /// Its value must be normalized.
    pub identifier: String,
    /// `label` is a string value: it works just like `title` on a link or a
    /// `lang` on code: character escapes and character references are parsed.
    ///
    /// To normalize a value, collapse markdown whitespace (`[\t\n\r ]+`) to a
    /// space, trim the optional initial and/or final space, and perform
    /// case-folding.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub label: Option<String>,
}

/// Image reference.
///
/// ```markdown
/// > | ![a]
///     ^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct ImageReference {
    // Void.
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Alternative.
    /// Equivalent content for environments that cannot represent the node as
    /// intended.
    pub alt: String,
    // Reference.
    /// Explicitness of a reference.
    #[cfg_attr(any(), serde(rename = "referenceType"))]
    pub reference_kind: ReferenceKind,
    // Association.
    /// Value that can match another node.
    /// `identifier` is a source value: character escapes and character references
    /// are *not* parsed.
    /// Its value must be normalized.
    pub identifier: String,
    /// `label` is a string value: it works just like `title` on a link or a
    /// `lang` on code: character escapes and character references are parsed.
    ///
    /// To normalize a value, collapse markdown whitespace (`[\t\n\r ]+`) to a
    /// space, trim the optional initial and/or final space, and perform
    /// case-folding.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub label: Option<String>,
}

/// GFM: footnote definition.
///
/// ```markdown
/// > | [^a]: b
///     ^^^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct FootnoteDefinition {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Association.
    /// Value that can match another node.
    /// `identifier` is a source value: character escapes and character references
    /// are *not* parsed.
    /// Its value must be normalized.
    pub identifier: String,
    /// `label` is a string value: it works just like `title` on a link or a
    /// `lang` on code: character escapes and character references are parsed.
    ///
    /// To normalize a value, collapse markdown whitespace (`[\t\n\r ]+`) to a
    /// space, trim the optional initial and/or final space, and perform
    /// case-folding.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub label: Option<String>,
}

/// GFM: footnote reference.
///
/// ```markdown
/// > | [^a]
///     ^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct FootnoteReference {
    // Void.
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Association.
    /// Value that can match another node.
    /// `identifier` is a source value: character escapes and character references
    /// are *not* parsed.
    /// Its value must be normalized.
    pub identifier: String,
    /// `label` is a string value: it works just like `title` on a link or a
    /// `lang` on code: character escapes and character references are parsed.
    ///
    /// To normalize a value, collapse markdown whitespace (`[\t\n\r ]+`) to a
    /// space, trim the optional initial and/or final space, and perform
    /// case-folding.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub label: Option<String>,
}

/// GFM: table.
///
/// ```markdown
/// > | | a |
///     ^^^^^
/// > | | - |
///     ^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Table {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // Extra.
    /// Represents how cells in columns are aligned.
    pub align: Vec<AlignKind>,
}

/// GFM: table row.
///
/// ```markdown
/// > | | a |
///     ^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct TableRow {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// GFM: table cell.
///
/// ```markdown
/// > | | a |
///     ^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct TableCell {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// GFM: delete.
///
/// ```markdown
/// > | ~~a~~
///     ^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Delete {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Frontmatter: yaml.
///
/// ```markdown
/// > | ---
///     ^^^
/// > | a: b
///     ^^^^
/// > | ---
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Yaml {
    // Void.
    /// Content model.
    pub value: String,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// Frontmatter: toml.
///
/// ```markdown
/// > | +++
///     ^^^
/// > | a: b
///     ^^^^
/// > | +++
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Toml {
    // Void.
    /// Content model.
    pub value: String,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
}

/// MDX: ESM.
///
/// ```markdown
/// > | import a from 'b'
///     ^^^^^^^^^^^^^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct MdxjsEsm {
    // Literal.
    /// Content model.
    pub value: String,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,

    // Custom data on where each slice of `value` came from.
    #[cfg_attr(any(), serde(rename = "_markdownRsStops"))]
    pub stops: Vec<Stop>,
}

/// MDX: expression (flow).
///
/// ```markdown
/// > | {a}
///     ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct MdxFlowExpression {
    // Literal.
    /// Content model.
    pub value: String,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,

    // Custom data on where each slice of `value` came from.
    #[cfg_attr(any(), serde(rename = "_markdownRsStops"))]
    pub stops: Vec<Stop>,
}

/// MDX: expression (text).
///
/// ```markdown
/// > | a {b}
///       ^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct MdxTextExpression {
    // Literal.
    /// Content model.
    pub value: String,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,

    // Custom data on where each slice of `value` came from.
    #[cfg_attr(any(), serde(rename = "_markdownRsStops"))]
    pub stops: Vec<Stop>,
}

/// MDX: JSX element (container).
///
/// ```markdown
/// > | <a />
///     ^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct MdxJsxFlowElement {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // JSX element.
    /// Name.
    ///
    /// Fragments have no name.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub name: Option<String>,
    /// Attributes.
    pub attributes: Vec<AttributeContent>,
}

/// MDX: JSX element (text).
///
/// ```markdown
/// > | <a />.
///     ^^^^^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct MdxJsxTextElement {
    // Parent.
    /// Content model.
    pub children: Vec<Node>,
    /// Positional info.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub position: Option<Position>,
    // JSX element.
    /// Name.
    ///
    /// Fragments have no name.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub name: Option<String>,
    /// Attributes.
    pub attributes: Vec<AttributeContent>,
}

/// MDX: JSX attribute.
///
/// ```markdown
/// > | <a b />
///        ^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
    any(),
    derive(serde::Serialize, serde::Deserialize),
    serde(tag = "type", rename = "mdxJsxAttribute")
)]
pub struct MdxJsxAttribute {
    // Void.
    /// Positional info.
    // pub position: Option<Position>,
    /// Key.
    pub name: String,
    /// Value.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub value: Option<AttributeValue>,
}

/// MDX: JSX expression attribute.
///
/// ```markdown
/// > | <a {...b} />
///        ^
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
    any(),
    derive(serde::Serialize, serde::Deserialize),
    serde(tag = "type", rename = "mdxJsxExpressionAttribute")
)]
pub struct MdxJsxExpressionAttribute {
    /// Value.
    pub value: String,
    /// Stops
    #[cfg_attr(any(), serde(rename = "_markdownRsStops"))]
    pub stops: Vec<Stop>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdx_parser::unist::Position;
    use alloc::{format, string::ToString, vec};

    #[test]
    fn to_string_nested() {
        let node = Node::Paragraph(Paragraph {
            position: None,
            children: vec![
                Node::Text(Text {
                    value: "a".into(),
                    position: None,
                }),
                Node::Emphasis(Emphasis {
                    position: None,
                    children: vec![
                        Node::Image(Image {
                            position: None,
                            alt: "x".into(),
                            url: "y".into(),
                            title: None,
                        }),
                        Node::Strong(Strong {
                            position: None,
                            children: vec![Node::Text(Text {
                                value: "b".into(),
                                position: None,
                            })],
                        }),
                        Node::InlineCode(InlineCode {
                            value: "c".into(),
                            position: None,
                        }),
                    ],
                }),
                Node::Text(Text {
                    value: "d".into(),
                    position: None,
                }),
            ],
        });

        assert_eq!(
            node.to_string(),
            "abcd",
            "should support `ToString` on nested parents, in order, without voids"
        );
    }

    // Literals.

    #[test]
    fn text() {
        let mut node = Node::Text(Text {
            value: "a".into(),
            position: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "Text { value: \"a\", position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "a", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Text { value: \"a\", position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn inline_code() {
        let mut node = Node::InlineCode(InlineCode {
            value: "a".into(),
            position: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "InlineCode { value: \"a\", position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "a", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "InlineCode { value: \"a\", position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn code() {
        let mut node = Node::Code(Code {
            value: "a".into(),
            position: None,
            lang: None,
            meta: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "Code { value: \"a\", position: None, lang: None, meta: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "a", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Code { value: \"a\", position: Some(1:1-1:2 (0-1)), lang: None, meta: None }",
            "should support `position_set`"
        );
    }

    #[test]
    fn inline_math() {
        let mut node = Node::InlineMath(InlineMath {
            value: "a".into(),
            position: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "InlineMath { value: \"a\", position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "a", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "InlineMath { value: \"a\", position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn math() {
        let mut node = Node::Math(Math {
            value: "a".into(),
            position: None,
            meta: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "Math { value: \"a\", position: None, meta: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "a", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Math { value: \"a\", position: Some(1:1-1:2 (0-1)), meta: None }",
            "should support `position_set`"
        );
    }

    #[test]
    fn html() {
        let mut node = Node::Html(Html {
            value: "a".into(),
            position: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "Html { value: \"a\", position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "a", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Html { value: \"a\", position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn mdx_text_expression() {
        let mut node = Node::MdxTextExpression(MdxTextExpression {
            value: "a".into(),
            stops: vec![],
            position: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "MdxTextExpression { value: \"a\", position: None, stops: [] }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "a", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "MdxTextExpression { value: \"a\", position: Some(1:1-1:2 (0-1)), stops: [] }",
            "should support `position_set`"
        );
    }

    #[test]
    fn mdx_flow_expression() {
        let mut node = Node::MdxFlowExpression(MdxFlowExpression {
            value: "a".into(),
            stops: vec![],
            position: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "MdxFlowExpression { value: \"a\", position: None, stops: [] }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "a", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "MdxFlowExpression { value: \"a\", position: Some(1:1-1:2 (0-1)), stops: [] }",
            "should support `position_set`"
        );
    }

    #[test]
    fn mdxjs_esm() {
        let mut node = Node::MdxjsEsm(MdxjsEsm {
            value: "a".into(),
            stops: vec![],
            position: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "MdxjsEsm { value: \"a\", position: None, stops: [] }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "a", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "MdxjsEsm { value: \"a\", position: Some(1:1-1:2 (0-1)), stops: [] }",
            "should support `position_set`"
        );
    }

    #[test]
    fn toml() {
        let mut node = Node::Toml(Toml {
            value: "a".into(),
            position: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "Toml { value: \"a\", position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "a", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Toml { value: \"a\", position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn yaml() {
        let mut node = Node::Yaml(Yaml {
            value: "a".into(),
            position: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "Yaml { value: \"a\", position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "a", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Yaml { value: \"a\", position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    // Voids.

    #[test]
    fn break_node() {
        let mut node = Node::Break(Break { position: None });

        assert_eq!(
            format!("{:?}", node),
            "Break { position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Break { position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn thematic_break() {
        let mut node = Node::ThematicBreak(ThematicBreak { position: None });

        assert_eq!(
            format!("{:?}", node),
            "ThematicBreak { position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "ThematicBreak { position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn footnote_reference() {
        let mut node = Node::FootnoteReference(FootnoteReference {
            position: None,
            identifier: "a".into(),
            label: Some("b".into()),
        });

        assert_eq!(
            format!("{:?}", node),
            "FootnoteReference { position: None, identifier: \"a\", label: Some(\"b\") }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "FootnoteReference { position: Some(1:1-1:2 (0-1)), identifier: \"a\", label: Some(\"b\") }",
            "should support `position_set`"
        );
    }

    #[test]
    fn image_reference() {
        let mut node = Node::ImageReference(ImageReference {
            position: None,
            alt: "a".into(),
            identifier: "b".into(),
            label: Some("c".into()),
            reference_kind: ReferenceKind::Full,
        });

        assert_eq!(
            format!("{:?}", node),
            "ImageReference { position: None, alt: \"a\", reference_kind: Full, identifier: \"b\", label: Some(\"c\") }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "ImageReference { position: Some(1:1-1:2 (0-1)), alt: \"a\", reference_kind: Full, identifier: \"b\", label: Some(\"c\") }",
            "should support `position_set`"
        );
    }

    #[test]
    fn image() {
        let mut node = Node::Image(Image {
            position: None,
            alt: "a".into(),
            url: "b".into(),
            title: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "Image { position: None, alt: \"a\", url: \"b\", title: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Image { position: Some(1:1-1:2 (0-1)), alt: \"a\", url: \"b\", title: None }",
            "should support `position_set`"
        );
    }

    #[test]
    fn definition() {
        let mut node = Node::Definition(Definition {
            position: None,
            identifier: "a".into(),
            label: None,
            url: "b".into(),
            title: None,
        });

        assert_eq!(
            format!("{:?}", node),
            "Definition { position: None, url: \"b\", title: None, identifier: \"a\", label: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), None, "should support `children_mut`");
        assert_eq!(node.children(), None, "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Definition { position: Some(1:1-1:2 (0-1)), url: \"b\", title: None, identifier: \"a\", label: None }",
            "should support `position_set`"
        );
    }

    // Parents.

    #[test]
    fn root() {
        let mut node = Node::Root(Root {
            position: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "Root { children: [], position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Root { children: [], position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn block_quote() {
        let mut node = Node::Blockquote(Blockquote {
            position: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "Blockquote { children: [], position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Blockquote { children: [], position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn delete() {
        let mut node = Node::Delete(Delete {
            position: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "Delete { children: [], position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Delete { children: [], position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn emphasis() {
        let mut node = Node::Emphasis(Emphasis {
            position: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "Emphasis { children: [], position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Emphasis { children: [], position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn strong() {
        let mut node = Node::Strong(Strong {
            position: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "Strong { children: [], position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Strong { children: [], position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn paragraph() {
        let mut node = Node::Paragraph(Paragraph {
            position: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "Paragraph { children: [], position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Paragraph { children: [], position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn table_row() {
        let mut node = Node::TableRow(TableRow {
            position: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "TableRow { children: [], position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "TableRow { children: [], position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn table_cell() {
        let mut node = Node::TableCell(TableCell {
            position: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "TableCell { children: [], position: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "TableCell { children: [], position: Some(1:1-1:2 (0-1)) }",
            "should support `position_set`"
        );
    }

    #[test]
    fn heading() {
        let mut node = Node::Heading(Heading {
            position: None,
            depth: 1,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "Heading { children: [], position: None, depth: 1 }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Heading { children: [], position: Some(1:1-1:2 (0-1)), depth: 1 }",
            "should support `position_set`"
        );
    }

    #[test]
    fn table() {
        let mut node = Node::Table(Table {
            position: None,
            align: vec![],
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "Table { children: [], position: None, align: [] }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Table { children: [], position: Some(1:1-1:2 (0-1)), align: [] }",
            "should support `position_set`"
        );
    }

    #[test]
    fn list_item() {
        let mut node = Node::ListItem(ListItem {
            position: None,
            spread: false,
            checked: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "ListItem { children: [], position: None, spread: false, checked: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "ListItem { children: [], position: Some(1:1-1:2 (0-1)), spread: false, checked: None }",
            "should support `position_set`"
        );
    }

    #[test]
    fn list() {
        let mut node = Node::List(List {
            position: None,
            spread: false,
            ordered: false,
            start: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "List { children: [], position: None, ordered: false, start: None, spread: false }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "List { children: [], position: Some(1:1-1:2 (0-1)), ordered: false, start: None, spread: false }",
            "should support `position_set`"
        );
    }

    #[test]
    fn link_reference() {
        let mut node = Node::LinkReference(LinkReference {
            position: None,
            identifier: "a".into(),
            label: None,
            reference_kind: ReferenceKind::Full,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "LinkReference { children: [], position: None, reference_kind: Full, identifier: \"a\", label: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "LinkReference { children: [], position: Some(1:1-1:2 (0-1)), reference_kind: Full, identifier: \"a\", label: None }",
            "should support `position_set`"
        );
    }

    #[test]
    fn link() {
        let mut node = Node::Link(Link {
            position: None,
            url: "a".into(),
            title: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "Link { children: [], position: None, url: \"a\", title: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "Link { children: [], position: Some(1:1-1:2 (0-1)), url: \"a\", title: None }",
            "should support `position_set`"
        );
    }

    #[test]
    fn footnote_definition() {
        let mut node = Node::FootnoteDefinition(FootnoteDefinition {
            position: None,
            identifier: "a".into(),
            label: None,
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "FootnoteDefinition { children: [], position: None, identifier: \"a\", label: None }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "FootnoteDefinition { children: [], position: Some(1:1-1:2 (0-1)), identifier: \"a\", label: None }",
            "should support `position_set`"
        );
    }

    #[test]
    fn mdx_jsx_flow_element() {
        let mut node = Node::MdxJsxFlowElement(MdxJsxFlowElement {
            position: None,
            name: None,
            attributes: vec![],
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "MdxJsxFlowElement { children: [], position: None, name: None, attributes: [] }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "MdxJsxFlowElement { children: [], position: Some(1:1-1:2 (0-1)), name: None, attributes: [] }",
            "should support `position_set`"
        );
    }

    #[test]
    fn mdx_jsx_text_element() {
        let mut node = Node::MdxJsxTextElement(MdxJsxTextElement {
            position: None,
            name: None,
            attributes: vec![],
            children: vec![],
        });

        assert_eq!(
            format!("{:?}", node),
            "MdxJsxTextElement { children: [], position: None, name: None, attributes: [] }",
            "should support `Debug`"
        );
        assert_eq!(node.to_string(), "", "should support `ToString`");
        assert_eq!(node.children_mut(), Some(&mut vec![]), "should support `children_mut`");
        assert_eq!(node.children(), Some(&vec![]), "should support `children`");
        assert_eq!(node.position(), None, "should support `position`");
        assert_eq!(node.position_mut(), None, "should support `position`");
        node.position_set(Some(Position::new(1, 1, 0, 1, 2, 1)));
        assert_eq!(
            format!("{:?}", node),
            "MdxJsxTextElement { children: [], position: Some(1:1-1:2 (0-1)), name: None, attributes: [] }",
            "should support `position_set`"
        );
    }
}
