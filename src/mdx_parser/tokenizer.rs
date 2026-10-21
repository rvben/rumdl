//! A tokenizer glues states from the state machine together.
//!
//! It facilitates everything needed to turn bytes into events with a state
//! machine.
//! It also enables the logic needed for parsing markdown, such as an
//! [`attempt`][] to try and parse something, which can succeed or, when
//! unsuccessful, revert the attempt.
//!
//! [`attempt`]: Tokenizer::attempt

use crate::mdx_parser::event::{Content, Event, Kind, Link, Name, Point, VOID_EVENTS};
use crate::mdx_parser::message;
use crate::mdx_parser::parser::ParseState;
use crate::mdx_parser::resolve::{Name as ResolveName, call as call_resolve};
use crate::mdx_parser::state::{State, call};
use crate::mdx_parser::subtokenize::Subresult;

#[cfg(any())]
use crate::mdx_parser::util::char::format_byte_opt;

use crate::mdx_parser::util::{constant::TAB_SIZE, edit_map::EditMap, mdx_collect};
use alloc::{boxed::Box, string::String, vec, vec::Vec};
use core::ops::Range;

/// Containers.
///
/// Containers are found when tokenizing
/// [document content][crate::mdx_parser::construct::document].
/// They parse a portion at the start of one or more lines.
/// The rest of those lines is a different content type (specifically, flow),
/// which they “contain”.
#[derive(Debug, Eq, PartialEq)]
pub enum Container {
    /// [Block quote][crate::mdx_parser::construct::block_quote].
    BlockQuote,
    /// [List item][crate::mdx_parser::construct::list_item].
    ListItem,
    /// [GFM: Footnote definition][crate::mdx_parser::construct::gfm_footnote_definition].
    GfmFootnoteDefinition,
}

/// Info used to tokenize a container.
///
/// Practically, these fields are only used for list items.
#[derive(Debug)]
pub struct ContainerState {
    /// Kind.
    pub kind: Container,
    /// Whether the first line was blank.
    pub blank_initial: bool,
    /// Size.
    pub size: usize,
}

/// How to handle a byte.
#[derive(Debug, PartialEq)]
enum ByteAction {
    /// This is a normal byte.
    ///
    /// Includes replaced bytes.
    Normal(u8),
    /// This byte must be ignored.
    Ignore,
    /// This is a new byte.
    Insert(u8),
}

/// Label start kind.
#[derive(Debug, PartialEq, Eq)]
pub enum LabelKind {
    /// Label (image) start.
    ///
    /// ```markdown
    /// > | a ![b] c
    ///       ^^
    /// ```
    ///
    /// Construct: [Label start (image)][crate::mdx_parser::construct::label_start_image].
    Image,
    /// Label (image) link.
    ///
    /// ```markdown
    /// > | a [b] c
    ///       ^
    /// ```
    ///
    /// Construct: [Label start (link)][crate::mdx_parser::construct::label_start_link].
    Link,
    /// GFM: Label (footnote) link.
    ///
    /// ```markdown
    /// > | a [^b] c
    ///       ^^
    /// ```
    ///
    /// Construct: [GFM: Label start (footnote)][crate::mdx_parser::construct::gfm_label_start_footnote].
    GfmFootnote,
    /// GFM: Label (footnote) link, not matching a footnote definition, so
    /// handled as a label (link) start.
    ///
    /// ```markdown
    /// > | a [^b](c) d
    ///       ^^
    /// ```
    ///
    /// Construct: [Label end][crate::mdx_parser::construct::label_end].
    GfmUndefinedFootnote,
}

/// Marker runs seen while code (text) or math (text) looked for a closer.
#[derive(Debug, Default)]
pub struct RawTextRuns {
    /// Start of the current look for a closer, while it records runs.
    pub recording: Option<usize>,
    /// All runs from this byte index to the end are in `last_start`.
    pub complete_from: Option<usize>,
    /// Per run size, sorted: the start of the last run seen of that size.
    pub last_start: Vec<(usize, usize)>,
}

