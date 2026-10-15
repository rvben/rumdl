//! A model of how Python-Markdown's block parser reads lists.
//!
//! Python-Markdown is not CommonMark: it splits a document at blank lines into
//! blocks, then hands each block to the first processor that claims it. For
//! lists the relevant processors are the indent processor (deeper blocks
//! belong to the open item), the code processor (four columns of indent with no
//! open item), and the ordered/unordered list processors (which split a block
//! into items and parse each item recursively). The tree they build decides
//! which item owns a line and whether the line is a paragraph or code.
//!
//! This module ports that control flow over a small arena tree and reports, per
//! source line, what the parser made of it. A rewrite that moves a line between
//! owners is harmless as long as every line keeps its kind and every paragraph
//! keeps its lines; a rewrite that turns prose into code, a marker into text or
//! merges two paragraphs changes the document. [`differing_regions`] compares
//! two texts line by line so a fixer can refuse such a rewrite.
//!
//! Only list handling is modelled. Headers, quotes, rules, references and the
//! MkDocs extensions are read as ordinary paragraph text, identically before
//! and after a rewrite.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

const TAB: usize = 4;
const MAX_DEPTH: usize = 160;
/// Upper bound on the summed squared indentation the model will parse.
const MAX_INDENT_WORK: usize = 100_000_000;

/// What one parser step made of a source line. A line can carry several: the
/// marker of an item, then the paragraph its text became.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Leaf {
    Marker,
    Para(usize),
    Code(usize),
    Heading(usize),
}

#[derive(Clone)]
struct Ln {
    idx: usize,
    text: String,
}

type Block = Vec<Ln>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tag {
    Root,
    List,
    Li,
    P,
    Pre,
}

struct Node {
    tag: Tag,
    children: Vec<usize>,
    /// Leaf id of text held directly in the element (a tight list item).
    text: Option<usize>,
    /// Leaf id of text following the element inside its parent.
    tail: Option<usize>,
    /// For a list: whether it renders as an ordered list.
    ordered: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    List,
    LooseList,
    Detabbed,
}

struct Parser {
    nodes: Vec<Node>,
    state: Vec<State>,
    kinds: Vec<Vec<Leaf>>,
    /// Per line, the text a code block holds for it after de-indenting.
    code_text: Vec<Option<String>>,
    /// Per line that starts a list item, the list the item was added to and
    /// whether that list renders as ordered.
    line_list: Vec<Option<(usize, bool)>>,
    next_id: usize,
    budget: usize,
    depth: usize,
    overrun: bool,
}

fn leading_spaces(text: &str) -> usize {
    text.bytes().take_while(|&b| b == b' ').count()
}

fn is_blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// Length of the marker at the start of `rest` (`1.`, `-`, `*`, `+`), when
/// `ordered_ok`/`bullet_ok` allow that kind.
fn marker_len(rest: &str, ordered_ok: bool, bullet_ok: bool) -> Option<usize> {
    let bytes = rest.as_bytes();
    match bytes.first()? {
        b'*' | b'+' | b'-' if bullet_ok => Some(1),
        b'0'..=b'9' if ordered_ok => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            (bytes.get(digits) == Some(&b'.')).then_some(digits + 1)
        }
        _ => None,
    }
}

/// `^[ ]{min,max}<marker>[ ]+(.*)`: the text after the marker's spaces.
fn item_match(text: &str, min: usize, max: usize, ordered_ok: bool, bullet_ok: bool) -> Option<&str> {
    let indent = leading_spaces(text);
    if indent < min || indent > max {
        return None;
    }
    let rest = &text[indent..];
    let len = marker_len(rest, ordered_ok, bullet_ok)?;
    let after = &rest[len..];
    let spaces = leading_spaces(after);
    (spaces > 0).then(|| &after[spaces..])
}

fn child_match(text: &str) -> Option<&str> {
    item_match(text, 0, TAB - 1, true, true)
}

fn indent_item_match(text: &str) -> bool {
    item_match(text, TAB, TAB * 2 - 1, true, true).is_some()
}

fn olist_test(block: &Block) -> bool {
    block
        .first()
        .is_some_and(|l| item_match(&l.text, 0, TAB - 1, true, false).is_some())
}

fn ulist_test(block: &Block) -> bool {
    block
        .first()
        .is_some_and(|l| item_match(&l.text, 0, TAB - 1, false, true).is_some())
}

fn starts_with_tab(block: &Block) -> bool {
    block.first().is_some_and(|l| l.text.starts_with("    "))
}

/// `"\n".join(block)` is not empty.
fn joined_nonempty(block: &Block) -> bool {
    block.len() > 1 || block.first().is_some_and(|l| !l.text.is_empty())
}

