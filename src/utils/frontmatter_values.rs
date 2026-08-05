//! Reading values out of a document's frontmatter.
//!
//! Frontmatter is YAML, TOML or JSON rather than Markdown, so the rules that
//! look at it need answers Markdown parsing cannot give: where the checkable
//! value on a line starts and ends, which top-level key owns a line, and
//! whether a value reads as a link destination. This module is the single
//! place those answers are computed, so every rule reading frontmatter agrees
//! about what it contains.
//!
//! Parsing is deliberately line-based and heuristic rather than a full YAML or
//! TOML parse: rules need byte spans inside the original line to report and fix
//! at, and a real parser hands back reconstructed values with no position.

use crate::discovery::MARKDOWN_EXTENSIONS;
use crate::lint_context::LintContext;
use crate::rules::front_matter_utils::FrontMatterUtils;
use std::collections::HashSet;
use std::ops::Range;

/// Delimiters that wrap a token from the outside (quotes, brackets, parens,
/// angle brackets) rather than appearing inside a path. Used only by the
/// edge-trimming pass: these characters legitimately occur inside real paths
/// (Next.js route groups `(marketing)/`, dynamic segments `[slug]`,
/// disambiguated filenames `myapp(1).md`), so they must not act as mid-token
/// boundaries, only as leading/trailing punctuation to peel off prose wrapping
/// such as `See (docs/a.md) here.`.
pub const PATH_TOKEN_WRAPPERS: &[char] = &['\'', '"', '`', '(', ')', '[', ']', '<', '>'];

/// Whether the frontmatter value starting at `value_start` is a quoted scalar:
/// the character immediately preceding `value_start` is a quote. Call this with
/// the span start returned by `value_span`, which always lands just past the
/// opening quote for quoted values. The raw `value_offset` does not carry that
/// guarantee: its helper `kv_value_offset` only skips the opening quote when the
/// whole trimmed remainder of the line starts and ends with the same quote
/// character, so a trailing comment or an unterminated quote leaves the offset
/// pointing AT the quote instead of past it.
pub fn value_is_quoted(line: &str, value_start: usize) -> bool {
    matches!(line[..value_start].chars().next_back(), Some('\'') | Some('"'))
}

/// Byte span of the semantic value on a frontmatter line: the checkable content
/// with a trailing comment excluded. For a quoted scalar the span ends at the
/// closing quote (or the trimmed end of line if the quote is unterminated), so
/// `#` and spaces inside it are literal, and the quote characters themselves
/// are never part of the span. `None` when the line carries no checkable value,
/// including an empty quoted value (`''`).
pub fn value_span(line: &str) -> Option<(usize, usize)> {
    let start = value_offset(line);
    if start == usize::MAX || start >= line.len() {
        return None;
    }

    // `value_offset` sometimes points past the opening quote already, and
    // sometimes points AT it (see `value_is_quoted` docs). Detect the quote from
    // either position so both cases converge on a `content_start` that is always
    // just past the opening quote.
    let before = line[..start].chars().next_back();
    let at = line[start..].chars().next();
    let (content_start, quote) = match (before, at) {
        (Some(q @ ('\'' | '"')), _) => (start, Some(q)),
        (_, Some(q @ ('\'' | '"'))) => (start + q.len_utf8(), Some(q)),
        _ => (start, None),
    };

    let end = if let Some(quote) = quote {
        let rest = &line[content_start..];
        match rest.find(quote) {
            Some(i) => content_start + i,
            None => content_start + rest.trim_end().len(),
        }
    } else {
        let rest = &line[content_start..];
        let raw_end = match rest.find(" #") {
            Some(i) => content_start + i,
            None => line.len(),
        };
        line[..raw_end].trim_end().len()
    };

    if end <= content_start {
        None
    } else {
        Some((content_start, end))
    }
}

