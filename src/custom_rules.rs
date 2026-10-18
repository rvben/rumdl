//! Experimental, report-only interchange for external Markdown checks.
//! This unstable interface is not covered by semver compatibility guarantees.
//!
//! No commands are spawned and no results are cached. All ranges address the
//! exact original UTF-8 source; this deliberately does not expose Rust AST types.
use crate::config::MarkdownFlavor;
use crate::lint_context::LintContext;
use crate::rule::{LintWarning, Severity};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_NODES: usize = 100_000;
const MAX_DIAGNOSTICS: usize = 10_000;

#[derive(Debug, Serialize)]
pub struct Document {
    pub protocol: &'static str,
    pub protocol_version: u32,
    pub path: String,
    pub source: String,
    pub source_hash: String,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Serialize)]
pub struct Node {
    pub id: usize,
    pub kind: &'static str,
    pub range: [usize; 2],
    pub parent: Option<usize>,
    pub prev_sibling: Option<usize>,
    pub heading_path: Vec<usize>,
    /// Original source slices, without code delimiters or block prefixes.
    /// They are raw source, not cooked/normalized parser text.
    pub content_ranges: Vec<[usize; 2]>,
    pub info: Option<String>,
    pub language: Option<String>,
}

pub fn source_hash(source: &str) -> String {
    blake3::hash(source.as_bytes()).to_hex().to_string()
}

pub fn export(path: &str, source: &str) -> Result<Document, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err("source exceeds 4 MiB".into());
    }
    let ctx = LintContext::new(source, MarkdownFlavor::Standard, None);
    let base = if ctx.front_matter_end_line() == 0 {
        0
    } else {
        ctx.lines
            .get(ctx.front_matter_end_line())
            .map_or(source.len(), |line| line.byte_offset)
    };
    let body = &source[base..];
    let mut builder = Builder {
        nodes: Vec::new(),
        parent: None,
        stack: Vec::new(),
        siblings: HashMap::new(),
        headings: Vec::new(),
    };
    for (event, range) in Parser::new_ext(body, crate::utils::rumdl_parser_options_for(body)).into_offset_iter() {
        let range = [range.start + base, range.end + base];
        match event {
            Event::Start(tag) => {
                // A heading in a quoted example or list item does not change
                // the surrounding document's section after that container ends.
                let outline = matches!(
                    tag,
                    Tag::Item | Tag::BlockQuote(_) | Tag::FootnoteDefinition(_) | Tag::DefinitionListDefinition
                )
                .then(|| builder.headings.clone());
                let (kind, info) = match &tag {
                    Tag::Heading { .. } => (Some("heading"), None),
                    Tag::Paragraph => (Some("paragraph"), None),
                    Tag::Item => (Some("list_item"), None),
                    Tag::BlockQuote(_) => (Some("blockquote"), None),
                    Tag::HtmlBlock => (Some("html_block"), None),
                    Tag::Table(_) => (Some("table"), None),
                    Tag::TableHead | Tag::TableRow => (Some("table_row"), None),
                    Tag::TableCell => (Some("table_cell"), None),
                    Tag::FootnoteDefinition(_) => (Some("footnote_definition"), None),
                    Tag::DefinitionListDefinition => (Some("definition_body"), None),
                    Tag::CodeBlock(CodeBlockKind::Fenced(info)) => (Some("fenced_code"), Some(info.to_string())),
                    Tag::CodeBlock(CodeBlockKind::Indented) => (Some("indented_code"), None),
                    _ => (None, None),
                };
                if let Tag::Heading { level, .. } = tag {
                    builder.headings.retain(|(prior, _)| *prior < level as usize);
                }
                let id = if let Some(kind) = kind {
                    let id = builder.add(kind, range, info)?;
                    if let Tag::Heading { level, .. } = tag {
                        builder.headings.push((level as usize, id));
                    }
                    Some(id)
                } else {
                    None
                };
                let previous_parent = builder.parent;
                if id.is_some() {
                    builder.parent = id;
                }
                builder.stack.push(Frame {
                    node: id,
                    outline,
                    previous_parent,
                });
            }
            Event::End(_) => {
                if let Some(frame) = builder.stack.pop() {
                    builder.parent = frame.previous_parent;
                    if let Some(id) = frame.node {
                        builder.nodes[id].range[1] = range[1];
                    }
                    if let Some(outline) = frame.outline {
                        builder.headings = outline;
                    }
                }
            }
            Event::Code(_) => {
                let id = builder.add("code_span", range, None)?;
                let raw = &source[range[0]..range[1]];
                let delimiters = raw.bytes().take_while(|b| *b == b'`').count();
                if delimiters * 2 <= raw.len() {
                    builder.nodes[id]
                        .content_ranges
                        .push([range[0] + delimiters, range[1] - delimiters]);
                }
            }
            Event::Text(_) => {
                if let Some(id) = builder.parent
                    && matches!(builder.nodes[id].kind, "fenced_code" | "indented_code")
                {
                    builder.nodes[id].content_ranges.push(range);
                }
            }
            _ => {}
        }
    }
    Ok(Document {
        protocol: "rumdl.elements",
        protocol_version: 1,
        path: path.into(),
        source: source.into(),
        source_hash: source_hash(source),
        nodes: builder.nodes,
    })
}

