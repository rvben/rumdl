//! Document and workspace symbol support.
//!
//! Markdown has no functions or classes, so rumdl exposes the document's heading
//! outline as LSP symbols: `textDocument/documentSymbol` returns a nested tree of
//! headings (each heading nests under the nearest preceding heading of a smaller
//! level), and `workspace/symbol` searches headings across the indexed workspace.

use tower_lsp::lsp_types::{DocumentSymbol, Location, Position, Range, SymbolInformation, SymbolKind, Url};

use super::completion::byte_to_utf16_offset;
use super::navigation::heading_text_byte_range;
use crate::lint_context::LintContext;
use crate::workspace_index::{HeadingIndex, WorkspaceIndex};

/// Placeholder name for an empty heading so symbol pickers never show a blank row.
const UNTITLED: &str = "(untitled)";

/// A heading flattened from a parsed document, in document order, ready to be
/// turned into a [`DocumentSymbol`]. Positions are LSP coordinates (0-based line,
/// UTF-16 code-unit character offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HeadingSymbol {
    /// Heading level (1-6).
    pub level: u8,
    /// Heading text (without markers or custom-id syntax).
    pub name: String,
    /// 0-based line of the heading.
    pub line: u32,
    /// UTF-16 character offset where the heading name starts on its line.
    pub name_start: u32,
    /// UTF-16 character offset where the heading name ends on its line.
    pub name_end: u32,
    /// 0-based last line of the heading's section (inclusive): the line just
    /// before the next heading of the same or a higher level, or the last line of
    /// the document.
    pub section_end_line: u32,
    /// UTF-16 length of `section_end_line`.
    pub section_end_char: u32,
}

/// Extract heading symbols from a parsed document, in document order.
///
/// Every heading rumdl recognizes is included, matching the workspace index and
/// link-navigation so the outline, anchors, and cross-file search agree. Each
/// heading's name range is the heading text on its line; its section runs until
/// the next heading of the same or a higher level, or the end of the document.
/// Positions are converted to UTF-16 for LSP.
pub(super) fn extract_heading_symbols(ctx: &LintContext) -> Vec<HeadingSymbol> {
    let line_texts: Vec<&str> = ctx.lines.iter().map(|li| li.content(ctx.content)).collect();
    let utf16_len = |line: u32| -> u32 {
        line_texts
            .get(line as usize)
            .map_or(0, |t| byte_to_utf16_offset(t, t.len()))
    };

    // First pass: one entry per heading, with its UTF-16 name range.
    let mut headings: Vec<HeadingSymbol> = Vec::new();
    for (i, line_info) in ctx.lines.iter().enumerate() {
        let Some(heading) = &line_info.heading else {
            continue;
        };
        let is_setext = matches!(
            heading.style,
            crate::lint_context::types::HeadingStyle::Setext1 | crate::lint_context::types::HeadingStyle::Setext2
        );
        let line_text = line_texts[i];
        // Prefer the precise heading-text span; fall back to the content column for
        // headings rumdl recognizes but that lack a space after `#` (e.g. `#Note`).
        let (start_byte, end_byte) = heading_text_byte_range(line_text, is_setext)
            .unwrap_or_else(|| (heading.content_column.min(line_text.len()), line_text.trim_end().len()));
        let (name_start, name_end) = (
            byte_to_utf16_offset(line_text, start_byte),
            byte_to_utf16_offset(line_text, end_byte.max(start_byte)),
        );
        headings.push(HeadingSymbol {
            level: heading.level,
            name: heading.text.clone(),
            line: i as u32,
            name_start,
            name_end,
            // Filled in by the second pass.
            section_end_line: i as u32,
            section_end_char: 0,
        });
    }

    // Second pass: each heading's section runs until the next heading of the same
    // or a higher (smaller-numbered) level, or the last line of the document.
    let last_line = ctx.lines.len().saturating_sub(1) as u32;
    for i in 0..headings.len() {
        let level = headings[i].level;
        let end_line = headings[i + 1..]
            .iter()
            .find(|h| h.level <= level)
            .map_or(last_line, |h| h.line.saturating_sub(1))
            .max(headings[i].line);
        headings[i].section_end_line = end_line;
        headings[i].section_end_char = utf16_len(end_line);
    }

    headings
}

/// Build the document symbol outline (nested heading tree) for a parsed document.
pub(super) fn document_symbols(ctx: &LintContext) -> Vec<DocumentSymbol> {
    build_symbol_tree(&extract_heading_symbols(ctx))
}

