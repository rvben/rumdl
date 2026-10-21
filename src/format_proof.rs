//! Invocation-local proofs that a built-in formatter policy cannot edit one
//! immutable source revision. Unknown syntax or configuration keeps the normal
//! checks. These facts never stand in for lint diagnostics or a public context.

#[cfg(test)]
pub(crate) mod native_regressions;

#[cfg(test)]
mod nested_regressions;

use crate::config::MarkdownFlavor;
use crate::lint_context::document_parser::ParsedDocument;
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use std::ops::Range;

#[cfg(test)]
pub(crate) const ALL_FLAVORS: [crate::config::MarkdownFlavor; 12] = [
    crate::config::MarkdownFlavor::Standard,
    crate::config::MarkdownFlavor::MkDocs,
    crate::config::MarkdownFlavor::MDX,
    crate::config::MarkdownFlavor::Pandoc,
    crate::config::MarkdownFlavor::Quarto,
    crate::config::MarkdownFlavor::Obsidian,
    crate::config::MarkdownFlavor::Kramdown,
    crate::config::MarkdownFlavor::AzureDevOps,
    crate::config::MarkdownFlavor::MyST,
    crate::config::MarkdownFlavor::Hugo,
    crate::config::MarkdownFlavor::MDG,
    crate::config::MarkdownFlavor::GhAw,
];

pub(crate) mod events;

/// Opaque certificate returned only by the built-in policy implementations.
/// Its constructor is private to this crate; custom rules keep normal checks.
#[derive(Clone, Copy)]
pub struct NativeFormatPolicy {
    rule: u8,
    owner: std::any::TypeId,
}

impl NativeFormatPolicy {
    pub(crate) fn for_rule<T: 'static>(rule: u8) -> Self {
        Self {
            rule,
            owner: std::any::TypeId::of::<T>(),
        }
    }

    #[cfg(test)]
    fn new(rule: u8) -> Self {
        Self::for_rule::<()>(rule)
    }

    /// Forwarding a built-in method does not certify a wrapper's own behavior.
    pub(crate) fn validated(rule: &dyn crate::rule::Rule) -> Option<Self> {
        rule.native_format_policy()
            .filter(|policy| policy.owner == rule.as_any().type_id())
    }

    #[cfg(test)]
    pub(crate) fn for_test_owner<T: 'static>(mut self) -> Self {
        self.owner = std::any::TypeId::of::<T>();
        self
    }

    pub(crate) fn proven(self, facts: &FormatProof<'_>) -> bool {
        facts.eligible() && facts.bits & (1u128 << self.rule) != 0
    }

    /// These policies describe settings that disable edits, independently of
    /// the document's syntax. The caller must first rule out inline overrides.
    pub(crate) const fn configuration_only(self) -> bool {
        matches!(self.rule, 0 | 13)
    }
}

#[derive(Default)]
pub(crate) struct FormatProof<'a> {
    bits: u128,
    parsed: Option<ParsedDocument<'a>>,
}

#[derive(Clone, Copy)]
struct Line {
    start: usize,
    end: usize,
}

impl Line {
    fn text(self, source: &str) -> &str {
        &source[self.start..self.end]
    }
}

struct List {
    range: Range<usize>,
    depth: usize,
    ordered: bool,
    start_value: u64,
    items: Vec<usize>,
}

struct Item {
    range: Range<usize>,
    line: usize,
    indent: usize,
    content_column: usize,
}

/// A second, conservative certificate for default CommonMark continuation
/// indentation. The strict check below also compares nested prose with every
/// ancestor; native MD077 instead defers to the nested item. Source guards in
/// `analyze_with` still reject ambiguous markers, tables, quotes and literals.
fn nested_continuations_are_unchanged(source: &str, lines: &[Line], items: &[Item], literals: &[Range<usize>]) -> bool {
    let mut open: Vec<usize> = Vec::new();
    let mut next_item = 0;
    let mut literal = 0;
    let mut tight = true;
    let mut closed_child_column: Option<usize> = None;
    let mut continuation_ceiling: Option<usize> = None;
    for (index, line) in lines.iter().copied().enumerate() {
        while open.last().is_some_and(|&item| items[item].range.end <= line.start) {
            let child = open.pop().unwrap();
            if !open.is_empty() {
                closed_child_column =
                    Some(closed_child_column.map_or(items[child].indent, |old| old.min(items[child].indent)));
            }
        }
        if let Some(item) = items.get(next_item)
            && item.line == index
        {
            // Multiple markers on a line and crossing ranges keep native
            // ownership, independently of the earlier source guards.
            if items.get(next_item + 1).is_some_and(|next| next.line == index)
                || open
                    .last()
                    .is_some_and(|&parent| item.range.end > items[parent].range.end)
            {
                return false;
            }
            open.push(next_item);
            next_item += 1;
            tight = true;
            closed_child_column = None;
            // This marker either reopens a child at the disputed ancestor's
            // content column or ends that ancestor's native walk. Deeper
            // markers alone cannot restore ownership we might have lost.
            if continuation_ceiling.is_some_and(|column| item.indent <= column) {
                continuation_ceiling = None;
            }
            continue;
        }
        let text = line.text(source);
        if text.trim_matches(' ').is_empty() {
            // Counting even a blank inside a literal is conservative: it can
            // only prevent us from certifying a later tight continuation.
            tight = false;
            continue;
        }
        while literals.get(literal).is_some_and(|range| range.end <= line.start) {
            literal += 1;
        }
        if literals.get(literal).is_some_and(|range| range.start <= line.start) {
            continue;
        }
        let Some(&owner) = open.last() else {
            continue;
        };
        let indent = text.len() - text.trim_start_matches(' ').len();
        // Native loose continuation can extend beyond a parser child's end.
        // At or left of every closed child's marker it either ends that
        // child's loose scope or is permitted tight under-indentation. Between
        // its marker and content columns it could still need a native fix.
        if closed_child_column.is_some_and(|marker| indent > marker)
            || continuation_ceiling.is_some_and(|column| indent > column)
        {
            return false;
        }
        let column = items[owner].content_column;
        if indent > column {
            return false;
        }
        if indent < column && (!tight || open.iter().any(|&item| items[item].content_column < indent)) {
            // Default MD077 accepts tight under-indentation. Requiring it to
            // be at or left of every ancestor's column rules out a native
            // over-indent on this line, independently of paragraph ownership.
            return false;
        }
        if indent < column && open.iter().any(|&item| items[item].content_column == indent) {
            // At an ancestor's exact content column, native lazy-continuation
            // detection might pop the deeper owner. Later prose must not
            // over-indent that ancestor until a suitable marker reopens it.
            continuation_ceiling = Some(continuation_ceiling.map_or(indent, |old| old.min(indent)));
        }
    }
    next_item == items.len()
}

/// Tables cannot change a list's native lazy-continuation boundary when they
/// are separate, unindented blocks with a blank on either side. Requiring an
/// explicit pipe at both edges of every row also gives the native list tracker
/// its own table boundary, independently of the primary parser's table event.
fn tables_are_isolated(source: &str, lines: &[Line], tables: &[Range<usize>], lists: &[List]) -> bool {
    let line_at = |byte: usize| lines.partition_point(|line| line.start <= byte).saturating_sub(1);
    let blank = |index: usize| lines[index].text(source).trim_matches(' ').is_empty();
    let mut next_list = 0;
    for table in tables {
        let start = line_at(table.start);
        let end = line_at(table.end.saturating_sub(1));
        if start > 0 && !blank(start - 1) || end + 1 < lines.len() && !blank(end + 1) {
            return false;
        }
        while lists.get(next_list).is_some_and(|list| list.range.end <= table.start) {
            next_list += 1;
        }
        if lists.get(next_list).is_some_and(|list| list.range.start < table.end) {
            return false;
        }
        for line in &lines[start..=end] {
            let row = line.text(source).trim_end_matches(' ');
            if !row.starts_with('|') || !row.ends_with('|') || !crate::utils::skip_context::is_table_line(row) {
                return false;
            }
        }
    }
    true
}

/// Positions where a lexical policy below can change its certificate. Searches
/// retain their next match so a rare delimiter is not searched for again from
/// every earlier match of another kind. Literal spans can advance the cursor
/// directly without visiting their interior candidates.
struct SourceCandidates<'a> {
    bytes: &'a [u8],
    next: [usize; 4],
}

impl<'a> SourceCandidates<'a> {
    fn find(bytes: &[u8], kind: usize, from: usize) -> usize {
        let tail = bytes.get(from..).unwrap_or_default();
        let found = match kind {
            0 => memchr::memchr3(b'@', b':', b')', tail),
            1 => memchr::memchr3(b'[', b']', b'{', tail),
            2 => memchr::memmem::find(tail, b"www."),
            3 => memchr::memmem::find(tail, b"  "),
            _ => unreachable!(),
        };
        found.map_or(usize::MAX, |at| from + at)
    }

    fn new(bytes: &'a [u8], from: usize) -> Self {
        Self {
            bytes,
            next: std::array::from_fn(|kind| Self::find(bytes, kind, from)),
        }
    }

    fn next_from(&mut self, from: usize) -> Option<usize> {
        for (kind, position) in self.next.iter_mut().enumerate() {
            if *position < from {
                *position = Self::find(self.bytes, kind, from);
            }
        }
        self.next.iter().copied().min().filter(|&at| at != usize::MAX)
    }
}

/// Native source classification can recognize a separator after trimming
/// whitespace that CommonMark leaves as text. This can change global heading
/// levels and styles even when the parser emitted no heading for the line.
/// Inspect the separator's edges, including after a container prefix, without
/// rejecting ordinary Unicode whitespace in prose.
fn has_unusual_separator_whitespace(text: &str) -> bool {
    let trimmed = text.trim_end();
    let Some(marker @ (b'=' | b'-' | b'*' | b'_')) = trimmed.as_bytes().last().copied() else {
        return false;
    };
    if trimmed.len() != text.trim_end_matches(' ').len() {
        return true;
    }
    trimmed
        .trim_end_matches(|c| c == char::from(marker) || c == ' ')
        .chars()
        .next_back()
        .is_some_and(char::is_whitespace)
}

/// A superset of the source candidates used by native table and list readers.
/// A pipe in literal text does not itself establish a table boundary. Check
/// the same row, delimiter and list-boundary predicates those readers use,
/// including every possible enclosing quote/list prefix. Context exclusions
/// can remove these candidates later, so retaining them here is conservative.
fn has_native_table_candidate(mut text: &str) -> bool {
    use crate::utils::table_utils::TableUtils;

    loop {
        if crate::utils::skip_context::is_table_line(text)
            || TableUtils::is_potential_table_row(text)
            || TableUtils::is_delimiter_row(text)
        {
            return true;
        }
        let after_quote = crate::utils::blockquote::strip_blockquote_prefix(text);
        let (list_prefix, after_list, _) = TableUtils::extract_list_prefix(after_quote);
        let next = if list_prefix.is_empty() {
            after_quote
        } else {
            after_list
        };
        if next.len() == text.len() {
            return false;
        }
        text = next;
    }
}

impl<'a> FormatProof<'a> {
    pub(crate) fn eligible(&self) -> bool {
        self.bits & 1 != 0
    }