struct Frame {
    node: Option<usize>,
    previous_parent: Option<usize>,
    outline: Option<Vec<(usize, usize)>>,
}

struct Builder {
    nodes: Vec<Node>,
    parent: Option<usize>,
    stack: Vec<Frame>,
    siblings: HashMap<Option<usize>, usize>,
    headings: Vec<(usize, usize)>,
}
impl Builder {
    fn add(&mut self, kind: &'static str, range: [usize; 2], info: Option<String>) -> Result<usize, String> {
        if self.nodes.len() >= MAX_NODES {
            return Err("document exceeds 100000 exported nodes".into());
        }
        let parent = self.parent;
        let id = self.nodes.len();
        let prev_sibling = self.siblings.insert(parent, id);
        let language = info
            .as_ref()
            .and_then(|s| s.split_whitespace().next())
            .map(str::to_owned);
        self.nodes.push(Node {
            id,
            kind,
            range,
            parent,
            prev_sibling,
            heading_path: self.headings.iter().map(|(_, id)| *id).collect(),
            content_ranges: Vec::new(),
            info,
            language,
        });
        Ok(id)
    }
}

#[derive(Deserialize)]
struct Response {
    protocol: String,
    protocol_version: u32,
    source_hash: String,
    diagnostics: Vec<Diagnostic>,
}
#[derive(Deserialize)]
struct Diagnostic {
    rule: String,
    start_byte: usize,
    end_byte: usize,
    message: String,
}

pub fn validate(source: &str, bytes: &[u8]) -> Result<Vec<LintWarning>, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err("source exceeds 4 MiB".into());
    }
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("diagnostic response exceeds 1 MiB".into());
    }
    let response: Response = serde_json::from_slice(bytes).map_err(|e| format!("invalid diagnostic JSON: {e}"))?;
    if response.protocol != "rumdl.diagnostics" || response.protocol_version != 1 {
        return Err("unsupported diagnostic protocol or version".into());
    }
    if response.source_hash != source_hash(source) {
        return Err("source hash mismatch: diagnostics are stale or belong to another document".into());
    }
    if response.diagnostics.len() > MAX_DIAGNOSTICS {
        return Err("response exceeds 10000 diagnostics".into());
    }

    // Validate the entire response before exposing even one finding.
    for diagnostic in &response.diagnostics {
        let parts: Vec<_> = diagnostic.rule.split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|part| {
                part.is_empty()
                    || !part
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            })
        {
            return Err("rule IDs must be lowercase namespace/name identifiers".into());
        }
        if diagnostic.message.trim().is_empty()
            || diagnostic.message.len() > 4096
            || diagnostic
                .message
                .chars()
                .any(|c| c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        {
            return Err(
                "diagnostic message must be nonempty, at most 4096 bytes, and contain no control characters".into(),
            );
        }
        let (start, end) = (diagnostic.start_byte, diagnostic.end_byte);
        if start > end || end > source.len() || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            return Err("diagnostic range must be ordered, in bounds, and on UTF-8 boundaries".into());
        }
    }
    let ctx = LintContext::new(source, MarkdownFlavor::Standard, None);
    let mut positions: BTreeMap<_, _> = response
        .diagnostics
        .iter()
        .flat_map(|d| [d.start_byte, d.end_byte])
        .map(|offset| (offset, (0, 0)))
        .collect();
    // Convert each requested boundary once, walking forward within a line.
    // Re-scanning a long line from its start for every diagnostic is quadratic.
    let mut previous: Option<(usize, usize, (usize, usize))> = None;
    for (&offset, position) in &mut positions {
        let group = ctx.line_offsets.partition_point(|start| *start <= offset);
        *position = if let Some((prior, prior_group, (line, column))) = previous
            && prior_group == group
        {
            (line, column + source[prior..offset].chars().count())
        } else {
            ctx.offset_to_line_col(offset)
        };
        previous = Some((offset, group, *position));
    }
    let mut warnings = Vec::new();
    for diagnostic in response.diagnostics {
        let (line, column) = positions[&diagnostic.start_byte];
        let (end_line, end_column) = positions[&diagnostic.end_byte];
        warnings.push(LintWarning {
            message: diagnostic.message,
            line,
            column,
            end_line,
            end_column,
            severity: Severity::Warning,
            fix: None,
            rule_name: Some(diagnostic.rule),
        });
    }
    warnings.sort_by_key(|w| {
        (
            w.line,
            w.column,
            w.end_line,
            w.end_column,
            w.rule_name.clone(),
            w.message.clone(),
        )
    });
    Ok(warnings)
}