/// `text.split("\n\n")` over lines, keeping each line's source index.
fn split_blocks(lines: &[Ln]) -> VecDeque<Block> {
    let mut joined = String::new();
    let mut offsets = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            joined.push('\n');
        }
        offsets.push(joined.len());
        joined.push_str(&line.text);
    }
    let mut out = VecDeque::new();
    let mut start = 0;
    let mut next_line = 0;
    loop {
        let (end, next) = match joined[start..].find("\n\n") {
            Some(p) => (start + p, Some(start + p + 2)),
            None => (joined.len(), None),
        };
        let mut block = Vec::new();
        while next_line < lines.len() && offsets[next_line] <= end {
            if offsets[next_line] >= start {
                block.push(lines[next_line].clone());
            }
            next_line += 1;
        }
        out.push_back(block);
        match next {
            Some(n) => start = n,
            None => break,
        }
    }
    out
}

/// Remove one tab of indent from the leading lines that have it; blank lines
/// pass; stop at the first other line. Returns (taken, rest).
fn detab(block: Block) -> (Block, Block) {
    let mut taken = Vec::new();
    let mut iter = block.into_iter();
    let mut rest = Vec::new();
    for line in iter.by_ref() {
        if line.text.starts_with("    ") {
            taken.push(Ln {
                idx: line.idx,
                text: line.text[TAB..].to_string(),
            });
        } else if is_blank(&line.text) {
            taken.push(Ln {
                idx: line.idx,
                text: String::new(),
            });
        } else {
            rest.push(line);
            break;
        }
    }
    rest.extend(iter);
    (taken, rest)
}

fn loose_detab(block: Block, level: usize) -> Block {
    let pad = " ".repeat(TAB * level);
    block
        .into_iter()
        .map(|l| {
            if l.text.starts_with(&pad) {
                Ln {
                    idx: l.idx,
                    text: l.text[pad.len()..].to_string(),
                }
            } else {
                l
            }
        })
        .collect()
}

impl Parser {
    fn new(total_lines: usize) -> Self {
        Parser {
            nodes: vec![Node {
                tag: Tag::Root,
                children: Vec::new(),
                text: None,
                tail: None,
                ordered: false,
            }],
            state: Vec::new(),
            kinds: vec![Vec::new(); total_lines],
            code_text: vec![None; total_lines],
            line_list: vec![None; total_lines],
            next_id: 0,
            budget: 64 * total_lines + 4096,
            depth: 0,
            overrun: false,
        }
    }

    fn is_state(&self, s: State) -> bool {
        self.state.last() == Some(&s)
    }

    fn fresh_id(&mut self) -> usize {
        self.next_id += 1;
        self.next_id
    }