/// Braces counted by MDX expressions (flow) without a JavaScript parser.
#[derive(Debug, Default)]
pub struct MdxBraces {
    /// Whether the current expression records its braces.
    pub recording: bool,
    /// Opening braces, sorted, and whether an expression there fails.
    pub opening: Vec<(usize, bool)>,
    /// Indices into `opening` of braces not yet closed.
    pub open: Vec<usize>,
    /// End of the last recorded expression; earlier starts do not record.
    pub until: usize,
}

/// Label start, looking for an end.
#[derive(Debug)]
pub struct LabelStart {
    /// Kind of start.
    pub kind: LabelKind,
    /// Indices of where the label starts and ends in `events`.
    pub start: (usize, usize),
}

/// Valid label.
#[derive(Debug)]
pub struct Label {
    pub kind: LabelKind,
    /// Indices of label start.
    pub start: (usize, usize),
    /// Indices of label end.
    pub end: (usize, usize),
}

/// Different kinds of attempts.
#[derive(Debug, PartialEq)]
enum AttemptKind {
    /// Discard what was tokenized when unsuccessful.
    Attempt,
    /// Discard always.
    Check,
}

/// How to handle [`State::Ok`][] or [`State::Nok`][].
#[derive(Debug)]
struct Attempt {
    /// Where to go to when successful.
    ok: State,
    /// Where to go to when unsuccessful.
    nok: State,
    /// Kind of attempt.
    kind: AttemptKind,
    /// If needed, the progress to revert to.
    ///
    /// It is not needed to discard an [`AttemptKind::Attempt`] that has a
    /// `nok` of [`State::Nok`][], because that means it is used in *another*
    /// attempt, which will receive that `Nok`, and has to handle it.
    progress: Option<Progress>,
}

/// The internal state of a tokenizer.
///
/// Not to be confused with states from the state machine, this instead is all
/// the information on where we currently are and what’s going on.
#[derive(Clone, Debug)]
struct Progress {
    /// Length of `events`.
    ///
    /// It’s not allowed to remove events, so reverting will just pop stuff off.
    events_len: usize,
    /// Length of the stack.
    ///
    /// It’s not allowed to decrease the stack in an attempt.
    stack_len: usize,
    /// Previous code.
    previous: Option<u8>,
    /// Current code.
    current: Option<u8>,
    /// Current place in the file.
    point: Point,
}

