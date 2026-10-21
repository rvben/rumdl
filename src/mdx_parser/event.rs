//! Semantic labels of things happening.

use crate::mdx_parser::unist;
use crate::mdx_parser::util::constant::TAB_SIZE;

/// Semantic label of a span.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Name {
    /// Attention sequence.
    ///
    /// > 👉 **Note**: this is used while parsing but compiled away.
    AttentionSequence,
    /// Whole autolink.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`AutolinkEmail`][Name::AutolinkEmail],
    ///   [`AutolinkMarker`][Name::AutolinkMarker],
    ///   [`AutolinkProtocol`][Name::AutolinkProtocol]
    /// * **Construct**:
    ///   [`autolink`][crate::mdx_parser::construct::autolink]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | <https://example.com> and <admin@example.com>
    ///     ^^^^^^^^^^^^^^^^^^^^^     ^^^^^^^^^^^^^^^^^^^
    /// ```
    Autolink,
    /// Email autolink w/o markers.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Autolink`][Name::Autolink]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`autolink`][crate::mdx_parser::construct::autolink]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | <admin@example.com>
    ///      ^^^^^^^^^^^^^^^^^
    /// ```
    AutolinkEmail,
    /// Marker of an autolink.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Autolink`][Name::Autolink]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`autolink`][crate::mdx_parser::construct::autolink]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | <https://example.com>
    ///     ^                   ^
    /// ```
    AutolinkMarker,
    /// Protocol autolink w/o markers.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Autolink`][Name::Autolink]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`autolink`][crate::mdx_parser::construct::autolink]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | <https://example.com>
    ///      ^^^^^^^^^^^^^^^^^^^
    /// ```
    AutolinkProtocol,
    /// Line ending preceded only by whitespace or nothing at all.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`blank_line`][crate::mdx_parser::construct::blank_line]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | ␠␠␊
    ///       ^
    /// ```
    BlankLineEnding,
    /// Whole block quote.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [document content][crate::mdx_parser::construct::document]
    /// * **Content model**:
    ///   [`BlockQuotePrefix`][Name::BlockQuotePrefix],
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Construct**:
    ///   [`block_quote`][crate::mdx_parser::construct::block_quote]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | > a
    ///     ^^^
    /// > | b
    ///     ^
    /// ```
    BlockQuote,
    /// Block quote marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`BlockQuotePrefix`][Name::BlockQuotePrefix]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`block_quote`][crate::mdx_parser::construct::block_quote]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | > a
    ///     ^
    ///   | b
    /// ```
    BlockQuoteMarker,
    /// Block quote prefix.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`BlockQuote`][Name::BlockQuote]
    /// * **Content model**:
    ///   [`BlockQuoteMarker`][Name::BlockQuoteMarker],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`block_quote`][crate::mdx_parser::construct::block_quote]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | > a
    ///     ^^
    ///   | b
    /// ```
    BlockQuotePrefix,
    /// Byte order mark.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   optional first event
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`document`][crate::mdx_parser::construct::document]
    ByteOrderMark,
    /// Whole character escape.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [string content][crate::mdx_parser::construct::string] or
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`CharacterEscapeMarker`][Name::CharacterEscapeMarker],
    ///   [`CharacterEscapeValue`][Name::CharacterEscapeValue]
    /// * **Construct**:
    ///   [`character_escape`][crate::mdx_parser::construct::character_escape]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a \- b
    ///       ^^
    /// ```
    CharacterEscape,
    /// Character escape marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CharacterEscape`][Name::CharacterEscape]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`character_escape`][crate::mdx_parser::construct::character_escape]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a \- b
    ///       ^
    /// ```
    CharacterEscapeMarker,
    /// Character escape value.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CharacterEscape`][Name::CharacterEscape]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`character_escape`][crate::mdx_parser::construct::character_escape]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a \- b
    ///        ^
    /// ```
    CharacterEscapeValue,
    /// Whole character reference.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [string content][crate::mdx_parser::construct::string] or
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`CharacterReferenceMarker`][Name::CharacterReferenceMarker],
    ///   [`CharacterReferenceMarkerHexadecimal`][Name::CharacterReferenceMarkerHexadecimal],
    ///   [`CharacterReferenceMarkerNumeric`][Name::CharacterReferenceMarkerNumeric],
    ///   [`CharacterReferenceMarkerSemi`][Name::CharacterReferenceMarkerSemi],
    ///   [`CharacterReferenceValue`][Name::CharacterReferenceValue]
    /// * **Construct**:
    ///   [`character_reference`][crate::mdx_parser::construct::character_reference]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a &amp; b &#8800; c &#x1D306; d
    ///       ^^^^^   ^^^^^^^   ^^^^^^^^^
    /// ```
    CharacterReference,
    /// Character reference opening marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CharacterReference`][Name::CharacterReference]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`character_reference`][crate::mdx_parser::construct::character_reference]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a &amp; b &#8800; c &#x1D306; d
    ///       ^       ^         ^
    /// ```
    CharacterReferenceMarker,
    /// Character reference hexadecimal numeric marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CharacterReference`][Name::CharacterReference]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`character_reference`][crate::mdx_parser::construct::character_reference]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a &amp; b &#8800; c &#x1D306; d
    ///                           ^
    /// ```
    CharacterReferenceMarkerHexadecimal,
    /// Character reference numeric marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CharacterReference`][Name::CharacterReference]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`character_reference`][crate::mdx_parser::construct::character_reference]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a &amp; b &#8800; c &#x1D306; d
    ///                ^         ^
    /// ```
    CharacterReferenceMarkerNumeric,
    /// Character reference closing marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CharacterReference`][Name::CharacterReference]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`character_reference`][crate::mdx_parser::construct::character_reference]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a &amp; b &#8800; c &#x1D306; d
    ///           ^         ^           ^
    /// ```
    CharacterReferenceMarkerSemi,
    /// Character reference value.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CharacterReference`][Name::CharacterReference]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`character_reference`][crate::mdx_parser::construct::character_reference]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a &amp; b &#8800; c &#x1D306; d
    ///        ^^^      ^^^^       ^^^^^
    /// ```
    CharacterReferenceValue,
    /// Whole code (fenced).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`CodeFencedFence`][Name::CodeFencedFence],
    ///   [`CodeFlowChunk`][Name::CodeFlowChunk],
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow]
    ///
    /// ## Example
    ///
    /// ````markdown
    /// > | ```js
    ///     ^^^^^
    /// > | console.log(1)
    ///     ^^^^^^^^^^^^^^
    /// > | ```
    ///     ^^^
    /// ````
    CodeFenced,
    /// A code (fenced) fence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CodeFenced`][Name::CodeFenced]
    /// * **Content model**:
    ///   [`CodeFencedFenceInfo`][Name::CodeFencedFenceInfo],
    ///   [`CodeFencedFenceMeta`][Name::CodeFencedFenceMeta],
    ///   [`CodeFencedFenceSequence`][Name::CodeFencedFenceSequence],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow]
    ///
    /// ## Example
    ///
    /// ````markdown
    /// > | ```js
    ///     ^^^^^
    ///   | console.log(1)
    /// > | ```
    ///     ^^^
    /// ````
    CodeFencedFence,
    /// A code (fenced) fence info word.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CodeFencedFence`][Name::CodeFencedFence]
    /// * **Content model**:
    ///   [string content][crate::mdx_parser::construct::string]
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow]
    ///
    /// ## Example
    ///
    /// ````markdown
    /// > | ```js
    ///        ^^
    ///   | console.log(1)
    ///   | ```
    /// ````
    CodeFencedFenceInfo,
    /// A code (fenced) fence meta string.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CodeFencedFence`][Name::CodeFencedFence]
    /// * **Content model**:
    ///   [string content][crate::mdx_parser::construct::string]
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow]
    ///
    /// ## Example
    ///
    /// ````markdown
    /// > | ```js highlight="1"
    ///           ^^^^^^^^^^^^^
    ///   | console.log(1)
    ///   | ```
    /// ````
    CodeFencedFenceMeta,
    /// A code (fenced) fence sequence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CodeFencedFenceSequence`][Name::CodeFencedFenceSequence]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow]
    ///
    /// ## Example
    ///
    /// ````markdown
    /// > | ```js
    ///     ^^^
    ///   | console.log(1)
    /// > | ```
    ///     ^^^
    /// ````
    CodeFencedFenceSequence,
    /// A code (fenced, indented) chunk.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CodeFenced`][Name::CodeFenced],
    ///   [`CodeIndented`][Name::CodeIndented]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow],
    ///   [`code_indented`][crate::mdx_parser::construct::code_indented]
    ///
    /// ## Example
    ///
    /// ````markdown
    ///   | ```js
    /// > | console.log(1)
    ///     ^^^^^^^^^^^^^^
    ///   | ```
    /// ````
    ///
    /// ```markdown
    /// > | ␠␠␠␠console.log(1)
    ///         ^^^^^^^^^^^^^^
    /// ```
    CodeFlowChunk,
    /// Whole code (indented).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`CodeFlowChunk`][Name::CodeFlowChunk],
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// ␠␠␠␠console.log(1)
    /// ^^^^^^^^^^^^^^^^^^
    /// ```
    CodeIndented,
    /// Whole code (text).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`CodeTextData`][Name::CodeTextData],
    ///   [`CodeTextSequence`][Name::CodeTextSequence],
    ///   [`LineEnding`][Name::LineEnding]
    /// * **Construct**:
    ///   [`raw_text`][crate::mdx_parser::construct::raw_text]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a `b` c
    ///       ^^^
    /// ```
    CodeText,
    /// Code (text) data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CodeText`][Name::CodeText]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`raw_text`][crate::mdx_parser::construct::raw_text]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a `b` c
    ///        ^
    /// ```
    CodeTextData,
    /// Code (text) sequence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`CodeText`][Name::CodeText]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`raw_text`][crate::mdx_parser::construct::raw_text]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a `b` c
    ///       ^ ^
    /// ```
    CodeTextSequence,
    /// Content.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [content][crate::mdx_parser::construct::content]
    /// * **Construct**:
    ///   [`content`][crate::mdx_parser::construct::content]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b
    ///     ^^^^^^
    /// > | c.
    ///     ^^
    /// ```
    Content,
    /// Data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [string content][crate::mdx_parser::construct::string],
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`data`][crate::mdx_parser::construct::partial_data]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | aa *bb* cc
    ///     ^^^ ^^ ^^^
    /// ```
    Data,
    /// Whole definition.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`DefinitionMarker`][Name::DefinitionMarker],
    ///   [`DefinitionLabel`][Name::DefinitionLabel],
    ///   [`DefinitionDestination`][Name::DefinitionDestination],
    ///   [`DefinitionTitle`][Name::DefinitionTitle],
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`definition`][crate::mdx_parser::construct::definition]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///     ^^^^^^^^^^
    /// ```
    Definition,
    /// Whole definition destination.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Definition`][Name::Definition]
    /// * **Content model**:
    ///   [`DefinitionDestinationLiteral`][Name::DefinitionDestinationLiteral],
    ///   [`DefinitionDestinationRaw`][Name::DefinitionDestinationRaw]
    /// * **Construct**:
    ///   [`destination`][crate::mdx_parser::construct::partial_destination]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///          ^
    /// > | [a]: <b> "c"
    ///          ^^^
    /// ```
    DefinitionDestination,
    /// Definition destination literal.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`DefinitionDestination`][Name::DefinitionDestination]
    /// * **Content model**:
    ///   [`DefinitionDestinationLiteralMarker`][Name::DefinitionDestinationLiteralMarker],
    ///   [`DefinitionDestinationString`][Name::DefinitionDestinationString]
    /// * **Construct**:
    ///   [`destination`][crate::mdx_parser::construct::partial_destination]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: <b> "c"
    ///          ^^^
    /// ```
    DefinitionDestinationLiteral,
    /// Definition destination literal marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`DefinitionDestinationLiteral`][Name::DefinitionDestinationLiteral]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`destination`][crate::mdx_parser::construct::partial_destination]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: <b> "c"
    ///          ^ ^
    /// ```
    DefinitionDestinationLiteralMarker,
    /// Definition destination raw.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`DefinitionDestination`][Name::DefinitionDestination]
    /// * **Content model**:
    ///   [`DefinitionDestinationString`][Name::DefinitionDestinationString]
    /// * **Construct**:
    ///   [`destination`][crate::mdx_parser::construct::partial_destination]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///          ^
    /// ```
    DefinitionDestinationRaw,
    /// Definition destination data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`DefinitionDestinationLiteral`][Name::DefinitionDestinationLiteral],
    ///   [`DefinitionDestinationRaw`][Name::DefinitionDestinationRaw]
    /// * **Content model**:
    ///   [string content][crate::mdx_parser::construct::string]
    /// * **Construct**:
    ///   [`destination`][crate::mdx_parser::construct::partial_destination]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///          ^
    /// > | [a]: <b> "c"
    ///           ^
    /// ```
    DefinitionDestinationString,
    /// Whole definition label.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Definition`][Name::Definition]
    /// * **Content model**:
    ///   [`DefinitionLabelMarker`][Name::DefinitionLabelMarker],
    ///   [`DefinitionLabelString`][Name::DefinitionLabelString],
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`label`][crate::mdx_parser::construct::partial_label]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///     ^^^
    /// ```
    DefinitionLabel,
    /// Definition label marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`DefinitionLabel`][Name::DefinitionLabel]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`label`][crate::mdx_parser::construct::partial_label]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///     ^ ^
    /// ```
    DefinitionLabelMarker,
    /// Definition label data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`DefinitionLabel`][Name::DefinitionLabel]
    /// * **Content model**:
    ///   [string content][crate::mdx_parser::construct::string]
    /// * **Construct**:
    ///   [`label`][crate::mdx_parser::construct::partial_label]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///      ^
    /// ```
    DefinitionLabelString,
    /// Definition marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Definition`][Name::Definition],
    ///   [`GfmFootnoteDefinition`][Name::GfmFootnoteDefinition]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`definition`][crate::mdx_parser::construct::definition]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///        ^
    /// ```
    DefinitionMarker,
    /// Whole definition title.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Definition`][Name::Definition]
    /// * **Content model**:
    ///   [`DefinitionTitleMarker`][Name::DefinitionTitleMarker],
    ///   [`DefinitionTitleString`][Name::DefinitionTitleString],
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`title`][crate::mdx_parser::construct::partial_title]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///            ^^^
    /// ```
    DefinitionTitle,
    /// Definition title marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`DefinitionTitle`][Name::DefinitionTitle]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`title`][crate::mdx_parser::construct::partial_title]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///            ^ ^
    /// ```
    DefinitionTitleMarker,
    /// Definition title data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`DefinitionTitle`][Name::DefinitionTitle]
    /// * **Content model**:
    ///   [string content][crate::mdx_parser::construct::string]
    /// * **Construct**:
    ///   [`title`][crate::mdx_parser::construct::partial_title]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///             ^
    /// ```
    DefinitionTitleString,
    /// Emphasis.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`EmphasisSequence`][Name::EmphasisSequence],
    ///   [`EmphasisText`][Name::EmphasisText]
    /// * **Construct**:
    ///   [`attention`][crate::mdx_parser::construct::attention]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | *a*
    ///     ^^^
    /// ```
    Emphasis,
    /// Emphasis sequence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Emphasis`][Name::Emphasis]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`attention`][crate::mdx_parser::construct::attention]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | *a*
    ///     ^ ^
    /// ```
    EmphasisSequence,
    /// Emphasis text.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Emphasis`][Name::Emphasis]
    /// * **Content model**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Construct**:
    ///   [`attention`][crate::mdx_parser::construct::attention]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | *a*
    ///      ^
    /// ```
    EmphasisText,
    /// Whole frontmatter.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [document content][crate::mdx_parser::construct::document]
    /// * **Content model**:
    ///   [`FrontmatterFence`][Name::FrontmatterFence],
    ///   [`FrontmatterChunk`][Name::FrontmatterChunk],
    ///   [`LineEnding`][Name::LineEnding]
    /// * **Construct**:
    ///   [`frontmatter`][crate::mdx_parser::construct::frontmatter]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | ---
    ///     ^^^
    /// > | title: Neptune
    ///     ^^^^^^^^^^^^^^
    /// > | ---
    ///     ^^^
    /// ```
    Frontmatter,
    /// Frontmatter chunk.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Frontmatter`][Name::Frontmatter]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`frontmatter`][crate::mdx_parser::construct::frontmatter]
    ///
    /// ## Example
    ///
    /// ```markdown
    ///   | ---
    /// > | title: Neptune
    ///     ^^^^^^^^^^^^^^
    ///   | ---
    /// ```
    FrontmatterChunk,
    /// Frontmatter fence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Frontmatter`][Name::Frontmatter]
    /// * **Content model**:
    ///   [`FrontmatterSequence`][Name::FrontmatterSequence],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`frontmatter`][crate::mdx_parser::construct::frontmatter]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | ---
    ///     ^^^
    ///   | title: Neptune
    /// > | ---
    ///     ^^^
    /// ```
    FrontmatterFence,
    /// Frontmatter sequence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`FrontmatterFence`][Name::FrontmatterFence]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`frontmatter`][crate::mdx_parser::construct::frontmatter]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | ---
    ///     ^^^
    ///   | title: Neptune
    /// > | ---
    ///     ^^^
    /// ```
    FrontmatterSequence,
    /// GFM extension: email autolink.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   void.
    /// * **Construct**:
    ///   [`gfm_autolink_literal`][crate::mdx_parser::construct::gfm_autolink_literal]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | context@example.com
    ///     ^^^^^^^^^^^^^^^^^^^
    /// ```
    GfmAutolinkLiteralEmail,
    /// GFM extension: email autolink w/ explicit `mailto`.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   void.
    /// * **Construct**:
    ///   [`gfm_autolink_literal`][crate::mdx_parser::construct::gfm_autolink_literal]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | mailto:context@example.com
    ///     ^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    GfmAutolinkLiteralMailto,
    /// GFM extension: autolink w/ protocol.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   void.
    /// * **Construct**:
    ///   [`gfm_autolink_literal`][crate::mdx_parser::construct::gfm_autolink_literal]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | https://example.com
    ///     ^^^^^^^^^^^^^^^^^^^
    /// ```
    GfmAutolinkLiteralProtocol,
    /// GFM extension: autolink w/ www.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   void.
    /// * **Construct**:
    ///   [`gfm_autolink_literal`][crate::mdx_parser::construct::gfm_autolink_literal]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | www.example.com
    ///     ^^^^^^^^^^^^^^^
    /// ```
    GfmAutolinkLiteralWww,
    /// GFM extension: email autolink w/ explicit `xmpp`.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   void.
    /// * **Construct**:
    ///   [`gfm_autolink_literal`][crate::mdx_parser::construct::gfm_autolink_literal]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | mailto:a@b.c/d
    ///     ^^^^^^^^^^^^^^
    /// ```
    GfmAutolinkLiteralXmpp,
    /// GFM extension: whole footnote call.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`Label`][Name::Label]
    /// * **Construct**:
    ///   [`label_end`][crate::mdx_parser::construct::label_end]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a [^b] c
    ///       ^^^^
    /// ```
    GfmFootnoteCall,
    /// GFM extension: label start (footnote).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Label`][Name::Label]
    /// * **Content model**:
    ///   [`GfmFootnoteCallMarker`][Name::GfmFootnoteCallMarker],
    ///   [`LabelMarker`][Name::LabelMarker]
    /// * **Construct**:
    ///   [`gfm_label_start_footnote`][crate::mdx_parser::construct::gfm_label_start_footnote]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a [^b] c
    ///       ^^
    /// ```
    GfmFootnoteCallLabel,
    /// GFM extension: label start (footnote) marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmFootnoteCallLabel`][Name::GfmFootnoteCallLabel]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`gfm_label_start_footnote`][crate::mdx_parser::construct::gfm_label_start_footnote]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a [^b] c
    ///        ^
    /// ```
    GfmFootnoteCallMarker,
    /// GFM extension: whole footnote definition.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [document content][crate::mdx_parser::construct::document]
    /// * **Content model**:
    ///   [`GfmFootnoteDefinitionPrefix`][Name::GfmFootnoteDefinitionPrefix],
    ///   [document content][crate::mdx_parser::construct::flow]
    /// * **Construct**:
    ///   [`gfm_footnote_definition`][crate::mdx_parser::construct::gfm_footnote_definition]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [^a]: b
    ///     ^^^^^^^
    /// ```
    GfmFootnoteDefinition,
    /// GFM extension: footnote definition prefix.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmFootnoteDefinition`][Name::GfmFootnoteDefinition]
    /// * **Content model**:
    ///   [`DefinitionMarker`][Name::DefinitionMarker],
    ///   [`GfmFootnoteDefinitionLabel`][Name::GfmFootnoteDefinitionLabel],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`gfm_footnote_definition`][crate::mdx_parser::construct::gfm_footnote_definition]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [^a]: b
    ///     ^^^^^^
    /// ```
    GfmFootnoteDefinitionPrefix,
    /// GFM extension: footnote definition label.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmFootnoteDefinitionPrefix`][Name::GfmFootnoteDefinitionPrefix]
    /// * **Content model**:
    ///   [`GfmFootnoteDefinitionLabelMarker`][Name::GfmFootnoteDefinitionLabelMarker],
    ///   [`GfmFootnoteDefinitionLabelString`][Name::GfmFootnoteDefinitionLabelString],
    ///   [`GfmFootnoteDefinitionMarker`][Name::GfmFootnoteDefinitionMarker]
    /// * **Construct**:
    ///   [`gfm_footnote_definition`][crate::mdx_parser::construct::gfm_footnote_definition]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [^a]: b
    ///     ^^^^
    /// ```
    GfmFootnoteDefinitionLabel,
    /// GFM extension: footnote definition label marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmFootnoteDefinitionLabel`][Name::GfmFootnoteDefinitionLabel]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`gfm_footnote_definition`][crate::mdx_parser::construct::gfm_footnote_definition]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [^a]: b
    ///     ^  ^
    GfmFootnoteDefinitionLabelMarker,
    /// GFM extension: footnote definition label string.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmFootnoteDefinitionLabel`][Name::GfmFootnoteDefinitionLabel]
    /// * **Content model**:
    ///   [string content][crate::mdx_parser::construct::string]
    /// * **Construct**:
    ///   [`gfm_footnote_definition`][crate::mdx_parser::construct::gfm_footnote_definition]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [^a]: b
    ///       ^
    GfmFootnoteDefinitionLabelString,
    /// GFM extension: footnote definition marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmFootnoteDefinitionLabel`][Name::GfmFootnoteDefinitionLabel]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`gfm_footnote_definition`][crate::mdx_parser::construct::gfm_footnote_definition]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | [^a]: b
    ///      ^
    GfmFootnoteDefinitionMarker,
    /// GFM extension: Strikethrough.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`GfmStrikethroughSequence`][Name::GfmStrikethroughSequence],
    ///   [`GfmStrikethroughText`][Name::GfmStrikethroughText]
    /// * **Construct**:
    ///   [`attention`][crate::mdx_parser::construct::attention]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | ~a~
    ///     ^^^
    /// ```
    GfmStrikethrough,
    /// GFM extension: Strikethrough sequence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmStrikethrough`][Name::GfmStrikethrough]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`attention`][crate::mdx_parser::construct::attention]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | ~a~
    ///     ^ ^
    /// ```
    GfmStrikethroughSequence,
    /// GFM extension: Strikethrough text.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmStrikethrough`][Name::GfmStrikethrough]
    /// * **Content model**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Construct**:
    ///   [`attention`][crate::mdx_parser::construct::attention]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | ~a~
    ///      ^
    /// ```
    GfmStrikethroughText,
    /// GFM extension: Table.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`GfmTableBody`][Name::GfmTableBody],
    ///   [`GfmTableHead`][Name::GfmTableHead],
    ///   [`LineEnding`][Name::LineEnding]
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | | a |
    ///     ^^^^^
    /// > | | - |
    ///     ^^^^^
    /// > | | b |
    ///     ^^^^^
    /// ```
    GfmTable,
    /// GFM extension: Table body.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTable`][Name::GfmTable]
    /// * **Content model**:
    ///   [`GfmTableRow`][Name::GfmTableRow],
    ///   [`LineEnding`][Name::LineEnding]
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    ///   | | a |
    ///   | | - |
    /// > | | b |
    ///     ^^^^^
    /// ```
    GfmTableBody,
    /// GFM extension: Table cell.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTableRow`][Name::GfmTableRow]
    /// * **Content model**:
    ///   [`GfmTableCellDivider`][Name::GfmTableCellDivider],
    ///   [`GfmTableCellText`][Name::GfmTableCellText],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | | a |
    ///     ^^^^^
    ///   | | - |
    /// > | | b |
    ///     ^^^^^
    /// ```
    GfmTableCell,
    /// GFM extension: Table cell text.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTableCell`][Name::GfmTableCell]
    /// * **Content model**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | | a |
    ///       ^
    ///   | | - |
    /// > | | b |
    ///       ^
    /// ```
    GfmTableCellText,
    /// GFM extension: Table cell divider.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTableCell`][Name::GfmTableCell]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | | a |
    ///     ^   ^
    /// > | | - |
    ///     ^   ^
    /// > | | b |
    ///     ^   ^
    /// ```
    GfmTableCellDivider,
    /// GFM extension: Table delimiter row.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTableHead`][Name::GfmTableHead]
    /// * **Content model**:
    ///   [`GfmTableDelimiterCell`][Name::GfmTableDelimiterCell]
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    ///   | | a |
    /// > | | - |
    ///     ^^^^^
    ///   | | b |
    /// ```
    GfmTableDelimiterRow,
    /// GFM extension: Table delimiter alignment marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTableDelimiterCellValue`][Name::GfmTableDelimiterCellValue]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    ///   | | a  |
    /// > | | :- |
    ///       ^
    ///   | | b  |
    /// ```
    GfmTableDelimiterMarker,
    /// GFM extension: Table delimiter cell.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTableDelimiterRow`][Name::GfmTableDelimiterRow]
    /// * **Content model**:
    ///   [`GfmTableCellDivider`][Name::GfmTableCellDivider],
    ///   [`GfmTableDelimiterCellValue`][Name::GfmTableDelimiterCellValue],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    ///   | | a |
    /// > | | - |
    ///     ^^^^^
    ///   | | b |
    /// ```
    GfmTableDelimiterCell,
    /// GFM extension: Table delimiter cell alignment.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTableDelimiterCell`][Name::GfmTableDelimiterCell]
    /// * **Content model**:
    ///   [`GfmTableDelimiterMarker`][Name::GfmTableDelimiterMarker],
    ///   [`GfmTableDelimiterFiller`][Name::GfmTableDelimiterFiller]
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    ///   | | a |
    /// > | | - |
    ///       ^
    ///   | | b |
    /// ```
    GfmTableDelimiterCellValue,
    /// GFM extension: Table delimiter filler.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTableDelimiterCellValue`][Name::GfmTableDelimiterCellValue]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    ///   | | a |
    /// > | | - |
    ///       ^
    ///   | | b |
    /// ```
    GfmTableDelimiterFiller,
    /// GFM extension: Table head.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTable`][Name::GfmTable]
    /// * **Content model**:
    ///   [`GfmTableRow`][Name::GfmTableRow],
    ///   [`GfmTableDelimiterRow`][Name::GfmTableDelimiterRow],
    ///   [`LineEnding`][Name::LineEnding]
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | | a |
    ///     ^^^^^
    /// > | | - |
    ///     ^^^^^
    ///   | | b |
    /// ```
    GfmTableHead,
    /// GFM extension: Table row.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTableBody`][Name::GfmTableBody],
    ///   [`GfmTableHead`][Name::GfmTableHead]
    /// * **Content model**:
    ///   [`GfmTableCell`][Name::GfmTableCell]
    /// * **Construct**:
    ///   [`gfm_table`][crate::mdx_parser::construct::gfm_table]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | | a |
    ///     ^^^^^
    ///   | | - |
    /// > | | b |
    ///     ^^^^^
    /// ```
    GfmTableRow,
    /// GFM extension: task list item check.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`GfmTaskListItemMarker`][Name::GfmTaskListItemMarker],
    ///   [`GfmTaskListItemValueChecked`][Name::GfmTaskListItemValueChecked],
    ///   [`GfmTaskListItemValueUnchecked`][Name::GfmTaskListItemValueUnchecked]
    /// * **Construct**:
    ///   [`gfm_task_list_item_check`][crate::mdx_parser::construct::gfm_task_list_item_check]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | * [x] y.
    ///       ^^^
    /// ```
    GfmTaskListItemCheck,
    /// GFM extension: task list item check marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTaskListItemCheck`][Name::GfmTaskListItemCheck]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`gfm_task_list_item_check`][crate::mdx_parser::construct::gfm_task_list_item_check]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | * [x] y.
    ///       ^ ^
    /// ```
    GfmTaskListItemMarker,
    /// GFM extension: task list item value: checked.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTaskListItemCheck`][Name::GfmTaskListItemCheck]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`gfm_task_list_item_check`][crate::mdx_parser::construct::gfm_task_list_item_check]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | * [x] y.
    ///        ^
    /// ```
    GfmTaskListItemValueChecked,
    /// GFM extension: task list item value: unchecked.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`GfmTaskListItemCheck`][Name::GfmTaskListItemCheck]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`gfm_task_list_item_check`][crate::mdx_parser::construct::gfm_task_list_item_check]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | * [ ] z.
    ///        ^
    /// ```
    GfmTaskListItemValueUnchecked,
    /// Whole hard break (escape).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`hard_break_escape`][crate::mdx_parser::construct::hard_break_escape]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a\␊
    ///      ^
    /// > | b
    /// ```
    HardBreakEscape,
    /// Whole hard break (trailing).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`whitespace`][crate::mdx_parser::construct::partial_whitespace]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a␠␠␊
    ///      ^^
    /// > | b
    /// ```
    HardBreakTrailing,
    /// Whole heading (atx).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`HeadingAtxSequence`][Name::HeadingAtxSequence],
    ///   [`HeadingAtxText`][Name::HeadingAtxText],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`heading_atx`][crate::mdx_parser::construct::heading_atx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | # alpha
    ///     ^^^^^^^
    /// ```
    HeadingAtx,
    /// Heading (atx) sequence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`HeadingAtx`][Name::HeadingAtx]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`heading_atx`][crate::mdx_parser::construct::heading_atx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | # alpha
    ///     ^
    /// ```
    HeadingAtxSequence,
    /// Heading (atx) data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`HeadingAtx`][Name::HeadingAtx]
    /// * **Content model**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Construct**:
    ///   [`heading_atx`][crate::mdx_parser::construct::heading_atx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | # alpha
    ///       ^^^^^
    /// ```
    HeadingAtxText,
    /// Whole heading (setext).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`HeadingSetextText`][Name::HeadingSetextText],
    ///   [`HeadingSetextUnderline`][Name::HeadingSetextUnderline],
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`heading_setext`][crate::mdx_parser::construct::heading_setext]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | alpha
    ///     ^^^^^
    /// > | =====
    ///     ^^^^^
    /// ```
    HeadingSetext,
    /// Heading (setext) data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`HeadingSetext`][Name::HeadingSetext]
    /// * **Content model**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Construct**:
    ///   [`heading_setext`][crate::mdx_parser::construct::heading_setext]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | alpha
    ///     ^^^^^
    ///   | =====
    /// ```
    HeadingSetextText,
    /// Heading (setext) underline.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`HeadingSetext`][Name::HeadingSetext]
    /// * **Content model**:
    ///   [`HeadingSetextUnderlineSequence`][Name::HeadingSetextUnderlineSequence],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`heading_setext`][crate::mdx_parser::construct::heading_setext]
    ///
    /// ## Example
    ///
    /// ```markdown
    ///   | alpha
    /// > | =====
    ///     ^^^^^
    /// ```
    HeadingSetextUnderline,
    /// Heading (setext) underline sequence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`HeadingSetext`][Name::HeadingSetext]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`heading_setext`][crate::mdx_parser::construct::heading_setext]
    ///
    /// ## Example
    ///
    /// ```markdown
    ///   | alpha
    /// > | =====
    ///     ^^^^^
    /// ```
    HeadingSetextUnderlineSequence,
    /// Whole html (flow).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`HtmlFlowData`][Name::HtmlFlowData],
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`html_flow`][crate::mdx_parser::construct::html_flow]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | <div>
    ///     ^^^^^
    /// ```
    HtmlFlow,
    /// HTML (flow) data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`HtmlFlow`][Name::HtmlFlow]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`html_flow`][crate::mdx_parser::construct::html_flow]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | <div>
    ///     ^^^^^
    /// ```
    HtmlFlowData,
    /// Whole html (text).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`HtmlTextData`][Name::HtmlTextData],
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`html_text`][crate::mdx_parser::construct::html_text]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b> c
    ///       ^^^
    /// ```
    HtmlText,
    /// HTML (text) data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`HtmlText`][Name::HtmlText]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`html_text`][crate::mdx_parser::construct::html_text]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b> c
    ///       ^^^
    /// ```
    HtmlTextData,
    /// Image.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`Label`][Name::Label],
    ///   [`Resource`][Name::Resource],
    ///   [`Reference`][Name::Reference]
    /// * **Construct**:
    ///   [`label_end`][crate::mdx_parser::construct::label_end]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b] c
    ///       ^^^^
    /// > | a ![b][c] d
    ///       ^^^^^^^
    /// > | a ![b](c) d
    ///       ^^^^^^^
    /// ```
    Image,
    /// Label.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Image`][Name::Image],
    ///   [`Link`][Name::Link]
    /// * **Content model**:
    ///   [`LabelImage`][Name::LabelImage],
    ///   [`LabelLink`][Name::LabelLink],
    ///   [`LabelEnd`][Name::LabelEnd],
    ///   [`LabelText`][Name::LabelText]
    /// * **Construct**:
    ///   [`label_end`][crate::mdx_parser::construct::label_end]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a [b] c
    ///       ^^^
    /// > | a ![b][c] d
    ///       ^^^^
    /// > | a [b](c) d
    ///       ^^^
    /// ```
    Label,
    /// Label end.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Label`][Name::Label]
    /// * **Content model**:
    ///   [`LabelMarker`][Name::LabelMarker]
    /// * **Construct**:
    ///   [`label_end`][crate::mdx_parser::construct::label_end]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](c) d
    ///          ^
    /// > | a [b](c) d
    ///         ^
    /// ```
    LabelEnd,
    /// Label start (image).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Label`][Name::Label]
    /// * **Content model**:
    ///   [`LabelImageMarker`][Name::LabelImageMarker],
    ///   [`LabelMarker`][Name::LabelMarker]
    /// * **Construct**:
    ///   [`label_start_image`][crate::mdx_parser::construct::label_start_image]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](c) d
    ///       ^^
    /// ```
    LabelImage,
    /// Label start (image) marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`LabelImage`][Name::LabelImage]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`label_start_image`][crate::mdx_parser::construct::label_start_image]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](c) d
    ///       ^
    /// ```
    LabelImageMarker,
    /// Label start (link).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Label`][Name::Label]
    /// * **Content model**:
    ///   [`LabelMarker`][Name::LabelMarker]
    /// * **Construct**:
    ///   [`label_start_link`][crate::mdx_parser::construct::label_start_link]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a [b](c) d
    ///       ^
    /// ```
    LabelLink,
    /// Label marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`LabelImage`][Name::LabelImage],
    ///   [`LabelLink`][Name::LabelLink],
    ///   [`LabelEnd`][Name::LabelEnd]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`label_start_image`][crate::mdx_parser::construct::label_start_image],
    ///   [`label_start_link`][crate::mdx_parser::construct::label_start_link],
    ///   [`label_end`][crate::mdx_parser::construct::label_end]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](c) d
    ///        ^ ^
    /// > | a [b](c) d
    ///       ^ ^
    /// ```
    LabelMarker,
    /// Label text.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Label`][Name::Label]
    /// * **Content model**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Construct**:
    ///   [`label_end`][crate::mdx_parser::construct::label_end]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a [b] c
    ///        ^
    /// > | a ![b][c] d
    ///         ^
    /// > | a [b](c) d
    ///        ^
    /// ```
    LabelText,
    /// Line ending.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   basically everywhere
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   n/a
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a␊
    ///      ^
    ///   | b
    /// ```
    LineEnding,
    /// Link.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`Label`][Name::Label],
    ///   [`Resource`][Name::Resource],
    ///   [`Reference`][Name::Reference]
    /// * **Construct**:
    ///   [`label_end`][crate::mdx_parser::construct::label_end]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a [b] c
    ///       ^^^
    /// > | a [b][c] d
    ///       ^^^^^^
    /// > | a [b](c) d
    ///       ^^^^^^
    /// ```
    Link,
    /// List item.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`ListOrdered`][Name::ListOrdered],
    ///   [`ListUnordered`][Name::ListUnordered]
    /// * **Content model**:
    ///   [`ListItemPrefix`][Name::ListItemPrefix],
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Construct**:
    ///   [`list item`][crate::mdx_parser::construct::list_item]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | * a
    ///     ^^^
    /// > | 1. b
    ///     ^^^^
    /// ```
    ListItem,
    /// List item (marker).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`ListItemPrefix`][Name::ListItemPrefix]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`list item`][crate::mdx_parser::construct::list_item]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | * a
    ///     ^
    /// > | 1. b
    ///      ^
    /// ```
    ListItemMarker,
    /// List item (prefix).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`ListItem`][Name::ListItem]
    /// * **Content model**:
    ///   [`ListItemMarker`][Name::ListItemMarker],
    ///   [`ListItemValue`][Name::ListItemValue],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`list item`][crate::mdx_parser::construct::list_item]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | * a
    ///     ^^
    /// > |   b
    ///     ^^
    /// ```
    ListItemPrefix,
    /// List item (value).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`ListItemPrefix`][Name::ListItemPrefix]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`list item`][crate::mdx_parser::construct::list_item]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | 1. b
    ///     ^
    /// ```
    ListItemValue,
    /// List (ordered).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [document content][crate::mdx_parser::construct::document]
    /// * **Content model**:
    ///   [`BlankLineEnding`][Name::BlankLineEnding],
    ///   [`BlockQuotePrefix`][Name::BlockQuotePrefix],
    ///   [`ListItem`][Name::ListItem],
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`list item`][crate::mdx_parser::construct::list_item]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | 1. a
    ///     ^^^^
    /// > | 2. b
    ///     ^^^^
    /// ```
    ListOrdered,
    /// List (unordered).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [document content][crate::mdx_parser::construct::document]
    /// * **Content model**:
    ///   [`BlankLineEnding`][Name::BlankLineEnding],
    ///   [`BlockQuotePrefix`][Name::BlockQuotePrefix],
    ///   [`ListItem`][Name::ListItem],
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`list item`][crate::mdx_parser::construct::list_item]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | * a
    ///     ^^^
    /// > | * b
    ///     ^^^
    /// ```
    ListUnordered,
    /// Whole math (flow).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`MathFlowFence`][Name::MathFlowFence],
    ///   [`MathFlowChunk`][Name::MathFlowChunk],
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | $$
    ///     ^^
    /// > | \frac{1}{2}
    ///     ^^^^^^^^^^^
    /// > | $$
    ///     ^^
    /// ```
    MathFlow,
    /// A math (flow) fence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MathFlow`][Name::MathFlow]
    /// * **Content model**:
    ///   [`MathFlowFenceMeta`][Name::MathFlowFenceMeta],
    ///   [`MathFlowFenceSequence`][Name::MathFlowFenceSequence],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | $$
    ///     ^^
    ///   | \frac{1}{2}
    /// > | $$
    ///     ^^
    /// ```
    MathFlowFence,
    /// A math (flow) fence meta string.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MathFlowFence`][Name::MathFlowFence]
    /// * **Content model**:
    ///   [string content][crate::mdx_parser::construct::string]
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | $$alpha bravo
    ///       ^^^^^^^^^^^
    ///   | \frac{1}{2}
    ///   | $$
    /// ```
    MathFlowFenceMeta,
    /// A math (flow) fence sequence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MathFlowFenceSequence`][Name::MathFlowFenceSequence]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | $$
    ///     ^^
    ///   | \frac{1}{2}
    /// > | $$
    ///     ^^
    /// ```
    MathFlowFenceSequence,
    /// A math (flow) chunk.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MathFlow`][Name::MathFlow]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`raw_flow`][crate::mdx_parser::construct::raw_flow]
    ///
    /// ## Example
    ///
    /// ```markdown
    ///   | $$
    /// > | \frac{1}{2}
    ///     ^^^^^^^^^^^
    ///   | $$
    /// ```
    MathFlowChunk,
    /// Whole math (text).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`MathTextData`][Name::MathTextData],
    ///   [`MathTextSequence`][Name::MathTextSequence],
    ///   [`LineEnding`][Name::LineEnding]
    /// * **Construct**:
    ///   [`raw_text`][crate::mdx_parser::construct::raw_text]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a $b$ c
    ///       ^^^
    /// ```
    MathText,
    /// Math (text) data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MathText`][Name::MathText]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`raw_text`][crate::mdx_parser::construct::raw_text]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a `b` c
    ///        ^
    /// ```
    MathTextData,
    /// Math (text) sequence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MathText`][Name::MathText]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`raw_text`][crate::mdx_parser::construct::raw_text]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a $b$ c
    ///       ^ ^
    /// ```
    MathTextSequence,
    /// MDX extension: ESM.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   void
    ///   [`MdxEsmData`][Name::MdxEsmData],
    ///   [`SpaceOrTab`][Name::SpaceOrTab],
    ///   [`LineEnding`][Name::LineEnding]
    /// * **Construct**:
    ///   [`mdx_esm`][crate::mdx_parser::construct::mdx_esm]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | import a from 'b'
    ///     ^^^^^^^^^^^^^^^^^
    /// ```
    MdxEsm,
    /// MDX extension: ESM data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxEsm`][Name::MdxEsm]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`mdx_esm`][crate::mdx_parser::construct::mdx_esm]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | import a from 'b'
    ///     ^^^^^^^^^^^^^^^^^
    /// ```
    MdxEsmData,
    /// MDX extension: expression marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxFlowExpression`][Name::MdxFlowExpression],
    ///   [`MdxTextExpression`][Name::MdxTextExpression],
    ///   [`MdxJsxTagAttributeExpression`][Name::MdxJsxTagAttributeExpression],
    ///   [`MdxJsxTagAttributeValueExpression`][Name::MdxJsxTagAttributeValueExpression]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_expression`][crate::mdx_parser::construct::partial_mdx_expression]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | {Math.PI}
    ///     ^       ^
    /// ```
    MdxExpressionMarker,
    /// MDX extension: expression data.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxFlowExpression`][Name::MdxFlowExpression],
    ///   [`MdxTextExpression`][Name::MdxTextExpression],
    ///   [`MdxJsxTagAttributeExpression`][Name::MdxJsxTagAttributeExpression],
    ///   [`MdxJsxTagAttributeValueExpression`][Name::MdxJsxTagAttributeValueExpression]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_expression`][crate::mdx_parser::construct::partial_mdx_expression]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | {Math.PI}
    ///      ^^^^^^^
    /// ```
    MdxExpressionData,
    /// MDX extension: expression (flow).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab],
    ///   [`MdxExpressionMarker`][Name::MdxExpressionMarker],
    ///   [`MdxExpressionData`][Name::MdxExpressionData]
    /// * **Construct**:
    ///   [`mdx_expression_flow`][crate::mdx_parser::construct::mdx_expression_flow]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | {Math.PI}
    ///     ^^^^^^^^^
    /// ```
    MdxFlowExpression,
    /// MDX extension: expression (text).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab],
    ///   [`MdxExpressionMarker`][Name::MdxExpressionMarker],
    ///   [`MdxExpressionData`][Name::MdxExpressionData]
    /// * **Construct**:
    ///   [`mdx_expression_text`][crate::mdx_parser::construct::mdx_expression_text]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a {Math.PI} b
    ///       ^^^^^^^^^
    /// ```
    MdxTextExpression,
    /// MDX extension: JSX (flow).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`MdxJsxEsWhitespace`][Name::MdxJsxEsWhitespace],
    ///   [`MdxJsxTagMarker`][Name::MdxJsxTagMarker],
    ///   [`MdxJsxTagClosingMarker`][Name::MdxJsxTagClosingMarker],
    ///   [`MdxJsxTagName`][Name::MdxJsxTagName],
    ///   [`MdxJsxTagAttribute`][Name::MdxJsxTagAttribute],
    ///   [`MdxJsxTagAttributeExpression`][Name::MdxJsxTagAttributeExpression],
    ///   [`MdxJsxTagSelfClosingMarker`][Name::MdxJsxTagSelfClosingMarker]
    /// * **Construct**:
    ///   [`mdx_jsx_flow`][crate::mdx_parser::construct::mdx_jsx_flow]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | <B />
    ///     ^^^^^
    /// ```
    MdxJsxFlowTag,
    /// MDX extension: JSX (text).
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`MdxJsxEsWhitespace`][Name::MdxJsxEsWhitespace],
    ///   [`MdxJsxTagMarker`][Name::MdxJsxTagMarker],
    ///   [`MdxJsxTagClosingMarker`][Name::MdxJsxTagClosingMarker],
    ///   [`MdxJsxTagName`][Name::MdxJsxTagName],
    ///   [`MdxJsxTagAttribute`][Name::MdxJsxTagAttribute],
    ///   [`MdxJsxTagAttributeExpression`][Name::MdxJsxTagAttributeExpression],
    ///   [`MdxJsxTagSelfClosingMarker`][Name::MdxJsxTagSelfClosingMarker]
    /// * **Construct**:
    ///   [`mdx_jsx_text`][crate::mdx_parser::construct::mdx_jsx_text]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <B /> c
    ///       ^^^^^
    /// ```
    MdxJsxTextTag,
    /// MDX extension: JSX: ECMAScript whitespace.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxFlowTag`][Name::MdxJsxFlowTag],
    ///   [`MdxJsxTextTag`][Name::MdxJsxTextTag],
    ///   [`MdxJsxTagName`][Name::MdxJsxTagName],
    ///   [`MdxJsxTagAttribute`][Name::MdxJsxTagAttribute],
    ///   [`MdxJsxTagAttributeName`][Name::MdxJsxTagAttributeName]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <B /> c
    ///         ^
    /// ```
    MdxJsxEsWhitespace,
    /// MDX extension: JSX: tag marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxFlowTag`][Name::MdxJsxFlowTag],
    ///   [`MdxJsxTextTag`][Name::MdxJsxTextTag]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <B /> c
    ///       ^   ^
    /// ```
    MdxJsxTagMarker,
    /// MDX extension: JSX: closing tag marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxFlowTag`][Name::MdxJsxFlowTag],
    ///   [`MdxJsxTextTag`][Name::MdxJsxTextTag]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a </B> c
    ///        ^
    /// ```
    MdxJsxTagClosingMarker,
    /// MDX extension: JSX: tag name.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxFlowTag`][Name::MdxJsxFlowTag],
    ///   [`MdxJsxTextTag`][Name::MdxJsxTextTag]
    /// * **Content model**:
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`MdxJsxEsWhitespace`][Name::MdxJsxEsWhitespace],
    ///   [`MdxJsxTagNamePrimary`][Name::MdxJsxTagNamePrimary],
    ///   [`MdxJsxTagNameMember`][Name::MdxJsxTagNameMember],
    ///   [`MdxJsxTagNameMemberMarker`][Name::MdxJsxTagNameMemberMarker],
    ///   [`MdxJsxTagNamePrefixMarker`][Name::MdxJsxTagNamePrefixMarker],
    ///   [`MdxJsxTagNameLocal`][Name::MdxJsxTagNameLocal]
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b> c
    ///        ^
    /// > | a <b:c> d
    ///        ^^^
    /// > | a <b.c> d
    ///        ^^^
    /// ```
    #[allow(clippy::enum_variant_names)]
    MdxJsxTagName,
    /// MDX extension: JSX: primary tag name.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagName`][Name::MdxJsxTagName]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b> c
    ///        ^
    /// > | a <b:c> d
    ///        ^
    /// > | a <b.c> d
    ///        ^
    /// ```
    MdxJsxTagNamePrimary,
    /// MDX extension: JSX: tag name member marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagName`][Name::MdxJsxTagName]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b.c> d
    ///         ^
    /// ```
    MdxJsxTagNameMemberMarker,
    /// MDX extension: JSX: tag name prefix marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagName`][Name::MdxJsxTagName]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b:c> d
    ///         ^
    /// ```
    MdxJsxTagNamePrefixMarker,
    /// MDX extension: JSX: tag name member.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagName`][Name::MdxJsxTagName]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b.c> d
    ///          ^
    /// ```
    MdxJsxTagNameMember,
    /// MDX extension: JSX: tag name local.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagName`][Name::MdxJsxTagName]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b:c> d
    ///          ^
    /// ```
    MdxJsxTagNameLocal,
    /// MDX extension: JSX: attribute.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxFlowTag`][Name::MdxJsxFlowTag],
    ///   [`MdxJsxTextTag`][Name::MdxJsxTextTag]
    /// * **Content model**:
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`MdxJsxEsWhitespace`][Name::MdxJsxEsWhitespace],
    ///   [`MdxJsxTagAttributeName`][Name::MdxJsxTagAttributeName],
    ///   [`MdxJsxTagAttributeInitializerMarker`][Name::MdxJsxTagAttributeInitializerMarker],
    ///   [`MdxJsxTagAttributeValueLiteral`][Name::MdxJsxTagAttributeValueLiteral],
    ///   [`MdxJsxTagAttributeValueExpression`][Name::MdxJsxTagAttributeValueExpression]
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b c> d
    ///          ^
    /// > | a <b c="d"> e
    ///          ^^^^^
    /// > | a <b c={d}> e
    ///          ^^^^^
    /// ```
    MdxJsxTagAttribute,
    /// MDX extension: JSX tag attribute expression.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxFlowTag`][Name::MdxJsxFlowTag],
    ///   [`MdxJsxTextTag`][Name::MdxJsxTextTag]
    /// * **Content model**:
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab],
    ///   [`MdxExpressionMarker`][Name::MdxExpressionMarker],
    ///   [`MdxExpressionData`][Name::MdxExpressionData]
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b {Math.PI} /> c
    ///          ^^^^^^^^^
    /// ```
    MdxJsxTagAttributeExpression,
    /// MDX extension: JSX: attribute name.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagAttribute`][Name::MdxJsxTagAttribute]
    /// * **Content model**:
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`MdxJsxEsWhitespace`][Name::MdxJsxEsWhitespace],
    ///   [`MdxJsxTagAttributePrimaryName`][Name::MdxJsxTagAttributePrimaryName],
    ///   [`MdxJsxTagAttributeNamePrefixMarker`][Name::MdxJsxTagAttributeNamePrefixMarker],
    ///   [`MdxJsxTagAttributeNameLocal`][Name::MdxJsxTagAttributeNameLocal]
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b c> d
    ///          ^
    /// > | a <b c:d="e"> f
    ///          ^^^
    /// ```
    #[allow(clippy::enum_variant_names)]
    MdxJsxTagAttributeName,
    /// MDX extension: JSX: primary attribute name.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagAttributeName`][Name::MdxJsxTagAttributeName]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b c> d
    ///          ^
    /// > | a <b c:d="e"> f
    ///          ^
    /// ```
    #[allow(clippy::enum_variant_names)]
    MdxJsxTagAttributePrimaryName,
    /// MDX extension: JSX: attribute name prefix marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagAttributeName`][Name::MdxJsxTagAttributeName]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b c:d="e"> f
    ///           ^
    /// ```
    MdxJsxTagAttributeNamePrefixMarker,
    /// MDX extension: JSX: local attribute name.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagAttributeName`][Name::MdxJsxTagAttributeName]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b c:d="e"> f
    ///            ^
    /// ```
    MdxJsxTagAttributeNameLocal,
    /// MDX extension: JSX: attribute initializer marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagAttribute`][Name::MdxJsxTagAttribute]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b c="d"> e
    ///           ^
    /// ```
    MdxJsxTagAttributeInitializerMarker,
    /// MDX extension: JSX tag attribute value expression.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxFlowTag`][Name::MdxJsxFlowTag],
    ///   [`MdxJsxTextTag`][Name::MdxJsxTextTag]
    /// * **Content model**:
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`SpaceOrTab`][Name::SpaceOrTab],
    ///   [`MdxExpressionMarker`][Name::MdxExpressionMarker],
    ///   [`MdxExpressionData`][Name::MdxExpressionData]
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b c={Math.PI} /> d
    ///            ^^^^^^^^^
    /// ```
    MdxJsxTagAttributeValueExpression,
    /// MDX extension: JSX: attribute value literal.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagAttribute`][Name::MdxJsxTagAttribute]
    /// * **Content model**:
    ///   [`LineEnding`][Name::LineEnding],
    ///   [`MdxJsxEsWhitespace`][Name::MdxJsxEsWhitespace],
    ///   [`MdxJsxTagAttributeValueLiteralMarker`][Name::MdxJsxTagAttributeValueLiteralMarker],
    ///   [`MdxJsxTagAttributeValueLiteralValue`][Name::MdxJsxTagAttributeValueLiteralValue]
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b c="d"> e
    ///            ^^^
    /// ```
    MdxJsxTagAttributeValueLiteral,
    /// MDX extension: JSX: attribute value literal marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagAttributeValueLiteral`][Name::MdxJsxTagAttributeValueLiteral]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b c="d"> e
    ///            ^ ^
    /// ```
    MdxJsxTagAttributeValueLiteralMarker,
    /// MDX extension: JSX: attribute value literal value.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxTagAttributeValueLiteral`][Name::MdxJsxTagAttributeValueLiteral]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b c="d"> e
    ///             ^
    /// ```
    MdxJsxTagAttributeValueLiteralValue,
    /// MDX extension: JSX: self-closing tag marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`MdxJsxFlowTag`][Name::MdxJsxFlowTag],
    ///   [`MdxJsxTextTag`][Name::MdxJsxTextTag]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`partial_mdx_jsx`][crate::mdx_parser::construct::partial_mdx_jsx]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a <b /> c
    ///          ^
    /// ```
    MdxJsxTagSelfClosingMarker,
    /// Paragraph.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [content][crate::mdx_parser::construct::content]
    /// * **Content model**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Construct**:
    ///   [`paragraph`][crate::mdx_parser::construct::paragraph]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a b
    ///     ^^^
    /// > | c.
    ///     ^^
    /// ```
    Paragraph,
    /// Reference.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Image`][Name::Image],
    ///   [`Link`][Name::Link]
    /// * **Content model**:
    ///   [`ReferenceMarker`][Name::ReferenceMarker],
    ///   [`ReferenceString`][Name::ReferenceString]
    /// * **Construct**:
    ///   [`label`][crate::mdx_parser::construct::partial_label]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b][c] d
    ///           ^^^
    /// ```
    Reference,
    /// Reference marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Reference`][Name::Reference]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`label`][crate::mdx_parser::construct::partial_label]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b][c] d
    ///           ^ ^
    /// ```
    ReferenceMarker,
    /// Reference string.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Reference`][Name::Reference]
    /// * **Content model**:
    ///   [string content][crate::mdx_parser::construct::string]
    /// * **Construct**:
    ///   [`label`][crate::mdx_parser::construct::partial_label]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b][c] d
    ///            ^
    /// ```
    ReferenceString,
    /// Resource.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Image`][Name::Image],
    ///   [`Link`][Name::Link]
    /// * **Content model**:
    ///   [`ResourceMarker`][Name::ResourceMarker],
    ///   [`ResourceDestination`][Name::ResourceDestination],
    ///   [`ResourceTitle`][Name::ResourceTitle],
    ///   [`SpaceOrTab`][Name::SpaceOrTab],
    ///   [`LineEnding`][Name::LineEnding]
    /// * **Construct**:
    ///   [`label_end`][crate::mdx_parser::construct::label_end]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](c "d") e
    ///           ^^^^^^^
    /// > | a [b](c) d
    ///          ^^^
    /// ```
    Resource,
    /// Resource destination.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Resource`][Name::Resource]
    /// * **Content model**:
    ///   [`ResourceDestinationLiteral`][Name::ResourceDestinationLiteral],
    ///   [`ResourceDestinationRaw`][Name::ResourceDestinationRaw]
    /// * **Construct**:
    ///   [`destination`][crate::mdx_parser::construct::partial_destination]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](c "d") e
    ///            ^
    /// ```
    ResourceDestination,
    /// Resource destination literal.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`ResourceDestination`][Name::ResourceDestination]
    /// * **Content model**:
    ///   [`ResourceDestinationLiteralMarker`][Name::ResourceDestinationLiteralMarker],
    ///   [`ResourceDestinationString`][Name::ResourceDestinationString]
    /// * **Construct**:
    ///   [`destination`][crate::mdx_parser::construct::partial_destination]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](<c> "d") e
    ///            ^^^
    /// ```
    ResourceDestinationLiteral,
    /// Resource destination literal marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`ResourceDestinationLiteral`][Name::ResourceDestinationLiteral]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`destination`][crate::mdx_parser::construct::partial_destination]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](<c> "d") e
    ///            ^ ^
    /// ```
    ResourceDestinationLiteralMarker,
    /// Resource destination raw.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`ResourceDestination`][Name::ResourceDestination]
    /// * **Content model**:
    ///   [`ResourceDestinationString`][Name::ResourceDestinationString]
    /// * **Construct**:
    ///   [`destination`][crate::mdx_parser::construct::partial_destination]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](c "d") e
    ///            ^
    /// ```
    ResourceDestinationRaw,
    /// Resource destination raw.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`ResourceDestinationLiteral`][Name::ResourceDestinationLiteral],
    ///   [`ResourceDestinationRaw`][Name::ResourceDestinationRaw]
    /// * **Content model**:
    ///   [string content][crate::mdx_parser::construct::string]
    /// * **Construct**:
    ///   [`destination`][crate::mdx_parser::construct::partial_destination]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](<c> "d") e
    ///             ^
    /// > | a ![b](c "d") e
    ///            ^
    /// ```
    ResourceDestinationString,
    /// Resource marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Resource`][Name::Resource]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`label_end`][crate::mdx_parser::construct::label_end]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](c "d") e
    ///           ^     ^
    /// ```
    ResourceMarker,
    /// Resource title.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Resource`][Name::Resource]
    /// * **Content model**:
    ///   [`ResourceTitleMarker`][Name::ResourceTitleMarker],
    ///   [`ResourceTitleString`][Name::ResourceTitleString]
    /// * **Construct**:
    ///   [`title`][crate::mdx_parser::construct::partial_title]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](<c> "d") e
    ///                ^^^
    /// ```
    ResourceTitle,
    /// Resource title marker.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`ResourceTitle`][Name::ResourceTitle]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`title`][crate::mdx_parser::construct::partial_title]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](<c> "d") e
    ///                ^ ^
    /// ```
    ResourceTitleMarker,
    /// Resource title string.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`ResourceTitle`][Name::ResourceTitle]
    /// * **Content model**:
    ///   [string content][crate::mdx_parser::construct::string]
    /// * **Construct**:
    ///   [`title`][crate::mdx_parser::construct::partial_title]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | a ![b](<c> "d") e
    ///                 ^
    /// ```
    ResourceTitleString,
    /// Space or tab.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   basically everywhere
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   n/a
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | ␠* * *␠
    ///     ^ ^ ^ ^
    /// ```
    SpaceOrTab,
    /// Strong.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Content model**:
    ///   [`StrongSequence`][Name::StrongSequence],
    ///   [`StrongText`][Name::StrongText]
    /// * **Construct**:
    ///   [`attention`][crate::mdx_parser::construct::attention]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | **a**
    ///     ^^^^^
    /// ```
    Strong,
    /// Strong sequence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Strong`][Name::Strong]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`attention`][crate::mdx_parser::construct::attention]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | **a**
    ///     ^^ ^^
    /// ```
    StrongSequence,
    /// Strong text.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`Strong`][Name::Strong]
    /// * **Content model**:
    ///   [text content][crate::mdx_parser::construct::text]
    /// * **Construct**:
    ///   [`attention`][crate::mdx_parser::construct::attention]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | **a**
    ///       ^
    /// ```
    StrongText,
    /// Whole thematic break.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [flow content][crate::mdx_parser::construct::flow]
    /// * **Content model**:
    ///   [`ThematicBreakSequence`][Name::ThematicBreakSequence],
    ///   [`SpaceOrTab`][Name::SpaceOrTab]
    /// * **Construct**:
    ///   [`thematic_break`][crate::mdx_parser::construct::thematic_break]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | * * *
    ///     ^^^^^
    /// ```
    ThematicBreak,
    /// Thematic break sequence.
    ///
    /// ## Info
    ///
    /// * **Context**:
    ///   [`ThematicBreak`][Name::ThematicBreak]
    /// * **Content model**:
    ///   void
    /// * **Construct**:
    ///   [`thematic_break`][crate::mdx_parser::construct::thematic_break]
    ///
    /// ## Example
    ///
    /// ```markdown
    /// > | * * *
    ///     ^ ^ ^
    /// ```
    ThematicBreakSequence,

    LinePrefix,
}