    fn sub_element(&mut self, parent: usize, tag: Tag) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            tag,
            children: Vec::new(),
            text: None,
            tail: None,
            ordered: false,
        });
        self.nodes[parent].children.push(id);
        id
    }

    fn last_child(&self, parent: usize) -> Option<usize> {
        self.nodes[parent].children.last().copied()
    }

    fn tag(&self, node: usize) -> Tag {
        self.nodes[node].tag
    }

    fn record(&mut self, block: &Block, leaf: impl Fn(&Ln) -> Option<Leaf>) {
        for line in block {
            if let Some(l) = leaf(line)
                && let Some(slot) = self.kinds.get_mut(line.idx)
            {
                slot.push(l);
            }
        }
    }

    fn parse_chunk(&mut self, parent: usize, block: &Block) {
        let blocks = split_blocks(block);
        self.parse_blocks(parent, blocks);
    }

    fn parse_blocks(&mut self, parent: usize, mut blocks: VecDeque<Block>) {
        if self.depth >= MAX_DEPTH {
            self.overrun = true;
            return;
        }
        self.depth += 1;
        while let Some(front) = blocks.front() {
            if self.overrun || self.budget == 0 {
                self.overrun = true;
                break;
            }
            self.budget -= 1;
            let blank = !joined_nonempty(front);
            let leading_empty = front.len() >= 2 && front[0].text.is_empty();
            if blank || leading_empty {
                self.run_empty(&mut blocks);
            } else if self.indent_test(parent, front) {
                self.run_indent(parent, &mut blocks);
            } else if starts_with_tab(front) {
                self.run_code(parent, &mut blocks);
            } else if let Some(at) = front.iter().position(|l| l.text.starts_with('#')) {
                self.run_header(parent, &mut blocks, at);
            } else if olist_test(front) || ulist_test(front) {
                self.run_list(parent, &mut blocks);
            } else {
                self.run_paragraph(parent, &mut blocks);
            }
        }
        self.depth -= 1;
    }

    fn run_empty(&mut self, blocks: &mut VecDeque<Block>) {
        let mut block = blocks.pop_front().unwrap_or_default();
        if joined_nonempty(&block) {
            block.remove(0);
            if joined_nonempty(&block) {
                blocks.push_front(block);
            }
        }
    }

    fn indent_test(&self, parent: usize, block: &Block) -> bool {
        starts_with_tab(block)
            && !self.is_state(State::Detabbed)
            && (self.tag(parent) == Tag::Li || self.last_child(parent).is_some_and(|c| self.tag(c) == Tag::List))
    }

    fn get_level(&self, parent: usize, block: &Block) -> (usize, usize) {
        let indent_level = block.first().map_or(0, |l| leading_spaces(&l.text) / TAB);
        let mut level = usize::from(self.is_state(State::List));
        let mut parent = parent;
        while indent_level > level {
            match self.last_child(parent) {
                Some(child) if matches!(self.tag(child), Tag::List | Tag::Li) => {
                    if self.tag(child) == Tag::List {
                        level += 1;
                    }
                    parent = child;
                }
                _ => break,
            }
        }
        (level, parent)
    }

    fn run_indent(&mut self, parent: usize, blocks: &mut VecDeque<Block>) {
        let block = blocks.pop_front().unwrap_or_default();
        let (level, sibling) = self.get_level(parent, &block);
        let block = loose_detab(block, level);
        self.state.push(State::Detabbed);
        if self.tag(parent) == Tag::Li {
            match self.last_child(parent) {
                Some(c) if self.tag(c) == Tag::List => self.parse_blocks(c, VecDeque::from([block])),
                _ => self.parse_blocks(parent, VecDeque::from([block])),
            }
        } else if self.tag(sibling) == Tag::Li {
            self.parse_blocks(sibling, VecDeque::from([block]));
        } else if let Some(li) = self.last_child(sibling).filter(|&c| self.tag(c) == Tag::Li) {
            self.wrap_text(li);
            self.parse_chunk(li, &block);
        } else {
            let li = self.sub_element(sibling, Tag::Li);
            self.parse_blocks(li, VecDeque::from([block]));
        }
        self.state.pop();
    }

    /// Move an item's direct text into a leading paragraph.
    fn wrap_text(&mut self, li: usize) {
        if let Some(id) = self.nodes[li].text.take() {
            let p = self.nodes.len();
            self.nodes.push(Node {
                tag: Tag::P,
                children: Vec::new(),
                text: Some(id),
                tail: None,
                ordered: false,
            });
            self.nodes[li].children.insert(0, p);
        }
    }

    fn run_code(&mut self, parent: usize, blocks: &mut VecDeque<Block>) {
        let sibling = self.last_child(parent);
        let block = blocks.pop_front().unwrap_or_default();
        let pre = match sibling {
            Some(s) if self.tag(s) == Tag::Pre => s,
            _ => self.sub_element(parent, Tag::Pre),
        };
        let (taken, rest) = detab(block);
        self.record(&taken, |l| (!is_blank(&l.text)).then_some(Leaf::Code(pre)));
        for line in taken.iter().filter(|l| !is_blank(&l.text)) {
            if let Some(slot) = self.code_text.get_mut(line.idx) {
                *slot = Some(line.text.clone());
            }
        }
        if joined_nonempty(&rest) {
            blocks.push_front(rest);
        }
    }

    fn get_items(&mut self, block: Block) -> Vec<Block> {
        let mut items: Vec<Block> = Vec::new();
        for line in block {
            if let Some(content) = child_match(&line.text) {
                let content = content.to_string();
                if let Some(slot) = self.kinds.get_mut(line.idx) {
                    slot.push(Leaf::Marker);
                }
                items.push(vec![Ln {
                    idx: line.idx,
                    text: content,
                }]);
            } else if indent_item_match(&line.text) {
                match items.last_mut() {
                    Some(last) if last[0].text.starts_with("    ") => last.push(line),
                    _ => items.push(vec![line]),
                }
            } else if let Some(last) = items.last_mut() {
                last.push(line);
            } else {
                items.push(vec![line]);
            }
        }
        items
    }

    fn note_list(&mut self, list: usize, item: &Block) {
        if let (Some(first), ordered) = (item.first(), self.nodes[list].ordered)
            && let Some(slot) = self.line_list.get_mut(first.idx)
        {
            *slot = Some((list, ordered));
        }
    }

    fn run_list(&mut self, parent: usize, blocks: &mut VecDeque<Block>) {
        let block = blocks.pop_front().unwrap_or_default();
        let ordered = olist_test(&block);
        let mut items = self.get_items(block).into_iter();
        let sibling = self.last_child(parent);
        let lst = match sibling {
            Some(s) if self.tag(s) == Tag::List => {
                if let Some(&last_li) = self.nodes[s].children.last() {
                    self.wrap_text(last_li);
                    if let Some(lch) = self.last_child(last_li)
                        && let Some(id) = self.nodes[lch].tail.take()
                    {
                        let p = self.sub_element(last_li, Tag::P);
                        self.nodes[p].text = Some(id);
                    }
                }
                let li = self.sub_element(s, Tag::Li);
                self.state.push(State::LooseList);
                if let Some(first) = items.next() {
                    self.note_list(s, &first);
                    self.parse_blocks(li, VecDeque::from([first]));
                }
                self.state.pop();
                s
            }
            _ if self.tag(parent) == Tag::List => parent,
            _ => {
                let created = self.sub_element(parent, Tag::List);
                self.nodes[created].ordered = ordered;
                created
            }
        };
        self.state.push(State::List);
        for item in items {
            if item.first().is_some_and(|l| l.text.starts_with("    ")) {
                if let Some(last) = self.last_child(lst) {
                    self.parse_blocks(last, VecDeque::from([item]));
                }
            } else {
                self.note_list(lst, &item);
                let li = self.sub_element(lst, Tag::Li);
                self.parse_blocks(li, VecDeque::from([item]));
            }
        }
        self.state.pop();
    }

    /// An ATX heading line splits its block: the lines before it are parsed
    /// first, the heading becomes its own element, and the lines after it are
    /// parsed as a block of their own.
    fn run_header(&mut self, parent: usize, blocks: &mut VecDeque<Block>, at: usize) {
        let mut block = blocks.pop_front().unwrap_or_default();
        let mut rest = block.split_off(at);
        let after = rest.split_off(1);
        if !block.is_empty() {
            self.parse_blocks(parent, VecDeque::from([block]));
        }
        let id = self.fresh_id();
        let heading = self.sub_element(parent, Tag::P);
        self.nodes[heading].text = Some(id);
        self.record(&rest, |_| Some(Leaf::Heading(id)));
        if !after.is_empty() {
            blocks.push_front(after);
        }
    }

    fn run_paragraph(&mut self, parent: usize, blocks: &mut VecDeque<Block>) {
        let block = blocks.pop_front().unwrap_or_default();
        if !block.iter().any(|l| !is_blank(&l.text)) {
            return;
        }
        let id = if self.is_state(State::List) {
            match self.last_child(parent) {
                Some(sibling) => match self.nodes[sibling].tail {
                    Some(id) => id,
                    None => {
                        let id = self.fresh_id();
                        self.nodes[sibling].tail = Some(id);
                        id
                    }
                },
                None => match self.nodes[parent].text {
                    Some(id) => id,
                    None => {
                        let id = self.fresh_id();
                        self.nodes[parent].text = Some(id);
                        id
                    }
                },
            }
        } else {
            let id = self.fresh_id();
            let p = self.sub_element(parent, Tag::P);
            self.nodes[p].text = Some(id);
            id
        };
        self.record(&block, |l| (!is_blank(&l.text)).then_some(Leaf::Para(id)));
    }
}

