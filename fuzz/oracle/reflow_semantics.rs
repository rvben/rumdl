//! Semantic oracle for MD013 reflow.
//!
//! Reflow chooses where lines end. It must never change what a document
//! renders to, and a second `fmt` pass must find nothing left to do. This
//! module checks both properties for one input under one reflow configuration,
//! through the same `DocumentRun::fix` entry point `rumdl fmt` uses.
//!
//! "Renders to" is judged by three independent CommonMark + GFM renderers:
//! markdown-rs, which rumdl does not use for parsing Standard-flavor documents,
//! pulldown-cmark, which it does, and comrak, a port of the cmark-gfm reference
//! implementation GitHub renders with. They disagree where the spec leaves
//! room or one of them departs from it (the whitespace a code span keeps from
//! an indented continuation line, for one), and a reflow must keep the
//! rendering under each. A change any renderer sees is a violation; the report
//! names which renderers saw it, so a disagreement between the parsers stays
//! distinguishable from a reflow defect.
//!
//! Rendered HTML is compared after collapsing each run of ASCII whitespace to
//! one space, since a soft line break and a space render the same. Non-ASCII
//! whitespace such as a no-break space is content and is kept. Code blocks and
//! code spans are compared byte for byte. Under `cjk-soft-break =
//! "join"`, a whitespace run containing a line ending between two characters
//! that CJK-aware renderers join (`joins_cjk_soft_break`) is removed on both
//! sides, because that mode's contract is that such a break renders as nothing.
//!
//! Shared by the `fuzz_reflow_semantics` fuzz target and the test crate (the
//! oracle's own tests and the corpus sweep), each of which includes this file
//! by path.

use rumdl_lib::config::{Config, MarkdownFlavor, RuleConfig};
use rumdl_lib::document_run::DocumentRun;
use rumdl_lib::rules::{all_rules, filter_rules};
use rumdl_lib::utils::unicode::joins_cjk_soft_break;

/// The iteration cap `rumdl fmt` passes to `DocumentRun::fix`.
const FMT_MAX_ITERATIONS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Default,
    Normalize,
    SentencePerLine,
    SentencePack,
    SemanticLineBreaks,
}

impl Mode {
    pub const ALL: [Mode; 5] = [
        Mode::Default,
        Mode::Normalize,
        Mode::SentencePerLine,
        Mode::SentencePack,
        Mode::SemanticLineBreaks,
    ];

    fn as_config(self) -> &'static str {
        match self {
            Mode::Default => "default",
            Mode::Normalize => "normalize",
            Mode::SentencePerLine => "sentence-per-line",
            Mode::SentencePack => "sentence-pack",
            Mode::SemanticLineBreaks => "semantic-line-breaks",
        }
    }
}

/// One MD013 reflow configuration. Every field maps to a documented MD013
/// option; `shell_args` prints the `rumdl fmt` flags that reproduce it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReflowSettings {
    pub mode: Mode,
    pub line_length: u64,
    pub length_mode_chars: bool,
    pub atomic_spans: bool,
    pub break_link_text: bool,
    pub cjk_join: bool,
    pub length_exemptions: bool,
    pub require_sentence_capital: bool,
}

const LINE_LENGTHS: [u64; 8] = [0, 10, 20, 30, 40, 60, 80, 120];

impl ReflowSettings {
    /// The settings a project gets from `reflow = true` plus a mode and a
    /// line length, with every other option at its default.
    pub fn with_mode(mode: Mode, line_length: u64) -> Self {
        Self {
            mode,
            line_length,
            length_mode_chars: false,
            atomic_spans: true,
            break_link_text: false,
            cjk_join: false,
            length_exemptions: false,
            require_sentence_capital: true,
        }
    }

    /// Decode settings from two fuzzer-controlled bytes, so every
    /// configuration is reachable and mutations explore them.
    pub fn from_bytes(a: u8, b: u8) -> Self {
        let mode = Mode::ALL[usize::from(a & 0b111) % Mode::ALL.len()];
        let line_length = LINE_LENGTHS[usize::from((a >> 3) & 0b111)];
        Self {
            length_mode_chars: a & 0b0100_0000 != 0,
            atomic_spans: a & 0b1000_0000 == 0,
            break_link_text: b & 0b0001 != 0,
            cjk_join: b & 0b0010 != 0,
            length_exemptions: b & 0b0100 != 0,
            require_sentence_capital: b & 0b1000 == 0,
            ..Self::with_mode(mode, line_length)
        }
    }

