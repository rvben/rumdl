use crate::mdx_parser::util::{
    line_ending::LineEnding,
    mdx::{EsmParse as MdxEsmParse, ExpressionParse as MdxExpressionParse},
};
use alloc::{boxed::Box, fmt, string::String};

/// Control which constructs are enabled.
///
/// Not all constructs can be configured.
/// Notably, blank lines and paragraphs cannot be turned off.
///
/// ## Examples
///
/// ```
/// use markdown::Constructs;
/// # fn main() {
///
/// // Use the default trait to get `CommonMark` constructs:
/// let commonmark = Constructs::default();
///
/// // To turn on all of GFM, use the `gfm` method:
/// let gfm = Constructs::gfm();
///
/// // Or, mix and match:
/// let custom = Constructs {
///   math_flow: true,
///   math_text: true,
///   ..Constructs::gfm()
/// };
/// # }
/// ```
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct Constructs {
    /// Attention.
    ///
    /// ```markdown
    /// > | a *b* c **d**.
    ///       ^^^   ^^^^^
    /// ```
    pub attention: bool,
    /// Autolink.
    ///
    /// ```markdown
    /// > | a <https://example.com> b <user@example.org>.
    ///       ^^^^^^^^^^^^^^^^^^^^^   ^^^^^^^^^^^^^^^^^^
    /// ```
    pub autolink: bool,
    /// Block quote.
    ///
    /// ```markdown
    /// > | > a
    ///     ^^^
    /// ```
    pub block_quote: bool,
    /// Character escape.
    ///
    /// ```markdown
    /// > | a \* b
    ///       ^^
    /// ```
    pub character_escape: bool,
    /// Character reference.
    ///
    /// ```markdown
    /// > | a &amp; b
    ///       ^^^^^
    /// ```
    pub character_reference: bool,
    /// Code (indented).
    ///
    /// ```markdown
    /// > |     a
    ///     ^^^^^
    /// ```
    pub code_indented: bool,
    /// Code (fenced).
    ///
    /// ```markdown
    /// > | ~~~js
    ///     ^^^^^
    /// > | console.log(1)
    ///     ^^^^^^^^^^^^^^
    /// > | ~~~
    ///     ^^^
    /// ```
    pub code_fenced: bool,
    /// Code (text).
    ///
    /// ```markdown
    /// > | a `b` c
    ///       ^^^
    /// ```
    pub code_text: bool,
    /// Definition.
    ///
    /// ```markdown
    /// > | [a]: b "c"
    ///     ^^^^^^^^^^
    /// ```
    pub definition: bool,
    /// Frontmatter.
    ///
    /// ````markdown
    /// > | ---
    ///     ^^^
    /// > | title: Neptune
    ///     ^^^^^^^^^^^^^^
    /// > | ---
    ///     ^^^
    /// ````
    pub frontmatter: bool,
    /// GFM: autolink literal.
    ///
    /// ```markdown
    /// > | https://example.com
    ///     ^^^^^^^^^^^^^^^^^^^
    /// ```
    pub gfm_autolink_literal: bool,
    /// GFM: footnote definition.
    ///
    /// ```markdown
    /// > | [^a]: b
    ///     ^^^^^^^
    /// ```
    pub gfm_footnote_definition: bool,
    /// GFM: footnote label start.
    ///
    /// ```markdown
    /// > | a[^b]
    ///      ^^
    /// ```
    pub gfm_label_start_footnote: bool,
    ///
    /// ```markdown
    /// > | a ~b~ c.
    ///       ^^^
    /// ```
    pub gfm_strikethrough: bool,
    /// GFM: table.
    ///
    /// ```markdown
    /// > | | a |
    ///     ^^^^^
    /// > | | - |
    ///     ^^^^^
    /// > | | b |
    ///     ^^^^^
    /// ```
    pub gfm_table: bool,
    /// GFM: task list item.
    ///
    /// ```markdown
    /// > | * [x] y.
    ///       ^^^
    /// ```
    pub gfm_task_list_item: bool,
    /// Hard break (escape).
    ///
    /// ```markdown
    /// > | a\
    ///      ^
    ///   | b
    /// ```
    pub hard_break_escape: bool,
    /// Hard break (trailing).
    ///
    /// ```markdown
    /// > | a␠␠
    ///      ^^
    ///   | b
    /// ```
    pub hard_break_trailing: bool,
    /// Heading (atx).
    ///
    /// ```markdown
    /// > | # a
    ///     ^^^
    /// ```
    pub heading_atx: bool,
    /// Heading (setext).
    ///
    /// ```markdown
    /// > | a
    ///     ^^
    /// > | ==
    ///     ^^
    /// ```
    pub heading_setext: bool,
    /// HTML (flow).
    ///
    /// ```markdown
    /// > | <div>
    ///     ^^^^^
    /// ```
    pub html_flow: bool,
    /// HTML (text).
    ///
    /// ```markdown
    /// > | a <b> c
    ///       ^^^
    /// ```
    pub html_text: bool,
    /// Label start (image).
    ///
    /// ```markdown
    /// > | a ![b](c) d
    ///       ^^
    /// ```
    pub label_start_image: bool,
    /// Label start (link).
    ///
    /// ```markdown
    /// > | a [b](c) d
    ///       ^
    /// ```
    pub label_start_link: bool,
    /// Label end.
    ///
    /// ```markdown
    /// > | a [b](c) d
    ///         ^^^^
    /// ```
    pub label_end: bool,
    /// List items.
    ///
    /// ```markdown
    /// > | * a
    ///     ^^^
    /// ```
    pub list_item: bool,
    /// Math (flow).
    ///
    /// ```markdown
    /// > | $$
    ///     ^^
    /// > | \frac{1}{2}
    ///     ^^^^^^^^^^^
    /// > | $$
    ///     ^^
    /// ```
    pub math_flow: bool,
    /// Math (text).
    ///
    /// ```markdown
    /// > | a $b$ c
    ///       ^^^
    /// ```
    pub math_text: bool,
    /// MDX: ESM.
    ///
    /// ```markdown
    /// > | import a from 'b'
    ///     ^^^^^^^^^^^^^^^^^
    /// ```
    ///
    /// > 👉 **Note**: to support ESM, you *must* pass
    /// > [`mdx_esm_parse`][MdxEsmParse] in [`ParseOptions`][] too.
    /// > Otherwise, ESM is treated as normal markdown.
    pub mdx_esm: bool,
    /// MDX: expression (flow).
    ///
    /// ```markdown
    /// > | {Math.PI}
    ///     ^^^^^^^^^
    /// ```
    ///
    /// > 👉 **Note**: You *can* pass
    /// > [`mdx_expression_parse`][MdxExpressionParse] in [`ParseOptions`][]
    /// > too, to parse expressions according to a certain grammar (typically,
    /// > a programming language).
    /// > Otherwise, expressions are parsed with a basic algorithm that only
    /// > cares about braces.
    pub mdx_expression_flow: bool,
    /// MDX: expression (text).
    ///
    /// ```markdown
    /// > | a {Math.PI} c
    ///       ^^^^^^^^^
    /// ```
    ///
    /// > 👉 **Note**: You *can* pass
    /// > [`mdx_expression_parse`][MdxExpressionParse] in [`ParseOptions`][]
    /// > too, to parse expressions according to a certain grammar (typically,
    /// > a programming language).
    /// > Otherwise, expressions are parsed with a basic algorithm that only
    /// > cares about braces.
    pub mdx_expression_text: bool,
    /// MDX: JSX (flow).
    ///
    /// ```markdown
    /// > | <Component />
    ///     ^^^^^^^^^^^^^
    /// ```
    ///
    /// > 👉 **Note**: You *must* pass `html_flow: false` to use this,
    /// > as it’s preferred when on over `mdx_jsx_flow`.
    ///
    /// > 👉 **Note**: You *can* pass
    /// > [`mdx_expression_parse`][MdxExpressionParse] in [`ParseOptions`][]
    /// > too, to parse expressions in JSX according to a certain grammar
    /// > (typically, a programming language).
    /// > Otherwise, expressions are parsed with a basic algorithm that only
    /// > cares about braces.
    pub mdx_jsx_flow: bool,
    /// MDX: JSX (text).
    ///
    /// ```markdown
    /// > | a <Component /> c
    ///       ^^^^^^^^^^^^^
    /// ```
    ///
    /// > 👉 **Note**: You *must* pass `html_text: false` to use this,
    /// > as it’s preferred when on over `mdx_jsx_text`.
    ///
    /// > 👉 **Note**: You *can* pass
    /// > [`mdx_expression_parse`][MdxExpressionParse] in [`ParseOptions`][]
    /// > too, to parse expressions in JSX according to a certain grammar
    /// > (typically, a programming language).
    /// > Otherwise, expressions are parsed with a basic algorithm that only
    /// > cares about braces.
    pub mdx_jsx_text: bool,
    /// Thematic break.
    ///
    /// ```markdown
    /// > | ***
    ///     ^^^
    /// ```
    pub thematic_break: bool,
}