/// A source line as the model reads it. `collapsed` lines (the inside of a
/// fenced block) are dropped, and the fence's opening line stands for the whole
/// block, as a fence extension replaces it with a placeholder before the block
/// parser runs.
pub(crate) struct Source<'a> {
    pub lines: &'a [&'a str],
    /// Line index of each fenced block's opening line.
    pub fence_openers: &'a BTreeSet<usize>,
    /// Lines that belong to a fenced block but are not its opener.
    pub fence_inside: &'a BTreeSet<usize>,
}

fn expand_tabs(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut col = 0;
    for ch in line.chars() {
        if ch == '\t' {
            let pad = TAB - col % TAB;
            out.extend(std::iter::repeat_n(' ', pad));
            col += pad;
        } else {
            out.push(ch);
            col += 1;
        }
    }
    out
}

/// Length of the indentation and list markers in front of a fence opener, so
/// an opener written on an item's own line keeps the marker.
fn fence_prefix_len(text: &str) -> usize {
    let mut at = leading_spaces(text);
    loop {
        let rest = &text[at..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let marker = if digits > 0 && matches!(rest.as_bytes().get(digits), Some(b'.' | b')')) {
            digits + 1
        } else if matches!(rest.as_bytes().first(), Some(b'-' | b'*' | b'+')) {
            1
        } else {
            return at;
        };
        let spaces = rest[marker..].bytes().take_while(|&b| b == b' ').count();
        if spaces == 0 || rest[marker + spaces..].is_empty() {
            return at;
        }
        at += marker + spaces;
    }
}

fn preprocess(source: &Source<'_>) -> Vec<Ln> {
    let mut out = Vec::with_capacity(source.lines.len());
    for (idx, raw) in source.lines.iter().enumerate() {
        if source.fence_inside.contains(&idx) {
            continue;
        }
        let mut text = expand_tabs(raw.trim_end_matches('\r'));
        if source.fence_openers.contains(&idx) {
            text = format!("{}\u{2}fence\u{3}", &text[..fence_prefix_len(&text)]);
        } else if idx > 0 && text.bytes().all(|b| b == b' ') {
            // Python-Markdown clears a whitespace-only line only when a newline
            // precedes it, so the first line of the document keeps its spaces.
            text.clear();
        }
        out.push(Ln { idx, text });
    }
    out
}

/// What the parser made of each source line.
struct LineModel {
    /// The leaves of each line.
    kinds: Vec<Vec<Leaf>>,
    /// The text a code block holds for each line after de-indenting.
    code_text: Vec<Option<String>>,
    /// How many list items enclose the line's paragraph or code, `None` for a
    /// line that holds neither (a blank line, a bare marker).
    item_depth: Vec<Option<usize>>,
    /// The list each item line was added to and whether it renders as ordered.
    line_list: Vec<Option<(usize, bool)>>,
}

impl Parser {
    /// The number of list items enclosing each paragraph and code leaf.
    fn leaf_depths(&self) -> (HashMap<usize, usize>, HashMap<usize, usize>) {
        let mut paras = HashMap::new();
        let mut codes = HashMap::new();
        let mut stack = vec![(0usize, 0usize)];
        while let Some((node, outer)) = stack.pop() {
            let depth = outer + usize::from(self.nodes[node].tag == Tag::Li);
            if let Some(id) = self.nodes[node].text {
                paras.insert(id, depth);
            }
            for &child in &self.nodes[node].children {
                if self.nodes[child].tag == Tag::Pre {
                    codes.insert(child, depth);
                } else {
                    stack.push((child, depth));
                }
                if let Some(id) = self.nodes[child].tail {
                    paras.insert(id, depth);
                }
            }
        }
        (paras, codes)
    }
}

/// Per source line, what the parser produced.
fn line_model(source: &Source<'_>) -> Option<LineModel> {
    let lines = preprocess(source);
    // Every nesting level re-measures the indentation of the lines below it, so
    // the work grows with the square of the indentation; a document past the
    // limit is left unmodeled instead of parsed.
    let work: usize = lines
        .iter()
        .map(|l| {
            let indent = leading_spaces(&l.text);
            indent * indent / 4
        })
        .sum();
    if work > MAX_INDENT_WORK {
        return None;
    }
    let mut parser = Parser::new(source.lines.len());
    let blocks = split_blocks(&lines);
    parser.parse_blocks(0, blocks);
    if parser.overrun {
        return None;
    }
    let (paras, codes) = parser.leaf_depths();
    let item_depth = parser
        .kinds
        .iter()
        .map(|leaves| {
            leaves.iter().rev().find_map(|leaf| match leaf {
                Leaf::Marker => None,
                Leaf::Para(id) | Leaf::Heading(id) => paras.get(id).copied(),
                Leaf::Code(id) => codes.get(id).copied(),
            })
        })
        .collect();
    Some(LineModel {
        kinds: parser.kinds,
        code_text: parser.code_text,
        item_depth,
        line_list: parser.line_list,
    })
}

/// Whether the parser reads each line as the start of a list item. `None`
/// when the document is too deeply nested to model.
pub(crate) fn marker_lines(source: &Source<'_>) -> Option<Vec<bool>> {
    Some(
        line_model(source)?
            .kinds
            .iter()
            .map(|leaves| leaves.contains(&Leaf::Marker))
            .collect(),
    )
}

/// Region of each line: a run of blocks that starts at a root-level block that
/// is neither indented nor a list. Lines in different regions cannot influence
/// each other.
fn regions(source: &Source<'_>) -> Vec<usize> {
    let lines = preprocess(source);
    let mut region_of = vec![0; source.lines.len()];
    let mut starts: Vec<usize> = Vec::new();
    for block in split_blocks(&lines) {
        let Some(first) = block.first() else { continue };
        let boundary =
            !first.text.is_empty() && !first.text.starts_with("    ") && !olist_test(&block) && !ulist_test(&block);
        if boundary {
            starts.push(first.idx);
        }
    }
    let mut region = 0;
    let mut next = 0;
    for (idx, slot) in region_of.iter_mut().enumerate() {
        while next < starts.len() && starts[next] <= idx {
            next += 1;
            region += 1;
        }
        *slot = region;
    }
    region_of
}

/// Renumber the lists of `line_list` by first appearance in each region, so two
/// parses compare which items share a list and what kind of list it is.
fn canonical_lists(line_list: &[Option<(usize, bool)>], region_of: &[usize]) -> Vec<Option<(usize, bool)>> {
    let mut maps: HashMap<(usize, usize), usize> = HashMap::new();
    let mut counters: HashMap<usize, usize> = HashMap::new();
    line_list
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            entry.map(|(list, ordered)| {
                let region = region_of[idx];
                let counter = counters.entry(region).or_insert(0);
                let n = *maps.entry((region, list)).or_insert_with(|| {
                    *counter += 1;
                    *counter
                });
                (n, ordered)
            })
        })
        .collect()
}