/// A lot of shared fields used to tokenize things.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
pub struct TokenizeState<'a> {
    // Couple complex fields used to tokenize the document.
    /// Tokenizer, used to tokenize flow in document.
    pub document_child: Option<Box<Tokenizer<'a>>>,
    /// State, used to tokenize containers.
    pub document_child_state: Option<State>,
    /// Stack of currently active containers.
    pub document_container_stack: Vec<ContainerState>,
    /// How many active containers continued.
    pub document_continued: usize,
    /// Index of last `data`.
    pub document_data_index: Option<usize>,
    /// Container exits by line number.
    pub document_exits: Vec<Option<Vec<Event>>>,
    /// Whether the previous flow was a paragraph or a definition.
    pub document_lazy_accepting_before: bool,
    /// Whether this is the first paragraph (potentially after definitions) in
    /// a list item.
    /// Used for GFM task list items.
    pub document_at_first_paragraph_of_list_item: bool,
    /// Start and end of the last scan for a blank rest of line.
    pub document_blank_scan: (usize, usize),
    /// Container index at which this line was first seen to be empty.
    pub document_blank_from: Option<usize>,
    /// Containers from this index continued on the previous, empty, line, so
    /// they continue on an empty line again; `usize::MAX` if unknown.
    pub document_blank_skip: usize,
    /// Range in which list item markers cannot start a thematic break.
    pub document_thematic_break_scan: Range<usize>,

    // Couple of very frequent settings for parsing whitespace.
    pub space_or_tab_eol_content: Option<Content>,
    pub space_or_tab_eol_connect: bool,
    pub space_or_tab_eol_ok: bool,
    pub space_or_tab_connect: bool,
    pub space_or_tab_content: Option<Content>,
    pub space_or_tab_min: usize,
    pub space_or_tab_max: usize,
    pub space_or_tab_size: usize,
    pub space_or_tab_token: Name,

    // Couple of media related fields.
    /// List of usable label starts.
    ///
    /// Used when tokenizing [text content][crate::mdx_parser::construct::text].
    pub label_starts: Vec<LabelStart>,
    /// Link starts below this index in `label_starts` are inactive; they stay
    /// to balance brackets.
    pub label_starts_inactive: usize,
    /// Where the current HTML (text) starts.
    pub html_text_start: usize,
    /// Per HTML (text) kind with a closer: starts from here find none.
    pub html_text_unclosed: [usize; 4],
    /// Marker runs seen by code (text) and math (text), for `` ` `` and `$`.
    pub raw_text_runs: [Option<Box<RawTextRuns>>; 2],
    /// Start of the current trailing punctuation check of an autolink literal.
    pub gfm_autolink_literal_trail_start: usize,
    /// Last failed trailing punctuation check; checks starting in it fail too.
    pub gfm_autolink_literal_trail_nok: Range<usize>,
    /// Start of the current `www.` domain; `usize::MAX` for a protocol literal.
    pub gfm_autolink_literal_www_start: usize,
    /// Last invalid `www.` domain; domains starting in it are invalid too.
    pub gfm_autolink_literal_www_nok: Range<usize>,
    /// List of unusable label starts.
    ///
    /// Used when tokenizing [text content][crate::mdx_parser::construct::text].
    pub label_starts_loose: Vec<LabelStart>,
    /// Stack of images and links.
    ///
    /// Used when tokenizing [text content][crate::mdx_parser::construct::text].
    pub labels: Vec<Label>,

    /// List of defined definition identifiers.
    pub definitions: Vec<String>,
    /// List of defined GFM footnote definition identifiers.
    pub gfm_footnote_definitions: Vec<String>,

    // Last error message provided at an EOF of an expression.
    pub mdx_last_parse_error: Option<(String, String, String)>,
    /// Body of the current MDX expression or ESM so far, and the next event to
    /// collect.
    pub mdx_collect: Option<Box<(usize, mdx_collect::Result)>>,
    /// MDX expression (flow) braces, once one fails after its closing brace.
    pub mdx_braces: Option<Box<MdxBraces>>,

    /// Whether to connect events.
    pub connect: bool,
    /// Marker.
    pub marker: u8,
    /// Secondary marker.
    pub marker_b: u8,
    /// Bytes that data stops at: markers, and the bytes in `LINE_STOP`.
    pub markers: &'a ByteSet,
    /// Whether something was seen.
    pub seen: bool,
    /// Size.
    pub size: usize,
    /// Secondary size.
    pub size_b: usize,
    /// Tertiary size.
    pub size_c: usize,
    /// Index.
    pub start: usize,
    /// Index.
    pub end: usize,
    /// Slot for an event name.
    pub token_1: Name,
    /// Slot for an event name.
    pub token_2: Name,
    /// Slot for an event name.
    pub token_3: Name,
    /// Slot for an event name.
    pub token_4: Name,
    /// Slot for an event name.
    pub token_5: Name,
    /// Slot for an event name.
    pub token_6: Name,
}

/// A tokenizer itself.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
pub struct Tokenizer<'a> {
    /// Jump between line endings.
    column_start: Vec<(usize, usize)>,
    // First line where this tokenizer starts.
    first_line: usize,
    /// Current point after the last line ending (excluding jump).
    line_start: Point,
    /// Track whether the current byte is already consumed (`true`) or expected
    /// to be consumed (`false`).
    ///
    /// Tracked to make sure everything’s valid.
    consumed: bool,
    /// Stack of how to handle attempts.
    attempts: Vec<Attempt>,
    /// Index of the end of what is being pushed.
    push_end: usize,
    /// Current byte.
    pub current: Option<u8>,
    /// Previous byte.
    pub previous: Option<u8>,
    /// Current relative and absolute place in the file.
    pub point: Point,
    /// Semantic labels.
    pub events: Vec<Event>,
    /// Hierarchy of semantic labels.
    ///
    /// Tracked to make sure everything’s valid.
    pub stack: Vec<Name>,
    /// Edit map, to batch changes.
    pub map: EditMap,
    /// List of resolvers.
    pub resolvers: Vec<ResolveName>,
    /// Shared parsing state across tokenizers.
    pub parse_state: &'a ParseState<'a>,
    /// A lot of shared fields used to tokenize things.
    pub tokenize_state: TokenizeState<'a>,
    /// Whether we would be interrupting something.
    ///
    /// Used when tokenizing [flow content][crate::mdx_parser::construct::flow].
    pub interrupt: bool,
    /// Whether containers cannot “pierce” into the current construct.
    ///
    /// Used when tokenizing [document content][crate::mdx_parser::construct::document].
    pub concrete: bool,
    /// Whether this row is piercing into the current construct with more
    /// containers.
    ///
    /// Used when tokenizing [document content][crate::mdx_parser::construct::document].
    pub pierce: bool,
    /// Whether this line is lazy: there are less containers than before.
    pub lazy: bool,
}