/// For a frontmatter line, the byte offset where the checkable value portion
/// starts. Returns `usize::MAX` if the entire line should be skipped
/// (frontmatter delimiters, key-only lines, YAML comments, flow constructs).
pub fn value_offset(line: &str) -> usize {
    let trimmed = line.trim();

    // Skip frontmatter delimiters and empty lines
    if trimmed == "---" || trimmed == "+++" || trimmed.is_empty() {
        return usize::MAX;
    }

    // Skip YAML comments
    if trimmed.starts_with('#') {
        return usize::MAX;
    }

    // YAML list item: "  - item" or "  - key: value"
    let stripped = line.trim_start();
    if let Some(after_dash) = stripped.strip_prefix("- ") {
        let leading = line.len() - stripped.len();
        // Check if the list item contains a mapping (e.g., "- key: value")
        if let Some(result) = kv_value_offset(line, after_dash, leading + 2) {
            return result;
        }
        // Bare list item value (no colon) - check content after "- "
        return leading + 2;
    }
    if stripped == "-" {
        return usize::MAX;
    }

    // Key-value pair with colon separator (YAML): "key: value"
    if let Some(result) = kv_value_offset(line, stripped, line.len() - stripped.len()) {
        return result;
    }

    // Key-value pair with equals separator (TOML): "key = value"
    if let Some(eq_pos) = line.find('=') {
        let after_eq = eq_pos + 1;
        if after_eq < line.len() && line.as_bytes()[after_eq] == b' ' {
            let value_start = after_eq + 1;
            let value_slice = &line[value_start..];
            let value_trimmed = value_slice.trim();
            if value_trimmed.is_empty() {
                return usize::MAX;
            }
            // For quoted values, skip the opening quote character
            if (value_trimmed.starts_with('"') && value_trimmed.ends_with('"'))
                || (value_trimmed.starts_with('\'') && value_trimmed.ends_with('\''))
            {
                let quote_offset = value_slice.find(['"', '\'']).unwrap_or(0);
                return value_start + quote_offset + 1;
            }
            return value_start;
        }
        // Equals with no space after or at end of line -> no value to check
        return usize::MAX;
    }

    // No separator found - continuation line or bare value, check the whole line
    0
}

/// Parse a key-value pair using colon separator within `content` that starts at
/// `base_offset` in the original line. Returns `Some(offset)` if a colon
/// separator is found, `None` if no colon is present.
fn kv_value_offset(line: &str, content: &str, base_offset: usize) -> Option<usize> {
    let colon_pos = content.find(':')?;
    let abs_colon = base_offset + colon_pos;
    let after_colon = abs_colon + 1;
    if after_colon < line.len() && line.as_bytes()[after_colon] == b' ' {
        let value_start = after_colon + 1;
        let value_slice = &line[value_start..];
        let value_trimmed = value_slice.trim();
        if value_trimmed.is_empty() {
            return Some(usize::MAX);
        }
        // Skip flow mappings and flow sequences - too complex for heuristic parsing
        if value_trimmed.starts_with('{') || value_trimmed.starts_with('[') {
            return Some(usize::MAX);
        }
        // For quoted values, skip the opening quote character
        if (value_trimmed.starts_with('"') && value_trimmed.ends_with('"'))
            || (value_trimmed.starts_with('\'') && value_trimmed.ends_with('\''))
        {
            let quote_offset = value_slice.find(['"', '\'']).unwrap_or(0);
            return Some(value_start + quote_offset + 1);
        }
        return Some(value_start);
    }
    // Colon with no space after or at end of line -> no value to check
    Some(usize::MAX)
}

/// Bounds of the whitespace-delimited token containing `pos`, clamped to
/// `[value_start, value_end)`. The clamp is what keeps this search inside a
/// single frontmatter value: it can never walk past the value's own boundaries,
/// so it can never wander into Markdown link syntax on the same line
/// (frontmatter has none) or onto a neighboring line.
pub fn token_bounds(line: &str, pos: usize, value_start: usize, value_end: usize) -> (usize, usize) {
    let before = &line[value_start..pos];
    let start = before.rfind(char::is_whitespace).map_or(value_start, |i| {
        value_start + i + before[i..].chars().next().unwrap().len_utf8()
    });

    let after = &line[pos..value_end];
    let end = after.find(char::is_whitespace).map_or(value_end, |i| pos + i);

    (start, end)
}