    fn md013_values(&self) -> Vec<(&'static str, toml::Value)> {
        use toml::Value::{Boolean, Integer, String as Str};
        let line_length = i64::try_from(self.line_length).expect("line length fits in i64");
        vec![
            ("reflow", Boolean(true)),
            ("reflow-mode", Str(self.mode.as_config().to_string())),
            ("line-length", Integer(line_length)),
            (
                "length-mode",
                Str(if self.length_mode_chars { "chars" } else { "visual" }.to_string()),
            ),
            ("atomic-spans", Boolean(self.atomic_spans)),
            ("reflow-break-link-text", Boolean(self.break_link_text)),
            (
                "cjk-soft-break",
                Str(if self.cjk_join { "join" } else { "space" }.to_string()),
            ),
            ("reflow-length-exemptions", Boolean(self.length_exemptions)),
            ("require-sentence-capital", Boolean(self.require_sentence_capital)),
        ]
    }

    /// A Standard-flavor config that enables MD013 alone, with these settings.
    pub fn config(&self) -> Config {
        let mut config = Config::default();
        config.global.flavor = MarkdownFlavor::Standard;
        config.global.enable = vec!["MD013".to_string()];
        let mut rule_config = RuleConfig::default();
        for (key, value) in self.md013_values() {
            rule_config.values.insert(key.to_string(), value);
        }
        config.rules.insert("MD013".to_string(), rule_config);
        config
    }

    /// `rumdl fmt` arguments, before the file path, that reproduce this
    /// configuration.
    pub fn cli_args(&self) -> Vec<String> {
        let mut args: Vec<String> = ["--no-config", "--no-cache", "--enable", "MD013"]
            .map(String::from)
            .into();
        for (key, value) in self.md013_values() {
            args.push("-c".to_string());
            args.push(format!("MD013.{key} = {value}"));
        }
        args
    }