impl Default for Constructs {
    /// `CommonMark`.
    ///
    /// `CommonMark` is a relatively strong specification of how markdown
    /// works.
    /// Most markdown parsers try to follow it.
    ///
    /// For more information, see the `CommonMark` specification:
    /// <https://spec.commonmark.org>.
    fn default() -> Self {
        Self {
            attention: true,
            autolink: true,
            block_quote: true,
            character_escape: true,
            character_reference: true,
            code_indented: true,
            code_fenced: true,
            code_text: true,
            definition: true,
            frontmatter: false,
            gfm_autolink_literal: false,
            gfm_label_start_footnote: false,
            gfm_footnote_definition: false,
            gfm_strikethrough: false,
            gfm_table: false,
            gfm_task_list_item: false,
            hard_break_escape: true,
            hard_break_trailing: true,
            heading_atx: true,
            heading_setext: true,
            html_flow: true,
            html_text: true,
            label_start_image: true,
            label_start_link: true,
            label_end: true,
            list_item: true,
            math_flow: false,
            math_text: false,
            mdx_esm: false,
            mdx_expression_flow: false,
            mdx_expression_text: false,
            mdx_jsx_flow: false,
            mdx_jsx_text: false,
            thematic_break: true,
        }
    }
}

impl Constructs {
    /// GFM.
    ///
    /// GFM stands for **GitHub flavored markdown**.
    /// GFM extends `CommonMark` and adds support for autolink literals,
    /// footnotes, strikethrough, tables, and tasklists.
    ///
    /// For more information, see the GFM specification:
    /// <https://github.github.com/gfm/>.
    pub fn gfm() -> Self {
        Self {
            gfm_autolink_literal: true,
            gfm_footnote_definition: true,
            gfm_label_start_footnote: true,
            gfm_strikethrough: true,
            gfm_table: true,
            gfm_task_list_item: true,
            ..Self::default()
        }
    }