/// Strip wrapping delimiters, then trailing sentence punctuation, repeating both
/// passes until a full pass leaves the bounds unchanged. Punctuation removal can
/// expose a wrapper underneath it (`"docs/myapp.md",` sheds the comma to reveal
/// a trailing quote), so a single sequential pass is not enough to reach a
/// stable result.
pub fn trim_token_bounds(line: &str, mut start: usize, mut end: usize) -> (usize, usize) {
    const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?'];
    while start < end && line[start..end].starts_with(PATH_TOKEN_WRAPPERS) {
        start += line[start..].chars().next().unwrap().len_utf8();
    }
    loop {
        let before = (start, end);
        while end > start && line[start..end].ends_with(PATH_TOKEN_WRAPPERS) {
            end -= line[..end].chars().next_back().unwrap().len_utf8();
        }
        while end > start && line[start..end].ends_with(TRAILING) {
            end -= line[..end].chars().next_back().unwrap().len_utf8();
        }
        if (start, end) == before {
            break;
        }
    }
    (start, end)
}

/// Byte offset of the first occurrence of `target` in `s` that is outside a
/// single- or double-quoted span, skipping escaped characters inside double
/// quotes. `None` if `target` never occurs unquoted.
fn find_unquoted(s: &str, target: char) -> Option<usize> {
    let mut in_double = false;
    let mut in_single = false;
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        if in_double {
            if c == '\\' {
                chars.next();
            } else if c == '"' {
                in_double = false;
            }
        } else if in_single {
            if c == '\'' {
                in_single = false;
            }
        } else if c == target {
            return Some(i);
        } else if c == '"' {
            in_double = true;
        } else if c == '\'' {
            in_single = true;
        }
    }
    None
}

/// Inner key path of a real TOML table header, `[seo]` or `[[authors]]`.
///
/// A header, after stripping an optional trailing `#comment` that is outside
/// quotes and trimming, must START with `[` and END with the matching `]` or
/// `]]` and nothing else, and its inner key path must contain no unquoted
/// comma: a comma never appears in a real header key (a bare or dotted path
/// like `params.seo`), only in an array literal like `1, 2`. This rejects a
/// column-0 array element such as `[1, 2],`: TOML does not require array
/// elements to be indented, so without this check the line is misread as a
/// header named `1, 2`.
///
/// A quoted key that itself contains a comma (`["a,b"]`) is valid TOML and is
/// still recognized here, since the comma is inside the quotes and
/// `find_unquoted` skips it.
fn toml_table_header(trimmed: &str) -> Option<&str> {
    let head = match find_unquoted(trimmed, '#') {
        Some(i) => trimmed[..i].trim_end(),
        None => trimmed,
    };

    let inner = if let Some(rest) = head.strip_prefix("[[") {
        rest.strip_suffix("]]")?
    } else {
        head.strip_prefix('[')?.strip_suffix(']')?
    };

    if find_unquoted(inner, ',').is_some() {
        return None;
    }

    let inner = inner.trim();
    if inner.is_empty() { None } else { Some(inner) }
}

/// Signed count of `[` minus `]` on a TOML line, ignoring bracket characters
/// inside quoted strings. Used to track how deep the parser is inside an
/// unclosed `key = [ ... ]` array so a nested element like `[1, 2],` is never
/// misread as a table header.
fn toml_bracket_delta(trimmed: &str) -> i32 {
    let mut delta = 0i32;
    let mut chars = trimmed.chars();
    let mut in_double = false;
    let mut in_single = false;
    while let Some(c) = chars.next() {
        if in_double {
            if c == '\\' {
                chars.next();
            } else if c == '"' {
                in_double = false;
            }
        } else if in_single {
            if c == '\'' {
                in_single = false;
            }
        } else {
            match c {
                '"' => in_double = true,
                '\'' => in_single = true,
                '[' => delta += 1,
                ']' => delta -= 1,
                _ => {}
            }
        }
    }
    delta
}

fn strip_key_quotes(raw: &str) -> &str {
    raw.strip_prefix('"')
        .and_then(|k| k.strip_suffix('"'))
        .or_else(|| raw.strip_prefix('\'').and_then(|k| k.strip_suffix('\'')))
        .unwrap_or(raw)
}