/// Build the flat document symbol outline for clients that do not support
/// hierarchical document symbols. Each heading becomes a [`SymbolInformation`]
/// whose `container_name` is the nearest enclosing (smaller-level) heading.
pub(super) fn document_symbols_flat(ctx: &LintContext, uri: &Url) -> Vec<SymbolInformation> {
    let headings = extract_heading_symbols(ctx);
    let mut stack: Vec<(u8, String)> = Vec::new();
    let mut symbols = Vec::with_capacity(headings.len());
    for heading in &headings {
        while stack.last().is_some_and(|(level, _)| *level >= heading.level) {
            stack.pop();
        }
        let container_name = stack.last().map(|(_, name)| name.clone());
        let name = if heading.name.is_empty() {
            UNTITLED.to_string()
        } else {
            heading.name.clone()
        };
        symbols.push(heading_symbol_information(uri, heading, &name, container_name));
        stack.push((heading.level, name));
    }
    symbols
}

#[allow(deprecated)] // `SymbolInformation::deprecated` is a required struct field.
fn heading_symbol_information(
    uri: &Url,
    heading: &HeadingSymbol,
    name: &str,
    container_name: Option<String>,
) -> SymbolInformation {
    SymbolInformation {
        name: name.to_string(),
        kind: SymbolKind::STRING,
        tags: None,
        deprecated: None,
        location: Location {
            uri: uri.clone(),
            range: Range {
                start: Position {
                    line: heading.line,
                    character: heading.name_start,
                },
                end: Position {
                    line: heading.line,
                    character: heading.name_end,
                },
            },
        },
        container_name,
    }
}

/// Build a nested [`DocumentSymbol`] tree from headings in document order.
///
/// Each heading becomes a child of the nearest preceding heading with a strictly
/// smaller level; headings with no smaller-level ancestor are roots. Skipped
/// levels (an `H1` directly followed by an `H3`) and documents that do not start
/// at `H1` are handled gracefully - the shallower heading simply becomes the
/// parent or a root.
pub(super) fn build_symbol_tree(headings: &[HeadingSymbol]) -> Vec<DocumentSymbol> {
    let mut idx = 0;
    build_children(headings, &mut idx, 0)
}

/// Collect every heading at `headings[*idx..]` deeper than `parent_level` into a
/// sibling list, recursing to attach descendants. Advances `*idx` past everything
/// consumed.
fn build_children(headings: &[HeadingSymbol], idx: &mut usize, parent_level: u8) -> Vec<DocumentSymbol> {
    let mut siblings = Vec::new();
    while *idx < headings.len() {
        let level = headings[*idx].level;
        if level <= parent_level {
            break;
        }
        let heading = &headings[*idx];
        *idx += 1;
        let children = build_children(headings, idx, level);
        siblings.push(to_document_symbol(heading, children));
    }
    siblings
}

/// Search every indexed file's headings for `query` (already lowercased; an empty
/// query matches all) and return them as a flat list of [`SymbolInformation`],
/// ordered by file path. Workspace symbols carry the same heading set the document
/// outline and link navigation use, so all three stay consistent.
pub(super) fn workspace_symbols(index: &WorkspaceIndex, query: &str) -> Vec<SymbolInformation> {
    let mut symbols = Vec::new();
    for (path, file_index) in index.files_sorted() {
        let Ok(uri) = Url::from_file_path(path) else {
            continue;
        };
        for heading in &file_index.headings {
            if !query.is_empty() && !heading.text.to_lowercase().contains(query) {
                continue;
            }
            symbols.push(to_symbol_information(&uri, heading));
        }
    }
    symbols
}

#[allow(deprecated)] // `SymbolInformation::deprecated` is a required struct field.
fn to_symbol_information(uri: &Url, heading: &HeadingIndex) -> SymbolInformation {
    let line = (heading.line.saturating_sub(1)) as u32;
    let position = Position { line, character: 0 };
    SymbolInformation {
        name: if heading.text.is_empty() {
            UNTITLED.to_string()
        } else {
            heading.text.clone()
        },
        kind: SymbolKind::STRING,
        tags: None,
        deprecated: None,
        location: Location {
            uri: uri.clone(),
            range: Range {
                start: position,
                end: position,
            },
        },
        container_name: None,
    }
}