/// Renumber leaf ids by first appearance so two parses compare structurally.
fn canonical(kinds: &[Vec<Leaf>], region_of: &[usize]) -> Vec<Vec<Leaf>> {
    let mut maps: HashMap<(usize, u8, usize), usize> = HashMap::new();
    let mut counters: HashMap<usize, usize> = HashMap::new();
    kinds
        .iter()
        .enumerate()
        .map(|(idx, leaves)| {
            let region = region_of[idx];
            leaves
                .iter()
                .map(|leaf| match *leaf {
                    Leaf::Marker => Leaf::Marker,
                    Leaf::Para(id) | Leaf::Code(id) | Leaf::Heading(id) => {
                        let kind = match leaf {
                            Leaf::Para(_) => 0,
                            Leaf::Code(_) => 1,
                            _ => 2,
                        };
                        let counter = counters.entry(region).or_insert(0);
                        let n = *maps.entry((region, kind, id)).or_insert_with(|| {
                            *counter += 1;
                            *counter
                        });
                        match kind {
                            0 => Leaf::Para(n),
                            1 => Leaf::Code(n),
                            _ => Leaf::Heading(n),
                        }
                    }
                })
                .collect()
        })
        .collect()
}

/// Regions whose lines are read differently in `after` than in `before`: a
/// line changed kind (paragraph, code, item marker), paragraphs and code
/// blocks were merged or split, or a line joined or left a list. `None` when the model cannot decide (the line
/// counts differ or the document is too deeply nested to model).
pub(crate) fn differing_regions(
    before: &Source<'_>,
    after: &Source<'_>,
    owner_of_line: &HashMap<usize, usize>,
) -> Option<BTreeSet<usize>> {
    if before.lines.len() != after.lines.len() {
        return None;
    }
    let region_of = regions(before);
    let (old, new) = (line_model(before)?, line_model(after)?);
    let a = canonical(&old.kinds, &region_of);
    let b = canonical(&new.kinds, &region_of);
    // The line of the item marker that opens `depth` for the line at `at`. Only
    // lines that start items are candidates, and the answer for a run of lines
    // between two candidates is the same, so it is computed once per run.
    let markers: Vec<(usize, usize)> = (0..new.kinds.len())
        .filter_map(|j| {
            let count = new.kinds[j].iter().filter(|k| **k == Leaf::Marker).count();
            (count > 0 && new.item_depth[j].is_some()).then_some((j, count))
        })
        .collect();
    let mut opening_cache: HashMap<(usize, usize), Option<usize>> = HashMap::new();
    let mut opening_marker = |at: usize, depth: usize| {
        let before = markers.partition_point(|&(j, _)| j < at);
        *opening_cache.entry((before, depth)).or_insert_with(|| {
            markers[..before]
                .iter()
                .rev()
                .find(|&&(j, count)| new.item_depth[j].is_some_and(|d| d >= depth && d + 1 - count <= depth))
                .map(|&(j, _)| j)
        })
    };
    // A line may move deeper inside the list it is already in (a paragraph
    // re-parented under the nested item). A line from outside every list may
    // join one only into the item the fix meant it for, and only when
    // Python-Markdown reads that item's marker as one. No line may leave a
    // list or move to a shallower one.
    let mut moved_across_a_list = |i: usize| match (old.item_depth[i], new.item_depth[i]) {
        (Some(0), Some(to)) if to > 0 => owner_of_line
            .get(&i)
            .is_none_or(|&owner| opening_marker(i, to) != Some(owner)),
        (Some(from), Some(to)) => to < from,
        _ => false,
    };
    let lists_before = canonical_lists(&old.line_list, &region_of);
    let lists_after = canonical_lists(&new.line_list, &region_of);
    let mut differing: BTreeSet<usize> = (0..a.len())
        .filter(|&i| {
            a[i] != b[i]
                || old.code_text[i] != new.code_text[i]
                || lists_before[i] != lists_after[i]
                || moved_across_a_list(i)
        })
        .map(|i| region_of[i])
        .collect();
    differing.extend(unmodeled_regions(before, &region_of));
    Some(differing)
}