/// The lowercased top-level frontmatter key owning each line, indexed by line
/// number. `None` where no owner is determinable, which leaves the line
/// checked.
///
/// Attribution is a heuristic. It is deliberately biased so an uncertain line
/// falls back to being checked: an indent-0 YAML key line always starts a new
/// key, so a bracket inside a block scalar can never cause a later real key to
/// be suppressed. The cost is that an indent-0 flow continuation is attributed
/// to its own text rather than to its parent.
pub fn field_map(ctx: &LintContext) -> Vec<Option<String>> {
    let mut map = vec![None; ctx.lines.len()];
    let mut current: Option<String> = None;
    let mut toml = false;
    let mut in_toml_table = false;
    // Depth of unclosed `[` inside the current TOML `key = [ ... ]` array,
    // across lines. TOML only: see the comment in the YAML branch below for why
    // this tracking does not extend there.
    let mut toml_array_depth: i32 = 0;

    for (idx, info) in ctx.lines.iter().enumerate() {
        if !info.in_front_matter {
            continue;
        }
        let line = info.content(ctx.content);
        let trimmed = line.trim();

        if trimmed == "---" || trimmed == "+++" {
            toml = trimmed == "+++";
            current = None;
            in_toml_table = false;
            toml_array_depth = 0;
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            map[idx].clone_from(&current);
            continue;
        }

        if toml {
            // A table header can only ever be found while `toml_array_depth` is
            // zero: valid TOML never lets a `[table]`/`[[array-of-tables]]`
            // header appear inside an unclosed array value, so a bracket-only
            // line seen while depth is above zero, such as a column-0 array
            // element `[1, 2]` or `[2]`, is always a continuation, never a
            // header, regardless of whether it happens to satisfy
            // `toml_table_header`'s shape check on its own.
            //
            // An indent-0 assignment, by contrast, always resyncs `current` and
            // clears the stuck depth, even while `toml_array_depth` is stuck
            // above zero from an unclosed array (a forgotten closing bracket).
            // Without this, a malformed array would misattribute every following
            // key to the array's key for the rest of the frontmatter.
            let indent = line.len() - line.trim_start().len();
            let header = if indent == 0 && toml_array_depth == 0 {
                toml_table_header(trimmed)
            } else {
                None
            };
            let assignment_eq = if indent == 0 {
                FrontMatterUtils::separator_pos_outside_quoted_key(trimmed, '=')
            } else {
                None
            };
            let resync = header.is_some() || assignment_eq.is_some();

            if resync {
                if let Some(name) = header {
                    current = Some(FrontMatterUtils::toml_root_key(name).to_lowercase());
                    in_toml_table = true;
                } else if !in_toml_table && let Some(eq) = assignment_eq {
                    let root = FrontMatterUtils::toml_root_key(trimmed[..eq].trim());
                    current = Some(root.to_lowercase());
                }
                // Inside a table, assignments belong to the table. Array
                // continuations match neither branch and inherit.
                toml_array_depth = 0;
            }
            toml_array_depth = (toml_array_depth + toml_bracket_delta(trimmed)).max(0);
        } else {
            // YAML deliberately does not track bracket/flow depth the way the
            // TOML branch does. YAML has block scalars (`description: |`) whose
            // content is arbitrary text that could contain an unmatched `[`, and
            // a running depth counter would misread that as an open array and
            // wrongly swallow a later, real top-level key. TOML has no block
            // scalars, so depth tracking is safe there. YAML instead stays with
            // the simpler, safer rule: every indent-0 key line always starts a
            // new key.
            let indent = line.len() - line.trim_start().len();
            if indent == 0 {
                if trimmed.starts_with("- ") || trimmed == "-" {
                    current = None;
                } else if let Some(colon) = FrontMatterUtils::separator_pos_outside_quoted_key(trimmed, ':') {
                    let raw = trimmed[..colon].trim();
                    current = Some(strip_key_quotes(raw).to_lowercase());
                }
            }
            // Indented lines inherit, which covers nested maps, sequence items
            // and block-scalar continuations.
        }
        map[idx].clone_from(&current);
    }
    map
}

/// A frontmatter value that reads as a link destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatterLink {
    /// 1-indexed line the destination sits on.
    pub line: usize,
    /// Byte range of the destination within its own line, with quotes and other
    /// wrapping punctuation excluded.
    pub range: Range<usize>,
    /// The lowercased top-level key owning the value, or `None` where no owner
    /// is determinable. See [`field_map`] for how attribution is biased.
    pub field: Option<String>,
}

impl FrontMatterLink {
    /// Whether this link's owning field is in a set of lowercased field names.
    ///
    /// A link with no determinable owner belongs to no field, so it is never
    /// excluded by name.
    pub fn field_is_in(&self, fields: &HashSet<String>) -> bool {
        self.field.as_ref().is_some_and(|field| fields.contains(field))
    }
}