#[allow(deprecated)] // `DocumentSymbol::deprecated` is a required struct field.
fn to_document_symbol(heading: &HeadingSymbol, children: Vec<DocumentSymbol>) -> DocumentSymbol {
    let name = if heading.name.is_empty() {
        UNTITLED.to_string()
    } else {
        heading.name.clone()
    };
    DocumentSymbol {
        name,
        detail: None,
        kind: SymbolKind::STRING,
        tags: None,
        deprecated: None,
        range: Range {
            start: Position {
                line: heading.line,
                character: 0,
            },
            end: Position {
                line: heading.section_end_line,
                character: heading.section_end_char,
            },
        },
        selection_range: Range {
            start: Position {
                line: heading.line,
                character: heading.name_start,
            },
            end: Position {
                line: heading.line,
                character: heading.name_end,
            },
        },
        children: if children.is_empty() { None } else { Some(children) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(level: u8, name: &str, line: u32) -> HeadingSymbol {
        HeadingSymbol {
            level,
            name: name.to_string(),
            line,
            name_start: 0,
            name_end: name.len() as u32,
            section_end_line: line,
            section_end_char: 0,
        }
    }

    use crate::config::MarkdownFlavor;

    fn symbols_for(md: &str) -> Vec<DocumentSymbol> {
        let ctx = LintContext::new(md, MarkdownFlavor::Standard, None);
        document_symbols(&ctx)
    }

    #[test]
    fn test_document_symbols_tree_and_section_ranges() {
        let md = "# Title\n\nIntro paragraph.\n\n## Section A\n\nContent A.\n\n### Subsection\n\nMore.\n\n## Section B\n\nEnd.\n";
        let tree = symbols_for(md);

        assert_eq!(tree.len(), 1, "single H1 root: {tree:?}");
        let title = &tree[0];
        assert_eq!(title.name, "Title");
        // Section spans the whole document (no other H1).
        assert_eq!(title.range.start.line, 0);
        assert_eq!(title.range.end.line, 14);
        // Selection range is the heading text `Title` after `# `.
        assert_eq!(title.selection_range.start.character, 2);
        assert_eq!(title.selection_range.end.character, 7);

        let children = title.children.as_ref().expect("Title has children");
        assert_eq!(children.len(), 2, "Section A and Section B");
        let (a, b) = (&children[0], &children[1]);
        assert_eq!(a.name, "Section A");
        assert_eq!(b.name, "Section B");
        // Section A ends just before Section B.
        assert_eq!(a.range.start.line, 4);
        assert_eq!(a.range.end.line, 11);
        // Section B runs to the end.
        assert_eq!(b.range.start.line, 12);
        assert_eq!(b.range.end.line, 14);

        let sub = a.children.as_ref().expect("Section A has a child");
        assert_eq!(sub.len(), 1);
        assert_eq!(sub[0].name, "Subsection");
        assert_eq!(sub[0].range.start.line, 8);
        assert_eq!(sub[0].range.end.line, 11);
    }

    #[test]
    fn test_document_symbols_utf16_name_range() {
        // Non-ASCII before and within the heading text: the name range must use
        // UTF-16 code units, not bytes or chars.
        let md = "# café ☕ bar\n";
        let tree = symbols_for(md);
        assert_eq!(tree.len(), 1);
        let h = &tree[0];
        assert_eq!(h.name, "café ☕ bar");
        // `# ` is 2 UTF-16 units; `café ☕ bar` is 10 UTF-16 units (☕ is one unit
        // in the BMP), so the name ends at 12.
        assert_eq!(h.selection_range.start.character, 2);
        assert_eq!(h.selection_range.end.character, 12);
    }

    #[test]
    fn test_document_symbols_respect_flavor() {
        // `# -8<- [start:section]` is a heading in Standard markdown but a MkDocs
        // snippet marker (not a heading) in MkDocs mode. The outline must reflect
        // whichever flavor the document is parsed with.
        let md = "# Real Heading\n\n# -8<- [start:section]\n";

        let standard = document_symbols(&LintContext::new(md, MarkdownFlavor::Standard, None));
        assert_eq!(standard.len(), 2, "Standard treats the snippet line as a heading");

        let mkdocs = document_symbols(&LintContext::new(md, MarkdownFlavor::MkDocs, None));
        assert_eq!(mkdocs.len(), 1, "MkDocs excludes the snippet marker: {mkdocs:?}");
        assert_eq!(mkdocs[0].name, "Real Heading");
    }

    #[test]
    fn test_document_symbols_setext_headings() {
        // Setext: `Title`/`=====` is H1, `Sub`/`-----` is H2 nesting under it.
        let md = "Title\n=====\n\nbody\n\nSub\n---\n\nmore\n";
        let tree = symbols_for(md);
        assert_eq!(tree.len(), 1, "single setext H1 root: {tree:?}");
        assert_eq!(tree[0].name, "Title");
        assert_eq!(
            tree[0].selection_range.start.character, 0,
            "setext name starts at column 0"
        );
        assert_eq!(tree[0].selection_range.end.character, 5);
        let children = tree[0].children.as_ref().expect("Title has a child");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "Sub");
    }

    #[test]
    fn test_document_symbols_custom_id_excluded_from_name_range() {
        // The `{#id}` suffix is not part of the heading name or its selection range.
        let md = "## Setup Guide {#setup}\n";
        let tree = symbols_for(md);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "Setup Guide");
        assert_eq!(
            tree[0].selection_range.start.character, 3,
            "name starts after the marker"
        );
        assert_eq!(
            tree[0].selection_range.end.character, 14,
            "name ends before the custom-id suffix"
        );
    }

    #[test]
    fn test_document_symbols_closing_sequence_and_custom_id_excluded() {
        // A heading using both a closing `##` sequence and a custom id: the name
        // range covers only `Title`, not `Title ##`.
        let md = "## Title ## {#t}\n";
        let tree = symbols_for(md);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "Title");
        assert_eq!(tree[0].selection_range.start.character, 3);
        assert_eq!(
            tree[0].selection_range.end.character, 8,
            "excludes the closing ## and custom id"
        );
    }

    #[test]
    fn test_document_symbols_setext_custom_id_excluded_from_name_range() {
        // A custom id on a Setext heading is excluded from the selection range too,
        // so it matches the symbol name like ATX headings do.
        let md = "Title {#t}\n=====\n";
        let tree = symbols_for(md);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "Title");
        assert_eq!(tree[0].selection_range.start.character, 0);
        assert_eq!(
            tree[0].selection_range.end.character, 5,
            "excludes the custom-id suffix"
        );
    }

    #[test]
    fn test_document_symbols_match_recognized_headings() {
        // The outline includes every heading rumdl recognizes, so it stays
        // consistent with anchors and cross-file navigation. `#Note` (no space, a
        // heading rumdl recognizes and flags via MD018) is included with a sensible
        // name range; a fenced `# Code` line is not a heading and is excluded.
        let md = "# Real\n\n#Note\n\n```\n# Code\n```\n";
        let tree = symbols_for(md);
        let names: Vec<&str> = tree.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Real", "Note"], "fenced code is not a heading: {tree:?}");
        // The `#Note` name range covers `Note` (after the single `#`), not the marker.
        assert_eq!(tree[1].selection_range.start.character, 1);
        assert_eq!(tree[1].selection_range.end.character, 5);
    }

    fn heading_index(text: &str, line: usize) -> HeadingIndex {
        HeadingIndex {
            text: text.to_string(),
            auto_anchor: text.to_lowercase().replace(' ', "-"),
            custom_anchor: None,
            line,
            is_setext: false,
        }
    }

    /// Absolute, cross-platform path so `Url::from_file_path` succeeds on Windows.
    fn ws_path(rel: &str) -> std::path::PathBuf {
        std::env::temp_dir().join("rumdl-sym-ut").join(rel)
    }

    fn index_with(files: &[(&str, &[(&str, usize)])]) -> WorkspaceIndex {
        use crate::workspace_index::FileIndex;
        let mut index = WorkspaceIndex::new();
        for (path, headings) in files {
            let mut fi = FileIndex::default();
            for (text, line) in *headings {
                fi.headings.push(heading_index(text, *line));
            }
            index.insert_file(ws_path(path), fi);
        }
        index
    }

    #[test]
    fn test_workspace_symbols_filters_by_query() {
        let index = index_with(&[
            ("a.md", &[("Installation Guide", 1), ("Usage", 5)]),
            ("b.md", &[("Install From Source", 1)]),
        ]);

        // Query is matched case-insensitively as a substring of the heading text.
        let results = workspace_symbols(&index, "install");
        let names: Vec<&str> = results.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Installation Guide", "Install From Source"]);

        // The location points at the heading's line (0-based).
        assert_eq!(results[0].location.range.start.line, 0);
        assert!(results[0].location.uri.as_str().ends_with("a.md"));
    }

    #[test]
    fn test_workspace_symbols_empty_query_returns_all() {
        let index = index_with(&[("a.md", &[("One", 1), ("Two", 2)]), ("b.md", &[("Three", 1)])]);
        let results = workspace_symbols(&index, "");
        assert_eq!(results.len(), 3, "empty query returns every heading");
        // Results are ordered by file path.
        let names: Vec<&str> = results.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["One", "Two", "Three"]);
    }

    #[test]
    fn test_build_symbol_tree_nests_by_level() {
        // H1 / H2 / H2 / H3 / H1
        let headings = [
            h(1, "Top", 0),
            h(2, "A", 1),
            h(2, "B", 2),
            h(3, "B.1", 3),
            h(1, "Second", 4),
        ];
        let tree = build_symbol_tree(&headings);

        assert_eq!(tree.len(), 2, "two top-level headings");
        assert_eq!(tree[0].name, "Top");
        assert_eq!(tree[1].name, "Second");

        let top_children = tree[0].children.as_ref().expect("Top has children");
        assert_eq!(top_children.len(), 2, "A and B nest under Top");
        assert_eq!(top_children[0].name, "A");
        assert!(top_children[0].children.is_none(), "A has no children");
        assert_eq!(top_children[1].name, "B");

        let b_children = top_children[1].children.as_ref().expect("B has a child");
        assert_eq!(b_children.len(), 1);
        assert_eq!(b_children[0].name, "B.1");

        assert!(tree[1].children.is_none(), "Second has no children");
    }
}