    /// `cli_args` as one shell-quoted string.
    pub fn shell_args(&self) -> String {
        self.cli_args()
            .iter()
            .map(|arg| {
                if arg.contains(' ') {
                    format!("'{arg}'")
                } else {
                    arg.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Renderer {
    MarkdownRs,
    PulldownCmark,
    Comrak,
}

impl Renderer {
    pub const ALL: [Renderer; 3] = [Renderer::MarkdownRs, Renderer::PulldownCmark, Renderer::Comrak];

    pub fn name(self) -> &'static str {
        match self {
            Renderer::MarkdownRs => "markdown-rs",
            Renderer::PulldownCmark => "pulldown-cmark",
            Renderer::Comrak => "comrak",
        }
    }

    pub fn render(self, markdown: &str) -> String {
        match self {
            Renderer::MarkdownRs => {
                // Front matter stays off: markdown-rs 1.0 misparses an unclosed
                // `---` opener (it renders the rest of the document as broken
                // HTML, or overflows an integer and panics), and reflow never
                // touches front matter, which pulldown-cmark still recognizes.
                let mut options = markdown::Options::gfm();
                options.compile.allow_dangerous_html = true;
                options.compile.allow_dangerous_protocol = true;
                let expanded = expand_tabs_after_container_markers(markdown);
                let mut source = escape_continuation_ordered_markers(&expanded).into_owned();
                // markdown-rs 1.0 ends a list item before a lazy continuation
                // line starting with `<` when that line ends the document
                // without a line ending. A final line ending is not content to
                // CommonMark, and with one markdown-rs keeps the line in the
                // item, as cmark, pulldown-cmark and micromark do.
                if !source.ends_with(['\n', '\r']) {
                    source.push('\n');
                }
                markdown::to_html_with_options(&source, &options)
                    .expect("markdown-rs only rejects MDX, which these options leave off")
            }
            Renderer::PulldownCmark => {
                let mut out = String::new();
                pulldown_cmark::html::push_html(
                    &mut out,
                    pulldown_cmark::Parser::new_ext(markdown, pulldown_options()),
                );
                out
            }
            Renderer::Comrak => comrak::markdown_to_html(markdown, &comrak_options()),
        }
    }
}

/// GitHub's extensions, with raw HTML rendered rather than omitted, since a
/// reflow that changes HTML must show up as a change.
fn comrak_options() -> comrak::Options<'static> {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.autolink = true;
    options.extension.front_matter_delimiter = Some("---".to_string());
    options.render.r#unsafe = true;
    options
}

fn pulldown_options() -> pulldown_cmark::Options {
    use pulldown_cmark::Options;
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
}

/// Escape the delimiter of an ordered list marker that continues a paragraph,
/// for markdown-rs only. A marker not numbered 1 cannot interrupt a paragraph,
/// so such a line is paragraph text, which cmark, pulldown-cmark and micromark
/// all render that way. markdown-rs 1.0 opens a list there when the paragraph
/// is the first block after a list, so a correct reflow that wraps before
/// `18. For now` would read as a render change. Escaping the `.` or `)` renders
/// the same text and keeps the line out of markdown-rs's list check.
///
/// Paragraph text comes from pulldown-cmark, and only a delimiter inside a text
/// event is escaped: in a code span or inline HTML the backslash would be
/// content. A wrap that puts such a marker at a line start inside a code span
/// is therefore still reported, by markdown-rs alone.
fn escape_continuation_ordered_markers(markdown: &str) -> std::borrow::Cow<'_, str> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    let mut paragraph_start = None;
    let mut delimiters = Vec::new();
    for (event, range) in Parser::new_ext(markdown, pulldown_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Paragraph) => paragraph_start = Some(range.start),
            Event::End(TagEnd::Paragraph) => paragraph_start = None,
            // pulldown-cmark ends a text event at every line ending, so a line
            // the paragraph continues starts a text event of its own.
            Event::Text(_) => {
                let line_start = markdown[..range.start].rfind('\n').map_or(0, |i| i + 1);
                if paragraph_start.is_some_and(|start| line_start > start)
                    && markdown[line_start..range.start]
                        .trim_start_matches([' ', '\t', '>'])
                        .is_empty()
                    && let Some(delimiter) = continuation_marker_delimiter(&markdown[line_start..])
                    && range.contains(&(line_start + delimiter))
                {
                    delimiters.push(line_start + delimiter);
                }
            }
            _ => {}
        }
    }
    if delimiters.is_empty() {
        return std::borrow::Cow::Borrowed(markdown);
    }
    let mut out = String::with_capacity(markdown.len() + delimiters.len());
    let mut copied = 0;
    for at in delimiters {
        out.push_str(&markdown[copied..at]);
        out.push('\\');
        copied = at;
    }
    out.push_str(&markdown[copied..]);
    std::borrow::Cow::Owned(out)
}

/// Write each tab among the container markers opening a line as the spaces
/// that reach the next tab stop, for markdown-rs only. CommonMark reads a tab
/// in block structure as reaching the next multiple of four columns, so the
/// spaces read the same. markdown-rs 1.0 counts a tab after a list marker as
/// one column, which puts the item's content column at 2 for `-\t` where
/// cmark, pulldown-cmark, comrak and micromark put it at 4. A later line
/// indented between the two is then paragraph text to markdown-rs and a block
/// of the item (a quote, a heading) to everyone else.
///
/// The lines are read from their text alone, so a line in a fenced code block
/// that starts with a marker and a tab has its tab expanded too. Both documents
/// a check compares get the same treatment, and the other renderers still see
/// the tab, so a reflow that rewrites it is still reported.
fn expand_tabs_after_container_markers(markdown: &str) -> std::borrow::Cow<'_, str> {
    if !markdown.contains('\t') {
        return std::borrow::Cow::Borrowed(markdown);
    }
    let mut out = String::with_capacity(markdown.len());
    for line in markdown.split_inclusive('\n') {
        let prefix_len = container_prefix_len(line);
        let mut column = 0;
        for c in line[..prefix_len].chars() {
            if c == '\t' {
                let width = 4 - column % 4;
                out.extend(std::iter::repeat_n(' ', width));
                column += width;
            } else {
                out.push(c);
                column += 1;
            }
        }
        out.push_str(&line[prefix_len..]);
    }
    std::borrow::Cow::Owned(out)
}