/// List of void events, used to make sure everything is working well.
pub const VOID_EVENTS: [Name; 76] = [
    Name::AttentionSequence,
    Name::AutolinkEmail,
    Name::AutolinkMarker,
    Name::AutolinkProtocol,
    Name::BlankLineEnding,
    Name::BlockQuoteMarker,
    Name::ByteOrderMark,
    Name::CharacterEscapeMarker,
    Name::CharacterEscapeValue,
    Name::CharacterReferenceMarker,
    Name::CharacterReferenceMarkerHexadecimal,
    Name::CharacterReferenceMarkerNumeric,
    Name::CharacterReferenceMarkerSemi,
    Name::CharacterReferenceValue,
    Name::CodeFencedFenceSequence,
    Name::CodeFlowChunk,
    Name::CodeTextData,
    Name::CodeTextSequence,
    Name::Data,
    Name::DefinitionDestinationLiteralMarker,
    Name::DefinitionLabelMarker,
    Name::DefinitionMarker,
    Name::DefinitionTitleMarker,
    Name::EmphasisSequence,
    Name::FrontmatterChunk,
    Name::GfmAutolinkLiteralEmail,
    Name::GfmAutolinkLiteralProtocol,
    Name::GfmAutolinkLiteralWww,
    Name::GfmFootnoteCallMarker,
    Name::GfmFootnoteDefinitionLabelMarker,
    Name::GfmFootnoteDefinitionMarker,
    Name::GfmStrikethroughSequence,
    Name::GfmTableCellDivider,
    Name::GfmTableDelimiterMarker,
    Name::GfmTableDelimiterFiller,
    Name::GfmTaskListItemMarker,
    Name::GfmTaskListItemValueChecked,
    Name::GfmTaskListItemValueUnchecked,
    Name::FrontmatterSequence,
    Name::HardBreakEscape,
    Name::HardBreakTrailing,
    Name::HeadingAtxSequence,
    Name::HeadingSetextUnderlineSequence,
    Name::HtmlFlowData,
    Name::HtmlTextData,
    Name::LabelImageMarker,
    Name::LabelMarker,
    Name::LineEnding,
    Name::ListItemMarker,
    Name::ListItemValue,
    Name::MathFlowFenceSequence,
    Name::MathFlowChunk,
    Name::MathTextData,
    Name::MathTextSequence,
    Name::MdxEsmData,
    Name::MdxExpressionMarker,
    Name::MdxExpressionData,
    Name::MdxJsxTagMarker,
    Name::MdxJsxTagClosingMarker,
    Name::MdxJsxTagNamePrimary,
    Name::MdxJsxTagNameMemberMarker,
    Name::MdxJsxTagNamePrefixMarker,
    Name::MdxJsxTagNameMember,
    Name::MdxJsxTagNameLocal,
    Name::MdxJsxTagSelfClosingMarker,
    Name::MdxJsxTagAttributeNamePrefixMarker,
    Name::MdxJsxTagAttributeInitializerMarker,
    Name::MdxJsxTagAttributeNameLocal,
    Name::MdxJsxTagAttributeValueLiteralMarker,
    Name::MdxJsxEsWhitespace,
    Name::ReferenceMarker,
    Name::ResourceMarker,
    Name::ResourceTitleMarker,
    Name::SpaceOrTab,
    Name::StrongSequence,
    Name::ThematicBreakSequence,
];