/// Whether a line starts a construct the model does not parse (quotes,
/// admonitions, definition lists, tables), so the model cannot tell how it
/// reads around a moved line.
fn is_unmodeled(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('>')
        || t.starts_with("*[")
        || t.starts_with("!!!")
        || t.starts_with("???")
        || t.starts_with(": ")
        || t.contains('|')
        || is_reference_definition(t)
        || is_thematic_break(t)
        || is_setext_underline(t)
        || t.starts_with('<')
}

/// `[id]: url`, which Python-Markdown removes from the text once the line
/// starts a paragraph.
fn is_reference_definition(trimmed: &str) -> bool {
    trimmed.starts_with('[') && trimmed.contains("]:")
}

/// Three or more of one of `-`, `*` or `_`, with spaces between them.
fn is_thematic_break(trimmed: &str) -> bool {
    let Some(mark @ ('-' | '*' | '_')) = trimmed.chars().next() else {
        return false;
    };
    trimmed.chars().all(|c| c == mark || c == ' ') && trimmed.chars().filter(|&c| c == mark).count() >= 3
}

/// Whether a line inside a fenced block starts at the column of the opener.
/// Python-Markdown keeps the leading spaces of an indented fence's lines in
/// the paragraph it reads them as, so a fix that re-indents the block must
/// move every line together; the model cannot verify that.
fn flush_with_opener(source: &Source<'_>, idx: usize) -> bool {
    let opener = source.fence_openers.range(..idx).next_back();
    opener.is_none_or(|&o| {
        leading_spaces(&expand_tabs(source.lines[idx])) == leading_spaces(&expand_tabs(source.lines[o]))
    })
}

/// A run of `=`, or of two or more `-`, which turns the paragraph above it
/// into a heading.
fn is_setext_underline(trimmed: &str) -> bool {
    let t = trimmed.trim_end();
    (!t.is_empty() && t.chars().all(|c| c == '=')) || (t.len() >= 2 && t.chars().all(|c| c == '-'))
}