/// The byte length of the container markers opening `line` (`>`, a bullet, an
/// ordered marker) with the whitespace around them, or 0 when none opens it.
fn container_prefix_len(line: &str) -> usize {
    let bytes = line.as_bytes();
    let skip_blanks = |mut at: usize| {
        while matches!(bytes.get(at), Some(b' ' | b'\t')) {
            at += 1;
        }
        at
    };
    let mut prefix_len = 0;
    loop {
        let at = skip_blanks(prefix_len);
        let marker_len = match bytes.get(at) {
            Some(b'>') => 1,
            Some(b'-' | b'*' | b'+') => 1,
            Some(b'0'..=b'9') => {
                let digits = bytes[at..].iter().take_while(|b| b.is_ascii_digit()).count();
                if digits <= 9 && matches!(bytes.get(at + digits), Some(b'.' | b')')) {
                    digits + 1
                } else {
                    return prefix_len;
                }
            }
            _ => return prefix_len,
        };
        let after = at + marker_len;
        // A list marker needs whitespace after it, a `>` does not.
        if bytes[at] != b'>' && !matches!(bytes.get(after), None | Some(b' ' | b'\t' | b'\n' | b'\r')) {
            return prefix_len;
        }
        prefix_len = skip_blanks(after);
    }
}

/// The offset of the delimiter when `line`, after its container prefix, starts
/// with an ordered list marker numbered other than 1.
fn continuation_marker_delimiter(line: &str) -> Option<usize> {
    let text = line.trim_start_matches([' ', '\t', '>']);
    let prefix = line.len() - text.len();
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let after = text.as_bytes().get(digits + 1).copied();
    let is_marker = (1..=9).contains(&digits)
        && matches!(text.as_bytes().get(digits), Some(b'.' | b')'))
        && matches!(after, None | Some(b' ' | b'\t' | b'\n' | b'\r'))
        && text[..digits].trim_start_matches('0') != "1";
    is_marker.then_some(prefix + digits)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationKind {
    /// The reflowed document renders differently. `renderers` lists every
    /// renderer that saw the change; `before` and `after` are the first one's
    /// normalized HTML.
    RenderChanged {
        renderers: Vec<Renderer>,
        before: String,
        after: String,
    },
    /// A second `fmt` pass changed the reflowed document again.
    NotIdempotent { second: String },
    /// The fix pipeline returned an error.
    FixFailed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub kind: ViolationKind,
    pub input: String,
    pub output: String,
}

impl Violation {
    pub fn label(&self) -> String {
        match &self.kind {
            ViolationKind::RenderChanged { renderers, .. } => {
                let names: Vec<&str> = renderers.iter().map(|r| r.name()).collect();
                format!("render-changed:{}", names.join("+"))
            }
            ViolationKind::NotIdempotent { .. } => "not-idempotent".to_string(),
            ViolationKind::FixFailed(_) => "fix-failed".to_string(),
        }
    }
}

/// What the oracle concluded about one input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Reflow left the document byte-identical.
    Unchanged,
    /// Reflow rewrote the document and every property held.
    Rewritten,
}

/// Run `rumdl fmt`'s fix path with MD013 alone under `settings`.
pub fn reflow(input: &str, settings: &ReflowSettings) -> Result<String, String> {
    let config = settings.config();
    let rules = filter_rules(&all_rules(&config), &config.global);
    DocumentRun::new(input, &rules, &config)
        .fix(FMT_MAX_ITERATIONS)
        .map(|(output, _)| output)
}

/// Check that reflowing `input` under `settings` preserves its rendering and
/// converges in one pass.
pub fn check(input: &str, settings: &ReflowSettings) -> Result<Outcome, Violation> {
    check_with(input, settings, reflow)
}

