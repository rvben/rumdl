//! Wrapping of long lines in YAML and TOML front matter.
//!
//! Front matter is data, not Markdown, so a line can only be wrapped where the
//! format lets the value keep its meaning:
//!
//! - YAML plain and quoted scalars fold a line break into one space, so a
//!   scalar can continue on a deeper-indented line.
//! - TOML basic strings cannot hold a newline, but a multi-line string whose
//!   lines end in `\` drops the newline and the next line's indentation, so a
//!   `"..."` string can become `"""..."""` without changing its value.
//! - TOML arrays may span lines, so a long one is rewritten one packed row at
//!   a time.
//!
//! Where a value sits comes from the shared front matter helpers (YAML, whose
//! parser reports no positions) or from `toml_edit`'s spans (TOML). Whether an
//! edit is safe is never guessed: the front matter is parsed before and after
//! with `serde_saphyr` / `toml`, and an edit that changes the parsed value is
//! dropped.

use super::MD013LineLength;
use crate::rule::Fix;
use crate::rules::front_matter_utils::FrontMatterType;
use crate::utils::frontmatter_values;
use std::collections::HashMap;
use toml_edit::{Item, Table, Value};

type Measure<'a> = &'a dyn Fn(&str) -> usize;

/// Edits that wrap the long front matter lines of `ctx`, keyed by zero-based
/// line index. Empty unless the front matter is YAML or TOML.
pub(super) fn wrap_fixes(
    rule: &MD013LineLength,
    ctx: &crate::lint_context::LintContext,
    limit: usize,
) -> HashMap<usize, Fix> {
    let mut fixes = HashMap::new();
    if limit == 0 {
        return fixes;
    }
    let format = crate::rules::front_matter_utils::FrontMatterUtils::detect_front_matter_type(ctx.content);
    // `front_matter_end_line` is the 1-based number of the closing delimiter.
    let end = ctx.front_matter_end_line();
    if !matches!(format, FrontMatterType::Yaml | FrontMatterType::Toml) || end < 3 {
        return fixes;
    }
    let first = 1;
    let body: Vec<&str> = (first..end - 1)
        .map(|idx| ctx.lines[idx].content(ctx.content).trim_end_matches('\r'))
        .collect();

    let measure = |s: &str| rule.calculate_string_length(s);
    let replacements = if format == FrontMatterType::Yaml {
        let edits = body
            .iter()
            .map(|line| yaml_wrap_line(line.trim_end(), limit, &measure))
            .collect();
        keep_verified(&body, edits, |text| {
            let mut options = crate::utils::yaml::options();
            // Non-finite numbers are valid YAML. Compare their canonical spellings
            // rather than rejecting all edits because an unrelated field uses one.
            options.non_finite_float_policy = serde_saphyr::NonFiniteFloatPolicy::AsString;
            serde_saphyr::from_str_with_options::<serde_json::Value>(text, options).ok()
        })
    } else {
        let edits = toml_wrap_lines(&body, limit, &measure);
        keep_verified(&body, edits, |text| toml::from_str::<toml::Value>(text).ok())
    };

    for (offset, replacement) in replacements.into_iter().enumerate() {
        let Some(replacement) = replacement else { continue };
        let idx = first + offset;
        let start = ctx.lines[idx].byte_offset;
        let end = start + body[offset].len();
        let eol = if ctx.content[end..].starts_with("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        fixes.insert(idx, Fix::new(start..end, replacement.replace('\n', eol)));
    }
    fixes
}

/// Keep only the edits that leave the parsed front matter unchanged. Nothing
/// is kept when the front matter does not parse to begin with.
fn keep_verified<T: PartialEq>(
    body: &[&str],
    mut edits: Vec<Option<String>>,
    parse: impl Fn(&str) -> Option<T>,
) -> Vec<Option<String>> {
    if edits.iter().all(Option::is_none) {
        return edits;
    }
    let Some(original) = parse(&body.join("\n")) else {
        return vec![None; body.len()];
    };
    let apply = |edits: &[Option<String>]| {
        body.iter()
            .zip(edits)
            .map(|(line, edit)| edit.as_deref().unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let preserves = |edits: &[Option<String>]| parse(&apply(edits)).is_some_and(|parsed| parsed == original);

    if preserves(&edits) {
        return edits;
    }
    // Some edit changed the value: keep the ones that are safe on their own.
    for idx in 0..edits.len() {
        if edits[idx].is_none() {
            continue;
        }
        let mut alone = vec![None; edits.len()];
        alone[idx].clone_from(&edits[idx]);
        if !preserves(&alone) {
            edits[idx] = None;
        }
    }
    if preserves(&edits) {
        edits
    } else {
        vec![None; body.len()]
    }
}

/// How the pieces of a wrapped value are laid out on its lines.
struct Layout<'a> {
    first: String,
    cont: String,
    /// Appended to every line but the last.
    nonlast: &'a str,
    /// Appended to the last line.
    last: &'a str,
}

/// Byte positions of the single spaces a value may be broken at: a lone space
/// between two non-space characters, outside an escape sequence when
/// `escapes` is set.
fn break_points(text: &str, escapes: bool) -> Vec<usize> {
    let mut points = Vec::new();
    let mut prev: Option<char> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if escapes && c == '\\' {
            prev = chars.next().map(|(_, escaped)| escaped).or(Some(c));
            continue;
        }
        if c == ' ' && prev.is_some_and(|p| p != ' ') && chars.peek().is_some_and(|&(_, next)| next != ' ') {
            points.push(i);
        }
        prev = Some(c);
    }
    points
}

fn split_at_points<'a>(text: &'a str, points: &[usize]) -> Vec<&'a str> {
    let mut words = Vec::with_capacity(points.len() + 1);
    let mut start = 0;
    for &p in points {
        words.push(&text[start..p]);
        start = p + 1;
    }
    words.push(&text[start..]);
    words
}