    /// MDX.
    ///
    /// This turns on `CommonMark`, turns off some conflicting constructs
    /// (autolinks, code (indented), and HTML), and turns on MDX (ESM,
    /// expressions, and JSX).
    ///
    /// For more information, see the MDX website:
    /// <https://mdxjs.com>.
    ///
    /// > 👉 **Note**: to support ESM, you *must* pass
    /// > [`mdx_esm_parse`][MdxEsmParse] in [`ParseOptions`][] too.
    /// > Otherwise, ESM is treated as normal markdown.
    /// >
    /// > You *can* pass
    /// > [`mdx_expression_parse`][MdxExpressionParse]
    /// > to parse expressions according to a certain grammar (typically, a
    /// > programming language).
    /// > Otherwise, expressions are parsed with a basic algorithm that only
    /// > cares about braces.
    pub fn mdx() -> Self {
        Self {
            autolink: false,
            code_indented: false,
            html_flow: false,
            html_text: false,
            mdx_esm: true,
            mdx_expression_flow: true,
            mdx_expression_text: true,
            mdx_jsx_flow: true,
            mdx_jsx_text: true,
            ..Self::default()
        }
    }
}

/// Configuration that describes how to compile to HTML.
///
/// You likely either want to turn on the dangerous options
/// (`allow_dangerous_html`, `allow_dangerous_protocol`) when dealing with
/// input you trust, or want to customize how GFM footnotes are compiled
/// (typically because the input markdown is not in English).
///
/// ## Examples
///
/// ```
/// use markdown::CompileOptions;
/// # fn main() {
///
/// // Use the default trait to get safe defaults:
/// let safe = CompileOptions::default();
///
/// // Live dangerously / trust the author:
/// let danger = CompileOptions {
///   allow_dangerous_html: true,
///   allow_dangerous_protocol: true,
///   ..CompileOptions::default()
/// };
///
/// // In French:
/// let enFrançais = CompileOptions {
///   gfm_footnote_back_label: Some("Arrière".into()),
///   gfm_footnote_label: Some("Notes de bas de page".into()),
///   ..CompileOptions::default()
/// };
/// # }
/// ```
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default)]
#[cfg_attr(
    any(),
    derive(serde::Serialize, serde::Deserialize),
    serde(default, rename_all = "camelCase")
)]
pub struct CompileOptions {
    /// Whether to allow all values in images.
    ///
    /// The default is `false`,
    /// which lets `allow_dangerous_protocol` control protocol safety for
    /// both links and images.
    ///
    /// Pass `true` to allow all values as `src` on images,
    /// regardless of `allow_dangerous_protocol`.
    /// This is safe because the
    /// [HTML specification][whatwg-html-image-processing]
    /// does not allow executable code in images.
    ///
    /// [whatwg-html-image-processing]: https://html.spec.whatwg.org/multipage/images.html#images-processing-model
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html_with_options, CompileOptions, Options};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // By default, some protocols in image sources are dropped:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "![](data:image/gif;base64,R0lGODlhAQABAAAAACH5BAEKAAEALAAAAAABAAEAAAICTAEAOw==)",
    ///         &Options::default()
    ///     )?,
    ///     "<p><img src=\"\" alt=\"\" /></p>"
    /// );
    ///
    /// // Turn `allow_any_img_src` on to allow all values as `src` on images.
    /// // This is safe because browsers do not execute code in images.
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "![](javascript:alert(1))",
    ///         &Options {
    ///             compile: CompileOptions {
    ///               allow_any_img_src: true,
    ///               ..CompileOptions::default()
    ///             },
    ///             ..Options::default()
    ///         }
    ///     )?,
    ///     "<p><img src=\"javascript:alert(1)\" alt=\"\" /></p>"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub allow_any_img_src: bool,

    /// Whether to allow (dangerous) HTML.
    ///
    /// The default is `false`, which still parses the HTML according to
    /// `CommonMark` but shows the HTML as text instead of as elements.
    ///
    /// Pass `true` for trusted content to get actual HTML elements.
    ///
    /// When using GFM, make sure to also turn off `gfm_tagfilter`.
    /// Otherwise, some dangerous HTML is still ignored.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html, to_html_with_options, CompileOptions, Options};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // `markdown-rs` is safe by default:
    /// assert_eq!(
    ///     to_html("Hi, <i>venus</i>!"),
    ///     "<p>Hi, &lt;i&gt;venus&lt;/i&gt;!</p>"
    /// );
    ///
    /// // Turn `allow_dangerous_html` on to allow potentially dangerous HTML:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "Hi, <i>venus</i>!",
    ///         &Options {
    ///             compile: CompileOptions {
    ///               allow_dangerous_html: true,
    ///               ..CompileOptions::default()
    ///             },
    ///             ..Options::default()
    ///         }
    ///     )?,
    ///     "<p>Hi, <i>venus</i>!</p>"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub allow_dangerous_html: bool,

    /// Whether to allow dangerous protocols in links and images.
    ///
    /// The default is `false`, which drops URLs in links and images that use
    /// dangerous protocols.
    ///
    /// Pass `true` for trusted content to support all protocols.
    ///
    /// URLs that have no protocol (which means it’s relative to the current
    /// page, such as `./some/page.html`) and URLs that have a safe protocol
    /// (for images: `http`, `https`; for links: `http`, `https`, `irc`,
    /// `ircs`, `mailto`, `xmpp`), are safe.
    /// All other URLs are dangerous and dropped.
    ///
    /// When the option `allow_all_protocols_in_img` is enabled,
    /// `allow_dangerous_protocol` only applies to links.
    ///
    /// This is safe because the
    /// [HTML specification][whatwg-html-image-processing]
    /// does not allow executable code in images.
    /// All modern browsers respect this.
    ///
    /// [whatwg-html-image-processing]: https://html.spec.whatwg.org/multipage/images.html#images-processing-model
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html, to_html_with_options, CompileOptions, Options};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // `markdown-rs` is safe by default:
    /// assert_eq!(
    ///     to_html("<javascript:alert(1)>"),
    ///     "<p><a href=\"\">javascript:alert(1)</a></p>"
    /// );
    ///
    /// // Turn `allow_dangerous_protocol` on to allow potentially dangerous protocols:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "<javascript:alert(1)>",
    ///         &Options {
    ///             compile: CompileOptions {
    ///               allow_dangerous_protocol: true,
    ///               ..CompileOptions::default()
    ///             },
    ///             ..Options::default()
    ///         }
    ///     )?,
    ///     "<p><a href=\"javascript:alert(1)\">javascript:alert(1)</a></p>"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub allow_dangerous_protocol: bool,

    // To do: `doc_markdown` is broken.
    #[allow(clippy::doc_markdown)]
    /// Default line ending to use when compiling to HTML, for line endings not
    /// in `value`.
    ///
    /// Generally, `markdown-rs` copies line endings (`\r`, `\n`, `\r\n`) in
    /// the markdown document over to the compiled HTML.
    /// In some cases, such as `> a`, CommonMark requires that extra line
    /// endings are added: `<blockquote>\n<p>a</p>\n</blockquote>`.
    ///
    /// To create that line ending, the document is checked for the first line
    /// ending that is used.
    /// If there is no line ending, `default_line_ending` is used.
    /// If that isn’t configured, `\n` is used.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html, to_html_with_options, CompileOptions, LineEnding, Options};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // `markdown-rs` uses `\n` by default:
    /// assert_eq!(
    ///     to_html("> a"),
    ///     "<blockquote>\n<p>a</p>\n</blockquote>"
    /// );
    ///
    /// // Define `default_line_ending` to configure the default:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "> a",
    ///         &Options {
    ///             compile: CompileOptions {
    ///               default_line_ending: LineEnding::CarriageReturnLineFeed,
    ///               ..CompileOptions::default()
    ///             },
    ///             ..Options::default()
    ///         }
    ///     )?,
    ///     "<blockquote>\r\n<p>a</p>\r\n</blockquote>"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub default_line_ending: LineEnding,

    /// Textual label to describe the backreference back to footnote calls.
    ///
    /// The default value is `"Back to content"`.
    /// Change it when the markdown is not in English.
    ///
    /// This label is used in the `aria-label` attribute on each backreference
    /// (the `↩` links).
    /// It affects users of assistive technology.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html_with_options, CompileOptions, Options, ParseOptions};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // `"Back to content"` is used by default:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "[^a]\n\n[^a]: b",
    ///         &Options::gfm()
    ///     )?,
    ///     "<p><sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>b <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n"
    /// );
    ///
    /// // Pass `gfm_footnote_back_label` to use something else:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "[^a]\n\n[^a]: b",
    ///         &Options {
    ///             parse: ParseOptions::gfm(),
    ///             compile: CompileOptions {
    ///               gfm_footnote_back_label: Some("Arrière".into()),
    ///               ..CompileOptions::gfm()
    ///             }
    ///         }
    ///     )?,
    ///     "<p><sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>b <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Arrière\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub gfm_footnote_back_label: Option<String>,

    /// Prefix to use before the `id` attribute on footnotes to prevent them
    /// from *clobbering*.
    ///
    /// The default is `"user-content-"`.
    /// Pass `Some("".into())` for trusted markdown and when you are careful
    /// with polyfilling.
    /// You could pass a different prefix.
    ///
    /// DOM clobbering is this:
    ///
    /// ```html
    /// <p id="x"></p>
    /// <script>alert(x) // `x` now refers to the `p#x` DOM element</script>
    /// ```
    ///
    /// The above example shows that elements are made available by browsers,
    /// by their ID, on the `window` object.
    /// This is a security risk because you might be expecting some other
    /// variable at that place.
    /// It can also break polyfills.
    /// Using a prefix solves these problems.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html_with_options, CompileOptions, Options, ParseOptions};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // `"user-content-"` is used by default:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "[^a]\n\n[^a]: b",
    ///         &Options::gfm()
    ///     )?,
    ///     "<p><sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>b <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n"
    /// );
    ///
    /// // Pass `gfm_footnote_clobber_prefix` to use something else:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "[^a]\n\n[^a]: b",
    ///         &Options {
    ///             parse: ParseOptions::gfm(),
    ///             compile: CompileOptions {
    ///               gfm_footnote_clobber_prefix: Some("".into()),
    ///               ..CompileOptions::gfm()
    ///             }
    ///         }
    ///     )?,
    ///     "<p><sup><a href=\"#fn-a\" id=\"fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"fn-a\">\n<p>b <a href=\"#fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub gfm_footnote_clobber_prefix: Option<String>,

    /// Attributes to use on the footnote label.
    ///
    /// The default value is `"class=\"sr-only\""`.
    /// Change it to show the label and add other attributes.
    ///
    /// This label is typically hidden visually (assuming a `sr-only` CSS class
    /// is defined that does that), and thus affects screen readers only.
    /// If you do have such a class, but want to show this section to everyone,
    /// pass an empty string.
    /// You can also add different attributes.
    ///
    /// > 👉 **Note**: `id="footnote-label"` is always added, because footnote
    /// > calls use it with `aria-describedby` to provide an accessible label.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html_with_options, CompileOptions, Options, ParseOptions};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // `"class=\"sr-only\""` is used by default:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "[^a]\n\n[^a]: b",
    ///         &Options::gfm()
    ///     )?,
    ///     "<p><sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>b <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n"
    /// );
    ///
    /// // Pass `gfm_footnote_label_attributes` to use something else:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "[^a]\n\n[^a]: b",
    ///         &Options {
    ///             parse: ParseOptions::gfm(),
    ///             compile: CompileOptions {
    ///               gfm_footnote_label_attributes: Some("class=\"footnote-heading\"".into()),
    ///               ..CompileOptions::gfm()
    ///             }
    ///         }
    ///     )?,
    ///     "<p><sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"footnote-heading\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>b <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub gfm_footnote_label_attributes: Option<String>,

    /// HTML tag name to use for the footnote label element.
    ///
    /// The default value is `"h2"`.
    /// Change it to match your document structure.
    ///
    /// This label is typically hidden visually (assuming a `sr-only` CSS class
    /// is defined that does that), and thus affects screen readers only.
    /// If you do have such a class, but want to show this section to everyone,
    /// pass different attributes with the `gfm_footnote_label_attributes`
    /// option.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html_with_options, CompileOptions, Options, ParseOptions};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // `"h2"` is used by default:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "[^a]\n\n[^a]: b",
    ///         &Options::gfm()
    ///     )?,
    ///     "<p><sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>b <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n"
    /// );
    ///
    /// // Pass `gfm_footnote_label_tag_name` to use something else:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "[^a]\n\n[^a]: b",
    ///         &Options {
    ///             parse: ParseOptions::gfm(),
    ///             compile: CompileOptions {
    ///               gfm_footnote_label_tag_name: Some("h1".into()),
    ///               ..CompileOptions::gfm()
    ///             }
    ///         }
    ///     )?,
    ///     "<p><sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h1 id=\"footnote-label\" class=\"sr-only\">Footnotes</h1>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>b <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub gfm_footnote_label_tag_name: Option<String>,

    /// Textual label to use for the footnotes section.
    ///
    /// The default value is `"Footnotes"`.
    /// Change it when the markdown is not in English.
    ///
    /// This label is typically hidden visually (assuming a `sr-only` CSS class
    /// is defined that does that), and thus affects screen readers only.
    /// If you do have such a class, but want to show this section to everyone,
    /// pass different attributes with the `gfm_footnote_label_attributes`
    /// option.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html_with_options, CompileOptions, Options, ParseOptions};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // `"Footnotes"` is used by default:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "[^a]\n\n[^a]: b",
    ///         &Options::gfm()
    ///     )?,
    ///     "<p><sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>b <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n"
    /// );
    ///
    /// // Pass `gfm_footnote_label` to use something else:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "[^a]\n\n[^a]: b",
    ///         &Options {
    ///             parse: ParseOptions::gfm(),
    ///             compile: CompileOptions {
    ///               gfm_footnote_label: Some("Notes de bas de page".into()),
    ///               ..CompileOptions::gfm()
    ///             }
    ///         }
    ///     )?,
    ///     "<p><sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Notes de bas de page</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>b <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub gfm_footnote_label: Option<String>,

    /// Whether or not GFM task list html `<input>` items are enabled.
    ///
    /// This determines whether or not the user of the browser is able
    /// to click and toggle generated checkbox items. The default is false.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html_with_options, CompileOptions, Options, ParseOptions};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // With `gfm_task_list_item_checkable`, generated `<input type="checkbox" />`
    /// // tags do not contain the attribute `disabled=""` and are thus toggleable by
    /// // browser users.
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "* [x] y.",
    ///         &Options {
    ///             parse: ParseOptions::gfm(),
    ///             compile: CompileOptions {
    ///                 gfm_task_list_item_checkable: true,
    ///                 ..CompileOptions::gfm()
    ///             }
    ///         }
    ///     )?,
    ///     "<ul>\n<li><input type=\"checkbox\" checked=\"\" /> y.</li>\n</ul>"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub gfm_task_list_item_checkable: bool,

    /// Whether to support the GFM tagfilter.
    ///
    /// This option does nothing if `allow_dangerous_html` is not turned on.
    /// The default is `false`, which does not apply the GFM tagfilter to HTML.
    /// Pass `true` for output that is a bit closer to GitHub’s actual output.
    ///
    /// The tagfilter is kinda weird and kinda useless.
    /// The tag filter is a naïve attempt at XSS protection.
    /// You should use a proper HTML sanitizing algorithm instead.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html_with_options, CompileOptions, Options, ParseOptions};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // With `allow_dangerous_html`, `markdown-rs` passes HTML through untouched:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "<iframe>",
    ///         &Options {
    ///             parse: ParseOptions::gfm(),
    ///             compile: CompileOptions {
    ///               allow_dangerous_html: true,
    ///               ..CompileOptions::default()
    ///             }
    ///         }
    ///     )?,
    ///     "<iframe>"
    /// );
    ///
    /// // Pass `gfm_tagfilter: true` to make some of that safe:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "<iframe>",
    ///         &Options {
    ///             parse: ParseOptions::gfm(),
    ///             compile: CompileOptions {
    ///               allow_dangerous_html: true,
    ///               gfm_tagfilter: true,
    ///               ..CompileOptions::default()
    ///             }
    ///         }
    ///     )?,
    ///     "&lt;iframe>"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## References
    ///
    /// * [*§ 6.1 Disallowed Raw HTML (extension)* in GFM](https://github.github.com/gfm/#disallowed-raw-html-extension-)
    /// * [`cmark-gfm#extensions/tagfilter.c`](https://github.com/github/cmark-gfm/blob/master/extensions/tagfilter.c)
    pub gfm_tagfilter: bool,
}