impl<'a> Tokenizer<'a> {
    /// Create a new tokenizer.
    pub fn new(point: Point, parse_state: &'a ParseState) -> Tokenizer<'a> {
        Tokenizer {
            previous: None,
            current: None,
            // To do: reserve size when feeding?
            column_start: vec![],
            first_line: point.line,
            line_start: point.clone(),
            consumed: true,
            attempts: vec![],
            push_end: 0,
            point,
            stack: vec![],
            events: vec![],
            parse_state,
            tokenize_state: TokenizeState {
                connect: false,
                document_container_stack: vec![],
                document_exits: vec![],
                document_continued: 0,
                document_blank_scan: (usize::MAX, 0),
                document_blank_from: None,
                document_blank_skip: usize::MAX,
                document_thematic_break_scan: 0..0,
                document_lazy_accepting_before: false,
                document_data_index: None,
                document_child_state: None,
                document_child: None,
                document_at_first_paragraph_of_list_item: false,
                definitions: vec![],
                gfm_footnote_definitions: vec![],
                mdx_last_parse_error: None,
                mdx_collect: None,
                mdx_braces: None,
                end: 0,
                label_starts: vec![],
                label_starts_inactive: 0,
                html_text_start: 0,
                html_text_unclosed: [usize::MAX; 4],
                raw_text_runs: [None, None],
                gfm_autolink_literal_trail_start: 0,
                gfm_autolink_literal_trail_nok: 0..0,
                gfm_autolink_literal_www_start: usize::MAX,
                gfm_autolink_literal_www_nok: 0..0,
                label_starts_loose: vec![],
                marker: 0,
                marker_b: 0,
                markers: &LINE_STOP,
                labels: vec![],
                seen: false,
                size: 0,
                size_b: 0,
                size_c: 0,
                space_or_tab_eol_content: None,
                space_or_tab_eol_connect: false,
                space_or_tab_eol_ok: false,
                space_or_tab_connect: false,
                space_or_tab_content: None,
                space_or_tab_min: 0,
                space_or_tab_max: 0,
                space_or_tab_size: 0,
                space_or_tab_token: Name::SpaceOrTab,
                start: 0,
                token_1: Name::Data,
                token_2: Name::Data,
                token_3: Name::Data,
                token_4: Name::Data,
                token_5: Name::Data,
                token_6: Name::Data,
            },
            map: EditMap::new(),
            interrupt: false,
            pierce: false,
            concrete: false,
            lazy: false,
            resolvers: vec![],
        }
    }

    /// Register a resolver.
    pub fn register_resolver(&mut self, name: ResolveName) {
        if !self.resolvers.contains(&name) {
            self.resolvers.push(name);
        }
    }

    /// Register a resolver, before others.
    pub fn register_resolver_before(&mut self, name: ResolveName) {
        if !self.resolvers.contains(&name) {
            self.resolvers.insert(0, name);
        }
    }

    /// Define a jump between two places.
    ///
    /// This defines to which future index we move after a line ending.
    pub fn define_skip(&mut self, mut point: Point) {
        move_point_back(self, &mut point);

        let info = (point.index, point.vs);

        #[cfg(any())]
        log::trace!("position: define skip: {:?} -> ({:?})", point.line, info);

        let at = point.line - self.first_line;

        if at >= self.column_start.len() {
            self.column_start.push(info);
        } else {
            self.column_start[at] = info;
        }

        self.account_for_potential_skip();
    }

    /// Increment the current positional info if we’re right after a line
    /// ending, which has a skip defined.
    fn account_for_potential_skip(&mut self) {
        let at = self.point.line - self.first_line;

        if self.point.column == 1 && at != self.column_start.len() {
            self.move_to(self.column_start[at]);
        }
    }

    /// Prepare for a next byte to get consumed.
    fn expect(&mut self, byte: Option<u8>) {
        debug_assert!(self.consumed, "expected previous byte to be consumed");
        self.consumed = false;
        self.current = byte;
    }

    /// Consume the current byte.
    /// Each state function is expected to call this to signal that this code is
    /// used, or call a next function.
    pub fn consume(&mut self) {
        debug_assert!(
            !self.consumed,
            "expected code to *not* have been consumed: this might be because `State::Retry(x)` instead of `State::Next(x)` was returned"
        );
        self.move_one();

        self.previous = self.current;
        // While we’re not at eof, it is at least better to not have the
        // same current code as `previous` *and* `current`.
        self.current = None;
        // Mark as consumed.
        self.consumed = true;
    }

    /// Consume the current byte and following bytes up to one in `stop`, which
    /// must include `\n`, `\r`, and `\t`.
    pub fn consume_run(&mut self, stop: &ByteSet) {
        debug_assert!(stop[usize::from(b'\n')] && stop[usize::from(b'\r')] && stop[usize::from(b'\t')]);
        self.consume();
        let bytes = self.parse_state.bytes;
        let start = self.point.index;
        let mut index = start;

        while index < self.push_end && !stop[usize::from(bytes[index])] {
            index += 1;
        }

        if index > start {
            self.point.index = index;
            self.point.column += index - start;
            self.previous = Some(bytes[index - 1]);
        }
    }

    /// Same as `consume_run` with `LINE_STOP`.
    pub fn consume_line_run(&mut self) {
        self.consume();
        let bytes = self.parse_state.bytes;
        let start = self.point.index;
        let rest = &bytes[start..self.push_end.max(start)];
        let len = memchr::memchr3(b'\n', b'\r', b'\t', rest).unwrap_or(rest.len());

        if len > 0 {
            self.point.index = start + len;
            self.point.column += len;
            self.previous = Some(bytes[start + len - 1]);
        }
    }

    /// Move to the next (virtual) byte.
    fn move_one(&mut self) {
        match byte_action(self.parse_state.bytes, &self.point) {
            ByteAction::Ignore => {
                self.point.index += 1;
            }
            ByteAction::Insert(byte) => {
                self.previous = Some(byte);
                self.point.column += 1;
                self.point.vs += 1;
            }
            ByteAction::Normal(byte) => {
                self.previous = Some(byte);
                self.point.vs = 0;
                self.point.index += 1;

                if byte == b'\n' {
                    self.point.line += 1;
                    self.point.column = 1;

                    if self.point.line - self.first_line + 1 > self.column_start.len() {
                        self.column_start.push((self.point.index, self.point.vs));
                    }

                    self.line_start = self.point.clone();

                    self.account_for_potential_skip();

                    #[cfg(any())]
                    log::trace!("position: after eol: `{:?}`", self.point);
                } else {
                    self.point.column += 1;
                }
            }
        }
    }

    /// Move (virtual) bytes.
    fn move_to(&mut self, to: (usize, usize)) {
        let (to_index, to_vs) = to;
        while self.point.index < to_index || self.point.index == to_index && self.point.vs < to_vs {
            self.move_one();
        }
    }

    /// Mark the start of a semantic label.
    pub fn enter(&mut self, name: Name) {
        enter_impl(self, name, None);
    }

    /// Enter with a link.
    pub fn enter_link(&mut self, name: Name, link: Link) {
        enter_impl(self, name, Some(link));
    }

    /// Mark the end of a semantic label.
    pub fn exit(&mut self, name: Name) {
        let current = self.stack.pop().expect("cannot close w/o open tokens");

        debug_assert_eq!(current, name, "expected exit event to match current event");

        let previous = self.events.last().expect("cannot close w/o open event");
        let mut point = self.point.clone();

        debug_assert!(
            current != previous.name || previous.point.index != point.index || previous.point.vs != point.vs,
            "expected non-empty event"
        );

        if VOID_EVENTS.iter().any(|d| d == &name) {
            debug_assert!(
                current == previous.name,
                "expected event to be void, instead of including something"
            );
        }

        // A bit weird, but if we exit right after a line ending, we *don’t* want to consider
        // potential skips.
        if matches!(self.previous, Some(b'\n')) {
            point = self.line_start.clone();
        } else {
            move_point_back(self, &mut point);
        }

        #[cfg(any())]
        log::debug!("exit:    `{:?}`", name);

        let event = Event {
            kind: Kind::Exit,
            name,
            point,
            link: None,
        };
        self.events.push(event);
    }

    /// Capture the tokenizer progress.
    fn capture(&mut self) -> Progress {
        Progress {
            previous: self.previous,
            current: self.current,
            point: self.point.clone(),
            events_len: self.events.len(),
            stack_len: self.stack.len(),
        }
    }

    /// Apply tokenizer progress.
    fn free(&mut self, previous: Progress) {
        self.previous = previous.previous;
        self.current = previous.current;
        self.point = previous.point;
        debug_assert!(
            self.events.len() >= previous.events_len,
            "expected to restore less events than before"
        );
        self.events.truncate(previous.events_len);
        debug_assert!(
            self.stack.len() >= previous.stack_len,
            "expected to restore less stack items than before"
        );
        self.stack.truncate(previous.stack_len);
    }

    /// Stack an attempt, moving to `ok` on [`State::Ok`][] and `nok` on
    /// [`State::Nok`][], reverting in both cases.
    pub fn check(&mut self, ok: State, nok: State) {
        // Always capture (and restore) when checking.
        // No need to capture (and restore) when `nok` is `State::Nok`, because the
        // parent attempt will do it.
        let progress = Some(self.capture());
        let attempt = Attempt {
            kind: AttemptKind::Check,
            progress,
            ok,
            nok,
        };
        self.attempts.push(attempt);
    }

    /// Stack an attempt, moving to `ok` on [`State::Ok`][] and `nok` on
    /// [`State::Nok`][], reverting in the latter case.
    pub fn attempt(&mut self, ok: State, nok: State) {
        // Always capture (and restore) when checking.
        // No need to capture (and restore) when `nok` is `State::Nok`, because the
        // parent attempt will do it.
        let progress = if matches!(nok, State::Nok) {
            None
        } else {
            Some(self.capture())
        };

        let attempt = Attempt {
            kind: AttemptKind::Attempt,
            progress,
            ok,
            nok,
        };
        self.attempts.push(attempt);
    }

    /// Tokenize.
    pub fn push(&mut self, from: (usize, usize), to: (usize, usize), state: State) -> State {
        push_impl(self, from, to, state, false)
    }

    /// Flush.
    pub fn flush(&mut self, state: State, resolve: bool) -> Result<Subresult, message::Message> {
        let to = (self.point.index, self.point.vs);
        let state = push_impl(self, to, to, state, true);

        state.to_result()?;

        let mut value = Subresult {
            done: false,
            gfm_footnote_definitions: self.tokenize_state.gfm_footnote_definitions.split_off(0),
            definitions: self.tokenize_state.definitions.split_off(0),
        };

        if resolve {
            let resolvers = self.resolvers.split_off(0);
            let mut index = 0;
            let defs = &mut value.definitions;
            let fn_defs = &mut value.gfm_footnote_definitions;
            while index < resolvers.len() {
                if let Some(mut result) = call_resolve(self, resolvers[index])? {
                    fn_defs.append(&mut result.gfm_footnote_definitions);
                    defs.append(&mut result.definitions);
                }
                index += 1;
            }

            self.map.consume(&mut self.events);
        }

        Ok(value)
    }
}

/// Move back past ignored bytes.
fn move_point_back(tokenizer: &mut Tokenizer, point: &mut Point) {
    while point.index > 0 {
        point.index -= 1;
        let action = byte_action(tokenizer.parse_state.bytes, point);
        if !matches!(action, ByteAction::Ignore) {
            point.index += 1;
            break;
        }
    }
}

/// Enter.
fn enter_impl(tokenizer: &mut Tokenizer, name: Name, link: Option<Link>) {
    let mut point = tokenizer.point.clone();
    move_point_back(tokenizer, &mut point);

    #[cfg(any())]
    log::debug!("enter:   `{:?}`", name);

    tokenizer.stack.push(name.clone());
    tokenizer.events.push(Event {
        kind: Kind::Enter,
        name,
        point,
        link,
    });
}

/// Run the tokenizer.
fn push_impl(
    tokenizer: &mut Tokenizer,
    from: (usize, usize),
    to: (usize, usize),
    mut state: State,
    flush: bool,
) -> State {
    debug_assert!(
        from.0 > tokenizer.point.index || (from.0 == tokenizer.point.index && from.1 >= tokenizer.point.vs),
        "cannot move backwards"
    );

    tokenizer.move_to(from);
    tokenizer.push_end = to.0;

    loop {
        match state {
            State::Error(_) => break,
            State::Ok | State::Nok => {
                if let Some(attempt) = tokenizer.attempts.pop() {
                    if attempt.kind == AttemptKind::Check || matches!(state, State::Nok) {
                        if let Some(progress) = attempt.progress {
                            tokenizer.free(progress);
                        }
                    }

                    tokenizer.consumed = true;

                    let next = if matches!(state, State::Ok) {
                        attempt.ok
                    } else {
                        attempt.nok
                    };

                    #[cfg(any())]
                    log::trace!("attempt: `{:?}` -> `{:?}`", state, next);

                    state = next;
                } else {
                    break;
                }
            }
            State::Next(name) => {
                let action =
                    if tokenizer.point.index < to.0 || (tokenizer.point.index == to.0 && tokenizer.point.vs < to.1) {
                        Some(byte_action(tokenizer.parse_state.bytes, &tokenizer.point))
                    } else if flush {
                        None
                    } else {
                        break;
                    };

                if let Some(ByteAction::Ignore) = action {
                    tokenizer.move_one();
                } else {
                    let byte = if let Some(ByteAction::Insert(byte) | ByteAction::Normal(byte)) = action {
                        Some(byte)
                    } else {
                        None
                    };

                    #[cfg(any())]
                    log::trace!("feed:    {} to {:?}", format_byte_opt(byte), name);

                    tokenizer.expect(byte);
                    state = call(tokenizer, name);
                }
            }
            State::Retry(name) => {
                #[cfg(any())]
                log::trace!("retry:   `{:?}`", name);

                state = call(tokenizer, name);
            }
        }
    }

    tokenizer.consumed = true;

    if flush {
        debug_assert!(matches!(state, State::Ok | State::Error(_)), "must be ok");
    } else {
        debug_assert!(
            matches!(state, State::Next(_) | State::Error(_)),
            "must have a next state"
        );
    }

    state
}

/// Set of bytes, as a table.
pub type ByteSet = [bool; 256];

/// Bytes that move differently than others: runs stop at them.
pub const LINE_STOP: ByteSet = with_bytes([false; 256], b"\n\r\t");

/// `set` with `bytes` added.
pub const fn with_bytes(mut set: ByteSet, bytes: &[u8]) -> ByteSet {
    let mut index = 0;
    while index < bytes.len() {
        set[bytes[index] as usize] = true;
        index += 1;
    }
    set
}

/// Figure out how to handle a byte.
fn byte_action(bytes: &[u8], point: &Point) -> ByteAction {
    if point.index < bytes.len() {
        let byte = bytes[point.index];

        if byte == b'\r' {
            // CRLF.
            if point.index < bytes.len() - 1 && bytes[point.index + 1] == b'\n' {
                ByteAction::Ignore
            }
            // CR.
            else {
                ByteAction::Normal(b'\n')
            }
        } else if byte == b'\t' {
            let remainder = point.column % TAB_SIZE;
            let vs = if remainder == 0 { 0 } else { TAB_SIZE - remainder };

            // On the tab itself, first send it.
            if point.vs == 0 {
                if vs == 0 {
                    ByteAction::Normal(byte)
                } else {
                    ByteAction::Insert(byte)
                }
            } else if vs == 0 {
                ByteAction::Normal(b' ')
            } else {
                ByteAction::Insert(b' ')
            }
        } else {
            ByteAction::Normal(byte)
        }
    } else {
        unreachable!("out of bounds")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_size() {
        // Boxed per document; glibc allocates larger boxes on a slower path.
        assert!(
            core::mem::size_of::<Tokenizer>() <= 1000,
            "expected `Tokenizer` to stay small enough to allocate as small ({} bytes)",
            core::mem::size_of::<Tokenizer>()
        );
    }
}