/// Embedded content type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Content {
    /// Represents [flow content][crate::mdx_parser::construct::flow].
    Flow,
    /// Represents [content][crate::mdx_parser::construct::content].
    #[allow(clippy::enum_variant_names)]
    Content,
    /// Represents [string content][crate::mdx_parser::construct::string].
    String,
    /// Represents [text content][crate::mdx_parser::construct::text].
    Text,
}

/// Link to another event.
#[derive(Clone, Debug)]
pub struct Link {
    /// Previous event.
    pub previous: Option<usize>,
    /// Next event.
    pub next: Option<usize>,
    /// Content type.
    pub content: Content,
}

/// Place in the document.
///
/// The interface for the location in the document comes from unist
/// [`Point`](https://github.com/syntax-tree/unist#point).
#[derive(Clone, Debug)]
pub struct Point {
    /// 1-indexed line number.
    pub line: usize,
    /// 1-indexed column number.
    ///
    /// This is increased up to a tab stop for tabs.
    /// Some editors count tabs as 1 character, so this position is not the
    /// same as editors.
    pub column: usize,
    /// 0-indexed position in the document.
    ///
    /// Also an `index` into `bytes`.
    pub index: usize,
    /// Virtual step on the same `index`.
    pub vs: usize,
}

impl Point {
    /// Create a unist point.
    pub fn to_unist(&self) -> unist::Point {
        unist::Point {
            line: self.line,
            column: self.column,
            offset: self.index,
        }
    }

    /// Create a new point, that is shifted from the close earlier current
    /// point, to `index`.
    pub fn shift_to(&self, bytes: &[u8], index: usize) -> Point {
        let mut next = self.clone();
        debug_assert!(index > next.index, "expected to shift forward");

        while next.index < index {
            match bytes[next.index] {
                b'\n' | b'\r' => unreachable!("cannot move past line endings"),
                b'\t' => {
                    let remainder = next.column % TAB_SIZE;
                    let vs = if remainder == 0 { 0 } else { TAB_SIZE - remainder };
                    next.index += 1;
                    next.column += 1 + vs;
                }
                _ => {
                    next.index += 1;
                    next.column += 1;
                }
            }
        }

        next
    }
}

/// Event kinds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Kind {
    /// The start of something.
    Enter,
    /// The end of something.
    Exit,
}

/// Something semantic happening somewhere.
#[derive(Clone, Debug)]
pub struct Event {
    /// Kind of event.
    pub kind: Kind,
    /// Name of event.
    pub name: Name,
    /// Place where this happens.
    pub point: Point,
    /// Link to another event.
    pub link: Option<Link>,
}