impl CompileOptions {
    /// GFM.
    ///
    /// GFM stands for **GitHub flavored markdown**.
    /// On the compilation side, GFM turns on the GFM tag filter.
    /// The tagfilter is useless, but it’s included here for consistency, and
    /// this method exists for parity to parse options.
    ///
    /// For more information, see the GFM specification:
    /// <https://github.github.com/gfm/>.
    pub fn gfm() -> Self {
        Self {
            gfm_tagfilter: true,
            ..Self::default()
        }
    }
}

/// Configuration that describes how to parse from markdown.
///
/// You can use this:
///
/// * To control what markdown constructs are turned on and off
/// * To control some of those constructs
/// * To add support for certain programming languages when parsing MDX
///
/// In most cases, you will want to use the default trait or `gfm` method.
///
/// ## Examples
///
/// ```
/// use markdown::ParseOptions;
/// # fn main() {
///
/// // Use the default trait to parse markdown according to `CommonMark`:
/// let commonmark = ParseOptions::default();
///
/// // Use the `gfm` method to parse markdown according to GFM:
/// let gfm = ParseOptions::gfm();
/// # }
/// ```
#[allow(clippy::struct_excessive_bools)]
#[cfg_attr(
    any(),
    derive(serde::Serialize, serde::Deserialize),
    serde(default, rename_all = "camelCase")
)]
pub struct ParseOptions {
    // Note: when adding fields, don’t forget to add them to `fmt::Debug` below.
    /// Which constructs to enable and disable.
    ///
    /// The default is to follow `CommonMark`.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html, to_html_with_options, Constructs, Options, ParseOptions};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // `markdown-rs` follows CommonMark by default:
    /// assert_eq!(
    ///     to_html("    indented code?"),
    ///     "<pre><code>indented code?\n</code></pre>"
    /// );
    ///
    /// // Pass `constructs` to choose what to enable and disable:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "    indented code?",
    ///         &Options {
    ///             parse: ParseOptions {
    ///               constructs: Constructs {
    ///                 code_indented: false,
    ///                 ..Constructs::default()
    ///               },
    ///               ..ParseOptions::default()
    ///             },
    ///             ..Options::default()
    ///         }
    ///     )?,
    ///     "<p>indented code?</p>"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(any(), serde(default))]
    pub constructs: Constructs,