/// Every frontmatter value that reads as a link destination, in document order,
/// each attributed to the top-level key that owns it.
///
/// No configuration is applied: callers decide which links their own settings
/// exclude, which is what lets one caller index every link while another
/// reports only some of them.
pub fn link_destinations(ctx: &LintContext) -> Vec<FrontMatterLink> {
    let mut links = Vec::new();
    if ctx.front_matter_end_line() == 0 {
        return links;
    }

    for (idx, info) in ctx.lines.iter().enumerate() {
        if !info.in_front_matter {
            continue;
        }

        let line = info.content(ctx.content);
        let Some((value_start, value_end)) = value_span(line) else {
            continue;
        };
        let (start, end) = trim_token_bounds(line, value_start, value_end);
        if start >= end || !is_link_destination(&line[start..end]) {
            continue;
        }
        links.push(FrontMatterLink {
            line: idx + 1,
            range: start..end,
            field: None,
        });
    }

    // Attribution walks the document, so it is worth doing only once there is
    // something to attribute. Frontmatter holding no link destination at all is
    // the overwhelmingly common case.
    if !links.is_empty() {
        let fields = field_map(ctx);
        for link in &mut links {
            link.field = fields.get(link.line - 1).cloned().flatten();
        }
    }

    links
}

/// Whether a frontmatter value reads as a link destination rather than prose.
///
/// Frontmatter has no syntax marking a value as a link, so the answer comes
/// from the shape of the value alone. It is deliberately strict, because a rule
/// acting on a `true` here reports a finding: a value that is merely
/// path-shaped, such as a tag pair `ci/cd`, a date `2026/07/31` or a version
/// `1.2.3`, must not qualify.
///
/// A value qualifies when it holds no whitespace and one of:
///
/// - it starts with `#`, so it is a fragment;
/// - it names a markdown file, so a bare `myapp.md` qualifies while a dotted
///   proper name such as `Node.js` does not;
/// - it holds a `/` and either starts with a path prefix (`./`, `../`, `~/`,
///   `/`) or ends in a file extension.
pub fn is_link_destination(value: &str) -> bool {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return false;
    }

    let path = match value.find('#') {
        Some(0) => return true,
        Some(i) => &value[..i],
        None => value,
    };
    // A query string belongs to the destination, not to the path it names, so
    // `page.md?raw=true` names `page.md`. Body links are resolved the same way.
    let path = path.split('?').next().unwrap_or(path);

    let last_segment = path.rsplit('/').next().unwrap_or(path);
    if has_markdown_extension(last_segment) {
        return true;
    }

    path.contains('/')
        && (path.starts_with('/')
            || path.starts_with("./")
            || path.starts_with("../")
            || path.starts_with("~/")
            || has_file_extension(last_segment))
}

/// Whether `segment` ends in one of the extensions rumdl treats as markdown.
fn has_markdown_extension(segment: &str) -> bool {
    segment.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && MARKDOWN_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known))
    })
}