    pub(crate) fn take_parsed(&mut self) -> Option<ParsedDocument<'a>> {
        self.parsed.take()
    }

    fn reject(&mut self, rules: &[u8]) {
        for &rule in rules {
            self.bits &= !(1u128 << rule);
        }
    }

    pub(crate) fn analyze(source: &'a str, flavor: MarkdownFlavor) -> Self {
        Self::analyze_with(source, flavor, |offsets, observe| {
            ParsedDocument::parse_for_format_proof(source, offsets, observe)
        })
    }

    pub(crate) fn analyze_handoff(
        source: &'a str,
        flavor: MarkdownFlavor,
        handoff: &mut Option<crate::lint_context::document_parser::handoff::OwnedParsedDocument>,
    ) -> Self {
        use crate::lint_context::document_parser::handoff::OwnedParsedDocument;

        if handoff.as_ref().is_some_and(|parsed| !parsed.matches(source)) {
            // Never replay facts, or build a native context, for other bytes.
            *handoff = None;
            return Self::default();
        }
        if !handoff
            .as_ref()
            .is_some_and(OwnedParsedDocument::has_format_proof_events)
        {
            return Self::default();
        }
        Self::analyze_with(source, flavor, |_, observe| {
            let mut parsed = handoff.take().expect("matching proof handoff");
            let events = parsed.take_format_proof_events().expect("captured proof observations");
            events.observe(|event, range| observe(event, range));
            (parsed.into_document(source), events.references)
        })
    }

    fn analyze_with(
        source: &'a str,
        flavor: MarkdownFlavor,
        parse: impl FnOnce(&[usize], &mut dyn FnMut(&Event<'a>, &Range<usize>)) -> (ParsedDocument<'a>, Vec<Range<usize>>),
    ) -> Self {
        // A settings override can change a policy even when its base instance
        // has a certificate. Flavors and multiline literal regions whose
        // interpretation differs from this compact model keep native analysis.
        if flavor != MarkdownFlavor::Standard || source.contains(['\r', '\0', '\t']) {
            return Self::default();
        }
        // Native display-math scanning can close at a delimiter inside a code
        // span, across blocks the primary parser keeps separate. Its line map
        // can then hide list markers and change which earlier item owns later
        // prose. Parser events alone cannot certify that source reading.
        if source.contains("$$") {
            return Self::default();
        }
        let first = source.lines().next().unwrap_or("");
        let front_matter_candidate = ["---", "+++", "{", "- --", "-- -"].iter().any(|marker| {
            first
                .strip_prefix(marker)
                .is_some_and(|tail| tail.chars().all(char::is_whitespace))
        });
        let front_matter_end = if front_matter_candidate {
            crate::rules::front_matter_utils::FrontMatterUtils::front_matter_boundary(source)
                .filter(|_| source.lines().nth(2).is_some())
                .map_or(0, |(_, end)| end)
        } else {
            0
        };
        if front_matter_candidate && front_matter_end == 0 {
            return Self::default();
        }

        let mut facts = Self {
            bits: u128::MAX,
            parsed: None,
        };
        // Native MD048 scans physical fence candidates even inside HTML
        // comments and other literal regions. Primary parser code-block events
        // alone cannot prove its source-wide style is uniform. A raw superset
        // containing both markers keeps the complete native style analysis.
        if memchr::memmem::find(source.as_bytes(), b"~~~").is_some()
            && memchr::memmem::find(source.as_bytes(), b"```").is_some()
        {
            facts.reject(&[48]);
        }
        // Enable/disable directives only remove edits from these uniform
        // default-policy certificates. Option overrides can change behavior.
        let has_config_candidates = crate::inline_config::inline_rule_option_candidate_offsets(source)
            .next()
            .is_some();
        let mut lines = Vec::new();
        let mut offset = 0;
        for text in source.lines() {
            lines.push(Line {
                start: offset,
                end: offset + text.len(),
            });
            offset += text.len() + 1;
        }
        let line_at = |byte: usize| lines.partition_point(|line| line.start <= byte).saturating_sub(1);
        let front_matter_byte_end = if front_matter_end > 0 {
            lines.get(front_matter_end).map_or(source.len(), |line| line.start)
        } else {
            0
        };
        if front_matter_end > 0 {
            let metadata = &source[..front_matter_byte_end];
            // Some native policies intentionally still inspect metadata's
            // links, fences and table-shaped text. Only simple scalar metadata
            // can use the opaque boundary; richer syntax keeps every native
            // policy and its original source view.
            if metadata.contains(['[', ']', '|', '`', '~', '<', '>', '$'])
                || metadata.contains("{{")
                || metadata.contains("{%")
            {
                return Self::default();
            }
        }
        let mut literals = Vec::<Range<usize>>::new();
        if front_matter_byte_end > 0 {
            literals.push(0..front_matter_byte_end);
        }
        let mut inline_code = Vec::<Range<usize>>::new();
        let mut links = Vec::<Range<usize>>::new();
        let mut unprotected_urls = Vec::<Range<usize>>::new();
        let mut fences = Vec::<Range<usize>>::new();
        let mut config_literals = Vec::<Range<usize>>::new();
        let mut tables = Vec::<Range<usize>>::new();
        let mut headings = Vec::<(usize, usize)>::new();
        let mut thematic_breaks = Vec::<usize>::new();
        let mut lists = Vec::<List>::new();
        let mut items = Vec::<Item>::new();
        let mut list_stack = Vec::<usize>::new();
        let mut quote_depth = 0;
        let mut fence_marker = None;
        let mut html_block = false;
        let mut unknown_html = false;
        let mut strong_style = None;

        // Match source_layout and the native guard: zero plus every LF end,
        // including the empty line after a final newline.
        let offsets: Vec<_> = std::iter::once(0)
            .chain(memchr::memchr_iter(b'\n', source.as_bytes()).map(|byte| byte + 1))
            .collect();
        let mut unsupported = false;
        let (parsed, references) = parse(&offsets, &mut |event, range| {
            // Continue native parsing for the handoff, but stop collecting a
            // certificate after its unsupported syntax has made it unusable.
            if unsupported {
                return;
            }
            if range.start < front_matter_byte_end {
                // A construct crossing the native metadata boundary has a
                // different source view; keep its complete native analysis.
                unsupported |= range.end > front_matter_byte_end;
                return;
            }
            match event {
                Event::Start(Tag::CodeBlock(kind)) => {
                    literals.push(range.clone());
                    if lines[line_at(range.start)].text(source).starts_with(' ') {
                        facts.reject(&[77]);
                    }
                    match kind {
                        CodeBlockKind::Fenced(_) => {
                            if has_config_candidates && list_stack.is_empty() && quote_depth == 0 {
                                config_literals.push(range.clone());
                            }
                            let text = &source[range.clone()];
                            let first_line = text.lines().next().unwrap_or("").trim_start();
                            let marker = first_line.as_bytes().first().copied().unwrap_or(0);
                            if fence_marker.replace(marker).is_some_and(|old| old != marker) {
                                facts.reject(&[48]);
                            }
                            let run = first_line.bytes().take_while(|&b| b == marker).count();
                            if crate::utils::fence_language::classify(&first_line[run..], flavor)
                                == crate::utils::fence_language::FenceLanguage::Missing
                            {
                                facts.reject(&[40]);
                            }
                            let mut block_lines = text.lines();
                            block_lines.next();
                            let closing = block_lines.last().unwrap_or("");
                            let closer = closing.trim_start_matches(' ');
                            let closer_run = closer.bytes().take_while(|&b| b == marker).count();
                            if !list_stack.is_empty()
                                || quote_depth > 0
                                || closing.len() - closer.len() > 3
                                || closer_run < run
                                || !closer[closer_run..].trim().is_empty()
                            {
                                facts.reject(&[46]);
                            }
                            if run != 3 {
                                facts.reject(&[70]);
                            }
                            fences.push(range.clone());
                        }
                        CodeBlockKind::Indented => facts.reject(&[46]),
                    }
                }
                Event::Code(_) => {
                    if has_config_candidates {
                        config_literals.push(range.clone());
                    }
                    let raw = &source[range.clone()];
                    let delimiter = raw.bytes().take_while(|&byte| byte == b'`').count();
                    let body_end = range.end.saturating_sub(delimiter);
                    let body = source.get(range.start + delimiter..body_end);
                    if delimiter == 0
                        || body.is_none_or(|body| {
                            crate::rules::MD038NoSpaceInCode::editable_code_span_edges(source, body, body_end).is_some()
                        })
                    {
                        facts.reject(&[38]);
                    }
                    inline_code.push(range.clone());
                }
                Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                    if dest_url.trim().is_empty() || dest_url.trim() == "#" {
                        facts.reject(&[42]);
                    }
                    // Native MD034 still reads the label of a reference link
                    // or image whose destination is empty. An empty inline
                    // destination can conservatively keep that native check too.
                    if dest_url.is_empty() {
                        unprotected_urls.push(range.clone());
                    }
                    links.push(range.clone());
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    let line = line_at(range.start);
                    if !list_stack.is_empty() || quote_depth > 0 {
                        facts.reject(&[1, 3, 18, 19, 20, 21, 22, 23, 25, 26]);
                    }
                    if !list_stack.is_empty() {
                        // Native list boundaries can end before a heading
                        // contained by the primary parser's item range.
                        facts.reject(&[4, 5, 7, 29, 30, 32, 69, 76, 77]);
                    }
                    headings.push((line, *level as usize));
                }
                Event::Start(Tag::Strong) => {
                    // MD050 consumes these same parser spans. A uniform
                    // superset stays uniform after its context exclusions.
                    let marker = source.as_bytes()[range.start];
                    if strong_style.replace(marker).is_some_and(|old| old != marker) {
                        facts.reject(&[50]);
                    }
                }
                Event::Start(Tag::List(number)) => {
                    if quote_depth > 0 {
                        facts.reject(&[4, 5, 7, 29, 30, 32, 69, 76, 77]);
                    }
                    let index = lists.len();
                    lists.push(List {
                        range: range.clone(),
                        depth: list_stack.len(),
                        ordered: number.is_some(),
                        start_value: number.unwrap_or(1),
                        items: Vec::new(),
                    });
                    list_stack.push(index);
                }
                Event::End(TagEnd::List(_)) => {
                    list_stack.pop();
                }
                Event::Start(Tag::Item) => {
                    let line = line_at(range.start);
                    let raw = lines[line].text(source);
                    let indent = raw.len() - raw.trim_start_matches(' ').len();
                    let marker = raw.trim_start_matches(' ');
                    let marker_len = if marker.starts_with(['-', '+', '*']) {
                        1
                    } else {
                        marker.bytes().take_while(u8::is_ascii_digit).count() + 1
                    };
                    let content_column = indent + marker_len + 1;
                    if range.start != lines[line].start + indent
                        || marker.as_bytes().get(marker_len) != Some(&b' ')
                        || marker.as_bytes().get(marker_len + 1).is_none_or(|&b| b == b' ')
                    {
                        // This compact item model reads one marker followed
                        // by one space at the physical line's indentation.
                        // Another item on the same line has its own marker
                        // and padding, which native MD030 still visits. Wide
                        // padding can also expose source-level boundaries to
                        // MD032 that differ from this parser's list range.
                        // Keep all native list policies for either layout.
                        facts.reject(&[4, 5, 7, 29, 30, 32, 69, 76, 77]);
                    }
                    let item = items.len();
                    items.push(Item {
                        range: range.clone(),
                        line,
                        indent,
                        content_column,
                    });
                    if let Some(&list) = list_stack.last() {
                        lists[list].items.push(item);
                    } else {
                        facts.reject(&[4, 5, 7, 29, 30, 32, 69, 76, 77]);
                    }
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    quote_depth += 1;
                    if !list_stack.is_empty() {
                        facts.reject(&[4, 5, 7, 27, 29, 30, 32, 69, 76, 77]);
                    }
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    quote_depth -= 1;
                }
                Event::Start(Tag::HtmlBlock) => {
                    html_block = true;
                    let raw = source[range.clone()].trim();
                    if raw.starts_with("<!--") && raw.ends_with("-->") {
                        literals.push(range.clone());
                        return;
                    }
                    if !raw.contains('\n')
                        && raw.starts_with('<')
                        && list_stack.is_empty()
                        && quote_depth == 0
                        && raw[1..]
                            .trim_start_matches('/')
                            .as_bytes()
                            .first()
                            .is_some_and(u8::is_ascii_alphabetic)
                        && raw.find('>') == Some(raw.len() - 1)
                        && !raw[1..raw.len() - 1].contains('<')
                        && !raw
                            .as_bytes()
                            .windows(8)
                            .any(|part| part.eq_ignore_ascii_case(b"markdown"))
                    {
                        literals.push(range.clone());
                        return;
                    }
                    if list_stack.is_empty()
                        && quote_depth == 0
                        && crate::utils::html_block::complete_tag_len(raw.lines().next().unwrap_or("")).is_some()
                        && !raw
                            .as_bytes()
                            .windows(8)
                            .any(|part| part.eq_ignore_ascii_case(b"markdown"))
                    {
                        // Keep reading the HTML's raw source below: policies
                        // such as MD049 have lexical candidates even where the
                        // structural parser emits no Markdown. Native heading
                        // and list views have no additional parser events here.
                        // MD048 independently reads raw fences, so one hidden
                        // in HTML still needs its native style calculation.
                        if raw.contains("```") || raw.contains("~~~") {
                            facts.reject(&[48]);
                        }
                        return;
                    }
                    unknown_html = true;
                    // Native dialect handling can reopen Markdown within an
                    // HTML block; only inline HTML is covered in this version.
                    facts.reject(&[
                        1, 3, 4, 5, 7, 9, 11, 12, 18, 19, 20, 21, 22, 23, 25, 26, 27, 29, 30, 31, 32, 34, 35, 36, 38,
                        39, 40, 42, 46, 48, 49, 50, 55, 56, 58, 62, 64, 65, 69, 70, 75, 76, 77,
                    ]);
                }
                Event::End(TagEnd::HtmlBlock) => {
                    html_block = false;
                }
                Event::InlineHtml(_) if !html_block => {
                    inline_code.push(range.clone());
                }
                Event::Rule => {
                    thematic_breaks.push(line_at(range.start));
                }
                Event::Start(Tag::Table(_)) => {
                    if list_stack.is_empty() && quote_depth == 0 {
                        tables.push(range.clone());
                    } else {
                        facts.reject(&[55, 56, 58, 75]);
                        if !list_stack.is_empty() {
                            facts.reject(&[4, 5, 7, 29, 30, 32, 69, 76, 77]);
                        }
                    }
                }
                Event::Start(Tag::FootnoteDefinition(_) | Tag::DefinitionList | Tag::MetadataBlock(_))
                | Event::InlineMath(_)
                | Event::DisplayMath(_) => {
                    // These share context with policy families beyond the
                    // parser's own view. An unknown revision proves nothing.
                    unsupported = true;
                }
                _ => {}
            }
        });
        facts.parsed = Some(parsed);
        if unsupported {
            facts.bits = 0;
            return facts;
        }
        for reference in &references {
            // MD034 skips only the definition's opening physical line. A URL
            // in its continued destination or title still gets the native
            // check, even though the parser's definition span contains it.
            if let Some(next) = lines.get(line_at(reference.start) + 1)
                && next.start < reference.end
            {
                unprotected_urls.push(next.start..reference.end);
            }
        }
        links.extend(references);

        // Ranges retain only source positions and are disjointed once here;
        // every lexical policy below consumes the same borrowed line slices.
        fn merge(ranges: &mut Vec<Range<usize>>) {
            ranges.sort_unstable_by_key(|range| range.start);
            let mut count = 0;
            for index in 0..ranges.len() {
                if count > 0 && ranges[index].start <= ranges[count - 1].end {
                    ranges[count - 1].end = ranges[count - 1].end.max(ranges[index].end);
                } else {
                    ranges[count] = ranges[index].clone();
                    count += 1;
                }
            }
            ranges.truncate(count);
        }
        merge(&mut literals);
        merge(&mut inline_code);
        merge(&mut links);
        merge(&mut unprotected_urls);
        merge(&mut config_literals);
        let contains = |ranges: &[Range<usize>], pos: usize| {
            let end = ranges.partition_point(|range| range.start <= pos);
            end > 0 && pos < ranges[end - 1].end
        };
        if has_config_candidates
            && (unknown_html
                || crate::inline_config::inline_rule_option_candidate_offsets(source)
                    .any(|offset| !contains(&config_literals, offset)))
        {
            facts.bits = 0;
            return facts;
        }
        let is_blank = |index: usize| {
            lines
                .get(index)
                .is_some_and(|line| line.text(source).trim_matches(' ').is_empty())
        };
        let mut blank_run = 0;
        let mut hr_style = None;
        let mut raw_prompt_block = false;
        let mut raw_prompt_opener = "";
        let mut raw_prompt_lines = Vec::new();
        let mut code_index = 0;
        let mut link_index = 0;
        let mut item_lines: Vec<_> = items.iter().map(|item| item.line).collect();
        let mut item_ranges: Vec<_> = items.iter().map(|item| item.range.clone()).collect();
        merge(&mut item_ranges);
        let mut emphasis_markers = Vec::new();
        let mut emphasis_style = None;
        // Native list coverage and table confirmation share source boundaries.
        // Disjoint primary ranges alone do not settle native lazy continuation.

        let isolated_tables = tables_are_isolated(source, &lines, &tables, &lists);

        if !lists.is_empty() && !tables.is_empty() && !isolated_tables {
            facts.reject(&[4, 5, 7, 29, 30, 32, 69, 76, 77]);
        }
        item_lines.sort_unstable();
        item_lines.dedup();
        for (index, line) in lines.iter().copied().enumerate() {
            let text = line.text(source);
            let trimmed = text.trim_start_matches(' ');
            // MD035 reads source candidates, including ones the parser places
            // in a table or Setext heading. Its context filters only remove
            // candidates, so a uniform, unindented superset cannot need an
            // edit. Indentation or different source styles keeps native MD035.
            if crate::rules::MD035HRStyle::is_horizontal_rule(text) {
                let style = text.trim();
                if text.len() != text.trim_start().len() || hr_style.replace(style).is_some_and(|old| old != style) {
                    facts.reject(&[35]);
                }
            }
            // MD014 deliberately has a raw triple-backtick reading; it is
            // not interchangeable with the structural parser's code blocks.
            if text.trim_start().starts_with("```") {
                if raw_prompt_block {
                    if crate::rules::MD014CommandsShowOutput::default_block_would_edit(
                        raw_prompt_opener,
                        &raw_prompt_lines,
                    ) {
                        facts.reject(&[14]);
                    }
                    raw_prompt_lines.clear();
                } else {
                    raw_prompt_opener = text;
                }
                raw_prompt_block = !raw_prompt_block;
            } else if raw_prompt_block {
                raw_prompt_lines.push(text);
            }
            if text.trim_end_matches(' ').len() != text.len() {
                facts.reject(&[9]);
            }
            if contains(&literals, line.start + text.len().saturating_sub(trimmed.len())) {
                blank_run = 0;
                continue;
            }
            if has_unusual_separator_whitespace(text) {
                facts.bits = 0;
                return facts;
            }
            if trimmed.is_empty() {
                blank_run += 1;
                if blank_run > 1 {
                    facts.reject(&[12]);
                }
                continue;
            }
            blank_run = 0;
            // Native heading detection reads source-level Setext candidates,
            // including table boundaries the event parser classifies as a rule.
            // Without that complete native paragraph/container view, the event
            // list cannot certify any heading policy on this revision.
            let heading_source =
                crate::utils::blockquote::parse_blockquote_prefix(text).map_or(text, |prefix| prefix.content);
            if crate::lint_context::is_setext_underline_content(heading_source) {
                facts.reject(&[1, 3, 18, 19, 20, 21, 22, 23, 25, 26]);
                if !lists.is_empty() {
                    facts.reject(&[4, 5, 7, 29, 30, 32, 69, 76, 77]);
                }
            }
            if facts.bits & (1u128 << 49) != 0 && text.contains(['*', '_']) {
                // Use MD049's lexical pairing, rather than inferring its
                // source-aware behavior from the Markdown emphasis events.
                // Its later context filters only remove these candidates.
                use crate::utils::emphasis_utils::{
                    find_emphasis_markers_into, find_single_emphasis_edges, replace_inline_code_cow,
                };
                let without_code = replace_inline_code_cow(text);
                emphasis_markers.clear();
                find_emphasis_markers_into(&without_code, &mut emphasis_markers);
                for span in find_single_emphasis_edges(&without_code, &emphasis_markers) {
                    let marker = span.opening.as_char();
                    if emphasis_style.replace(marker).is_some_and(|old| old != marker) {
                        facts.reject(&[49]);
                    }
                }
            }
            let in_table = contains(&tables, line.start);

            let possible_native_table = text.contains('|') && has_native_table_candidate(text);

            if possible_native_table && !in_table {
                facts.reject(&[55, 56, 58, 75]);
                // Native list coverage treats table-shaped source as a
                // boundary even without a confirmed parser table. After list
                // padding changes, that boundary can expose a required blank
                // before a later item that the event parser still groups with
                // the first list. Keep the native list view on this revision.
                if !lists.is_empty() {
                    facts.reject(&[4, 5, 7, 29, 30, 32, 69, 76, 77]);
                }
            }
            let table_spacing = in_table && crate::utils::skip_context::is_table_line(text);
            if trimmed.starts_with('#') {
                let hashes = trimmed.bytes().take_while(|&byte| byte == b'#').count();
                if hashes <= 6 {
                    let body = &trimmed[hashes..];
                    if !body.is_empty() && !body.starts_with(' ') {
                        facts.reject(&[18, 20]);
                    }
                    if body.starts_with("  ") {
                        facts.reject(&[19]);
                    }
                    if text.len() != trimmed.len() {
                        facts.reject(&[23]);
                    }
                    let tail = body.trim_end();
                    if tail.ends_with('#') {
                        // Closing ATX syntax is uncommon and has independent
                        // source-spacing rules. Let those rules settle it.
                        facts.reject(&[3, 20, 21, 26]);
                    }
                }
            }
            if text.trim_start().starts_with('>') {
                if let Some(prefix) = crate::utils::blockquote::parse_blockquote_prefix(text) {
                    if prefix.spaces_after_marker.len() > 1 || prefix.prefix.contains("  ") {
                        facts.reject(&[27]);
                    }
                    let body = prefix.content.trim_start_matches(' ');
                    let possible_bullet =
                        body.starts_with(['-', '+', '*']) && body[1..].starts_with(char::is_whitespace);
                    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
                    let possible_ordered = digits > 0 && matches!(body.as_bytes().get(digits), Some(b'.' | b')'));
                    if possible_bullet || possible_ordered {
                        facts.reject(&[4, 5, 7, 29, 30, 32, 69, 76, 77]);
                    }
                    if body.starts_with('#') || possible_bullet || possible_ordered {
                        facts.reject(&[1, 3, 18, 19, 20, 21, 22, 23, 25, 26]);
                    }
                } else {
                    // Native malformed-quote repair also accepts leading
                    // Unicode whitespace that CommonMark treats as text.
                    facts.reject(&[27]);
                }
            }

            let bytes = text.as_bytes();
            let indent = text.len() - trimmed.len();
            if indent > 0 && item_lines.binary_search(&index).is_err() && !contains(&item_ranges, line.start) {
                // Native MD077 also owns under-indented loose continuation
                // beyond the parser's item range. Keep its source-aware walk.
                facts.reject(&[77]);
            }
            let mut at = indent;
            let mut candidates = SourceCandidates::new(bytes, at);
            while let Some(candidate) = candidates.next_from(at) {
                at = candidate;
                let absolute = line.start + at;
                while inline_code.get(code_index).is_some_and(|range| range.end <= absolute) {
                    code_index += 1;
                }
                if let Some(range) = inline_code.get(code_index).filter(|range| range.start <= absolute) {
                    at = (range.end - line.start).min(bytes.len());
                    continue;
                }
                while links.get(link_index).is_some_and(|range| range.end <= absolute) {
                    link_index += 1;
                }
                let in_link = links.get(link_index).is_some_and(|range| range.start <= absolute);
                let url_is_protected = in_link && !contains(&unprotected_urls, absolute);
                match bytes[at] {
                    // Standard's native code-span ranges come from these
                    // same primary parser events. Unmatched source backticks
                    // cannot add a native MD038 edit. Its context exclusions
                    // only remove spans; other flavors keep native analysis.
                    b'@' if !url_is_protected => facts.reject(&[34]),
                    b':' if !url_is_protected && (text[at..].starts_with("://") || text[..at].ends_with("xmpp")) => {
                        facts.reject(&[34])
                    }
                    b'w' if text[at..].starts_with("www.") && !url_is_protected => facts.reject(&[34]),
                    b')' if bytes.get(at + 1) == Some(&b'[') => facts.reject(&[11]),
                    b'[' => {
                        if bytes.get(at + 1).is_none_or(u8::is_ascii_whitespace) {
                            facts.reject(&[39, 42]);
                        }
                    }
                    b']' => {
                        if at == 0 || bytes[at - 1].is_ascii_whitespace() {
                            facts.reject(&[39]);
                        }
                        let after = &text[at + 1..];
                        if after.starts_with("()") || after.starts_with("(#)") {
                            facts.reject(&[42]);
                        }
                        if let Some(destination) = after.strip_prefix('(') {
                            if destination.starts_with(char::is_whitespace) {
                                facts.reject(&[62]);
                            }
                            if let Some(end) = destination.find(')') {
                                let raw = &destination[..end];
                                // Titles and complex destinations keep the
                                // native source-aware whitespace policy.
                                if raw.chars().any(char::is_whitespace) || raw.contains(['(', '\\', '<']) {
                                    facts.reject(&[62]);
                                }
                            } else {
                                facts.reject(&[62]);
                            }
                        } else if after.trim_start().starts_with('(') {
                            facts.reject(&[62]);
                        }
                    }
                    b' ' if bytes.get(at + 1) == Some(&b' ') && !table_spacing => facts.reject(&[64]),
                    b'{' if matches!(bytes.get(at + 1), Some(b'{' | b'%'))
                        && (!text[at..].starts_with("{{#") || !text[at..].contains("}}")) =>
                    {
                        // A complete mdBook directive has no quoted Jinja
                        // payload. Other template syntax keeps native analysis;
                        // delimiters in code and comments were skipped above.
                        facts.bits = 0;
                        return facts;
                    }
                    _ => {}
                }
                // Move by bytes, but slice only at ASCII bytes above. A `w`
                // and a `]` are always valid UTF-8 boundaries.
                at += 1;
            }
            // An apparent marker not matched by a structural item can still
            // be a malformed construct the native spacing policies recognize.
            if trimmed.starts_with(['-', '+', '*'])
                && trimmed.as_bytes().get(1).is_some_and(u8::is_ascii_whitespace)
                && item_lines.binary_search(&index).is_err()
            {
                facts.reject(&[4, 5, 7, 30, 32, 69, 76, 77]);
            }
            // A non-one ordered marker cannot interrupt a CommonMark
            // paragraph, but MD032 deliberately recognizes that source and
            // can insert a blank before it. Earlier parsed lists do not make
            // the later, unparsed marker part of their native coverage.
            // Retain the native list readers for these apparent markers.
            let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
            let apparent_ordered = digits > 0
                && matches!(trimmed.as_bytes().get(digits), Some(b'.' | b')'))
                && trimmed[digits + 1..].starts_with(char::is_whitespace);
            // Native regexes accept Unicode decimal digits and whitespace.
            // Those uncommon prefixes keep native analysis as well; no
            // ASCII-only approximation may certify their absence.
            let unusual_ordered_prefix = trimmed.as_bytes().first().is_some_and(|byte| !byte.is_ascii())
                && trimmed.trim_start().chars().next().is_some_and(char::is_numeric);
            if (apparent_ordered || unusual_ordered_prefix) && item_lines.binary_search(&index).is_err() {
                facts.reject(&[4, 5, 7, 29, 30, 32, 69, 76, 77]);
            }
        }
        if lines
            .last()
            .is_some_and(|line| line.text(source).trim_matches(' ').is_empty())
        {
            facts.reject(&[12]);
        }
        if !source.is_empty() && !source.ends_with('\n') {
            facts.reject(&[47]);
        }

        if front_matter_end > 0 && front_matter_end < lines.len() && !is_blank(front_matter_end) {
            facts.reject(&[71]);
        }
        let mut last_level = usize::from(crate::rules::MD001HeadingIncrement::default().has_front_matter_title(source));
        // Native metadata detection stops at its closing delimiter.
        let mut h1 = if front_matter_end > 0 {
            let metadata: Vec<_> = lines
                .iter()
                .take(front_matter_end)
                .map(|line| line.text(source))
                .collect();
            usize::from(crate::rules::MD025SingleTitle::default_front_matter_lines_have_title(
                &metadata,
            ))
        } else {
            0
        };
        for &(line, level) in &headings {
            let text = lines[line].text(source);
            let hashes = text.bytes().take_while(|&byte| byte == b'#').count();
            // A Setext title can itself start with the same number of hashes
            // as its level. Only a legal ATX opener establishes the style.
            if hashes != level || text.as_bytes().get(hashes).is_some_and(|&byte| byte != b' ') {
                facts.reject(&[3, 23, 26]);
            }
            if last_level > 0 && level > last_level + 1 {
                facts.reject(&[1]);
            }
            last_level = level;
            h1 += usize::from(level == 1);
            if line > 0 && !is_blank(line - 1) || line + 1 < lines.len() && !is_blank(line + 1) {
                facts.reject(&[22]);
            }
            // Generic types in inline code do not change the heading's text.
            // Actual anchors and IDs can expose trailing punctuation, so use
            // native heading extraction for either spelling rather than
            // rejecting every heading that contains an angle/curly bracket.
            let display_text = text
                .contains(['{', '<'])
                .then(|| crate::lint_context::atx_display_text_for_proof(text));
            let punctuation_candidate = match display_text {
                Some(Some(ref display)) => display.as_str(),
                Some(None) => {
                    facts.reject(&[26]);
                    text
                }
                None => text,
            };
            if crate::rules::MD026NoTrailingPunctuation::default_source_has_trailing_punctuation(punctuation_candidate)
            {
                facts.reject(&[26]);
            }
        }
        if h1 > 1 {
            facts.reject(&[25]);
        }
        for table in &tables {
            use crate::utils::table_utils::TableUtils;
            let start = line_at(table.start);
            let end = line_at(table.end.saturating_sub(1));
            let header = lines[start].text(source);
            // The event parser can accept a header that the native table
            // finder rejects, for example an unmatched backtick before its
            // pipe. Native analysis may then open a different table on the
            // following delimiter-shaped rows. Its boundaries and spacing
            // cannot be inferred from this parser table's outer range.
            if !TableUtils::is_potential_table_row(header)
                || !lines
                    .get(start + 1)
                    .is_some_and(|line| TableUtils::is_delimiter_row(line.text(source)))
            {
                facts.reject(&[55, 56, 58, 75]);
            }
            let columns = TableUtils::count_cells_with_flavor(header, flavor);
            let style = TableUtils::determine_pipe_style(header);
            for line in lines.iter().take(end + 1).skip(start) {
                let row = line.text(source);
                if line.start > lines[start + 1].start && TableUtils::starts_block_after_table(row) {
                    // Native tables stop at extension container openers such
                    // as ::: even when the primary parser emits another row.
                    facts.reject(&[55, 56, 58, 75]);
                }
                if row.starts_with(' ') {
                    facts.reject(&[55, 56, 58, 75]);
                }
                if columns == 0 || TableUtils::count_cells_with_flavor(row, flavor) != columns {
                    facts.reject(&[56]);
                }
                if style.is_none() || TableUtils::determine_pipe_style(row) != style {
                    facts.reject(&[55]);
                }
            }
            if start > 0 && !is_blank(start - 1) || end + 1 < lines.len() && !is_blank(end + 1) {
                facts.reject(&[58]);
            }
        }
        for &line in &thematic_breaks {
            if line > 0 && !is_blank(line - 1) || line + 1 < lines.len() && !is_blank(line + 1) {
                facts.reject(&[65]);
            }
        }
        for fence in &fences {
            let start = line_at(fence.start);
            let end = line_at(fence.end.saturating_sub(1));
            if start > 0 && !is_blank(start - 1) || end + 1 < lines.len() && !is_blank(end + 1) {
                facts.reject(&[31]);
            }
            // A fence-like line may sit behind quote markers and indentation;
            // MD070 judges it relative to its container, so any such prefix
            // here must decline the proof rather than guess where it ends.
            for line in lines.iter().take(end).skip(start + 1) {
                let text = line.text(source).trim_start_matches(['>', ' ', '\t']);
                if text.starts_with("```") || text.starts_with("~~~") {
                    facts.reject(&[70]);
                }
            }
        }

        let mut bullet = None;
        let mut strict_continuations = true;
        for list in &lists {
            let mut previous_indent = None;
            let mut next_number = list.start_value;
            let all_ones = list.items.iter().all(|&item_index| {
                let raw = lines[items[item_index].line].text(source).trim_start_matches(' ');
                let digits = raw.bytes().take_while(u8::is_ascii_digit).count();
                raw[..digits].parse::<u64>().ok() == Some(1)
            });
            let mut spacing = None;
            for (position, &item_index) in list.items.iter().enumerate() {
                let item = &items[item_index];
                let raw = lines[item.line].text(source).trim_start_matches(' ');
                if list.depth == 0 && item.indent != 0 {
                    facts.reject(&[5]);
                }
                if previous_indent
                    .replace(item.indent)
                    .is_some_and(|old| old != item.indent)
                {
                    facts.reject(&[5]);
                }
                if list.ordered {
                    let digits = raw.bytes().take_while(u8::is_ascii_digit).count();
                    if raw[..digits].parse::<u64>().ok() != Some(if all_ones { 1 } else { next_number }) {
                        facts.reject(&[29]);
                    }
                    next_number += 1;
                    if list.depth > 0 {
                        facts.reject(&[7, 77]);
                    }
                } else {
                    let marker = raw.as_bytes().first().copied();
                    if bullet.replace(marker).is_some_and(|old| old != marker) {
                        facts.reject(&[4]);
                    }
                    if item.indent != list.depth * 2 {
                        facts.reject(&[7]);
                    }
                }
                let body = lines[item.line].text(source).get(item.content_column..).unwrap_or("");
                if body.starts_with(['#', '>'])
                    || body.starts_with(['-', '+', '*']) && body[1..].starts_with(char::is_whitespace)
                {
                    facts.reject(&[18, 19, 20, 21, 22, 23, 69]);
                }
                if position > 0 {
                    let loose = item.line > 0 && is_blank(item.line - 1);
                    if spacing.replace(loose).is_some_and(|old| old != loose) {
                        facts.reject(&[76]);
                    }
                }
                if facts.bits & (1u128 << 77) != 0 && strict_continuations {
                    let end = line_at(item.range.end.saturating_sub(1));
                    for (line_index, line) in lines.iter().enumerate().take(end + 1).skip(item.line + 1) {
                        let text = line.text(source);
                        if is_blank(line_index)
                            || contains(&literals, line.start)
                            || item_lines.binary_search(&line_index).is_ok()
                        {
                            continue;
                        }
                        let indent = text.len() - text.trim_start_matches(' ').len();
                        if indent != item.content_column {
                            strict_continuations = false;
                            break;
                        }
                    }
                }
            }
            if list.depth == 0 {
                let first = line_at(list.range.start);
                let mut last = line_at(list.range.end.saturating_sub(1));
                // A list's parser range may include its following blank line.
                // MD032 measures the boundary from its final nonblank line.
                while last > first && is_blank(last) {
                    last -= 1;
                }
                if first > 0 && !is_blank(first - 1) || last + 1 < lines.len() && !is_blank(last + 1) {
                    facts.reject(&[32]);
                }
            }
        }
        if !strict_continuations
            && facts.bits & (1 << 77) != 0
            && !nested_continuations_are_unchanged(source, &lines, &items, &literals)
        {
            facts.reject(&[77]);
        }
        facts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::lint_context::LintContext;
    use crate::rule::FixCapability;
    use proptest::prelude::*;

    #[test]
    fn unconditional_policies_remain_configuration_free() {
        for rule in crate::rules::all_rules(&Config::default()) {
            if [
                "MD011", "MD020", "MD021", "MD023", "MD034", "MD039", "MD042", "MD047", "MD056", "MD062", "MD069",
                "MD070", "MD075", "MD086",
            ]
            .contains(&rule.name())
            {
                assert!(
                    rule.default_config_section()
                        .is_none_or(|(_, value)| value.as_table().is_some_and(toml::map::Map::is_empty)),
                    "{} gained options; qualify its policy",
                    rule.name()
                );
                assert!(
                    NativeFormatPolicy::validated(rule.as_ref()).is_some(),
                    "{} has the wrong policy owner",
                    rule.name()
                );
            }
        }
    }

    #[test]
    fn whitespace_padded_destinations_keep_the_native_empty_link_fixer() {
        for source in [
            "[https://x.com](< >)\n",
            "[https://x.com](< # >)\n",
            "![https://x.com](<  >)\n",
            "[https://x.com][r]\n\n[r]: < >\n",
            "[text](< >)\n",
        ] {
            assert_proofs(source);
        }
    }

    #[test]
    fn complex_destinations_metadata_and_inline_html_keep_native_fixers() {
        for source in [
            "[a](b(c) )\n",
            "[a](b\\) )\n",
            "![a](b(c) )\n",
            "[a](b(c))\n",
            "+++\ntitle = \"Book\"\n+++\n\n### Section\n",
            "{\n\"title\": \"Book\"\n}\n\n### Section\n",
            "text <!-- https://x.com -->\n",
            "<span title=\"https://x.com\">a</span>\n",
            "text <!-- first\nhttps://x.com\nlast -->\n",
        ] {
            assert!(
                proof_failures(source).is_empty(),
                "{source:?}: {:?}",
                proof_failures(source)
            );
        }
    }

    fn proof_failures(source: &str) -> Vec<String> {
        proof_failures_for_flavor(source, MarkdownFlavor::Standard)
    }

    fn proof_failures_for_flavor(source: &str, flavor: MarkdownFlavor) -> Vec<String> {
        // A policy never certifies a source the complete model rejected.

        let facts = FormatProof::analyze(source, flavor);
        let config = Config::default();
        let rules = crate::rules::all_rules(&config);
        let mut context = None;
        let mut failures = Vec::new();

        if !source.contains(['\r', '\0', '\t']) {
            let (parsed, _, events) = ParsedDocument::parse_for_proof_guard(source, flavor);
            let mut handoff = Some(parsed.into_handoff().with_format_proof_events(events));
            let replayed = FormatProof::analyze_handoff(source, flavor, &mut handoff);
            if replayed.bits != facts.bits {
                failures.push(format!("proof replay mismatch: {source:?}"));
            }
        }
        for rule in rules {
            if rule.fix_capability() == FixCapability::Unfixable
                || !NativeFormatPolicy::validated(rule.as_ref()).is_some_and(|policy| policy.proven(&facts))
            {
                continue;
            }
            let ctx = context.get_or_insert_with(|| LintContext::new(source, flavor, None));
            let warnings = rule.check(ctx).unwrap();
            if warnings.is_empty() {
                continue;
            }
            let fixed = rule.fix(ctx).unwrap();
            if fixed != source {
                failures.push(format!(
                    "{} incorrectly proved unchanged: {source:?}; native result: {fixed:?}",
                    rule.name()
                ));
            }
        }
        failures
    }

    #[test]
    fn reference_url_certificates_follow_native_line_and_destination_guards() {
        use crate::rule::Rule;

        let rule = crate::rules::MD034NoBareUrls;
        let policy = NativeFormatPolicy::validated(&rule).unwrap();
        for source in [
            "[x]:\n  https://example.com\n",
            "[x]: https://example.com\n  \"name@example.com\"\n",
            "> [x]:\n>   https://example.com\n",
            "![www.example.com][r]\n\n[r]: <>\n",
            "[https://example.com][r]\n\n[r]: <>\n",
        ] {
            let context = LintContext::new(source, MarkdownFlavor::Standard, None);
            assert_ne!(rule.fix(&context).unwrap(), source);
            assert!(!policy.proven(&FormatProof::analyze(source, MarkdownFlavor::Standard)));
            assert_proofs(source);
        }
        for source in [
            "[x]: https://example.com\n",
            "[x]:\n  /path\n",
            "[https://example.com](#)\n",
            "[https://example.com](< >)\n",
            "[https://example.com][r]\n\n[r]: target\n",
            "![name@example.com][r]\n\n[r]: target\n",
        ] {
            let context = LintContext::new(source, MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&context).unwrap(), source);
            assert!(policy.proven(&FormatProof::analyze(source, MarkdownFlavor::Standard)));
            assert_proofs(source);
        }
    }

    #[test]
    fn source_display_math_keeps_native_list_ownership() {
        use crate::rule::Rule;

        let source = "1. a\n\n$$x\n- root\n  see `$$`\n  more\n";
        let expected = "1. a\n\n$$x\n- root\n   see `$$`\n   more\n";
        let context = LintContext::new(source, MarkdownFlavor::Standard, None);
        let rule = crate::rules::MD077ListContinuationIndent::default();
        let warnings = rule.check(&context).unwrap();
        assert_eq!(warnings.len(), 2);
        assert_eq!(rule.fix(&context).unwrap(), expected);
        assert!(!FormatProof::analyze(source, MarkdownFlavor::Standard).eligible());
        assert_proofs(source);
    }

    #[test]
    fn nested_default_continuations_can_be_proven_unchanged() {
        use crate::rule::Rule;

        for source in [
            "- outer\n  - inner\n    nested prose\n",
            "- outer\n  - inner\nlazy continuation\n",
            "- outer\n  - inner\n  ancestor column\n  - sibling\n    sibling prose\n",
            "- outer\n  - middle\n    - inner\n      nested prose\n\n    middle prose\n\n  outer prose\n",
        ] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(NativeFormatPolicy::new(77).proven(&facts), "{source:?}");
            let context = LintContext::new(source, MarkdownFlavor::Standard, None);
            let rule = crate::rules::MD077ListContinuationIndent::default();
            assert!(rule.check(&context).unwrap().is_empty(), "{source:?}");
            assert_proofs(source);
        }
    }

    #[test]
    fn ambiguous_loose_nested_continuations_keep_native_fixes() {
        use crate::rule::Rule;

        for source in [
            "- outer\n  - inner\n\n   escaped child\n",
            "- outer\n  - inner\n\n     over-indented child\n",
            "- outer\n\n   over-indented outer\n",
        ] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(!NativeFormatPolicy::new(77).proven(&facts), "{source:?}");
            let context = LintContext::new(source, MarkdownFlavor::Standard, None);
            let rule = crate::rules::MD077ListContinuationIndent::default();
            assert_ne!(rule.fix(&context).unwrap(), source, "{source:?}");
            assert_proofs(source);
        }
    }

    #[test]
    fn ancestor_aligned_lazy_boundaries_keep_native_ownership() {
        for source in [
            "- outer\n  - inner\n    [r]: /x\n  parent column\n    child column\n",
            "- outer\n  - inner\n  [r]: /x\n  parent column\n    child column\n",
            "- outer\n  - inner\n  parent column\n    - deeper marker\n\n    inner prose\n",
        ] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(!NativeFormatPolicy::new(77).proven(&facts), "{source:?}");
            assert_proofs(source);
        }
    }

    #[test]
    fn unchanged_nested_lists_avoid_full_context() {
        let source = "- outer\n  - inner\n    nested prose\n";
        let config = Config::default();
        let rules = crate::rules::filter_rules(&crate::rules::all_rules(&config), &config.global);
        let mut content = source.to_owned();
        let result = crate::fix_coordinator::FixCoordinator::new()
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 100, None)
            .unwrap();

        assert_eq!(content, source);
        assert_eq!(result.context_creations, 0);
        assert!(result.converged);
    }

    #[test]
    fn native_regressions_preserve_certificates() {
        for original in super::native_regressions::SOURCES {
            for flavor in ALL_FLAVORS {
                for ending in ["\n", "\r\n", "\r", "\n\r\n"] {
                    let source = original.replace('\n', ending);
                    let failures = proof_failures_for_flavor(&source, flavor);
                    assert!(failures.is_empty(), "{flavor}: {}", failures.join("\n"));
                }
            }
        }
    }

    fn assert_proofs(source: &str) {
        let failures = proof_failures(source);
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn raw_fence_markers_in_literal_regions_keep_native_style_analysis() {
        use crate::rule::Rule;

        let regression = "<!-- comment\n~~~\n~~~\n```rust\n-->\n";
        let context = LintContext::new(regression, MarkdownFlavor::Standard, None);
        let rule = crate::rules::MD048CodeFenceStyle::from_config(&Config::default());
        assert!(!rule.check(&context).unwrap().is_empty());
        assert_ne!(rule.fix(&context).unwrap(), regression);
        for source in [
            regression,
            "<!-- comment\n~~~\n~~~\n-->\n\n```rust\ncode\n```\n",
            "<!-- comment\n```rust\n~~~\n~~~\n-->\n",
            "prefix <!-- comment\n~~~\n~~~\n-->\n\n```rust\ncode\n```\n",
            "`inline\n~~~\n~~~\n`\n\n```rust\ncode\n```\n",
            "<div>\n~~~\n~~~\n</div>\n\n```rust\ncode\n```\n",
            "```markdown\n~~~rust\ncode\n~~~\n```\n",
        ] {
            assert!(!NativeFormatPolicy::new(48).proven(&FormatProof::analyze(source, MarkdownFlavor::Standard)));
            assert_proofs(source);
        }
        for source in ["~~~rust\ncode\n~~~\n", "```rust\ncode\n```\n"] {
            assert!(NativeFormatPolicy::new(48).proven(&FormatProof::analyze(source, MarkdownFlavor::Standard)));
            assert_proofs(source);
        }
    }

    #[test]
    fn native_list_markers_keep_same_line_items_and_wide_padding_fallbacks() {
        for source in [
            "1. -    item\n\n\n           ```rust\n           body\n           ```\n\nAfter.\n",
            "- -   item\n\n  continuation\n",
            "*     item\n    text\n    ===\n\nAfter.\n",
            "-     item\n   text\n   ---\n\nAfter.\n",
        ] {
            assert_proofs(source);
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            for policy in [30, 32] {
                assert!(
                    !NativeFormatPolicy::new(policy).proven(&facts),
                    "MD{policy:03}: {source:?}"
                );
            }
        }
    }

    #[test]
    fn structural_code_span_edges_keep_native_no_edit_certificates() {
        for source in [
            "Text `a\n` tail\n",
            "Text ` \na` tail\n",
            "> Text `a\n>     ` tail\n",
            ">> Text `a\n>>     ` tail\n",
            "text\n    ```\n    x\n    ```\n",
            "Prose ``  `literal`  `` here.\n",
        ] {
            assert_proofs(source);
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(NativeFormatPolicy::new(38).proven(&facts), "{source:?}");
        }
        for source in [
            "Text `a\nb ` tail\n",
            "> text `a\n> b ` tail\n",
            "- text `a\n  b ` tail\n",
            "Text ` a\n    >` tail\n",
        ] {
            assert_proofs(source);
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(!NativeFormatPolicy::new(38).proven(&facts), "{source:?}");
        }
        for delimiter in ["`", "``", "````"] {
            for prefix in ["Text ", "> Text ", ">> Text ", "- Text ", "  Text "] {
                for leading in ["", " ", "  ", "\n", " \n", "\u{a0}"] {
                    for trailing in ["", " ", "  ", "\n", "\n>   ", "\u{2003}"] {
                        for body in ["value", "α🦀", "a\nb", "a\n> b", "`nested`"] {
                            assert_proofs(&format!(
                                "{prefix}{delimiter}{leading}{body}{trailing}{delimiter} tail\n"
                            ));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn vectorized_candidates_preserve_lexical_positions_and_literal_jumps() {
        fn expected(bytes: &[u8], from: usize) -> Option<usize> {
            (from..bytes.len()).find(|&at| match bytes[at] {
                b'@' | b':' | b')' | b'[' | b']' | b'{' => true,
                b'w' => bytes[at..].starts_with(b"www."),
                b' ' => bytes.get(at + 1) == Some(&b' '),
                _ => false,
            })
        }
        let fragments = [
            "plain prose with words and single spaces",
            "é🦀@host www.example.com xmpp:target )[]{}",
            "wwwww.www.  a   b    ",
            "`[ ignored ](www.example.com)  ` tail",
            "> quoted [link](target) and { template }",
            "<!-- [comment] @ www. -->",
            "\0\r\n\t\u{a0}\u{2003}",
            "",
        ];
        for left in fragments {
            for right in fragments {
                let source = format!("{left}{right}");
                let bytes = source.as_bytes();
                for start in 0..=bytes.len() {
                    for jump in [1, 2, 7, 31, 64] {
                        let mut positions = SourceCandidates::new(bytes, start);
                        let mut from = start;
                        loop {
                            let found = positions.next_from(from);
                            assert_eq!(found, expected(bytes, from), "{source:?}; from={from}; jump={jump}");
                            let Some(at) = found else { break };
                            from = at.saturating_add(jump).min(bytes.len());
                        }
                    }
                }
                assert_proofs(&source);
            }
        }
    }

    #[test]
    fn heading_punctuation_certificates_use_the_native_default_set() {
        for ending in ['?', '。', '，', '；', '：', '！', '？', 'α', '🦀'] {
            for suffix in ["", " ", "  ", "\u{a0}", "\u{2003}"] {
                let source = format!("# A question{ending}{suffix}\n\nPlain prose.\n");
                assert_proofs(&source);
                assert!(
                    NativeFormatPolicy::new(26).proven(&FormatProof::analyze(&source, MarkdownFlavor::Standard)),
                    "{source:?}"
                );
            }
        }
        for ending in ['.', ',', ';', ':', '!'] {
            let source = format!("# Heading{ending}\n\nPlain prose.\n");
            assert_proofs(&source);
            assert!(
                !NativeFormatPolicy::new(26).proven(&FormatProof::analyze(&source, MarkdownFlavor::Standard)),
                "{source:?}"
            );
        }
        let configured = crate::rules::MD026NoTrailingPunctuation::new(Some("?？".into()));
        assert!(crate::rule::Rule::native_format_policy(&configured).is_none());
    }

    #[test]
    fn native_heading_text_keeps_generics_and_hidden_punctuation_exact() {
        for source in [
            "# Using `Result<T, E>` in tests\n",
            "## `RefCell<T>` and the interior mutability pattern\n",
            "# Heading {#id}\n",
            "# Heading <a id=\"id\"></a>\n",
            "# Heading <a title=\"punctuation!\" id=\"id\"></a>\n",
            "# `literal <a id=\"id\"></a>`\n",
            "# Heading <span>text.</span>\n",
        ] {
            assert_proofs(source);
            assert!(NativeFormatPolicy::new(26).proven(&FormatProof::analyze(source, MarkdownFlavor::Standard)));
        }
        for source in [
            "# Heading. {#id}\n",
            "# Heading. <a id=\"id\"></a>\n",
            "# Heading. ## {#id}\n",
            "# Heading! {: #id .class}\n",
        ] {
            assert_proofs(source);
            assert!(!NativeFormatPolicy::new(26).proven(&FormatProof::analyze(source, MarkdownFlavor::Standard)));
        }
        for prefix in ["# ", "## ", "###  "] {
            for text in [
                "Heading",
                "`Box<T>`",
                "`literal {#id}`",
                "`<a id=\"id\"></a>`",
                "<a id=\"id\"></a>Heading",
                "![<a id=\"id\"></a>](image.png)",
                "<!-- <a id=\"id\"></a> -->Heading",
                "é🦀<T>",
            ] {
                for punctuation in ["", ".", " :", "?", "！"] {
                    for suffix in [
                        "",
                        " {#id}",
                        " {: #id .class}",
                        " <a id=\"id\"></a>",
                        " <a id=\"id\"></a> {#other}",
                        " ## {#id}",
                        " <a title=\"a > b\" name=\"id\"></a>",
                    ] {
                        assert_proofs(&format!("{prefix}{text}{punctuation}{suffix}\n"));
                    }
                }
            }
        }
    }

    #[test]
    fn combined_native_defaults_and_list_fallbacks_keep_exact_policy_results() {
        for source in [
            "# A question?\n\nPlain prose.\n",
            "Text `a\n` tail\n",
            "1. -    item\n\n           ```rust\n           body\n           ```\n",
            "*     item\n    text\n    ===\n\nAfter.\n",
            "www.example.com [ label ]( target )   spaced text\n",
        ] {
            assert_proofs(source);
        }
    }

    #[test]
    fn standard_code_span_events_cover_native_spacing_candidates() {
        for source in [
            "An unmatched ` backtick\n",
            "An escaped \\` backtick\n",
            "A `` delimiter without its mate\n",
            "`balanced` and `` ` nested ` ``\n",
            "` one space ` and `   `\n",
            "a `multiple\nlines` span\n",
            "<div title=\"` source backtick\">\nraw ` text\n</div>\n",
            "[label with ` unmatched](target)\n",
            "![image with ` unmatched](target)\n",
        ] {
            assert_proofs(source);
            assert!(NativeFormatPolicy::new(38).proven(&FormatProof::analyze(source, MarkdownFlavor::Standard)));
        }
        for source in ["` leading`\n", "`trailing `\n", "``  both  ``\n"] {
            assert_proofs(source);
            assert!(!NativeFormatPolicy::new(38).proven(&FormatProof::analyze(source, MarkdownFlavor::Standard)));
        }
    }

    #[test]
    fn root_html_keeps_lexical_policies_and_native_fallbacks() {
        let openers = [
            "<div>",
            "<section class=\"x\">",
            "<pre>",
            "<script>",
            "<custom-tag>",
            "<div markdown=\"1\">",
        ];
        let bodies = [
            "plain\n",
            "#Missing\n### Third\n",
            "Title\n---\n",
            "- first\n+ second\n",
            "1. first\n3. third\n",
            ">  quote\n",
            "**strong** and __strong__\n",
            "*emphasis* and _emphasis_\n",
            "` leading` and `trailing `\n",
            "(target)[label] and [ label ]( )\n",
            "www.example.com and person@example.com\n",
            "```sh\n$ ls\n```\n~~~rust\ncode\n~~~\n",
            "| a | b |\n| --- | --- |\n| c | d |\n\n| e | f |\n",
            "value  \n\n\nnext\n",
            "<!-- rumdl-configure-file {\"MD004\":{\"style\":\"plus\"}} -->\n- item\n",
        ];
        for opener in openers {
            for body in bodies {
                for closer in ["", "</div>\n"] {
                    for indent in ["", " ", "   "] {
                        for (before, after) in [
                            ("# First", "## Last"),
                            ("# First\n\n* first\n\n**strong**", "## Last\n\n- last\n\n__strong__"),
                            ("~~~rust\ncode\n~~~", "```rust\ncode\n```"),
                        ] {
                            assert_proofs(&format!("{before}\n\n{indent}{opener}\n{body}{closer}\n{after}\n"));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn accepted_candidate_proofs_preserve_metadata_boundaries_and_exact_bytes() {
        for source in [
            "---\ntitle: Plain\n---\n\n# Heading\n",
            "{\n\"title\": \"Plain\"\n}\nbody without separator\n",
            "+++\ntitle = \"Plain\"\n+++\n\n- item\n",
            "# Heading\n\n- [link](target) and ![image](#)\n\n> quoted **strong**\n",
            "<div>\nmarkdown text *inside*\n</div>\n",
        ] {
            assert_proofs(source);
        }
        let (parsed, _, events) = ParsedDocument::parse_for_proof_guard("- old\n", MarkdownFlavor::Standard);
        let mut handoff = Some(parsed.into_handoff().with_format_proof_events(events));
        let proof = FormatProof::analyze_handoff("# new\n", MarkdownFlavor::Standard, &mut handoff);
        assert!(!proof.eligible());
        assert!(handoff.is_none());
    }

    #[test]
    fn canonical_and_malformed_families_match_native_fixes() {
        let fragments = [
            "",
            "plain",
            "# Heading\n",
            "#Heading\n",
            "##  Heading##\n",
            "title\n---\n",
            "- item\n",
            "1. item\n",
            "- - duplicate\n",
            "- item\n  continuation\n",
            "> quote\n",
            "*italic*",
            "_italic_",
            "**bold**",
            "__bold__",
            "` code `",
            "`code`",
            "[a](b)",
            "[ a ]( b )",
            "(url)[label]",
            "www.example.com",
            "name@example.com",
            "```rust\ncode\n```\n",
            "```sh\n$ ls\n```\n",
            "---\n",
            "a  b\n",
            "é🦀\n",
            "\n",
            "\n\n",
            "\t",
            "<!-- comment -->\n",
            "<span>text</span>",
            "[ref]: https://example.com\n",
            "<!-- comment\n# hidden\n-->\n",
            "| a | b |\n| --- | --- |\n| c | d |\n",
            "{{#include source.rs}}\n",
            "{{ \"# hidden\n## hidden\" }}\n",
            "{% set text = \"# hidden\n## hidden\" %}\n",
        ];
        for first in fragments {
            for second in fragments {
                assert_proofs(&format!("{first}{second}"));
            }
        }
    }

    #[test]
    fn fence_ended_by_its_list_item_is_proven_for_md070() {
        // The list item ends the unclosed `yaml` fence, so the fence after the
        // list is a block of its own. MD070 agrees with the proof, so `fmt` on
        // stdin and on a file give the same result.
        let source = "- Item:\n\n  ```yaml\n  key: |\n    raw\n\nAfter the list.\n\n```text\nplain\n```\n";
        assert!(NativeFormatPolicy::new(70).proven(&FormatProof::analyze(source, MarkdownFlavor::Standard)));
        assert_proofs(source);
    }

    #[test]
    fn fence_collisions_behind_container_prefixes_are_not_proven_for_md070() {
        for source in [
            "> ```markdown\n> ```bash\n> x\n> ```\n> ```\n",
            "10. a\n\n    ```markdown\n    ```bash\n    x\n    ```\n    ```\n",
            "- a\n  - b\n\n    ```markdown\n    ```bash\n    x\n    ```\n    ```\n",
            "- a\n\n  > ~~~markdown\n  > ~~~bash\n  > x\n  > ~~~\n  > ~~~\n",
        ] {
            assert!(
                !NativeFormatPolicy::new(70).proven(&FormatProof::analyze(source, MarkdownFlavor::Standard)),
                "{source:?}"
            );
            assert_proofs(source);
        }
    }

    #[test]
    fn closer_valid_inside_its_list_item_agrees_with_md070() {
        // Five raw columns is three past the item's content: the fence closes
        // there, so `fmt` on stdin and on a file leave the document alone.
        let source = "- a\n\n  ```md\n  x\n     ```\n\n```text\ny\n```\n";
        assert_proofs(source);
    }

    #[test]
    fn unclosed_fences_keep_native_repair() {
        for source in ["```sh\n# H\n", "plain```rust\ncode\n```\n", "~~~sh\ncode\n"] {
            assert_proofs(source);
        }
    }

    #[test]
    fn setext_titles_with_matching_hash_counts_keep_native_style_checks() {
        for source in [
            "#Heading\n===\n\n# Other\n",
            "##Heading\n---\n```\n```\n##Heading",
            "##\u{a0}Heading\n---\n\n## Other\n",
        ] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(!NativeFormatPolicy::new(3).proven(&facts), "{source:?}");
            assert_proofs(source);
        }
    }

    #[test]
    fn source_thematic_breaks_keep_native_indentation_and_style() {
        for source in [
            " ---\n",
            "| only |\n| --- |\n --- \nAfter\n",
            "| only |\n| --- |\n\n ---\n\nAfter\n",
            "***\n\n---\n",
            "```markdown\n***\n```\n\n---\n",
        ] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(!NativeFormatPolicy::new(35).proven(&facts), "{source:?}");
            assert_proofs(source);
        }
        for source in ["***\n", "***\n\n***\n", "prose\n\n---\n"] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(NativeFormatPolicy::new(35).proven(&facts), "{source:?}");
            assert_proofs(source);
        }
    }

    #[test]
    fn isolated_top_level_tables_keep_native_list_certificates() {
        for list in ["- item\n- other", "1. item\n2. other", "- item\n  - child\n- other"] {
            for table in [
                "| cell | other |\n| --- | --- |",
                "| cell | other |\n| :--- | ---: |\n| `value` | α🦀 |",
            ] {
                for source in [
                    format!("{list}\n\n{table}\n"),
                    format!("{table}\n\n{list}\n"),
                    format!("{list}\n\n{table}\n\n{list}\n"),
                ] {
                    assert_proofs(&source);
                    let facts = FormatProof::analyze(&source, MarkdownFlavor::Standard);
                    for policy in [4, 5, 7, 29, 30, 32, 69, 76, 77] {
                        assert!(
                            NativeFormatPolicy::new(policy).proven(&facts),
                            "MD{policy:03}: {source:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn table_edges_and_container_boundaries_keep_native_list_analysis() {
        for source in [
            "- item\n| cell | other |\n| --- | --- |\n",
            "| cell | other |\n| --- | --- |\n- item\n",
            "- item\n\ncell | other\n--- | ---\n",
            "- item\n\n | cell | other |\n | --- | --- |\n",
            "- item\n\n  | cell | other |\n  | --- | --- |\n",
            "- list\n   \n| cell | other |\n| cell | other |\n| --- | --- |\n    indented",
        ] {
            assert_proofs(source);
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(!NativeFormatPolicy::new(32).proven(&facts), "{source:?}");
        }
    }

    proptest! {
        #[test]
        fn generated_isolated_table_boundaries_keep_native_edits(
            list in prop::sample::select(vec![
                "- item\n- other", "1. item\n2. other", "- item\n  - child\n- other",
                "- item\\", "- item  ", "- item\n  continuation", "- item\n\n  continuation",
                "- item\n  # Heading", "> - item\n> - other", "-    item", "- - item",
                "* item\n+ other", "1. item\n1. other", "- `code`\n- [link](target)",
            ]),
            table in prop::sample::select(vec![
                "| cell | other |\n| --- | --- |", "| cell | other |\n| :--- | ---: |\n| `value` | α🦀 |",
                "cell | other\n--- | ---", " | cell | other |\n | --- | --- |",
                "  | cell | other |\n  | --- | --- |", "> | cell | other |\n> | --- | --- |",
                "| cell | other |\n| --- | --- |\n::: note", "| cell | other |\n| --- | --- |\n---",
                "| cell | other |\n| --- | --- |\n    continuation", "| cell | other |\n| --- | --- |\n| [link](x) | **text** |",
            ]),
            before in prop::sample::select(vec!["\n", "\n\n", "\n \n", "\n\n\n"]),
            after in prop::sample::select(vec!["\n", "\n\n", "\n \n", "\n\n\n"]),
            layout in 0usize..3,
        ) {
            let source = match layout {
                0 => format!("{list}{before}{table}{after}"),
                1 => format!("{table}{before}{list}{after}"),
                _ => format!("{list}{before}{table}{after}{list}\n"),
            };
            let failures = proof_failures(&source);
            prop_assert!(failures.is_empty(), "{}", failures.join("\n"));
        }
    }

    #[test]
    fn orphan_pipe_rows_keep_native_list_boundaries() {
        for source in [
            "- list\n| cell | other |\n\n  indented\n- list\n  indented",
            "- list\ncell | other\n\n  indented\n- list\n  indented",
            "1. list\n| cell | other |\n\n   indented\n2. list\n   indented",
        ] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(!NativeFormatPolicy::new(32).proven(&facts), "{source:?}");
            assert_proofs(source);
        }
    }

    #[test]
    fn literal_pipes_keep_native_table_and_list_certificates() {
        for middle in [
            "Choose `none|object|all` here.",
            "Use `cargo [add|remove|update]` here.",
            "Run `cargo install --list | grep tool` here.",
            "A \\| literal pipe.",
            "An escaped \\` marker and `x|y` span.",
            "A `unclosed | other",
        ] {
            assert!(!has_native_table_candidate(middle), "{middle:?}");
            let source = format!("- item\n\n{middle}\n\n- item\n");
            let facts = FormatProof::analyze(&source, MarkdownFlavor::Standard);
            for rule in [4, 5, 7, 29, 30, 32, 55, 56, 58, 69, 75, 76, 77] {
                assert!(NativeFormatPolicy::new(rule).proven(&facts), "MD{rule:03}: {source:?}");
            }
            assert_proofs(&source);
        }
    }

    #[test]
    fn native_table_candidate_superset_keeps_all_container_boundaries() {
        for row in ["| cell | other |", "cell | other", "--- | ---", "`x|y` | other"] {
            for prefix in ["", "  ", "> ", "- ", "1. ", "> - ", "- > ", "1. - > "] {
                let source = format!("{prefix}{row}");
                assert!(has_native_table_candidate(&source), "{source:?}");
                assert_proofs(&format!("- item\n{source}\n\n  indented\n- item\n  indented"));
            }
        }
    }

    proptest! {
        #[test]
        fn native_table_candidate_crossproduct_preserves_every_certificate(
            parts in prop::collection::vec(prop::sample::select(vec![
                "- item\n", "1. item\n", "2. item\n", "  continuation\n", "\n", "# H\n",
                "`x|y`\n", "text `x|y`\n", "text \\| other\n", "cell | other\n", "| cell | other |\n",
                "--- | ---\n", "| --- | --- |\n", "> `x|y`\n", "> cell | other\n", "- > cell | other\n",
                "- `x|y`\n", "- | cell | other |\n", "1. | cell | other |\n", "`unclosed | other\n",
                "``x|y``\n", "`x|\ny`\n", "$x|y$\n", "`x|y` | other\n", "<!-- | x | -->\n",
                "```text\n", "```\n", "[x](u|v)\n", "[^n]: `x|y`\n", "é🦀 | cell\n",
            ]), 0..24)
        ) {
            let source = parts.concat();
            let failures = proof_failures(&source);
            prop_assert!(failures.is_empty(), "{}", failures.join("\n"));
        }
    }

    #[test]
    fn lists_containing_headings_and_tables_keep_native_boundaries() {
        for source in [
            "- item\n  # Title\n  \n  | cell | other |\n  | --- | --- |\n  --\n",
            "- list\n   \n| cell | other |\n| cell | other |\n| --- | --- |\n    indented",
            "- item\n  # Title\n",
            "- item\n  \n  | cell | other |\n  | --- | --- |\n",
        ] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(!NativeFormatPolicy::new(32).proven(&facts), "{source:?}");
            assert_proofs(source);
        }
    }

    #[test]
    fn native_table_block_openers_keep_native_boundaries() {
        for opener in ["::: note", "::: ", ":::", "   ::: note", ":::: note"] {
            let source = format!("| cell | other |\n| --- | --- |\n{opener}\n```text\n```\n");
            let facts = FormatProof::analyze(&source, MarkdownFlavor::Standard);
            assert!(!NativeFormatPolicy::new(58).proven(&facts), "{source:?}");
            assert_proofs(&source);
        }
    }

    #[test]
    fn source_setext_candidates_keep_native_heading_policies() {
        for source in [
            "é🦀 prose with spaces and ` code `\n| cell | other |\n| --- | --- |\n---\né🦀 prose\n",
            "| cell | other |\n| --- | --- |\n---\nprose\n",
            "| cell | other |\n| --- | --- |\n===\nprose\n",
            "# Heading\n\n---\nAfter\n",
            "> | cell | other |\n> | --- | --- |\n> ---\n> prose\n",
        ] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            for policy in [1, 3, 18, 19, 20, 21, 22, 23, 25, 26] {
                assert!(
                    !NativeFormatPolicy::new(policy).proven(&facts),
                    "MD{policy:03}: {source:?}"
                );
            }
            assert_proofs(source);
        }
    }

    #[test]
    fn native_noop_options_and_balanced_code_spaces_keep_certificates() {
        use crate::rule::Rule;
        let emphasis = crate::rules::MD036NoEmphasisAsHeading::default();
        for source in ["**Label**\n", "_Phrase_\n", "*name*\n"] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(emphasis.native_format_policy().unwrap().proven(&facts));
            assert_proofs(source);
        }
        for body in ["code", " code ", " ", "  ", "\u{a0}code\u{a0}", " `code` "] {
            let source = format!("Prose ``{body}`` here.\n");
            let facts = FormatProof::analyze(&source, MarkdownFlavor::Standard);
            assert!(NativeFormatPolicy::new(38).proven(&facts), "{source:?}");
            assert_proofs(&source);
        }
        for body in [" code", "code ", "  code ", " code  "] {
            let source = format!("Prose ``{body}`` here.\n");
            assert_proofs(&source);
        }
    }

    #[test]
    fn indented_top_level_list_keeps_native_repair() {
        assert_proofs("  - nested\n");
        assert_proofs("  1. item\n");
    }

    #[test]
    fn canonical_list_boundaries_and_uniform_emphasis_have_certificates() {
        for source in ["- item\n\nprose\n", "1. item\n\nprose\n"] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(NativeFormatPolicy::new(32).proven(&facts));
            assert_proofs(source);
        }
        for source in ["source_file_name.rs\n", "_one_ and _two_\n", "__one__ and __two__\n"] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(NativeFormatPolicy::new(49).proven(&facts));
            assert!(NativeFormatPolicy::new(50).proven(&facts));
            assert_proofs(source);
        }
        for source in ["_one_ and *two*\n", "__one__ and **two**\n", "~^*~\n\0*"] {
            assert_proofs(source);
        }
    }

    #[test]
    fn unparsed_ordered_markers_keep_native_spacing_analysis() {
        for prefix in ["", "- item\n\n"] {
            for prose in ["Before after.\n", "Before  after.\n"] {
                for marker in ["2.", "3.", "10.", "22)", "٢٢.", "\u{a0}3."] {
                    let source = format!("{prefix}{prose}{marker} third\n");
                    let facts = FormatProof::analyze(&source, MarkdownFlavor::Standard);
                    assert!(!NativeFormatPolicy::new(32).proven(&facts), "{source:?}");
                    assert_proofs(&source);

                    {
                        let config = Config::default();
                        let rules = crate::rules::all_rules(&config);
                        let run = crate::document_run::DocumentRun::new(&source, &rules, &config);
                        for limit in 0..8 {
                            let (expected, native) = run.fix(limit).unwrap();
                            let (actual, candidate) = run.fix_for_formatter(limit).unwrap();
                            assert_eq!(actual, expected, "{source:?} limit={limit}");
                            assert_eq!(candidate.rules_fixed, native.rules_fixed);
                            assert_eq!(candidate.iterations, native.iterations);
                            assert_eq!(candidate.fixed_rule_names, native.fixed_rule_names);
                            assert_eq!(candidate.converged, native.converged);
                            assert_eq!(candidate.conflicting_rules, native.conflicting_rules);
                            assert_eq!(candidate.conflict_cycle, native.conflict_cycle);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn native_default_list_styles_and_plain_quotes_keep_certificates() {
        for source in [
            "> quoted prose\n",
            "> > nested prose\n",
            "> quoted prose\n\n# Heading\n\n- item\n  continuation\n",
            "1. first\n1. second\n",
            "11. first\n12. second\n",
            "<!-- rumdl-disable MD009 -->\n\n- **bold item**\n  continuation\n",
        ] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(facts.eligible());
            for rule in [18, 19, 20, 21, 22, 23, 27, 29, 77] {
                assert!(NativeFormatPolicy::new(rule).proven(&facts), "MD{rule:03}: {source:?}");
            }
            assert_proofs(source);
        }
        for source in [
            ">  quote\n",
            "\u{a0}> quote\n",
            "> #Heading\n",
            "> - item\n> continuation\n",
            "- > quoted item\n",
            "> ```\n> code\n> ```\n",
            "> ---\n> text\n",
            "1. item\n\n  continuation\n",
            "1. first\n1. second\n3. third\n",
        ] {
            assert_proofs(source);
        }
    }

    #[test]
    fn native_front_matter_and_command_policies_are_shared() {
        for source in [
            "---\nkey: value\n---\n\n# Heading\n",
            "---\ntitle: Book\n---\n\n## Section\n",
            "+++\ntitle = \"Book\"\n+++\n\n## Section\n",
            "{\n\"title\": \"Book\"\n}\n\n## Section\n",
            "```console\n$ cargo run\noutput\n```\n",
            "```rust\n$ literal\n```\n",
        ] {
            let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
            assert!(facts.eligible(), "{source:?}");
            for rule in [1, 14, 25, 71] {
                assert!(NativeFormatPolicy::new(rule).proven(&facts), "MD{rule:03}: {source:?}");
            }
            assert_proofs(source);
        }
        for source in [
            "---\ntitle: Book\n---\n\n### Section\n",
            "---\ntitle: Book\n---\n\n# Title\n",
            "---\nkey: value\n---\nprose\n",
            "---\nkey: value\n---",
            "---\n<!-- open\n---\n\n# Hidden\n",
            "```console\n$ cargo run\n```\n",
            "```console\n$ mkdir directory\n```\n",
            "```console\n$ cargo run\n# comment\n```\n",
            "```console\n$ cargo run\n",
        ] {
            assert_proofs(source);
        }
        for source in [
            "---\n[label\n](url)\n---\n\n# Heading\n",
            "---\n```\ncode\n```\n---\n\n# Heading\n",
            "---\n| a | b |\n| --- | --- |\n| c | d |\n---\n\n# Heading\n",
            "---\nkey: $$ literal\n---\n\n# Heading\n",
        ] {
            assert!(!FormatProof::analyze(source, MarkdownFlavor::Standard).eligible());
            assert_proofs(source);
        }
    }

    #[test]
    fn native_source_aware_heading_and_continuation_cases() {
        for source in [
            "1. ordered\n\n  continuation\n",
            "1. ordered\n\n  ```rust\n  code\n  ```\n",
            "# Closed #\n\n## Plain\n",
            "# Title! #\n",
            "# Title! {#id}\n",
            "# Title! <a id=\"id\"></a>\n",
            "  title\n---\n",
            "First\nsecond!\n---\n",
            "[\ntext](url)\n",
            "[text\n](url)\n",
            "![\ntext](url)\n",
        ] {
            assert_proofs(source);
        }
    }

    #[test]
    fn inline_configuration_examples_keep_literal_scope() {
        for source in [
            "```markdown\n<!-- rumdl-configure-file {\"MD013\": {\"reflow\": true}} -->\n```\n",
            "`<!-- rumdl-disable MD009 -->`\n",
        ] {
            assert!(FormatProof::analyze(source, MarkdownFlavor::Standard).eligible());
            assert_proofs(source);
        }
        for source in [
            "<!-- rumdl-configure-file {\"MD013\": {\"reflow\": true}} -->\nprose\n",
            "- ```markdown <!-- rumdl-configure-file {} -->\n  ```\n",
        ] {
            assert!(!FormatProof::analyze(source, MarkdownFlavor::Standard).eligible());
        }
        assert!(FormatProof::analyze("<!-- rumdl-disable MD009 -->\nprose\n", MarkdownFlavor::Standard).eligible());
    }

    #[test]
    fn native_table_helpers_and_single_html_tags_match_fixes() {
        for source in [
            "| a | b |\n| --- | --- |\n| c | d |\n",
            "| a   | b   |\n| --- | --- |\n| c   | d   |\n",
            "| a | b |\n| --- | --- |\n| c | d | e |\n",
            "| a | b |\n| --- | --- |\nc | d\n",
            "| a | b |\n| --- | --- |\n| c | d |\n\n| e | f |\n",
            "<Listing file-name=\"source.rs\">\n\n```rust\ncode\n```\n\n</Listing>\n",
            "<a id=\"heading\"></a>\n\n# Heading\n",
            "<span class=\"filename\">Filename: source.rs</span>\n",
            "<div markdown>\n\n# Heading\n\n</div>\n",
        ] {
            assert_proofs(source);
        }
    }

    #[test]
    fn parser_table_header_must_match_the_native_table_finder() {
        let source = "`unclosed | other\n--- | ---\n--- | ---\n";
        let facts = FormatProof::analyze(source, MarkdownFlavor::Standard);
        for rule in [55, 56, 58, 75] {
            assert!(!NativeFormatPolicy::new(rule).proven(&facts), "MD{rule:03}");
        }
        assert_proofs(source);
        for header in ["$unclosed | other", "``unclosed | other", "\\`unclosed | other"] {
            assert_proofs(&format!("{header}\n--- | ---\n--- | ---\n"));
        }
    }

    #[test]
    fn fence_info_classification_uses_native_policy() {
        for info in ["{python}", "{}", "title=example", ".rust", "#id", "\u{a0}", "rust"] {
            assert_proofs(&format!("~~~{info}\ncode\n~~~\n"));
        }
    }

    #[test]
    fn source_only_separators_keep_native_heading_analysis() {
        for whitespace in [
            "\u{b}", "\u{c}", "\u{85}", "\u{a0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}",
            "\u{2004}", "\u{2005}", "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200a}", "\u{2028}", "\u{2029}",
            "\u{202f}", "\u{205f}", "\u{3000}",
        ] {
            for marker in ["=", "===", "-", "---", "***", "___", "* * *", "_ _ _", "- - -"] {
                for prefix in ["", "> ", "> > ", "- ", "1. ", "  "] {
                    for line in [
                        format!("{prefix}{whitespace}{marker}"),
                        format!("{prefix}{marker}{whitespace}"),
                        format!("{prefix}{whitespace}{marker}{whitespace}"),
                    ] {
                        assert!(has_unusual_separator_whitespace(&line), "{line:?}");
                        assert_proofs(&format!("# First\n\nTitle\n{line}\n\n### Third\n"));
                    }
                }
            }
        }
        for text in [
            "===",
            " --- ",
            "> * * *",
            "text\u{a0}inside *emphasis*",
            "text\u{2003}inside **strong**",
        ] {
            assert!(!has_unusual_separator_whitespace(text), "{text:?}");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(4096))]
        #[test]
        fn arbitrary_sources_never_prove_a_native_rewrite(source in ".{0,512}") {
            assert_proofs(&source);
        }

        #[test]
        fn source_fragments_never_prove_a_native_rewrite(parts in prop::collection::vec(
            prop::sample::select(vec!["# H\n", "## H\n", "\n", "- item\n", "  text\n", "- - item\n", "3. third\n", "10. tenth\n", "Before  after.\n", "*a*", "_a_", "__a__", "[ x ]( u )", "[https://x.com](< >)", "```sh\n", "$ ls\n", "```\n", "~~~\n", "<!-- comment\n", "-->\n", "`inline\n", "a  b", "é🦀", "<span>", "</span>", "www.a", "[x]: https://x\n"]), 0..30)) {
            assert_proofs(&parts.concat());
        }
    }
}