    /// Whether to support GFM strikethrough with a single tilde
    ///
    /// This option does nothing if `gfm_strikethrough` is not turned on in
    /// `constructs`.
    /// This option does not affect strikethrough with double tildes.
    ///
    /// The default is `true`, which follows how markdown on `github.com`
    /// works, as strikethrough with single tildes is supported.
    /// Pass `false`, to follow the GFM spec more strictly, by not allowing
    /// strikethrough with single tildes.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html_with_options, Constructs, Options, ParseOptions};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // `markdown-rs` supports single tildes by default:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "~a~",
    ///         &Options {
    ///             parse: ParseOptions {
    ///               constructs: Constructs::gfm(),
    ///               ..ParseOptions::default()
    ///             },
    ///             ..Options::default()
    ///         }
    ///     )?,
    ///     "<p><del>a</del></p>"
    /// );
    ///
    /// // Pass `gfm_strikethrough_single_tilde: false` to turn that off:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "~a~",
    ///         &Options {
    ///             parse: ParseOptions {
    ///               constructs: Constructs::gfm(),
    ///               gfm_strikethrough_single_tilde: false,
    ///               ..ParseOptions::default()
    ///             },
    ///             ..Options::default()
    ///         }
    ///     )?,
    ///     "<p>~a~</p>"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(any(), serde(default))]
    pub gfm_strikethrough_single_tilde: bool,