/// `check`, with the reflow step supplied by the caller: the sweep uses this to
/// run a `rumdl` binary built from another commit.
pub fn check_with(
    input: &str,
    settings: &ReflowSettings,
    reflow: impl Fn(&str, &ReflowSettings) -> Result<String, String>,
) -> Result<Outcome, Violation> {
    let violation = |kind, output: &str| Violation {
        kind,
        input: input.to_string(),
        output: output.to_string(),
    };

    let output = reflow(input, settings).map_err(|e| violation(ViolationKind::FixFailed(e), ""))?;
    if output == input {
        return Ok(Outcome::Unchanged);
    }

    let mut changed: Option<ViolationKind> = None;
    for renderer in Renderer::ALL {
        let before = normalize_html(&renderer.render(input), settings.cjk_join);
        let after = normalize_html(&renderer.render(&output), settings.cjk_join);
        if before == after {
            continue;
        }
        match &mut changed {
            Some(ViolationKind::RenderChanged { renderers, .. }) => renderers.push(renderer),
            _ => {
                changed = Some(ViolationKind::RenderChanged {
                    renderers: vec![renderer],
                    before,
                    after,
                });
            }
        }
    }
    if let Some(kind) = changed {
        return Err(violation(kind, &output));
    }

    let second = reflow(&output, settings).map_err(|e| violation(ViolationKind::FixFailed(e), &output))?;
    if second != output {
        return Err(violation(ViolationKind::NotIdempotent { second }, &output));
    }

    Ok(Outcome::Rewritten)
}

/// Collapse insignificant whitespace in rendered HTML, leaving code blocks and
/// code spans byte-exact. A code span's whitespace is content, and CommonMark
/// already renders a line ending inside one as a space, so a break reflow puts
/// inside a span compares equal while a lost or doubled space does not.
///
/// A line ending inside a `<code>` region can therefore only come from raw
/// inline HTML (`<code>a\nb</code>`, or a break inside the tag itself), where
/// it is ordinary inline whitespace. It compares as the one space a reflow puts
/// in its place, so the region still keeps every space count.
pub fn normalize_html(html: &str, cjk_join: bool) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let next = [("<pre", "</pre>"), ("<code", "</code>")]
            .into_iter()
            .filter_map(|(open, close)| rest.find(open).map(|start| (start, close)))
            .min_by_key(|&(start, _)| start);
        let Some((start, close)) = next else {
            break;
        };
        out.push_str(&drop_whitespace_at_block_edges(&collapse_whitespace(
            &rest[..start],
            cjk_join,
        )));
        let end = rest[start..]
            .find(close)
            .map_or(rest.len(), |offset| start + offset + close.len());
        let region = &rest[start..end];
        if close == "</code>" {
            out.push_str(&region.replace('\n', " "));
        } else {
            out.push_str(region);
        }
        rest = &rest[end..];
    }
    out.push_str(&drop_whitespace_at_block_edges(&collapse_whitespace(rest, cjk_join)));
    out
}

/// The HTML elements a renderer of CommonMark writes that lay out as blocks.
const BLOCK_ELEMENTS: [&str; 20] = [
    "blockquote",
    "dd",
    "div",
    "dl",
    "dt",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "li",
    "ol",
    "p",
    "table",
    "tbody",
    "td",
    "th",
    "ul",
];

/// Drop the spaces on either side of a block element's tags. A browser shows
/// no whitespace at the start or end of a block, so a renderer that keeps a
/// paragraph's leading tab (markdown-rs does for a tab-indented paragraph in a
/// list item) and one that drops it render the same page.
fn drop_whitespace_at_block_edges(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        let Some(close) = rest[open..].find('>').map(|offset| open + offset + 1) else {
            break;
        };
        let tag = &rest[open..close];
        let name = tag
            .trim_start_matches(['<', '/'])
            .split(|c: char| !c.is_ascii_alphanumeric())
            .next()
            .unwrap_or_default();
        if BLOCK_ELEMENTS.contains(&name) {
            out.push_str(rest[..open].trim_end_matches(' '));
            out.push_str(tag);
            rest = rest[close..].trim_start_matches(' ');
        } else {
            out.push_str(&rest[..close]);
            rest = &rest[close..];
        }
    }
    out.push_str(rest);
    out
}

fn collapse_whitespace(text: &str, cjk_join: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_whitespace() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let run_start = i;
        while i < chars.len() && chars[i].is_ascii_whitespace() {
            i += 1;
        }
        let has_line_ending = chars[run_start..i].iter().any(|&c| c == '\n' || c == '\r');
        let joined = cjk_join
            && has_line_ending
            && run_start > 0
            && i < chars.len()
            && joins_cjk_soft_break(chars[run_start - 1])
            && joins_cjk_soft_break(chars[i]);
        if !joined {
            out.push(' ');
        }
    }
    out
}