/// A line inside a fenced block that Python-Markdown would parse as structure
/// if it did not recognise the fence: a blank line, a list marker, a heading
/// or a quote.
fn is_structural_inside_fence(line: &str) -> bool {
    let t = line.trim_start();
    is_unmodeled(line)
        || t.is_empty()
        || t.starts_with('#')
        || t.starts_with('>')
        || t.starts_with("- ")
        || t.starts_with("* ")
        || t.starts_with("+ ")
        || t.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// A single `-` directly under a text line: Python-Markdown reads it as the
/// underline of a level-two heading (below a blank line it is a bare list marker).
fn is_lone_hyphen_under_text(line: &str, above: &str) -> bool {
    line.trim() == "-" && !above.trim().is_empty()
}

/// Whether a line holds whitespace other than spaces, tabs and a carriage
/// return. Python-Markdown and the line model disagree on whether such a line is
/// blank, so a block made of it may exist for one and not the other.
fn has_exotic_space(line: &str) -> bool {
    line.chars()
        .any(|c| c.is_whitespace() && !matches!(c, ' ' | '\t' | '\r'))
}

/// Whether a tab follows the line's text. Python-Markdown expands tabs to the
/// next multiple of four columns, so re-indenting the line moves where the tab
/// ends and changes the spaces it stands for.
fn has_inner_tab(line: &str) -> bool {
    line.trim_start().contains('\t')
}

/// Whether a line holds a backtick run that no run of the same length on the
/// line closes, or runs of different lengths (one can sit inside the other's
/// span, so pairing by length cannot tell what closes): a code span that continues across lines keeps its inner
/// indentation verbatim, so re-indenting only some of its lines changes its text.
fn has_unpaired_backticks(line: &str) -> bool {
    let mut runs: HashMap<usize, usize> = HashMap::new();
    let mut length = 0;
    for c in line.chars().chain(std::iter::once(' ')) {
        if c == '`' {
            length += 1;
        } else if length > 0 {
            *runs.entry(length).or_default() += 1;
            length = 0;
        }
    }
    runs.len() > 1 || runs.values().any(|count| count % 2 == 1)
}

/// Regions that hold an unmodeled construct below their first line. A region
/// that is itself a quote is left to the per-line model, which reads quoted
/// text as the paragraphs they are.
fn unmodeled_regions(source: &Source<'_>, region_of: &[usize]) -> BTreeSet<usize> {
    let mut first: HashMap<usize, usize> = HashMap::new();
    for (idx, &region) in region_of.iter().enumerate() {
        first.entry(region).or_insert(idx);
    }
    // A region holds a list when any of its lines reads as an item marker.
    let mut has_list: HashSet<usize> = HashSet::new();
    for (idx, &region) in region_of.iter().enumerate() {
        if child_match(source.lines[idx].trim_start()).is_some() {
            has_list.insert(region);
        }
    }
    let quoted_start = |region: usize| {
        !has_list.contains(&region)
            && first
                .get(&region)
                .is_some_and(|&i| source.lines[i].trim_start().starts_with('>'))
    };
    (0..source.lines.len())
        .filter(|&idx| {
            if has_exotic_space(source.lines[idx]) {
                true
            } else if source.fence_inside.contains(&idx) {
                is_structural_inside_fence(source.lines[idx])
                    || !flush_with_opener(source, idx)
                    || has_inner_tab(source.lines[idx])
            } else if source.fence_openers.contains(&idx) {
                false
            } else {
                (is_unmodeled(source.lines[idx]) && !quoted_start(region_of[idx]))
                    || has_unpaired_backticks(source.lines[idx])
                    || has_inner_tab(source.lines[idx])
                    || (idx > 0 && is_lone_hyphen_under_text(source.lines[idx], source.lines[idx - 1]))
            }
        })
        .map(|idx| region_of[idx])
        .collect()
}

/// The region of each line of `source`.
pub(crate) fn line_regions(source: &Source<'_>) -> Vec<usize> {
    regions(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaves(text: &str) -> Vec<Vec<Leaf>> {
        let lines: Vec<&str> = text.split('\n').collect();
        let none = BTreeSet::new();
        let source = Source {
            lines: &lines,
            fence_openers: &none,
            fence_inside: &none,
        };
        canonical(&line_model(&source).unwrap().kinds, &vec![0; lines.len()])
    }

    fn kinds(text: &str) -> String {
        leaves(text)
            .iter()
            .map(|l| {
                l.iter()
                    .map(|k| match k {
                        Leaf::Marker => 'M',
                        Leaf::Para(_) => 'P',
                        Leaf::Code(_) => 'C',
                        Leaf::Heading(_) => 'H',
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn a_heading_ends_the_paragraph_before_a_list() {
        assert_eq!(kinds("# Heading\n- a\n\n  text"), "H,MP,,P");
        assert_eq!(kinds("para\n# Heading\n- a"), "P,H,MP");
    }

    #[test]
    fn a_document_indented_too_deeply_to_model_in_bounded_time_is_not_modeled() {
        let text: String = (0..1200).map(|i| "    ".repeat(i) + "- x\n").collect();
        let lines: Vec<&str> = text.split('\n').collect();
        let none = BTreeSet::new();
        let source = Source {
            lines: &lines,
            fence_openers: &none,
            fence_inside: &none,
        };
        assert!(marker_lines(&source).is_none());
    }

    #[test]
    fn nested_item_continuation_is_a_paragraph() {
        assert_eq!(kinds("- a\n  - b\n\n    c"), "MP,MP,,P");
        assert_eq!(kinds("- a\n  - b\n\n        c"), "MP,MP,,C");
    }

    #[test]
    fn chained_item_with_a_text_marker_between_reads_text_as_code() {
        // `1)` is not a marker, so the four-column line after it is still the
        // outer item's text at one level, and the inner item's code at two.
        assert_eq!(kinds("- - a\n\n    1) b\n\n    text"), "MMP,,P,,P");
    }

    #[test]
    fn a_marker_indented_past_eight_columns_is_paragraph_text() {
        // Python-Markdown matches continuation items at four to seven columns
        // only; further in, the marker is plain text of the paragraph above.
        assert_eq!(kinds("- a\n      - b"), "MP,MP");
        assert_eq!(kinds("- a\n         - b"), "MP,P");
        assert_eq!(kinds("- b\n          -   c"), "MP,P");
    }

    #[test]
    fn a_paragraph_and_a_code_block_are_numbered_apart() {
        // The last paragraph is the fourth leaf: it never takes the number the
        // code block before it was given.
        let numbered = leaves(" - a\n\n        * a\n   10. word two\n      - - text\n");
        assert_eq!(
            numbered,
            vec![
                vec![Leaf::Marker, Leaf::Para(1)],
                vec![],
                vec![Leaf::Code(2)],
                vec![Leaf::Marker, Leaf::Para(3)],
                vec![Leaf::Marker, Leaf::Marker, Leaf::Para(4)],
                vec![],
            ]
        );
    }

    #[test]
    fn code_without_an_open_item() {
        assert_eq!(kinds("text\n\n    code"), "P,,C");
    }

    #[test]
    fn moving_a_paragraph_between_owners_keeps_its_kind() {
        assert_eq!(kinds("- a\n  - b\n\n  c"), kinds("- a\n  - b\n\n    c"));
    }

    #[test]
    fn turning_prose_into_code_is_a_difference() {
        let before: Vec<&str> = "- - a\n\n    1) b\n\n    text".split('\n').collect();
        let after: Vec<&str> = "- - a\n\n    1) b\n\n        text".split('\n').collect();
        let none = BTreeSet::new();
        let b = Source {
            lines: &before,
            fence_openers: &none,
            fence_inside: &none,
        };
        let a = Source {
            lines: &after,
            fence_openers: &none,
            fence_inside: &none,
        };
        assert_eq!(differing_regions(&b, &a, &HashMap::new()), Some(BTreeSet::from([0])));
    }

    fn differing(before: &str, after: &str) -> Option<BTreeSet<usize>> {
        differing_owned(before, after, &HashMap::new())
    }

    fn differing_owned(before: &str, after: &str, owners: &HashMap<usize, usize>) -> Option<BTreeSet<usize>> {
        let before: Vec<&str> = before.split('\n').collect();
        let after: Vec<&str> = after.split('\n').collect();
        let none = BTreeSet::new();
        let b = Source {
            lines: &before,
            fence_openers: &none,
            fence_inside: &none,
        };
        let a = Source {
            lines: &after,
            fence_openers: &none,
            fence_inside: &none,
        };
        differing_regions(&b, &a, owners)
    }

    #[test]
    fn a_paragraph_joining_a_list_is_accepted_only_under_its_owning_item() {
        let (before, after) = ("- a\n\n  text", "- a\n\n    text");
        // The owner is the item on line 0, which is the item the line joins.
        assert_eq!(
            differing_owned(before, after, &HashMap::from([(2, 0)])),
            Some(BTreeSet::new())
        );
        // An owner that is not the item the line joins (here: not a marker the
        // model reads) is a difference.
        assert_eq!(
            differing_owned(before, after, &HashMap::from([(2, 1)])).map(|r| r.len()),
            Some(1)
        );
        // Round 8: the owner `1) b` is text for Python-Markdown, not an item.
        assert_eq!(
            differing_owned(
                "- - a\n\n    1) b\n\n    text",
                "- - a\n\n    1) b\n\n        text",
                &HashMap::from([(4, 2)])
            )
            .map(|r| r.len()),
            Some(1)
        );
    }

    #[test]
    fn a_paragraph_joining_a_list_from_outside_it_is_a_difference() {
        // Two columns: a paragraph after the list. Four: inside the item.
        assert_eq!(
            differing("- a\n\n  text", "- a\n\n    text").map(|regions| regions.len()),
            Some(1)
        );
    }

    #[test]
    fn a_paragraph_moving_deeper_inside_its_list_is_not_a_difference() {
        // Six columns under `- b` at four: still the outer item's paragraph.
        // Eight: the nested item's.
        assert_eq!(
            differing("- a\n    - b\n\n      text", "- a\n    - b\n\n        text"),
            Some(BTreeSet::new())
        );
    }
}