    /// Whether to support math (text) with a single dollar
    ///
    /// This option does nothing if `math_text` is not turned on in
    /// `constructs`.
    /// This option does not affect math (text) with two or more dollars.
    ///
    /// The default is `true`, which is more close to how code (text) and
    /// Pandoc work, as it allows math with a single dollar to form.
    /// However, single dollars can interfere with “normal” dollars in text.
    /// Pass `false`, to only allow math (text) to form when two or more
    /// dollars are used.
    /// If you pass `false`, you can still use two or more dollars for text
    /// math.
    ///
    /// ## Examples
    ///
    /// ```
    /// use markdown::{to_html_with_options, Constructs, Options, ParseOptions};
    /// # fn main() -> Result<(), markdown::message::Message> {
    ///
    /// // `markdown-rs` supports single dollars by default:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "$a$",
    ///         &Options {
    ///             parse: ParseOptions {
    ///               constructs: Constructs {
    ///                 math_text: true,
    ///                 ..Constructs::default()
    ///               },
    ///               ..ParseOptions::default()
    ///             },
    ///             ..Options::default()
    ///         }
    ///     )?,
    ///     "<p><code class=\"language-math math-inline\">a</code></p>"
    /// );
    ///
    /// // Pass `math_text_single_dollar: false` to turn that off:
    /// assert_eq!(
    ///     to_html_with_options(
    ///         "$a$",
    ///         &Options {
    ///             parse: ParseOptions {
    ///               constructs: Constructs {
    ///                 math_text: true,
    ///                 ..Constructs::default()
    ///               },
    ///               math_text_single_dollar: false,
    ///               ..ParseOptions::default()
    ///             },
    ///             ..Options::default()
    ///         }
    ///     )?,
    ///     "<p>$a$</p>"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(any(), serde(default))]
    pub math_text_single_dollar: bool,

    /// Function to parse expressions with.
    ///
    /// This function can be used to add support for arbitrary programming
    /// languages within expressions.
    ///
    /// It only makes sense to pass this when compiling to a syntax tree
    /// with [`to_mdast()`][crate::mdx_parser::to_mdast()].
    ///
    /// For an example that adds support for JavaScript with SWC, see
    /// `tests/test_utils/mod.rs`.
    #[cfg_attr(any(), serde(skip))]
    pub mdx_expression_parse: Option<Box<MdxExpressionParse>>,