/// Greedily fill lines with `words`, joined by single spaces. `None` when the
/// result is not strictly narrower than `original`, so a value that cannot be
/// shortened is left to the warning alone.
fn fill(words: &[&str], layout: &Layout, limit: usize, measure: Measure, original: usize) -> Option<Vec<String>> {
    let mut lines = Vec::new();
    let mut current = layout.first.clone();
    let mut has_word = false;
    for (i, word) in words.iter().enumerate() {
        let tail = if i + 1 == words.len() {
            layout.last
        } else {
            layout.nonlast
        };
        if has_word && measure(&format!("{current} {word}{tail}")) > limit {
            lines.push(format!("{current}{}", layout.nonlast));
            current = format!("{}{word}", layout.cont);
        } else if has_word {
            current = format!("{current} {word}");
        } else {
            current = format!("{current}{word}");
        }
        has_word = true;
    }
    lines.push(format!("{current}{}", layout.last));
    let narrower = lines.len() > 1 && lines.iter().all(|line| measure(line) < original);
    narrower.then_some(lines)
}

// ---------------------------------------------------------------- YAML

/// Byte position of the closing quote of the scalar whose content starts at
/// `start`, honoring `\x` escapes in double quotes and `''` in single quotes.
fn quoted_content_end(line: &str, start: usize, quote: char) -> Option<usize> {
    let mut chars = line[start..].char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if quote == '"' && c == '\\' {
            chars.next();
        } else if c == quote {
            if quote == '\'' && chars.peek().is_some_and(|&(_, next)| next == '\'') {
                chars.next();
            } else {
                return Some(start + i);
            }
        }
    }
    None
}

/// Wrap the scalar on one YAML line. Whether the line really holds a wrappable
/// scalar (and not, say, a line of a block scalar) is decided later by
/// parsing the result.
fn yaml_wrap_line(line: &str, limit: usize, measure: Measure) -> Option<String> {
    let original = measure(line);
    if original <= limit {
        return None;
    }
    let (start, helper_end) = frontmatter_values::value_span(line)?;
    let quote = line[..start].chars().next_back().filter(|c| matches!(c, '"' | '\''));
    // The shared helper ends a quoted value at the first quote, `\"` and `''` included.
    let end = quote
        .and_then(|quote| quoted_content_end(line, start, quote))
        .unwrap_or(helper_end);
    let value = &line[start..end];

    // Column where the content starts once indentation and list dashes are gone.
    let mut content_col = line.len() - line.trim_start_matches(' ').len();
    while line[content_col..].starts_with("- ") {
        content_col += 2;
        content_col += line[content_col..].len() - line[content_col..].trim_start_matches(' ').len();
    }
    // A bare value (`- text`, or a continuation line) lines up with itself;
    // a `key: value` continues deeper than its key.
    let value_col = start - usize::from(quote.is_some());
    let cont_col = if value_col <= content_col {
        content_col
    } else {
        content_col + 2
    };

    let mut points = break_points(value, quote == Some('"'));
    if quote.is_none() {
        // A continuation line must not read as a comment or an indicator.
        points.retain(|&p| {
            let next = &value[p + 1..];
            !(next.starts_with('#') || next == "-" || next == "?" || next.starts_with("- "))
        });
    }
    let words = split_at_points(value, &points);
    let layout = Layout {
        first: line[..start].to_string(),
        cont: " ".repeat(cont_col),
        nonlast: "",
        last: &line[end..],
    };
    fill(&words, &layout, limit, measure, original).map(|lines| lines.join("\n"))
}