/// Whether `segment` ends in something that reads as a file extension: a short
/// run of alphanumerics carrying at least one letter. The letter requirement is
/// what keeps a version number such as `1.2.3` from reading as a file.
fn has_file_extension(segment: &str) -> bool {
    segment.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty()
            && (1..=8).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    fn destinations(content: &str) -> Vec<String> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        link_destinations(&ctx)
            .into_iter()
            .map(|link| {
                let line = ctx.lines[link.line - 1].content(ctx.content);
                line[link.range].to_string()
            })
            .collect()
    }

    #[test]
    fn a_relative_path_reads_as_a_destination() {
        assert!(is_link_destination("this/is/a/link/to/myapp.md"));
        assert!(is_link_destination("./other.md"));
        assert!(is_link_destination("../parent/other"));
        assert!(is_link_destination("~/notes/other.md"));
        assert!(is_link_destination("/absolute/other.md"));
        assert!(is_link_destination("assets/logo.png"));
    }

    #[test]
    fn a_bare_markdown_filename_reads_as_a_destination() {
        assert!(is_link_destination("myapp.md"));
        assert!(is_link_destination("report.QMD"));
    }

    #[test]
    fn a_fragment_reads_as_a_destination() {
        assert!(is_link_destination("#installation"));
        assert!(is_link_destination("other.md#installation"));
        assert!(is_link_destination("docs/other.md#installation"));
    }

    #[test]
    fn a_query_string_is_not_part_of_the_path() {
        assert!(is_link_destination("docs/other.md?raw=true"));
        assert!(is_link_destination("other.md?raw=true"));
        assert!(is_link_destination("docs/other.md?raw=true#installation"));
        // The query is what makes this path-shaped, so it stays prose.
        assert!(!is_link_destination("what?about/this"));
    }

    #[test]
    fn prose_and_path_shaped_values_do_not() {
        // A dotted proper name is not a file: only markdown extensions lift the
        // slash requirement.
        assert!(!is_link_destination("Node.js"));
        // Tag pairs, dates and versions are all path-shaped and none is a file.
        assert!(!is_link_destination("ci/cd"));
        assert!(!is_link_destination("2026/07/31"));
        assert!(!is_link_destination("1.2.3"));
        // An extensionless path with no prefix stays prose.
        assert!(!is_link_destination("docs/guides/intro"));
        // A destination never holds whitespace.
        assert!(!is_link_destination("a description of docs/a.md"));
        assert!(!is_link_destination(""));
    }

    #[test]
    fn a_destination_is_read_out_of_its_quotes() {
        assert_eq!(
            destinations("---\nlink: 'this/is/a/link/to/myapp.md'\n---\n\n# Title\n"),
            vec!["this/is/a/link/to/myapp.md"]
        );
        assert_eq!(
            destinations("---\nlink: \"docs/a.md\"\n---\n\n# Title\n"),
            vec!["docs/a.md"]
        );
    }

    #[test]
    fn a_trailing_comment_is_not_part_of_a_destination() {
        assert_eq!(
            destinations("---\nlink: docs/a.md # the guide\n---\n\n# Title\n"),
            vec!["docs/a.md"]
        );
    }

    #[test]
    fn only_frontmatter_is_read() {
        assert_eq!(
            destinations("---\nlink: docs/a.md\n---\n\nSee docs/b.md for more.\n"),
            vec!["docs/a.md"]
        );
    }

    #[test]
    fn a_sequence_item_carries_a_destination() {
        assert_eq!(
            destinations("---\nlinks:\n  - docs/a.md\n  - docs/b.md\n---\n\n# Title\n"),
            vec!["docs/a.md", "docs/b.md"]
        );
    }

    #[test]
    fn a_toml_value_carries_a_destination() {
        assert_eq!(
            destinations("+++\nlink = \"docs/a.md\"\n+++\n\n# Title\n"),
            vec!["docs/a.md"]
        );
    }

    #[test]
    fn a_destination_carries_the_field_owning_it_through_a_whole_subtree() {
        let content = "---\nlink: docs/a.md\nseo:\n  canonical: docs/b.md\n---\n\n# Title\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let links = link_destinations(&ctx);

        let owners: Vec<Option<&str>> = links.iter().map(|link| link.field.as_deref()).collect();
        assert_eq!(owners, vec![Some("link"), Some("seo")]);

        // The nested value is attributed to the top-level key, so excluding
        // `seo` by name hides its whole subtree.
        let ignored: HashSet<String> = ["seo".to_string()].into_iter().collect();
        let kept: Vec<String> = links
            .iter()
            .filter(|link| !link.field_is_in(&ignored))
            .map(|link| ctx.lines[link.line - 1].content(ctx.content)[link.range.clone()].to_string())
            .collect();
        assert_eq!(kept, vec!["docs/a.md"]);
    }

    #[test]
    fn a_destination_with_no_determinable_owner_belongs_to_no_field() {
        // A top-level sequence item has no owning key, so no field name can
        // exclude it. The distinction matters to callers that must still treat
        // it as frontmatter.
        let ctx = LintContext::new("---\n- docs/a.md\n---\n\n# Title\n", MarkdownFlavor::Standard, None);
        let links = link_destinations(&ctx);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].field, None);
        assert!(!links[0].field_is_in(&["docs".to_string()].into_iter().collect()));
    }

    #[test]
    fn a_document_without_frontmatter_has_no_destinations() {
        assert!(destinations("# Title\n\nSee docs/a.md.\n").is_empty());
    }
}