    /// Function to parse ESM with.
    ///
    /// This function can be used to add support for arbitrary programming
    /// languages within ESM blocks, however, the keywords (`export`,
    /// `import`) are currently hardcoded JavaScript-specific.
    ///
    /// > 👉 **Note**: please raise an issue if you’re interested in working on
    /// > MDX that is aware of, say, Rust, or other programming languages.
    ///
    /// It only makes sense to pass this when compiling to a syntax tree
    /// with [`to_mdast()`][crate::mdx_parser::to_mdast()].
    ///
    /// For an example that adds support for JavaScript with SWC, see
    /// `tests/test_utils/mod.rs`.
    #[cfg_attr(any(), serde(skip))]
    pub mdx_esm_parse: Option<Box<MdxEsmParse>>,

    /// How much of expressions and ESM may be read, as a multiple of the size
    /// of the document.
    ///
    /// Finding where an expression or ESM ends can read it again and again,
    /// which grows with the square of its size.
    /// With `Some(k)`, parsing fails with `mdx-parse-budget` past `k` times
    /// the size of the document.
    ///
    /// With the parse functions above, each `}` reads the expression so far
    /// again: an expression with an array of 100 small objects needs 51.
    ///
    /// The default, `None`, sets no limit.
    #[cfg_attr(any(), serde(skip_serializing_if = "Option::is_none"))]
    pub mdx_parse_budget_factor: Option<usize>,
    // Note: when adding fields, don’t forget to add them to `fmt::Debug` below.
}

impl fmt::Debug for ParseOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseOptions")
            .field("constructs", &self.constructs)
            .field("gfm_strikethrough_single_tilde", &self.gfm_strikethrough_single_tilde)
            .field("math_text_single_dollar", &self.math_text_single_dollar)
            .field(
                "mdx_expression_parse",
                &self.mdx_expression_parse.as_ref().map(|_d| "[Function]"),
            )
            .field("mdx_esm_parse", &self.mdx_esm_parse.as_ref().map(|_d| "[Function]"))
            .field("mdx_parse_budget_factor", &self.mdx_parse_budget_factor)
            .finish()
    }
}

impl Default for ParseOptions {
    /// `CommonMark` defaults.
    fn default() -> Self {
        Self {
            constructs: Constructs::default(),
            gfm_strikethrough_single_tilde: true,
            math_text_single_dollar: true,
            mdx_expression_parse: None,
            mdx_esm_parse: None,
            mdx_parse_budget_factor: None,
        }
    }
}

impl ParseOptions {
    /// GFM.
    ///
    /// GFM stands for GitHub flavored markdown.
    /// GFM extends `CommonMark` and adds support for autolink literals,
    /// footnotes, strikethrough, tables, and tasklists.
    ///
    /// For more information, see the GFM specification:
    /// <https://github.github.com/gfm/>
    pub fn gfm() -> Self {
        Self {
            constructs: Constructs::gfm(),
            ..Self::default()
        }
    }

    /// MDX.
    ///
    /// This turns on `CommonMark`, turns off some conflicting constructs
    /// (autolinks, code (indented), and HTML), and turns on MDX (ESM,
    /// expressions, and JSX).
    ///
    /// For more information, see the MDX website:
    /// <https://mdxjs.com>.
    ///
    /// > 👉 **Note**: to support ESM, you *must* pass
    /// > [`mdx_esm_parse`][MdxEsmParse] in [`ParseOptions`][] too.
    /// > Otherwise, ESM is treated as normal markdown.
    /// >
    /// > You *can* pass
    /// > [`mdx_expression_parse`][MdxExpressionParse]
    /// > to parse expressions according to a certain grammar (typically, a
    /// > programming language).
    /// > Otherwise, expressions are parsed with a basic algorithm that only
    /// > cares about braces.
    pub fn mdx() -> Self {
        Self {
            constructs: Constructs::mdx(),
            ..Self::default()
        }
    }
}

/// Configuration that describes how to parse from markdown and compile to
/// HTML.
///
/// In most cases, you will want to use the default trait or `gfm` method.
///
/// ## Examples
///
/// ```
/// use markdown::Options;
/// # fn main() {
///
/// // Use the default trait to compile markdown to HTML according to `CommonMark`:
/// let commonmark = Options::default();
///
/// // Use the `gfm` method to compile markdown to HTML according to GFM:
/// let gfm = Options::gfm();
/// # }
/// ```
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize), serde(default))]
pub struct Options {
    /// Configuration that describes how to parse from markdown.
    pub parse: ParseOptions,
    /// Configuration that describes how to compile to HTML.
    pub compile: CompileOptions,
}

