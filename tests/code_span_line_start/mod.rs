//! Code asks "does this line start inside a code span?" by name.
//!
//! The byte queries below answer "is this position inside a span", and a span
//! counts from its opening backtick. A line's first byte is that backtick when
//! the line opens with a code span, so passing a line start to them also
//! answers yes for a line that merely BEGINS a span. That is not what a caller
//! asking whether a line continues a span opened earlier wants: it made MD077
//! skip a column-0 continuation line such as `` `Sed` do eiusmod ``, and the
//! table detector miss a header row such as `` `a` | b ``.
//!
//! `line_starts_inside_code_span` (and its `_with_or_without_math` form) take
//! the line and count only a span opened on an earlier line.
//!
//! The scan is textual, over the raw text of every file under `src`. It sees a
//! line start passed under its usual names (`info.byte_offset`, `line_start`,
//! ...), and not an offset renamed through another local or a hand-written
//! `partition_point`; those are covered by the shared
//! `CodeBlockUtils::is_strictly_inside` primitive and the MD077 behavior matrix.
//! Comments and strings are scanned on purpose: a mention that trips the scan is
//! reworded.

use regex::Regex;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

/// Methods that answer "is this byte inside a span" with the span's opening
/// byte included.
const INCLUSIVE_QUERIES: &[&str] = &[
    "is_byte_offset_in_code_span",
    "is_in_code_span_byte",
    "is_in_code_span_byte_with_or_without_math",
    "is_code_span_text_byte",
    "is_in_code_block_or_span",
];

/// A method call of an inclusive query with a single argument free of calls and
/// parentheses (an argument that is a call or an expression is not a bare line
/// start). Path calls such as `CodeBlockUtils::is_in_code_block_or_span(blocks,
/// pos)` have no `.` before the name and take two arguments.
static CALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"\.({})\s*\(\s*([^()]*?)\s*,?\s*\)",
        INCLUSIVE_QUERIES.join("|")
    ))
    .unwrap()
});
static INDEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]").unwrap());
static LINE_START: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:(?:line_start|line_offset|line_byte_offset)|(?:\w+\.)*(?:info|line_info|line|lines\[_\])\.byte_offset)$",
    )
    .unwrap()
});

/// Whether `argument` is exactly a line's first byte: a line-start name, or a
/// path ending in the `byte_offset` of a line. The contents of an index are
/// ignored (`lines[idx + 1].byte_offset` still names one line's start), but
/// arithmetic on the offset itself (`+ n`, `- n`) is not a line start.
fn is_bare_line_start(argument: &str) -> bool {
    let compact: String = argument.chars().filter(|c| !c.is_whitespace()).collect();
    LINE_START.is_match(&INDEX.replace_all(&compact, "[_]"))
}

/// `(line number, query, argument)` for every inclusive query asked about a
/// bare line start.
fn offending_calls(source: &str) -> Vec<(usize, String, String)> {
    CALL.captures_iter(source)
        .filter(|m| is_bare_line_start(&m[2]))
        .map(|m| {
            let line = source[..m.get(0).unwrap().start()].matches('\n').count() + 1;
            (line, m[1].to_owned(), m[2].to_owned())
        })
        .collect()
}

/// Every `.rs` file below `dir`.
fn rust_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for path in fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()) {
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

#[test]
fn no_code_span_query_is_asked_about_a_bare_line_start() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = rust_files(&manifest.join("src"));
    assert!(files.len() > 100, "the scan found only {} source files", files.len());
    let mut offenders = Vec::new();
    for path in &files {
        for (line, query, argument) in offending_calls(&fs::read_to_string(path).unwrap()) {
            offenders.push(format!(
                "{}:{line}: {query}({argument})",
                path.strip_prefix(manifest).unwrap().display()
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "these queries count a span that OPENS at the position, so a line that begins a code \
         span reads as already inside one. Ask `line_starts_inside_code_span` (or \
         `line_starts_inside_code_span_with_or_without_math`) with the line instead:\n{}",
        offenders.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offending(source: &str) -> Vec<String> {
        offending_calls(source).into_iter().map(|(_, query, _)| query).collect()
    }

    #[test]
    fn flags_each_query_given_each_line_start_spelling() {
        for query in INCLUSIVE_QUERIES {
            for argument in [
                "info.byte_offset",
                "line.byte_offset",
                "ctx.lines[idx].byte_offset",
                "ctx.lines[idx + 1].byte_offset",
                "self.lines[i - 1].byte_offset",
                "line_start",
                "line_offset",
            ] {
                let source = format!("let x = ctx.{query}({argument});");
                assert_eq!(offending(&source), vec![(*query).to_owned()], "{source}");
            }
        }
    }

    #[test]
    fn flags_a_call_split_over_lines_and_reports_its_line() {
        let source = "fn f() {\n    ctx\n        .is_in_code_span_byte(\n            info.byte_offset,\n        )\n}\n";
        let calls = offending_calls(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, 3);
    }

    #[test]
    fn flags_a_mention_in_a_comment() {
        assert_eq!(offending("// ctx.is_in_code_span_byte(line_start)\n").len(), 1);
    }

    #[test]
    fn allows_an_offset_inside_the_line_or_an_element_start() {
        for source in [
            "ctx.is_in_code_span_byte(line.byte_offset + start)",
            "ctx.is_byte_offset_in_code_span(link.byte_offset)",
            "ctx.is_in_code_span_byte(end - ctx.lines[idx].byte_offset)",
            "ctx.is_in_code_span_byte(ctx.lines[idx].byte_offset + source_line_without_cr(ctx, idx).len())",
        ] {
            assert!(offending(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn allows_the_named_queries_the_block_range_form_and_definitions() {
        for source in [
            "ctx.line_starts_inside_code_span(info)",
            "ctx.line_starts_inside_code_span_with_or_without_math(&ctx.lines[idx])",
            "CodeBlockUtils::is_in_code_block_or_span(code_blocks, line_start)",
            "CodeBlockUtils::is_in_code_block_or_span(&self.code_blocks, line.byte_offset)",
            "pub fn is_in_code_span_byte(&self, pos: usize) -> bool { true }",
        ] {
            assert!(offending(source).is_empty(), "{source}");
        }
    }
}