// ---------------------------------------------------------------- TOML

/// Wrap the long strings and arrays of a TOML front matter body. Values are
/// located by `toml_edit`'s spans, so only a value that sits whole on one line
/// as a table entry is touched.
fn toml_wrap_lines(body: &[&str], limit: usize, measure: Measure) -> Vec<Option<String>> {
    let mut out = vec![None; body.len()];
    let text = body.join("\n");
    let Ok(doc) = toml_edit::Document::parse(text.as_str()) else {
        return out;
    };
    let ctx = TomlCtx {
        text: &text,
        body,
        limit,
        measure,
    };
    toml_visit_table(doc.as_table(), &ctx, &mut out);
    out
}

struct TomlCtx<'a> {
    text: &'a str,
    body: &'a [&'a str],
    limit: usize,
    measure: Measure<'a>,
}

fn toml_visit_table(table: &Table, ctx: &TomlCtx, out: &mut [Option<String>]) {
    for (_, item) in table {
        match item {
            Item::Table(inner) => toml_visit_table(inner, ctx, out),
            Item::ArrayOfTables(tables) => tables.iter().for_each(|inner| toml_visit_table(inner, ctx, out)),
            Item::Value(value) => toml_wrap_value(value, ctx, out),
            Item::None => {}
        }
    }
}

fn toml_wrap_value(value: &Value, ctx: &TomlCtx, out: &mut [Option<String>]) {
    let Some(span) = value.span() else { return };
    let raw = &ctx.text[span.clone()];
    if raw.contains('\n') {
        return;
    }
    let line_idx = ctx.text[..span.start].matches('\n').count();
    let line = ctx.body[line_idx].trim_end();
    let line_start = ctx.text[..span.start].rfind('\n').map_or(0, |p| p + 1);
    let (start, end) = (span.start - line_start, span.end - line_start);
    if out[line_idx].is_some() || end > line.len() {
        return;
    }
    let original = (ctx.measure)(line);
    if original <= ctx.limit {
        return;
    }
    let (head, tail) = (&line[..start], &line[end..]);
    let indent = " ".repeat(line.len() - line.trim_start().len());

    out[line_idx] = match value {
        Value::String(_) if raw.starts_with('"') && !raw.starts_with("\"\"\"") => {
            wrap_toml_string(head, &raw[1..raw.len() - 1], tail, &indent, ctx, original)
        }
        Value::Array(array) => {
            let elements: Option<Vec<&str>> = array
                .iter()
                .map(|element| element.span().map(|s| &ctx.text[s]))
                .collect();
            elements.and_then(|elements| wrap_toml_array(head, &elements, tail, &indent, ctx, original))
        }
        _ => None,
    };
}

/// Rewrite `key = "text"` as a multi-line basic string whose lines end in `\`.
/// `content` is the raw text between the quotes, escapes included.
fn wrap_toml_string(
    head: &str,
    content: &str,
    tail: &str,
    indent: &str,
    ctx: &TomlCtx,
    original: usize,
) -> Option<String> {
    let words = split_at_points(content, &break_points(content, true));
    let closer = format!("\"\"\"{tail}");
    let layout = Layout {
        first: format!("{head}\"\"\""),
        cont: format!("{indent}  "),
        nonlast: " \\",
        last: &closer,
    };
    fill(&words, &layout, ctx.limit, ctx.measure, original).map(|lines| lines.join("\n"))
}

/// Rewrite a one-line array as `key = [`, packed rows of elements, and `]`.
fn wrap_toml_array(
    head: &str,
    elements: &[&str],
    tail: &str,
    indent: &str,
    ctx: &TomlCtx,
    original: usize,
) -> Option<String> {
    if elements.is_empty() {
        return None;
    }
    let with_commas: Vec<String> = elements.iter().map(|e| format!("{e},")).collect();
    let words: Vec<&str> = with_commas.iter().map(String::as_str).collect();
    let row_indent = format!("{indent}  ");
    let layout = Layout {
        first: row_indent.clone(),
        cont: row_indent,
        nonlast: "",
        last: "",
    };
    // `fill` wants more than one row, but a single row is a fine result here:
    // the opener and closer are on lines of their own.
    let rows = fill(&words, &layout, ctx.limit, ctx.measure, original).or_else(|| {
        let single = format!("{}{}", layout.first, words.join(" "));
        ((ctx.measure)(&single) < original).then_some(vec![single])
    })?;
    let mut lines = vec![format!("{head}[")];
    lines.extend(rows);
    lines.push(format!("{indent}]{tail}"));
    Some(lines.join("\n"))
}