impl Options {
    /// GFM.
    ///
    /// GFM stands for GitHub flavored markdown.
    /// GFM extends `CommonMark` and adds support for autolink literals,
    /// footnotes, strikethrough, tables, and tasklists.
    /// On the compilation side, GFM turns on the GFM tag filter.
    /// The tagfilter is useless, but it’s included here for consistency.
    ///
    /// For more information, see the GFM specification:
    /// <https://github.github.com/gfm/>
    pub fn gfm() -> Self {
        Self {
            parse: ParseOptions::gfm(),
            compile: CompileOptions::gfm(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdx_parser::util::mdx::Signal;
    use alloc::format;

    #[test]
    fn test_constructs() {
        Constructs::default();
        Constructs::gfm();
        Constructs::mdx();

        let constructs = Constructs::default();
        assert!(constructs.attention, "should default to `CommonMark` (1)");
        assert!(!constructs.gfm_autolink_literal, "should default to `CommonMark` (2)");
        assert!(!constructs.mdx_jsx_flow, "should default to `CommonMark` (3)");
        assert!(!constructs.frontmatter, "should default to `CommonMark` (4)");

        let constructs = Constructs::gfm();
        assert!(constructs.attention, "should support `gfm` shortcut (1)");
        assert!(constructs.gfm_autolink_literal, "should support `gfm` shortcut (2)");
        assert!(!constructs.mdx_jsx_flow, "should support `gfm` shortcut (3)");
        assert!(!constructs.frontmatter, "should support `gfm` shortcut (4)");

        let constructs = Constructs::mdx();
        assert!(constructs.attention, "should support `gfm` shortcut (1)");
        assert!(!constructs.gfm_autolink_literal, "should support `mdx` shortcut (2)");
        assert!(constructs.mdx_jsx_flow, "should support `mdx` shortcut (3)");
        assert!(!constructs.frontmatter, "should support `mdx` shortcut (4)");
    }

    #[test]
    fn test_parse_options() {
        ParseOptions::default();
        ParseOptions::gfm();
        ParseOptions::mdx();

        let options = ParseOptions::default();
        assert!(options.constructs.attention, "should default to `CommonMark` (1)");
        assert!(
            !options.constructs.gfm_autolink_literal,
            "should default to `CommonMark` (2)"
        );
        assert!(!options.constructs.mdx_jsx_flow, "should default to `CommonMark` (3)");

        let options = ParseOptions::gfm();
        assert!(options.constructs.attention, "should support `gfm` shortcut (1)");
        assert!(
            options.constructs.gfm_autolink_literal,
            "should support `gfm` shortcut (2)"
        );
        assert!(!options.constructs.mdx_jsx_flow, "should support `gfm` shortcut (3)");

        let options = ParseOptions::mdx();
        assert!(options.constructs.attention, "should support `mdx` shortcut (1)");
        assert!(
            !options.constructs.gfm_autolink_literal,
            "should support `mdx` shortcut (2)"
        );
        assert!(options.constructs.mdx_jsx_flow, "should support `mdx` shortcut (3)");

        assert_eq!(
            format!("{:?}", ParseOptions::default()),
            "ParseOptions { constructs: Constructs { attention: true, autolink: true, block_quote: true, character_escape: true, character_reference: true, code_indented: true, code_fenced: true, code_text: true, definition: true, frontmatter: false, gfm_autolink_literal: false, gfm_footnote_definition: false, gfm_label_start_footnote: false, gfm_strikethrough: false, gfm_table: false, gfm_task_list_item: false, hard_break_escape: true, hard_break_trailing: true, heading_atx: true, heading_setext: true, html_flow: true, html_text: true, label_start_image: true, label_start_link: true, label_end: true, list_item: true, math_flow: false, math_text: false, mdx_esm: false, mdx_expression_flow: false, mdx_expression_text: false, mdx_jsx_flow: false, mdx_jsx_text: false, thematic_break: true }, gfm_strikethrough_single_tilde: true, math_text_single_dollar: true, mdx_expression_parse: None, mdx_esm_parse: None, mdx_parse_budget_factor: None }",
            "should support `Debug` trait"
        );
        assert_eq!(
            format!(
                "{:?}",
                ParseOptions {
                    mdx_esm_parse: Some(Box::new(|_value| { Signal::Ok })),
                    mdx_expression_parse: Some(Box::new(|_value, _kind| { Signal::Ok })),
                    ..Default::default()
                }
            ),
            "ParseOptions { constructs: Constructs { attention: true, autolink: true, block_quote: true, character_escape: true, character_reference: true, code_indented: true, code_fenced: true, code_text: true, definition: true, frontmatter: false, gfm_autolink_literal: false, gfm_footnote_definition: false, gfm_label_start_footnote: false, gfm_strikethrough: false, gfm_table: false, gfm_task_list_item: false, hard_break_escape: true, hard_break_trailing: true, heading_atx: true, heading_setext: true, html_flow: true, html_text: true, label_start_image: true, label_start_link: true, label_end: true, list_item: true, math_flow: false, math_text: false, mdx_esm: false, mdx_expression_flow: false, mdx_expression_text: false, mdx_jsx_flow: false, mdx_jsx_text: false, thematic_break: true }, gfm_strikethrough_single_tilde: true, math_text_single_dollar: true, mdx_expression_parse: Some(\"[Function]\"), mdx_esm_parse: Some(\"[Function]\"), mdx_parse_budget_factor: None }",
            "should support `Debug` trait on mdx functions"
        );
    }

    #[test]
    fn test_compile_options() {
        CompileOptions::default();
        CompileOptions::gfm();

        let options = CompileOptions::default();
        assert!(!options.allow_dangerous_html, "should default to safe `CommonMark` (1)");
        assert!(!options.gfm_tagfilter, "should default to safe `CommonMark` (2)");

        let options = CompileOptions::gfm();
        assert!(!options.allow_dangerous_html, "should support safe `gfm` shortcut (1)");
        assert!(options.gfm_tagfilter, "should support safe `gfm` shortcut (1)");
    }

    #[test]
    fn test_options() {
        Options::default();

        let options = Options::default();
        assert!(
            options.parse.constructs.attention,
            "should default to safe `CommonMark` (1)"
        );
        assert!(
            !options.parse.constructs.gfm_autolink_literal,
            "should default to safe `CommonMark` (2)"
        );
        assert!(
            !options.parse.constructs.mdx_jsx_flow,
            "should default to safe `CommonMark` (3)"
        );
        assert!(
            !options.compile.allow_dangerous_html,
            "should default to safe `CommonMark` (4)"
        );

        let options = Options::gfm();
        assert!(
            options.parse.constructs.attention,
            "should support safe `gfm` shortcut (1)"
        );
        assert!(
            options.parse.constructs.gfm_autolink_literal,
            "should support safe `gfm` shortcut (2)"
        );
        assert!(
            !options.parse.constructs.mdx_jsx_flow,
            "should support safe `gfm` shortcut (3)"
        );
        assert!(
            !options.compile.allow_dangerous_html,
            "should support safe `gfm` shortcut (4)"
        );
    }
}
