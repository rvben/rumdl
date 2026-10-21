use crate::lint_context::{column_at, is_horizontal_rule_content};
/// Rule MD022: Headings should be surrounded by blank lines
///
/// See [docs/md022.md](../../docs/md022.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::blank_lines::is_blank_or_comment_only;
use crate::utils::mdg;
use crate::utils::mkdocs_attr_list::is_block_attribute_line;
use crate::utils::pandoc;
use crate::utils::range_utils::calculate_heading_range;
use toml;

pub(crate) mod md022_config;
use md022_config::{HeadingBlankMode, MD022Config};

/// Whether a trimmed line begins with a list marker: `-`, `*`, or `+`, or one or more
/// digits followed by `.` or `)`, in each case followed by a space or the end of the line.
///
/// This is a parse-stable substitute for the per-line `list_item` flag when deciding whether
/// a heading is followed by a list. The flag depends on surrounding block context and can flip
/// when a blank line is inserted above the heading, which would make the blank-below fix
/// non-idempotent; the syntactic shape of the following line does not change.
///
/// A spaced thematic break (`* * *`, `- - -`, `- --`) opens with a marker and a space, so it has
/// to be excluded explicitly: the parser reads it as a thematic break rather than a list, and a
/// heading above it needs the same blank line that `***` and `_ _ _` already get.
/// `is_horizontal_rule_content` is what the per-line `is_horizontal_rule` flag is computed from,
/// so the exclusion matches how the rest of rumdl recognizes a break, and it reads the line text
/// alone, which keeps it as parse-stable as the rest of this test.
fn starts_with_list_marker(trimmed: &str) -> bool {
    if is_horizontal_rule_content(trimmed) {
        return false;
    }
    let bytes = trimmed.as_bytes();
    match bytes.first() {
        Some(b'-' | b'*' | b'+') => matches!(bytes.get(1), None | Some(b' ')),
        Some(b'0'..=b'9') => {
            let mut i = 0;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            matches!(bytes.get(i), Some(b'.' | b')')) && matches!(bytes.get(i + 1), None | Some(b' '))
        }
        _ => false,
    }
}

/// Whether the heading on `heading_idx` is a Gherkin structure annotated by the
/// tag line directly above it.
///
/// The blank line this rule would add above the heading lands between the tag
/// line and the structure it annotates. When the tag line opens the document,
/// Gherkin reads that blank as the Feature line and the Feature collapses into
/// an unnamed node. Lower down the blank leaves the parse intact, and the
/// suppression keeps the tags written against the structure they belong to.
///
/// Gherkin tags attach to structures, and MDG spells every structure as a
/// `Keyword: name` heading, so a heading that names none is ordinary prose that
/// keeps the normal blank-line requirement.
fn follows_mdg_tag_line(
    ctx: &crate::lint_context::LintContext,
    heading_idx: usize,
    heading: &crate::lint_context::HeadingInfo,
) -> bool {
    heading_idx > 0
        && mdg::keyword_split(&heading.text).is_some()
        && mdg::is_tag_line(ctx.lines[heading_idx - 1].content(ctx.content))
}

/// Index of the line a heading's text starts on. A heading is recorded on the
/// last line of its text, which for a setext heading is the last line of the
/// paragraph its underline ends.
fn first_text_idx(heading_idx: usize, heading: &crate::lint_context::HeadingInfo) -> usize {
    heading_idx + 1 - heading.text_lines
}

/// The text of line `idx` as the heading it neighbours reads it. A quoted
/// heading's neighbours are read without their quote markers, so a fence or a
/// list item inside the quote is recognised as one.
fn neighbour_text<'a>(ctx: &'a crate::lint_context::LintContext, idx: usize, quoted: bool) -> &'a str {
    let line = ctx.lines[idx].content(ctx.content);
    if quoted { strip_quote_markers(line) } else { line }
}

/// `line` without its leading quote markers; the line itself when it has none.
fn strip_quote_markers(line: &str) -> &str {
    crate::utils::blockquote::parse_blockquote_prefix(line).map_or(line, |parsed| parsed.content)
}

/// Number of blockquotes a line is written inside of: the `>` markers before
/// its text, with list markers and indentation passed over.
fn quote_depth(line: &str) -> usize {
    scan_container_prefix(line, usize::MAX).1
}

/// The part of a line that precedes its text, as a prefix for a blank line in
/// the same container: indentation and the first `depth` quote markers, with
/// each list marker replaced by the spaces that stand under it. Blank lines
/// written this way stay inside the list items and quotes around the heading.
fn blank_line_prefix(line: &str, depth: usize) -> String {
    if depth == 0 {
        return String::new();
    }
    scan_container_prefix(line, depth).0.trim_end().to_string()
}

/// Walks the container markers opening a line, up to `limit` quote markers.
/// Returns the prefix with list markers blanked and the quote markers found.
fn scan_container_prefix(line: &str, limit: usize) -> (String, usize) {
    let mut prefix = String::new();
    let mut depth = 0;
    let mut rest = line;
    while depth < limit {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        prefix.push_str(&rest[..rest.len() - trimmed.len()]);
        rest = trimmed;
        if let Some(after) = rest.strip_prefix('>') {
            prefix.push('>');
            depth += 1;
            rest = after;
            continue;
        }
        let token = rest.split([' ', '\t']).next().unwrap_or("");
        if is_list_marker_token(token) {
            prefix.push_str(&" ".repeat(token.len()));
            rest = &rest[token.len()..];
            continue;
        }
        break;
    }
    (prefix, depth)
}

/// The prefix of the blank line a fix adds next to the heading on `heading_line`
/// beside `adjacent_line`: the container both lines share, so the blank line
/// splits neither the quote nor the list item the heading is in.
fn blank_line_between(heading_line: &str, adjacent_line: &str) -> String {
    let shared = quote_depth(heading_line).min(quote_depth(adjacent_line));
    blank_line_prefix(heading_line, shared)
}

/// The prefix of the blank line a fix adds above the heading on `heading_line`,
/// `above` being the lines over it, nearest first.
///
/// A line over the heading without a quote marker is still in the heading's
/// quote when it continues the quoted paragraph above it lazily, and a blank
/// line written without markers there would split the quote in two. Where the
/// nearest line with a marker or a blank line is quoted text, the blank line
/// carries the heading's own quote markers; at worst that leaves a blank line
/// inside the quote where none was needed.
fn blank_line_above<'a>(heading_line: &str, mut above: impl Iterator<Item = &'a str>) -> String {
    let depth = quote_depth(heading_line);
    let Some(adjacent) = above.next() else {
        return blank_line_prefix(heading_line, 0);
    };
    let mut shared = depth.min(quote_depth(adjacent));
    let adjacent_depth = quote_depth(adjacent);
    if adjacent_depth < depth && !adjacent.trim().is_empty() {
        // Without any marker the quoted text above may be at any depth; with
        // some, it has to reach the heading's own depth to be the paragraph this
        // line continues.
        let needed = if adjacent_depth == 0 { 1 } else { depth };
        let anchor = std::iter::once(adjacent)
            .chain(above)
            .find(|line| quote_depth(line) >= needed || line.trim_start_matches(['>', ' ', '\t']).is_empty());
        if anchor
            .is_some_and(|line| quote_depth(line) >= needed && !line.trim_start_matches(['>', ' ', '\t']).is_empty())
        {
            shared = depth;
        }
    }
    blank_line_prefix(heading_line, shared)
}

/// Whether `token` is a list marker: a bullet, or one to nine digits and a
/// `.` or `)`.
fn is_list_marker_token(token: &str) -> bool {
    match token {
        "-" | "+" | "*" => true,
        _ => {
            let digits = token.trim_end_matches(['.', ')']);
            token.len() == digits.len() + 1
                && (1..=9).contains(&digits.len())
                && digits.bytes().all(|b| b.is_ascii_digit())
        }
    }
}

/// The column where the content of the `quotes`-th quote on `line` starts: just
/// past its marker and the one optional space or tab after it.
fn quote_margin_end(line: &str, quotes: usize) -> usize {
    if quotes == 0 {
        return 0;
    }
    let (prefix, _) = scan_container_prefix(line, quotes);
    let optional = usize::from(line[prefix.len()..].starts_with([' ', '\t']));
    column_at(line, prefix.len()) + optional
}

/// Whether the heading whose text starts on `first_idx` is the first thing in a
/// list item whose marker stands alone at the end of the line above it.
///
/// The marker is the innermost one on that line: `- -` leaves the second item
/// empty inside the first. The item's content starts on the next line, so the
/// heading already begins it: a blank line inserted between the two is the one
/// blank line an item may open with, which leaves the item empty and the
/// heading outside it. A heading indented less than the item's content column
/// is outside it already, and gets its blank line like any other.
fn starts_item_under_bare_marker(ctx: &crate::lint_context::LintContext, first_idx: usize) -> bool {
    let Some(marker_idx) = first_idx.checked_sub(1) else {
        return false;
    };
    let text = ctx.lines[marker_idx].content(ctx.content).trim_end();
    // Only list markers and quote markers may stand before the last marker.
    if text.is_empty()
        || !text
            .split_whitespace()
            .all(|token| token == ">" || is_list_marker_token(token))
        || !text.split_whitespace().next_back().is_some_and(is_list_marker_token)
    {
        return false;
    }
    let marker_end = column_at(text, text.len());
    let line = ctx.lines[first_idx].content(ctx.content);
    let quotes = quote_depth(line);
    if quotes > 0 {
        // A heading inside a quote has no recorded container: it is in the item
        // when it sits in the marker's quotes, or in quotes opened inside the
        // item, at or past the content column. The text after the marker's own
        // quote markers is what the column of the item is measured on.
        let marker_quotes = quote_depth(text);
        if quotes < marker_quotes {
            return false;
        }
        let (prefix, _) = scan_container_prefix(line, marker_quotes);
        let rest = &line[prefix.len()..];
        let text_start = prefix.len() + rest.len() - rest.trim_start_matches([' ', '\t']).len();
        // Columns are counted from the end of each line's own quote margin, so
        // `> > -` over `>>   ## H` compares the same two columns.
        let item_end = marker_end.saturating_sub(quote_margin_end(text, marker_quotes));
        return column_at(line, text_start).saturating_sub(quote_margin_end(line, marker_quotes)) > item_end;
    }
    ctx.heading_container(first_idx + 1)
        == crate::lint_context::HeadingContainer::Item {
            content_col: marker_end + 1,
        }
}

/// Whether the heading's text starts on the line that opens its list item
/// (`- ## a`, or `- a` over a `---` underline).
///
/// Nothing in the heading's own container sits above it: the line above is
/// either the previous item or the block before the list, and the list marker
/// on the heading's line is what separates them. A blank line there would
/// belong to the list, not to the heading, and would make a tight list loose.
/// Indentation and quote markers are not list markers: a heading that only
/// opens a quote is separated from the line above by nothing.
fn opens_item_on_marker_line(
    ctx: &crate::lint_context::LintContext,
    first_idx: usize,
    heading: &crate::lint_context::HeadingInfo,
) -> bool {
    let line = ctx.lines[first_idx].content(ctx.content);
    let margin = line.len() - line.trim_start_matches([' ', '\t', '>']).len();
    heading.first_line_prefix_len() > margin
}

/// Protect headings whose structural marker belongs to template code, while
/// retaining visible headings built from multiline template expressions.
fn heading_is_in_template_code(
    ctx: &crate::lint_context::LintContext,
    heading_idx: usize,
    heading: &crate::lint_context::HeadingInfo,
) -> bool {
    let marker_idx = if matches!(
        heading.style,
        crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
    ) {
        heading_idx + 1
    } else {
        heading_idx
    };
    ctx.lines
        .get(marker_idx)
        .is_some_and(|line| ctx.is_inside_template_code(line.byte_offset + line.indent))
}

/// Index of the line the document's first heading starts on, or `None` when
/// content the heading cannot open the document over comes first.
///
/// Blank lines, HTML comments, kramdown preamble lines and, in a Pandoc-compatible
/// flavor, div markers are transparent: they do not make the heading below them a
/// heading with content above it. So are the earlier text lines of a setext
/// heading, which are the heading itself rather than content above it.
fn heading_at_start_idx(ctx: &crate::lint_context::LintContext, is_pandoc: bool) -> Option<usize> {
    for (index, line) in ctx.lines.iter().enumerate() {
        if let Some(heading) = line.heading.as_deref() {
            return Some(first_text_idx(index, heading));
        }
        if line.is_blank || line.in_html_comment || line.in_mdx_comment || line.is_setext_heading_text {
            continue;
        }
        let trimmed = line.content(ctx.content).trim();
        if is_blank_or_comment_only(trimmed)
            || line.in_kramdown_extension_block
            || line.is_kramdown_block_ial
            || (is_pandoc && (pandoc::is_div_open(trimmed) || pandoc::is_div_close(trimmed)))
        {
            continue;
        }
        // Once real content precedes the first heading, no later line can
        // make that heading begin the document.
        return None;
    }
    None
}

///
/// This rule enforces consistent spacing around headings to improve document readability
/// and visual structure.
///
/// ## Purpose
///
/// - **Readability**: Blank lines create visual separation, making headings stand out
/// - **Parsing**: Many Markdown parsers require blank lines around headings for proper rendering
/// - **Consistency**: Creates a uniform document style throughout
/// - **Focus**: Helps readers identify and focus on section transitions
///
/// ## Configuration Options
///
/// The rule supports customizing the number of blank lines required:
///
/// ```yaml
/// MD022:
///   lines_above: 1  # Number of blank lines required above headings (default: 1)
///   lines_below: 1  # Number of blank lines required below headings (default: 1)
/// ```
///
/// ## Examples
///
/// ### Correct (with default configuration)
///
/// ```markdown
/// Regular paragraph text.
///
/// # Heading 1
///
/// Content under heading 1.
///
/// ## Heading 2
///
/// More content here.
/// ```
///
/// ### Incorrect (with default configuration)
///
/// ```markdown
/// Regular paragraph text.
/// # Heading 1
/// Content under heading 1.
/// ## Heading 2
/// More content here.
/// ```
///
/// ## Special Cases
///
/// This rule handles several special cases:
///
/// - **First Heading**: The first heading in a document doesn't require blank lines above
///   if it appears at the very start of the document
/// - **Front Matter**: YAML front matter is detected and skipped
/// - **Code Blocks**: Headings inside code blocks are ignored
/// - **Document Start/End**: Adjusts requirements for headings at the beginning or end of a document
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Adds the required number of blank lines above headings
/// - Adds the required number of blank lines below headings
/// - Preserves document structure and existing content
///
/// ## Performance Considerations
///
/// The rule is optimized for performance with:
/// - Efficient line counting algorithms
/// - Proper handling of front matter
/// - Smart code block detection
///
#[derive(Clone, Default)]
pub struct MD022BlanksAroundHeadings {
    config: MD022Config,
}

/// A heading with too few blank lines on one side.
struct Violation {
    /// Index of the line the heading is recorded on (the last text line of a setext heading).
    heading_line: usize,
    /// Index of the line the heading text starts on.
    first_line: usize,
    /// `"above"` or `"below"`.
    position: &'static str,
    needed_blanks: usize,
    heading_level: usize,
    is_setext: bool,
    /// The text of each blank line the fix adds: empty at the top level, the
    /// shared quote and list container prefix inside one.
    blank_line: String,
}

/// Actual blank source lines to remove above a heading; comments are never edits.
struct ExcessBlanks {
    heading_line: usize,
    first_line: usize,
    required: usize,
    lines: Vec<usize>,
}

impl MD022BlanksAroundHeadings {
    /// Create a new instance of the rule with default values:
    /// lines_above = 1, lines_below = 1
    pub fn new() -> Self {
        Self {
            config: MD022Config::default(),
        }
    }

    /// Create with custom numbers of blank lines (applies to all heading levels)
    pub fn with_values(lines_above: usize, lines_below: usize) -> Self {
        use md022_config::HeadingLevelConfig;
        Self {
            config: MD022Config {
                lines_above: HeadingLevelConfig::scalar(lines_above),
                lines_below: HeadingLevelConfig::scalar(lines_below),
                allowed_at_start: true,
                lines_above_mode: HeadingBlankMode::Minimum,
            },
        }
    }

    pub fn from_config_struct(config: MD022Config) -> Self {
        Self { config }
    }

    /// Exact mode caps actual blank lines. Comment-only lines still satisfy the
    /// existing minimum requirement, but are never removed or charged against this cap.
    fn excess_blank_lines(&self, ctx: &crate::lint_context::LintContext) -> Vec<ExcessBlanks> {
        if self.config.lines_above_mode != HeadingBlankMode::Exact {
            return Vec::new();
        }
        let is_pandoc = ctx.flavor.is_pandoc_compatible();
        let at_start = heading_at_start_idx(ctx, is_pandoc);
        // A gap shared by two headings must also satisfy the earlier heading's
        // minimum below it, including any attached attribute lines.
        let mut below_requirements = std::collections::HashMap::new();
        for parsed in ctx.headings() {
            let heading_line = parsed.line_num - 1;
            let last = heading_line + usize::from(parsed.is_setext());
            if parsed.line_info.in_pymdown_block
                || heading_is_in_template_code(ctx, heading_line, parsed.heading)
                || (parsed.first_line_num() - 1..=last).any(|idx| ctx.is_rule_disabled("MD022", idx + 1))
            {
                continue;
            }
            let Some(required) = self
                .config
                .lines_below
                .get_for_level(parsed.heading.level as usize)
                .required_count()
            else {
                continue;
            };
            let mut end = last;
            while end + 1 < ctx.lines.len()
                && is_block_attribute_line(neighbour_text(ctx, end + 1, parsed.is_blockquote()).trim(), ctx.flavor)
            {
                end += 1;
            }
            below_requirements.insert(end, required);
        }
        let mut excess = Vec::new();
        for parsed in ctx.headings() {
            let first = parsed.first_line_num() - 1;
            let heading_line = parsed.line_num - 1;
            let heading = parsed.heading;
            let Some(mut required) = self
                .config
                .lines_above
                .get_for_level(heading.level as usize)
                .required_count()
            else {
                continue;
            };
            let last = heading_line + usize::from(parsed.is_setext());
            if first == 0
                || (Some(first) == at_start && self.config.allowed_at_start)
                || parsed.line_info.in_pymdown_block
                || heading_is_in_template_code(ctx, heading_line, heading)
                || (first..=last).any(|idx| ctx.is_rule_disabled("MD022", idx + 1))
                || starts_item_under_bare_marker(ctx, first)
                || opens_item_on_marker_line(ctx, first, heading)
                || (ctx.flavor == crate::config::MarkdownFlavor::MDG && follows_mdg_tag_line(ctx, first, heading))
            {
                continue;
            }
            let quoted = parsed.is_blockquote();
            let depth = quote_depth(ctx.lines[first].content(ctx.content));
            let mut blanks = Vec::new();
            // A Setext heading needs a separator to avoid absorbing preceding text.
            if required == 0 && parsed.is_setext() {
                required = 1;
            }
            for idx in (0..first).rev() {
                let info = &ctx.lines[idx];
                let line = info.content(ctx.content);
                let trimmed = neighbour_text(ctx, idx, quoted).trim();
                if info.in_front_matter {
                    blanks.clear();
                    break;
                }
                if info.in_code_block || quote_depth(line) != depth {
                    break;
                }
                let in_comment = info.in_html_comment || info.in_mdx_comment;
                if trimmed.is_empty() && !in_comment {
                    blanks.push(idx);
                    continue;
                }
                // Only one blank run belongs to this heading. Crossing an earlier
                // comment, attribute or div could remove another block's separator.
                if !blanks.is_empty() {
                    if let Some(below) = below_requirements.get(&idx) {
                        required = required.max(*below);
                    }
                    if required == 0 && info.in_html_block {
                        required = 1;
                    }
                    break;
                }
                // Comments attached directly to the heading stay attached. Walk
                // through their interiors, but stop if an opener/closer shares text.
                if is_blank_or_comment_only(line) {
                    continue;
                }
                if info.in_html_comment && !line.contains("<!--") && !line.contains("-->") {
                    continue;
                }
                if info.in_mdx_comment
                    && (!trimmed.contains("{/*") || trimmed.starts_with("{/*"))
                    && (!trimmed.contains("*/}") || trimmed.ends_with("*/}"))
                {
                    continue;
                }
                break;
            }
            let lines: Vec<_> = blanks.into_iter().skip(required).collect();
            if !lines.is_empty() && !lines.iter().any(|&idx| ctx.is_rule_disabled("MD022", idx + 1)) {
                excess.push(ExcessBlanks {
                    heading_line,
                    first_line: first,
                    required,
                    lines,
                });
            }
        }
        excess
    }

    /// Fix a document by adding appropriate blank lines around headings
    fn fix_content(&self, ctx: &crate::lint_context::LintContext) -> String {
        // fix() runs on LF text: the CLI and DocumentRun::fix normalise the
        // document before fixing and restore its own line ending afterwards.
        let line_ending = "\n";
        let had_trailing_newline = ctx.content.ends_with('\n');
        let is_pandoc = ctx.flavor.is_pandoc_compatible();
        let is_mdg = ctx.flavor == crate::config::MarkdownFlavor::MDG;
        let mut result = Vec::new();
        let removed: std::collections::HashSet<usize> = self
            .excess_blank_lines(ctx)
            .into_iter()
            .flat_map(|excess| excess.lines)
            .collect();
        let mut skip_count: usize = 0;

        let heading_at_start_idx = heading_at_start_idx(ctx, is_pandoc);

        // Headings inside a blockquote by the line their text starts on; the
        // per-line `heading` field holds none of them.
        let quoted_headings: std::collections::HashMap<usize, crate::lint_context::ParsedHeading<'_>> = ctx
            .headings()
            .filter(crate::lint_context::ParsedHeading::is_blockquote)
            .map(|parsed| (parsed.first_line_num() - 1, parsed))
            .collect();

        for (i, line_info) in ctx.lines.iter().enumerate() {
            if skip_count > 0 {
                skip_count -= 1;
                continue;
            }
            if removed.contains(&i) {
                continue;
            }
            let line = line_info.content(ctx.content);

            if line_info.in_code_block {
                result.push(line.to_string());
                continue;
            }

            // Check if it's a heading. A setext heading is recorded on the last
            // line of its text, so a span reached at its first line looks ahead
            // for the line carrying it.
            let quoted_heading = quoted_headings.get(&i);
            let quoted = quoted_heading.is_some();
            let heading_idx = if let Some(parsed) = quoted_heading {
                Some(parsed.line_num - 1)
            } else if line_info.heading.is_some() {
                Some(i)
            } else if line_info.is_setext_heading_text {
                ctx.lines[i..]
                    .iter()
                    .position(|candidate| candidate.heading.is_some())
                    .map(|offset| i + offset)
            } else {
                None
            };

            if let Some(heading_idx) = heading_idx {
                let heading = match quoted_heading {
                    Some(parsed) => parsed.heading,
                    None => ctx.lines[heading_idx].heading.as_deref().unwrap(),
                };

                // The lines the heading occupies: the text it spans, and the
                // underline below a setext heading
                let heading_end_idx = if matches!(
                    heading.style,
                    crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
                ) && heading_idx + 1 < ctx.lines.len()
                {
                    heading_idx + 1
                } else {
                    heading_idx
                };

                // If the rule is disabled on any of the heading's lines, keep it
                // as written: the warning is dropped when any of them is
                // disabled, and the rewrite goes with it.
                if heading_is_in_template_code(ctx, heading_idx, heading)
                    || (i..=heading_end_idx).any(|idx| ctx.inline_config().is_rule_disabled("MD022", idx + 1))
                {
                    for idx in i..=heading_end_idx {
                        result.push(ctx.lines[idx].content(ctx.content).to_string());
                    }
                    skip_count += heading_end_idx - i;
                    continue;
                }

                // This is a heading line (ATX or Setext content)
                let is_first_heading = Some(i) == heading_at_start_idx;
                let heading_level = heading.level as usize;

                // Count existing blank lines above in the result, skipping HTML comments, IAL, and Quarto div markers
                let mut blank_lines_above = 0;
                let mut check_idx = result.len();
                while check_idx > 0 {
                    let prev_line = &result[check_idx - 1];
                    let trimmed = if quoted {
                        strip_quote_markers(prev_line).trim()
                    } else {
                        prev_line.trim()
                    };
                    if is_blank_or_comment_only(prev_line) {
                        // A line contributing nothing but comments counts as blank (#866)
                        blank_lines_above += 1;
                        check_idx -= 1;
                    } else if is_block_attribute_line(trimmed, ctx.flavor) {
                        // Skip kramdown IAL - they are attached to headings and transparent
                        check_idx -= 1;
                    } else if is_pandoc && (pandoc::is_div_open(trimmed) || pandoc::is_div_close(trimmed)) {
                        // Skip Pandoc/Quarto div markers — transparent for blank line counting
                        check_idx -= 1;
                    } else {
                        break;
                    }
                }

                // Determine how many blank lines we need above
                let requirement_above = self.config.lines_above.get_for_level(heading_level);
                let follows_mdg_tags = is_mdg && follows_mdg_tag_line(ctx, i, heading);
                let needed_blanks_above = if follows_mdg_tags
                    || (is_first_heading && self.config.allowed_at_start)
                    || starts_item_under_bare_marker(ctx, i)
                    || opens_item_on_marker_line(ctx, i, heading)
                    || (quoted && i == 0)
                {
                    0
                } else {
                    requirement_above.required_count().unwrap_or(0)
                };

                // Add missing blank lines above if needed
                if blank_lines_above < needed_blanks_above {
                    let blank = blank_line_above(line, result.iter().rev().map(String::as_str));
                    while blank_lines_above < needed_blanks_above {
                        result.push(blank.clone());
                        blank_lines_above += 1;
                    }
                }

                // Add the heading's own lines, underline included, so nothing
                // is inserted between two lines of one heading
                for idx in i..=heading_end_idx {
                    result.push(ctx.lines[idx].content(ctx.content).to_string());
                }
                skip_count += heading_end_idx - i; // Skip them in the main loop

                // Determine base index for checking lines below
                let mut effective_end_idx = heading_end_idx;

                // Add any kramdown IAL lines that immediately follow the heading
                // These are part of the heading element and should not be separated
                let mut ial_count = 0;
                while effective_end_idx + 1 < ctx.lines.len() {
                    let next_line = &ctx.lines[effective_end_idx + 1];
                    let next_trimmed = neighbour_text(ctx, effective_end_idx + 1, quoted).trim();
                    if is_block_attribute_line(next_trimmed, ctx.flavor) {
                        // A quoted attribute line keeps its quote markers
                        result.push(if quoted {
                            next_line.content(ctx.content).to_string()
                        } else {
                            next_trimmed.to_string()
                        });
                        effective_end_idx += 1;
                        ial_count += 1;
                    } else {
                        break;
                    }
                }

                // Now check blank lines below the heading (including underline and IAL)
                let mut blank_lines_below = 0;
                let mut next_content_line_idx = None;
                for j in (effective_end_idx + 1)..ctx.lines.len() {
                    if ctx.lines[j].is_blank || is_blank_or_comment_only(ctx.lines[j].content(ctx.content)) {
                        blank_lines_below += 1;
                    } else {
                        next_content_line_idx = Some(j);
                        break;
                    }
                }

                // Check if the next non-blank line is special (code fence or list item)
                let next_is_special = if let Some(idx) = next_content_line_idx {
                    let next_line = &ctx.lines[idx];
                    let trimmed = neighbour_text(ctx, idx, quoted).trim();
                    next_line.list_item.is_some()
                        || starts_with_list_marker(trimmed)
                        || ((trimmed.starts_with("```") || trimmed.starts_with("~~~"))
                            && (trimmed.len() == 3
                                || (trimmed.len() > 3
                                    && trimmed
                                        .chars()
                                        .nth(3)
                                        .is_some_and(|c| c.is_whitespace() || c.is_alphabetic()))))
                } else {
                    false
                };

                // Add missing blank lines below if needed
                let requirement_below = self.config.lines_below.get_for_level(heading_level);
                // A heading that ends the document has nothing to separate
                // from, and `check` asks for no blank line below it.
                let needed_blanks_below = if next_is_special || next_content_line_idx.is_none() {
                    0
                } else {
                    requirement_below.required_count().unwrap_or(0)
                };
                if blank_lines_below < needed_blanks_below {
                    let blank = blank_line_between(line, ctx.lines[effective_end_idx + 1].content(ctx.content));
                    for _ in 0..(needed_blanks_below - blank_lines_below) {
                        result.push(blank.clone());
                    }
                }

                // Skip the IAL lines in the main loop since we already added them
                skip_count += ial_count;
            } else {
                // Regular line - just add it
                result.push(line.to_string());
            }
        }

        // `ctx.lines` holds no entry after the final newline, so the file's
        // own final newline is restored after the join, even when the last
        // line is blank.
        let mut joined = result.join(line_ending);
        if had_trailing_newline {
            joined.push_str(line_ending);
        }
        joined
    }
}

impl Rule for MD022BlanksAroundHeadings {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        (self.config == Default::default()).then_some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(22))
    }

    fn name(&self) -> &'static str {
        "MD022"
    }

    fn description(&self) -> &'static str {
        "Headings should be surrounded by blank lines"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut result = Vec::new();

        // Skip if empty document
        if ctx.lines.is_empty() {
            return Ok(result);
        }

        // Fix replacements are written for LF text; where warnings are collected,
        // conform_fix_line_endings rewrites them for a CRLF document.
        let line_ending = "\n";
        let is_pandoc = ctx.flavor.is_pandoc_compatible();
        let is_mdg = ctx.flavor == crate::config::MarkdownFlavor::MDG;

        let heading_at_start_idx = heading_at_start_idx(ctx, is_pandoc);

        // Collect all headings first to batch process
        let mut heading_violations = Vec::new();

        for parsed in ctx.headings() {
            let line_num = parsed.line_num - 1;
            let line_info = parsed.line_info;

            // Skip headings inside PyMdown blocks (/// ... ///) - MkDocs flavor only
            if line_info.in_pymdown_block {
                continue;
            }

            let heading = parsed.heading;
            if heading_is_in_template_code(ctx, line_num, heading) {
                continue;
            }

            let heading_level = heading.level as usize;
            let quoted = parsed.is_blockquote();

            // What sits above the heading sits above the line its text starts
            // on, which for a setext heading is the first line of the paragraph
            // its underline ends
            let first_idx = parsed.first_line_num() - 1;

            // Check if this heading is at document start
            let is_first_heading = Some(first_idx) == heading_at_start_idx;

            // Get configured blank line requirements for this heading level
            let required_above_count = self.config.lines_above.get_for_level(heading_level).required_count();
            let required_below_count = self.config.lines_below.get_for_level(heading_level).required_count();

            // Count blank lines above if needed
            let should_check_above = required_above_count.is_some()
                && first_idx > 0
                && (!is_first_heading || !self.config.allowed_at_start)
                && !(is_mdg && follows_mdg_tag_line(ctx, first_idx, heading));
            // A blank line between a bare marker and the heading that begins its item
            // would empty the item and push the heading out of it.
            let should_check_above = should_check_above
                && !starts_item_under_bare_marker(ctx, first_idx)
                && !opens_item_on_marker_line(ctx, first_idx, heading);
            if should_check_above {
                let mut blank_lines_above = 0;
                let mut hit_frontmatter_end = false;
                for j in (0..first_idx).rev() {
                    let line_content = ctx.lines[j].content(ctx.content);
                    let trimmed = neighbour_text(ctx, j, quoted).trim();
                    if ctx.lines[j].is_blank || is_blank_or_comment_only(line_content) {
                        // A line contributing nothing but comments separates the blocks
                        // around it the way an empty one does (#866)
                        blank_lines_above += 1;
                    } else if ctx.lines[j].in_html_comment || ctx.lines[j].in_mdx_comment {
                        // Skip the interior of a multi-line comment - transparent for blank line counting
                        continue;
                    } else if is_block_attribute_line(trimmed, ctx.flavor) {
                        // Skip kramdown IAL - they are attached to headings and transparent for blank line counting
                        continue;
                    } else if is_pandoc && (pandoc::is_div_open(trimmed) || pandoc::is_div_close(trimmed)) {
                        // Skip Pandoc/Quarto div markers — transparent for blank line counting
                        continue;
                    } else if ctx.lines[j].in_front_matter {
                        // Skip frontmatter - first heading after frontmatter doesn't need blank line above
                        // Note: We only check in_front_matter flag, NOT the string "---", because
                        // a standalone "---" is a horizontal rule and should NOT exempt headings
                        // from requiring blank lines above
                        hit_frontmatter_end = true;
                        break;
                    } else {
                        break;
                    }
                }
                let required = required_above_count.unwrap();
                if !hit_frontmatter_end && blank_lines_above < required {
                    let needed_blanks = required - blank_lines_above;
                    heading_violations.push(Violation {
                        heading_line: line_num,
                        first_line: first_idx,
                        position: "above",
                        needed_blanks,
                        heading_level,
                        is_setext: parsed.is_setext(),
                        blank_line: blank_line_above(
                            ctx.lines[first_idx].content(ctx.content),
                            ctx.lines[..first_idx]
                                .iter()
                                .rev()
                                .map(|line| line.content(ctx.content)),
                        ),
                    });
                }
            }

            // Determine the effective last line of the heading
            let mut effective_last_line = if matches!(
                heading.style,
                crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
            ) {
                line_num + 1 // For Setext, include the underline
            } else {
                line_num
            };

            // Extend effective_last_line to include any kramdown IAL lines immediately following
            // IAL lines like `{: .class #id}` are part of the heading element
            while effective_last_line + 1 < ctx.lines.len() {
                let next_trimmed = neighbour_text(ctx, effective_last_line + 1, quoted).trim();
                if is_block_attribute_line(next_trimmed, ctx.flavor) {
                    effective_last_line += 1;
                } else {
                    break;
                }
            }

            // Check blank lines below
            if effective_last_line < ctx.lines.len() - 1 {
                // Find next non-blank line, skipping transparent elements (blank lines, HTML comments, Quarto div markers)
                let mut next_non_blank_idx = effective_last_line + 1;
                while next_non_blank_idx < ctx.lines.len() {
                    let check_line = &ctx.lines[next_non_blank_idx];
                    let check_trimmed = neighbour_text(ctx, next_non_blank_idx, quoted).trim();
                    if check_line.is_blank {
                        next_non_blank_idx += 1;
                    } else if check_line.in_html_comment
                        || check_line.in_mdx_comment
                        || is_blank_or_comment_only(check_line.content(ctx.content))
                    {
                        // Skip HTML comments - they are transparent for blank line counting
                        next_non_blank_idx += 1;
                    } else if is_pandoc && (pandoc::is_div_open(check_trimmed) || pandoc::is_div_close(check_trimmed)) {
                        // Skip Pandoc/Quarto div markers — transparent for blank line counting
                        next_non_blank_idx += 1;
                    } else {
                        break;
                    }
                }

                // If we've reached end of document (after skipping transparent elements), no blank needed
                if next_non_blank_idx >= ctx.lines.len() {
                    // End of document - no blank line needed after heading
                    continue;
                }

                // Check if next line is a code fence or list item
                let next_line_is_special = {
                    let next_line = &ctx.lines[next_non_blank_idx];
                    let next_trimmed = neighbour_text(ctx, next_non_blank_idx, quoted).trim();

                    // Check for code fence
                    let is_code_fence = (next_trimmed.starts_with("```") || next_trimmed.starts_with("~~~"))
                        && (next_trimmed.len() == 3
                            || (next_trimmed.len() > 3
                                && next_trimmed
                                    .chars()
                                    .nth(3)
                                    .is_some_and(|c| c.is_whitespace() || c.is_alphabetic())));

                    // Check for list item. The parser's `list_item` flag for a following
                    // marker is not parse-stable here: a sibling list directly above the
                    // heading can leak list context onto this line, so inserting the blank
                    // line above the heading flips the flag (e.g. `2. ` counts as a list
                    // item in one pass and not the next). Detect the marker syntactically as
                    // well, so the guard - and therefore the fix - stays idempotent.
                    let is_list_item = next_line.list_item.is_some() || starts_with_list_marker(next_trimmed);

                    is_code_fence || is_list_item
                };

                // Only generate warning if next line is NOT a code fence or list item
                if !next_line_is_special && let Some(required) = required_below_count {
                    // Count blank lines below (counting only blank lines, not skipped transparent lines)
                    let mut blank_lines_below = 0;
                    for k in (effective_last_line + 1)..next_non_blank_idx {
                        // A line contributing nothing but comments counts as blank (#866)
                        if ctx.lines[k].is_blank || is_blank_or_comment_only(ctx.lines[k].content(ctx.content)) {
                            blank_lines_below += 1;
                        }
                    }

                    if blank_lines_below < required {
                        let needed_blanks = required - blank_lines_below;
                        heading_violations.push(Violation {
                            heading_line: line_num,
                            first_line: first_idx,
                            position: "below",
                            needed_blanks,
                            heading_level,
                            is_setext: parsed.is_setext(),
                            blank_line: blank_line_between(
                                ctx.lines[first_idx].content(ctx.content),
                                ctx.lines[effective_last_line + 1].content(ctx.content),
                            ),
                        });
                    }
                }
            }
        }

        // Generate warnings for all violations
        for Violation {
            heading_line,
            first_line,
            position,
            needed_blanks,
            heading_level,
            is_setext,
            blank_line,
        } in heading_violations
        {
            let line_info = &ctx.lines[heading_line];

            // Calculate precise character range for the heading
            let (start_line, start_col, end_line, end_col) =
                calculate_heading_range(first_line + 1, heading_line + 1, line_info.content(ctx.content));

            // Each requirement is resolved inside the arm that uses it. A
            // requirement can be unlimited (a negative config value such as
            // `lines_above: -1`), in which case it has no required count and
            // never produces a violation for its own position. Resolving both
            // up front panicked whenever one position was unlimited and the
            // other reported.
            let (message, insertion_point) = match position {
                "above" => {
                    let Some(required_above_count) =
                        self.config.lines_above.get_for_level(heading_level).required_count()
                    else {
                        continue;
                    };
                    (
                        format!(
                            "Expected {} blank {} above heading",
                            required_above_count,
                            if required_above_count == 1 { "line" } else { "lines" }
                        ),
                        first_line, // Insert before the line the heading text starts on
                    )
                }
                "below" => {
                    let Some(required_below_count) =
                        self.config.lines_below.get_for_level(heading_level).required_count()
                    else {
                        continue;
                    };
                    // For Setext headings, insert after the underline
                    let insert_after = if is_setext { heading_line + 2 } else { heading_line + 1 };

                    (
                        format!(
                            "Expected {} blank {} below heading",
                            required_below_count,
                            if required_below_count == 1 { "line" } else { "lines" }
                        ),
                        insert_after,
                    )
                }
                _ => continue,
            };

            // Calculate byte range for insertion
            let byte_range = if insertion_point == 0 && position == "above" {
                // Insert at beginning of document (only for "above" case at line 0)
                0..0
            } else if position == "above" && insertion_point > 0 {
                // For "above", insert at the start of the heading line
                ctx.lines[insertion_point].byte_offset..ctx.lines[insertion_point].byte_offset
            } else if position == "below" && insertion_point - 1 < ctx.lines.len() {
                // For "below", insert after the line
                let line_idx = insertion_point - 1;
                let line_end_offset = if line_idx + 1 < ctx.lines.len() {
                    ctx.lines[line_idx + 1].byte_offset
                } else {
                    ctx.content.len()
                };
                line_end_offset..line_end_offset
            } else {
                // Insert at end of file
                let content_len = ctx.content.len();
                content_len..content_len
            };

            result.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message,
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                severity: Severity::Warning,
                fix: Some(Fix::new(
                    byte_range,
                    format!("{blank_line}{line_ending}").repeat(needed_blanks),
                )),
            });
        }

        for excess in self.excess_blank_lines(ctx) {
            let line = ctx.lines[excess.heading_line].content(ctx.content);
            let (start_line, start_col, end_line, end_col) =
                calculate_heading_range(excess.first_line + 1, excess.heading_line + 1, line);
            let mut edits = excess.lines.iter().map(|&idx| {
                Fix::new(
                    ctx.lines[idx].byte_offset..ctx.lines[idx + 1].byte_offset,
                    String::new(),
                )
            });
            let first = edits.next().expect("an excess contains blank lines");
            result.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                message: format!(
                    "Expected exactly {} blank {} above heading, found {}",
                    excess.required,
                    if excess.required == 1 { "line" } else { "lines" },
                    excess.required + excess.lines.len()
                ),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                severity: Severity::Warning,
                fix: Some(Fix::with_additional_edits(
                    first.range,
                    first.replacement,
                    edits.collect(),
                )),
            });
        }
        result.sort_by_key(|warning| (warning.line, warning.column));
        Ok(result)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if ctx.content.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Use a consolidated fix that avoids adding multiple blank lines
        let fixed = self.fix_content(ctx);

        Ok(fixed)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Fast path: check if document likely has headings
        if ctx.content.is_empty() || !ctx.likely_has_headings() {
            return true;
        }
        // Verify headings actually exist
        ctx.headings().next().is_none()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD022Config);

    fn polymorphic_config_keys(&self) -> &'static [&'static str] {
        // Both options accept either one integer for every heading level or an
        // array of six integers, one for each of h1 through h6. The serialized
        // defaults are scalar, so validation must not reject the array form
        // before MD022's deserializer can read it.
        &["lines-above", "lines-below"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_valid_headings() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "\n# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_missing_blank_above() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0); // No warning for first heading

        let fixed = rule.fix(&ctx).unwrap();

        // Test for the ability to handle the content without breaking it
        // Don't check for exact string equality which may break with implementation changes
        assert!(fixed.contains("# Heading 1"));
        assert!(fixed.contains("Some content."));
        assert!(fixed.contains("## Heading 2"));
        assert!(fixed.contains("More content."));
    }

    #[test]
    fn test_missing_blank_below() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "\n# Heading 1\nSome content.\n\n## Heading 2\n\nMore content.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);

        // Test the fix
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("# Heading 1\n\nSome content"));
    }

    #[test]
    fn test_missing_blank_above_and_below() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\nSome content.\n## Heading 2\nMore content.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3); // Missing blanks: below first heading, above second heading, below second heading

        // Test the fix
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("# Heading 1\n\nSome content"));
        assert!(fixed.contains("Some content.\n\n## Heading 2"));
        assert!(fixed.contains("## Heading 2\n\nMore content"));
    }

    #[test]
    fn test_fix_headings() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\nSome content.\n## Heading 2\nMore content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();

        let expected = "# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_consecutive_headings_pattern() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();

        // Using more specific assertions to check the structure
        let lines: Vec<&str> = result.lines().collect();
        assert!(!lines.is_empty());

        // Find the positions of the headings
        let h1_pos = lines.iter().position(|&l| l == "# Heading 1").unwrap();
        let h2_pos = lines.iter().position(|&l| l == "## Heading 2").unwrap();
        let h3_pos = lines.iter().position(|&l| l == "### Heading 3").unwrap();

        // Verify blank lines between headings
        assert!(
            h2_pos > h1_pos + 1,
            "Should have at least one blank line after first heading"
        );
        assert!(
            h3_pos > h2_pos + 1,
            "Should have at least one blank line after second heading"
        );

        // Verify there's a blank line between h1 and h2
        assert!(lines[h1_pos + 1].trim().is_empty(), "Line after h1 should be blank");

        // Verify there's a blank line between h2 and h3
        assert!(lines[h2_pos + 1].trim().is_empty(), "Line after h2 should be blank");
    }

    #[test]
    fn test_blanks_around_setext_headings() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "Heading 1\n=========\nSome content.\nHeading 2\n---------\nMore content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();

        // Check that the fix follows requirements without being too rigid about the exact output format
        let lines: Vec<&str> = result.lines().collect();

        // Verify key elements are present
        assert!(result.contains("Heading 1"));
        assert!(result.contains("========="));
        assert!(result.contains("Some content."));
        assert!(result.contains("Heading 2"));
        assert!(result.contains("---------"));
        assert!(result.contains("More content."));

        // Verify structure ensures blank lines are added after headings
        let heading1_marker_idx = lines.iter().position(|&l| l == "=========").unwrap();
        let some_content_idx = lines.iter().position(|&l| l == "Some content.").unwrap();
        assert!(
            some_content_idx > heading1_marker_idx + 1,
            "Should have a blank line after the first heading"
        );

        let heading2_marker_idx = lines.iter().position(|&l| l == "---------").unwrap();
        let more_content_idx = lines.iter().position(|&l| l == "More content.").unwrap();
        assert!(
            more_content_idx > heading2_marker_idx + 1,
            "Should have a blank line after the second heading"
        );

        // Verify that the fixed content has no warnings
        let fixed_ctx = LintContext::new(&result, crate::config::MarkdownFlavor::Standard, None);
        let fixed_warnings = rule.check(&fixed_ctx).unwrap();
        assert!(fixed_warnings.is_empty(), "Fixed content should have no warnings");
    }

    #[test]
    fn test_fix_specific_blank_line_cases() {
        let rule = MD022BlanksAroundHeadings::default();

        // Case 1: Testing consecutive headings
        let content1 = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx1 = LintContext::new(content1, crate::config::MarkdownFlavor::Standard, None);
        let result1 = rule.fix(&ctx1).unwrap();
        // Verify structure rather than exact content as the fix implementation may vary
        assert!(result1.contains("# Heading 1"));
        assert!(result1.contains("## Heading 2"));
        assert!(result1.contains("### Heading 3"));
        // Ensure each heading has a blank line after it
        let lines: Vec<&str> = result1.lines().collect();
        let h1_pos = lines.iter().position(|&l| l == "# Heading 1").unwrap();
        let h2_pos = lines.iter().position(|&l| l == "## Heading 2").unwrap();
        assert!(lines[h1_pos + 1].trim().is_empty(), "Should have a blank line after h1");
        assert!(lines[h2_pos + 1].trim().is_empty(), "Should have a blank line after h2");

        // Case 2: Headings with content
        let content2 = "# Heading 1\nContent under heading 1\n## Heading 2";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.fix(&ctx2).unwrap();
        // Verify structure
        assert!(result2.contains("# Heading 1"));
        assert!(result2.contains("Content under heading 1"));
        assert!(result2.contains("## Heading 2"));
        // Check spacing
        let lines2: Vec<&str> = result2.lines().collect();
        let h1_pos2 = lines2.iter().position(|&l| l == "# Heading 1").unwrap();
        let _content_pos = lines2.iter().position(|&l| l == "Content under heading 1").unwrap();
        assert!(
            lines2[h1_pos2 + 1].trim().is_empty(),
            "Should have a blank line after heading 1"
        );

        // Case 3: Multiple consecutive headings with blank lines preserved
        let content3 = "# Heading 1\n\n\n## Heading 2\n\n\n### Heading 3\n\nContent";
        let ctx3 = LintContext::new(content3, crate::config::MarkdownFlavor::Standard, None);
        let result3 = rule.fix(&ctx3).unwrap();
        // Just verify it doesn't crash and properly formats headings
        assert!(result3.contains("# Heading 1"));
        assert!(result3.contains("## Heading 2"));
        assert!(result3.contains("### Heading 3"));
        assert!(result3.contains("Content"));
    }

    #[test]
    fn test_fix_preserves_existing_blank_lines() {
        let rule = MD022BlanksAroundHeadings::new();
        let content = "# Title

## Section 1

Content here.

## Section 2

More content.
### Missing Blank Above

Even more content.

## Section 3

Final content.";

        let expected = "# Title

## Section 1

Content here.

## Section 2

More content.

### Missing Blank Above

Even more content.

## Section 3

Final content.";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix_content(&ctx);
        assert_eq!(
            result, expected,
            "Fix should only add missing blank lines, never remove existing ones"
        );
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD022BlanksAroundHeadings::new();

        // Test with trailing newline
        let content_with_newline = "# Title\nContent here.\n";
        let ctx = LintContext::new(content_with_newline, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();
        assert!(result.ends_with('\n'), "Should preserve trailing newline");

        // Test without trailing newline
        let content_without_newline = "# Title\nContent here.";
        let ctx = LintContext::new(content_without_newline, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix(&ctx).unwrap();
        assert!(
            !result.ends_with('\n'),
            "Should not add trailing newline if original didn't have one"
        );
    }

    #[test]
    fn test_fix_does_not_add_blank_lines_before_lists() {
        let rule = MD022BlanksAroundHeadings::new();
        let content = "## Configuration\n\nThis rule has the following configuration options:\n\n- `option1`: Description of option 1.\n- `option2`: Description of option 2.\n\n## Another Section\n\nSome content here.";

        let expected = "## Configuration\n\nThis rule has the following configuration options:\n\n- `option1`: Description of option 1.\n- `option2`: Description of option 2.\n\n## Another Section\n\nSome content here.";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.fix_content(&ctx);
        assert_eq!(result, expected, "Fix should not add blank lines before lists");
    }

    #[test]
    fn test_fix_idempotent_when_heading_follows_a_list_and_precedes_a_marker() {
        // Regression: a list directly above a heading (`- a\n# H`) makes the parser tag the
        // following marker line (`2. `) as a list item, so the blank-below was skipped.
        // Inserting the blank above closed that list and flipped the flag, so a second pass
        // then added the blank below — a non-idempotent fix. The marker is now recognized
        // syntactically, so the heading-followed-by-list decision is stable across passes.
        let rule = MD022BlanksAroundHeadings::default();
        let content = "- a\n# H\n2. ";
        for flavor in [
            crate::config::MarkdownFlavor::Standard,
            crate::config::MarkdownFlavor::MkDocs,
            crate::config::MarkdownFlavor::MDX,
        ] {
            let ctx1 = LintContext::new(content, flavor, None);
            let fixed1 = rule.fix(&ctx1).unwrap();
            let ctx2 = LintContext::new(&fixed1, flavor, None);
            let fixed2 = rule.fix(&ctx2).unwrap();
            assert_eq!(fixed1, fixed2, "MD022 fix must be idempotent (flavor={flavor:?})");
        }
    }

    #[test]
    fn test_thematic_break_below_heading_is_not_a_list_item() {
        // A break written with a marker and a space (`* * *`, `- - -`, `- --`) opens like a
        // list item, so the syntactic list test used to exempt it from the blank-below
        // requirement while every other spelling was reported. `---- ----` shows the
        // inconsistency from the other side: it was already reported, because its second
        // character is not a space. rumdl parses all of these as thematic breaks (MD032
        // sees no list), so they must behave identically.
        let rule = MD022BlanksAroundHeadings::default();
        for marker in [
            "* * *",
            "- - -",
            "_ _ _",
            "***",
            "---",
            "___",
            "- --",
            "* ** *",
            "---- ----",
        ] {
            let content = format!("text\n\n# Heading\n{marker}\nafter\n");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.len(),
                1,
                "a heading above `{marker}` needs a blank line below it, got {result:?}"
            );
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                format!("text\n\n# Heading\n\n{marker}\nafter\n"),
                "fix must insert the blank line below the heading for `{marker}`"
            );
        }
    }

    #[test]
    fn test_list_item_below_heading_is_still_exempt() {
        // The control for the exclusion above: real list items, including `+ + +`, which is
        // a list item and not a thematic break because `+` is not a thematic break marker.
        let rule = MD022BlanksAroundHeadings::default();
        for item in ["- item", "* item", "+ item", "1. item", "+ + +"] {
            let content = format!("text\n\n# Heading\n{item}\n");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            assert!(
                rule.check(&ctx).unwrap().is_empty(),
                "a list below a heading stays exempt, but `{item}` was reported"
            );
            assert_eq!(rule.fix(&ctx).unwrap(), content, "`{item}` must not be rewritten");
        }
    }

    #[test]
    fn test_per_level_configuration_no_blank_above_h1() {
        use md022_config::HeadingLevelConfig;

        // Configure: no blank above H1, 1 blank above H2-H6
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::per_level([0, 1, 1, 1, 1, 1]),
            lines_below: HeadingLevelConfig::scalar(1),
            allowed_at_start: false, // Disable special handling for first heading
            lines_above_mode: HeadingBlankMode::Minimum,
        });

        // H1 without blank above should be OK
        let content = "Some text\n# Heading 1\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "H1 without blank above should not trigger warning");

        // H2 without blank above should trigger warning
        let content = "Some text\n## Heading 2\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "H2 without blank above should trigger warning");
        assert!(warnings[0].message.contains("above"));
    }

    #[test]
    fn test_unlimited_above_with_limited_below_does_not_panic() {
        use md022_config::{HeadingBlankRequirement, HeadingLevelConfig};

        // `lines_above: -1` means "any number of blank lines above", so that
        // side has no required count. A violation on the *other* side used to
        // resolve both counts up front and panic on the unlimited one.
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::scalar_requirement(HeadingBlankRequirement::Unlimited),
            lines_below: HeadingLevelConfig::scalar(1),
            allowed_at_start: false,
            lines_above_mode: HeadingBlankMode::Minimum,
        });

        // "## Banana" has no blank line below it, so a "below" violation fires.
        let content = "# Title\n\nText\n## Banana\nText\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).expect("check must not fail");

        assert!(
            warnings.iter().any(|w| w.message.contains("below")),
            "expected a 'below' violation, got: {:?}",
            warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
        assert!(
            !warnings.iter().any(|w| w.message.contains("above")),
            "an unlimited 'above' requirement must never report: {:?}",
            warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_unlimited_below_with_limited_above_does_not_panic() {
        use md022_config::{HeadingBlankRequirement, HeadingLevelConfig};

        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::scalar(1),
            lines_below: HeadingLevelConfig::scalar_requirement(HeadingBlankRequirement::Unlimited),
            allowed_at_start: false,
            lines_above_mode: HeadingBlankMode::Minimum,
        });

        // "## Banana" has no blank line above it, so an "above" violation fires.
        let content = "# Title\n\nText\n## Banana\n\nText\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).expect("check must not fail");

        assert!(
            warnings.iter().any(|w| w.message.contains("above")),
            "expected an 'above' violation, got: {:?}",
            warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
        assert!(
            !warnings.iter().any(|w| w.message.contains("below")),
            "an unlimited 'below' requirement must never report: {:?}",
            warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_per_level_configuration_different_requirements() {
        use md022_config::HeadingLevelConfig;

        // Configure: 0 blank above H1, 1 above H2-H3, 2 above H4-H6
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::per_level([0, 1, 1, 2, 2, 2]),
            lines_below: HeadingLevelConfig::scalar(1),
            allowed_at_start: false,
            lines_above_mode: HeadingBlankMode::Minimum,
        });

        let content = "Text\n# H1\n\nText\n\n## H2\n\nText\n\n### H3\n\nText\n\n\n#### H4\n\nText";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should have no warnings - all headings satisfy their level-specific requirements
        assert_eq!(
            warnings.len(),
            0,
            "All headings should satisfy level-specific requirements"
        );
    }

    #[test]
    fn test_per_level_configuration_violations() {
        use md022_config::HeadingLevelConfig;

        // Configure: H4 needs 2 blanks above
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::per_level([1, 1, 1, 2, 1, 1]),
            lines_below: HeadingLevelConfig::scalar(1),
            allowed_at_start: false,
            lines_above_mode: HeadingBlankMode::Minimum,
        });

        // H4 with only 1 blank above should trigger warning
        let content = "Text\n\n#### Heading 4\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 1, "H4 with insufficient blanks should trigger warning");
        assert!(warnings[0].message.contains("2 blank lines above"));
    }

    #[test]
    fn test_per_level_fix_different_levels() {
        use md022_config::HeadingLevelConfig;

        // Configure: 0 blank above H1, 1 above H2, 2 above H3+
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::per_level([0, 1, 2, 2, 2, 2]),
            lines_below: HeadingLevelConfig::scalar(1),
            allowed_at_start: false,
            lines_above_mode: HeadingBlankMode::Minimum,
        });

        let content = "Text\n# H1\nContent\n## H2\nContent\n### H3\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Verify structure: H1 gets 0 blanks above, H2 gets 1, H3 gets 2
        assert!(fixed.contains("Text\n# H1\n\nContent"));
        assert!(fixed.contains("Content\n\n## H2\n\nContent"));
        assert!(fixed.contains("Content\n\n\n### H3\n\nContent"));
    }

    #[test]
    fn test_per_level_below_configuration() {
        use md022_config::HeadingLevelConfig;

        // Configure: different blank line requirements below headings
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::scalar(1),
            lines_below: HeadingLevelConfig::per_level([2, 1, 1, 1, 1, 1]), // H1 needs 2 blanks below
            allowed_at_start: true,
            lines_above_mode: HeadingBlankMode::Minimum,
        });

        // H1 with only 1 blank below should trigger warning
        let content = "# Heading 1\n\nSome text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(
            warnings.len(),
            1,
            "H1 with insufficient blanks below should trigger warning"
        );
        assert!(warnings[0].message.contains("2 blank lines below"));
    }

    #[test]
    fn test_scalar_configuration_still_works() {
        use md022_config::HeadingLevelConfig;

        // Ensure scalar configuration still works (backward compatibility)
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::scalar(2),
            lines_below: HeadingLevelConfig::scalar(2),
            allowed_at_start: false,
            lines_above_mode: HeadingBlankMode::Minimum,
        });

        let content = "Text\n# H1\nContent\n## H2\nContent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // All headings should need 2 blanks above and below
        assert!(!warnings.is_empty(), "Should have violations for insufficient blanks");
    }

    #[test]
    fn test_unlimited_configuration_skips_requirements() {
        use md022_config::{HeadingBlankRequirement, HeadingLevelConfig};

        // H1 can have any number of blank lines above/below; others require defaults
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above: HeadingLevelConfig::per_level_requirements([
                HeadingBlankRequirement::unlimited(),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
            ]),
            lines_below: HeadingLevelConfig::per_level_requirements([
                HeadingBlankRequirement::unlimited(),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
                HeadingBlankRequirement::limited(1),
            ]),
            allowed_at_start: false,
            lines_above_mode: HeadingBlankMode::Minimum,
        });

        let content = "# H1\nParagraph\n## H2\nParagraph";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // H1 has no blanks above/below but is unlimited; H2 should get violations
        assert_eq!(warnings.len(), 2, "Only non-unlimited headings should warn");
        assert!(
            warnings.iter().all(|w| w.line >= 3),
            "Warnings should target later headings"
        );

        // Fixing should insert blanks around H2 but leave H1 untouched
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.starts_with("# H1\nParagraph\n\n## H2"),
            "H1 should remain unchanged"
        );
    }

    #[test]
    fn test_html_comment_transparency() {
        // HTML comments are transparent for blank line counting
        // A heading following a blank line + HTML comment should be valid
        // Verified with markdownlint: no MD022 warning for this pattern
        let rule = MD022BlanksAroundHeadings::default();

        // Pattern: content, blank line, HTML comment, heading
        // The blank line before the HTML comment counts for the heading
        let content = "Some content\n\n<!-- markdownlint-disable-next-line MD001 -->\n#### Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "HTML comment is transparent - blank line above it counts for heading"
        );

        // Multi-line HTML comment is also transparent
        let content_multiline = "Some content\n\n<!-- This is a\nmulti-line comment -->\n#### Heading";
        let ctx_multiline = LintContext::new(content_multiline, crate::config::MarkdownFlavor::Standard, None);
        let warnings_multiline = rule.check(&ctx_multiline).unwrap();
        assert!(
            warnings_multiline.is_empty(),
            "Multi-line HTML comment is also transparent"
        );
    }

    #[test]
    fn test_frontmatter_transparency() {
        // Frontmatter is transparent for MD022 - heading can appear immediately after
        // Verified with markdownlint: no MD022 warning for heading after frontmatter
        let rule = MD022BlanksAroundHeadings::default();

        // Heading immediately after frontmatter closing ---
        let content = "---\ntitle: Test\n---\n# First heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Frontmatter is transparent - heading can appear immediately after"
        );

        // Heading with blank line after frontmatter is also valid
        let content_with_blank = "---\ntitle: Test\n---\n\n# First heading";
        let ctx_with_blank = LintContext::new(content_with_blank, crate::config::MarkdownFlavor::Standard, None);
        let warnings_with_blank = rule.check(&ctx_with_blank).unwrap();
        assert!(
            warnings_with_blank.is_empty(),
            "Heading with blank line after frontmatter should also be valid"
        );

        // TOML frontmatter (+++...+++) is also transparent
        let content_toml = "+++\ntitle = \"Test\"\n+++\n# First heading";
        let ctx_toml = LintContext::new(content_toml, crate::config::MarkdownFlavor::Standard, None);
        let warnings_toml = rule.check(&ctx_toml).unwrap();
        assert!(
            warnings_toml.is_empty(),
            "TOML frontmatter is also transparent for MD022"
        );
    }

    #[test]
    fn test_horizontal_rule_not_treated_as_frontmatter() {
        // Issue #238: Horizontal rules (---) should NOT be treated as frontmatter.
        // A heading after a horizontal rule MUST have a blank line above it.
        let rule = MD022BlanksAroundHeadings::default();

        // Case 1: Heading immediately after horizontal rule - SHOULD warn
        let content = "Some content\n\n---\n# Heading after HR";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            !warnings.is_empty(),
            "Heading after horizontal rule without blank line SHOULD trigger MD022"
        );
        assert!(
            warnings.iter().any(|w| w.line == 4),
            "Warning should be on line 4 (the heading line)"
        );

        // Case 2: Heading with blank line after HR - should NOT warn
        let content_with_blank = "Some content\n\n---\n\n# Heading after HR";
        let ctx_with_blank = LintContext::new(content_with_blank, crate::config::MarkdownFlavor::Standard, None);
        let warnings_with_blank = rule.check(&ctx_with_blank).unwrap();
        assert!(
            warnings_with_blank.is_empty(),
            "Heading with blank line after HR should not trigger MD022"
        );

        // Case 3: HR at start of document followed by heading - SHOULD warn
        let content_hr_start = "---\n# Heading";
        let ctx_hr_start = LintContext::new(content_hr_start, crate::config::MarkdownFlavor::Standard, None);
        let warnings_hr_start = rule.check(&ctx_hr_start).unwrap();
        assert!(
            !warnings_hr_start.is_empty(),
            "Heading after HR at document start SHOULD trigger MD022"
        );

        // Case 4: Multiple HRs then heading - SHOULD warn
        let content_multi_hr = "Content\n\n---\n\n---\n# Heading";
        let ctx_multi_hr = LintContext::new(content_multi_hr, crate::config::MarkdownFlavor::Standard, None);
        let warnings_multi_hr = rule.check(&ctx_multi_hr).unwrap();
        assert!(
            !warnings_multi_hr.is_empty(),
            "Heading after multiple HRs without blank line SHOULD trigger MD022"
        );
    }

    #[test]
    fn test_all_hr_styles_require_blank_before_heading() {
        // CommonMark defines HRs as 3+ of -, *, or _ with optional spaces between
        let rule = MD022BlanksAroundHeadings::default();

        // All valid HR styles that should trigger MD022 when followed by heading without blank
        let hr_styles = [
            "---", "***", "___", "- - -", "* * *", "_ _ _", "----", "****", "____", "- - - -",
            "-  -  -", // Multiple spaces between
            "  ---",   // 2 spaces indent (valid per CommonMark)
            "   ---",  // 3 spaces indent (valid per CommonMark)
        ];

        for hr in hr_styles {
            let content = format!("Content\n\n{hr}\n# Heading");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert!(
                !warnings.is_empty(),
                "HR style '{hr}' followed by heading should trigger MD022"
            );
        }
    }

    #[test]
    fn test_setext_heading_after_hr() {
        // Setext headings after HR should also require blank line
        let rule = MD022BlanksAroundHeadings::default();

        // Setext h1 after HR without blank - SHOULD warn
        let content = "Content\n\n---\nHeading\n======";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            !warnings.is_empty(),
            "Setext heading after HR without blank should trigger MD022"
        );

        // Setext h2 after HR without blank - SHOULD warn
        let content_h2 = "Content\n\n---\nHeading\n------";
        let ctx_h2 = LintContext::new(content_h2, crate::config::MarkdownFlavor::Standard, None);
        let warnings_h2 = rule.check(&ctx_h2).unwrap();
        assert!(
            !warnings_h2.is_empty(),
            "Setext h2 after HR without blank should trigger MD022"
        );

        // With blank line - should NOT warn
        let content_ok = "Content\n\n---\n\nHeading\n======";
        let ctx_ok = LintContext::new(content_ok, crate::config::MarkdownFlavor::Standard, None);
        let warnings_ok = rule.check(&ctx_ok).unwrap();
        assert!(
            warnings_ok.is_empty(),
            "Setext heading with blank after HR should not warn"
        );
    }

    #[test]
    fn test_hr_in_code_block_not_treated_as_hr() {
        // HR syntax inside code blocks should be ignored
        let rule = MD022BlanksAroundHeadings::default();

        // HR inside fenced code block - heading after code block needs blank line check
        // but the "---" inside is NOT an HR
        let content = "```\n---\n```\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        // The heading is after a code block fence, not after an HR
        // This tests that we don't confuse code block content with HRs
        assert!(!warnings.is_empty(), "Heading after code block still needs blank line");

        // With blank after code block - should be fine
        let content_ok = "```\n---\n```\n\n# Heading";
        let ctx_ok = LintContext::new(content_ok, crate::config::MarkdownFlavor::Standard, None);
        let warnings_ok = rule.check(&ctx_ok).unwrap();
        assert!(
            warnings_ok.is_empty(),
            "Heading with blank after code block should not warn"
        );
    }

    #[test]
    fn test_hr_in_html_comment_not_treated_as_hr() {
        // HR syntax inside HTML comments should be ignored
        let rule = MD022BlanksAroundHeadings::default();

        // "---" inside HTML comment is NOT an HR
        let content = "<!-- \n---\n -->\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        // HTML comments are transparent, so heading after comment at doc start is OK
        assert!(
            warnings.is_empty(),
            "HR inside HTML comment should be ignored - heading after comment is OK"
        );
    }

    #[test]
    fn test_invalid_hr_not_triggering() {
        // These should NOT be recognized as HRs per CommonMark
        let rule = MD022BlanksAroundHeadings::default();

        let invalid_hrs = [
            "    ---", // 4+ spaces is code block, not HR
            "\t---",   // Tab indent makes it code block
            "--",      // Only 2 dashes
            "**",      // Only 2 asterisks
            "__",      // Only 2 underscores
            "-*-",     // Mixed characters
            "---a",    // Extra character at end
            "a---",    // Extra character at start
        ];

        for invalid in invalid_hrs {
            // These are NOT HRs, so if followed by heading, the heading behavior depends
            // on what the content actually is (code block, paragraph, etc.)
            let content = format!("Content\n\n{invalid}\n# Heading");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            // We're just verifying the HR detection is correct
            // The actual warning behavior depends on what the "invalid HR" is parsed as
            let _ = rule.check(&ctx);
        }
    }

    #[test]
    fn test_frontmatter_vs_horizontal_rule_distinction() {
        // Ensure we correctly distinguish between frontmatter delimiters and standalone HRs
        let rule = MD022BlanksAroundHeadings::default();

        // Frontmatter followed by content, then HR, then heading
        // The HR here is NOT frontmatter, so heading needs blank line
        let content = "---\ntitle: Test\n---\n\nSome content\n\n---\n# Heading after HR";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            !warnings.is_empty(),
            "HR after frontmatter content should still require blank line before heading"
        );

        // Same but with blank line after HR - should be fine
        let content_ok = "---\ntitle: Test\n---\n\nSome content\n\n---\n\n# Heading after HR";
        let ctx_ok = LintContext::new(content_ok, crate::config::MarkdownFlavor::Standard, None);
        let warnings_ok = rule.check(&ctx_ok).unwrap();
        assert!(
            warnings_ok.is_empty(),
            "HR with blank line before heading should not warn"
        );
    }

    // ==================== Kramdown IAL Tests ====================

    #[test]
    fn test_kramdown_ial_after_heading_no_warning() {
        // Issue #259: IAL immediately after heading should not trigger MD022
        let rule = MD022BlanksAroundHeadings::default();
        let content = "## Table of Contents\n{: .hhc-toc-heading}\n\nSome content here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "IAL after heading should not require blank line between them: {warnings:?}"
        );
    }

    #[test]
    fn test_kramdown_ial_with_class() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:.highlight}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(warnings.is_empty(), "IAL with class should be part of heading");
    }

    #[test]
    fn test_kramdown_ial_with_id() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:#custom-id}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(warnings.is_empty(), "IAL with id should be part of heading");
    }

    #[test]
    fn test_kramdown_ial_with_multiple_attributes() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{: .class #id style=\"color: red\"}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "IAL with multiple attributes should be part of heading"
        );
    }

    #[test]
    fn test_kramdown_ial_missing_blank_after() {
        // IAL is part of heading, but blank line is still needed after IAL
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:.class}\nContent without blank.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(
            warnings.len(),
            1,
            "Should warn about missing blank after IAL (part of heading)"
        );
        assert!(warnings[0].message.contains("below"));
    }

    #[test]
    fn test_kramdown_ial_before_heading_transparent() {
        // IAL before heading should be transparent for "blank lines above" check
        let rule = MD022BlanksAroundHeadings::default();
        let content = "Content.\n\n{:.preclass}\n## Heading\n\nMore content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "IAL before heading should be transparent for blank line count"
        );
    }

    #[test]
    fn test_kramdown_ial_setext_heading() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "Heading\n=======\n{:.setext-class}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "IAL after Setext heading should be part of heading"
        );
    }

    #[test]
    fn test_kramdown_ial_fix_preserves_ial() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "Content.\n# Heading\n{:.class}\nMore content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should add blank line above heading and after IAL, but keep IAL attached to heading
        assert!(
            fixed.contains("# Heading\n{:.class}"),
            "IAL should stay attached to heading"
        );
        assert!(fixed.contains("{:.class}\n\nMore"), "Should add blank after IAL");
    }

    #[test]
    fn test_kramdown_ial_fix_does_not_separate() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:.class}\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Fix should NOT insert blank line between heading and IAL
        assert!(
            !fixed.contains("# Heading\n\n{:.class}"),
            "Should not add blank between heading and IAL"
        );
        assert!(fixed.contains("# Heading\n{:.class}"), "IAL should remain attached");
    }

    #[test]
    fn test_kramdown_multiple_ial_lines() {
        // Edge case: multiple IAL lines (unusual but valid)
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:.class1}\n{:#id}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Note: Kramdown only attaches one IAL, but we treat consecutive ones as all attached
        // to avoid false positives
        assert!(
            warnings.is_empty(),
            "Multiple consecutive IALs should be part of heading"
        );
    }

    #[test]
    fn test_kramdown_ial_with_blank_line_not_attached() {
        // If there's a blank line between heading and IAL, they're not attached
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n\n{:.class}\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // The IAL here is NOT attached to the heading (blank line separates them)
        // So this should NOT trigger a warning for missing blank below heading
        // The IAL is just a standalone block-level element
        assert!(warnings.is_empty(), "Blank line separates heading from IAL");
    }

    #[test]
    fn test_not_kramdown_ial_regular_braces() {
        // Regular braces that don't match IAL pattern
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{not an ial}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // {not an ial} is not IAL syntax, so it should be regular content
        assert_eq!(
            warnings.len(),
            1,
            "Non-IAL braces should be regular content requiring blank"
        );
    }

    #[test]
    fn test_kramdown_ial_at_document_end() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:.class}";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // No content after IAL, so no blank line needed
        assert!(warnings.is_empty(), "IAL at document end needs no blank after");
    }

    #[test]
    fn test_kramdown_ial_followed_by_code_fence() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:.class}\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Code fence is special - no blank required before it
        assert!(warnings.is_empty(), "No blank needed between IAL and code fence");
    }

    #[test]
    fn test_kramdown_ial_followed_by_list() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:.class}\n- List item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // List is special - no blank required before it
        assert!(warnings.is_empty(), "No blank needed between IAL and list");
    }

    #[test]
    fn test_kramdown_ial_fix_idempotent() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:.class}\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed_once = rule.fix(&ctx).unwrap();
        let ctx2 = LintContext::new(&fixed_once, crate::config::MarkdownFlavor::Standard, None);
        let fixed_twice = rule.fix(&ctx2).unwrap();

        assert_eq!(fixed_once, fixed_twice, "Fix should be idempotent");
    }

    #[test]
    fn test_kramdown_ial_whitespace_line_between_not_attached() {
        // A whitespace-only line (not truly blank) between heading and IAL
        // means the IAL is NOT attached to the heading
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n   \n{:.class}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Whitespace-only line is treated as blank, so IAL is NOT attached
        // The warning should be about the line after heading (whitespace line)
        // since {:.class} starts a new block
        assert!(
            warnings.is_empty(),
            "Whitespace between heading and IAL means IAL is not attached"
        );
    }

    #[test]
    fn test_kramdown_ial_html_comment_between() {
        // HTML comment between heading and IAL means IAL is NOT attached to heading
        // IAL must immediately follow the element it modifies
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n<!-- comment -->\n{:.class}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // The comment line is the separation the rule asks for, exactly as the
        // whitespace-only line above it is, so the detached IAL is not reported (#866)
        assert!(
            warnings.is_empty(),
            "A comment-only line below the heading is its blank line: {warnings:?}"
        );
    }

    #[test]
    fn test_kramdown_ial_text_beside_comment_between_is_still_reported() {
        // Control for the test above: the comment does not have the line to itself,
        // so what follows the heading is prose and the blank line really is missing
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\ntext <!-- comment -->\n{:.class}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 1, "Heading followed by prose: {warnings:?}");
    }

    #[test]
    fn test_kramdown_ial_generic_attribute() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:data-toc=\"true\" style=\"color: red\"}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(warnings.is_empty(), "Generic attributes should be recognized as IAL");
    }

    #[test]
    fn test_kramdown_ial_fix_multiple_lines_preserves_all() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:.class1}\n{:#id}\n{:data-x=\"y\"}\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // All IAL lines should be preserved
        assert!(fixed.contains("{:.class1}"), "First IAL should be preserved");
        assert!(fixed.contains("{:#id}"), "Second IAL should be preserved");
        assert!(fixed.contains("{:data-x=\"y\"}"), "Third IAL should be preserved");
        // Blank line should be after all IALs, before content
        assert!(
            fixed.contains("{:data-x=\"y\"}\n\nContent"),
            "Blank line should be after all IALs"
        );
    }

    #[test]
    fn test_kramdown_ial_crlf_line_endings() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\r\n{:.class}\r\n\r\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(warnings.is_empty(), "CRLF should work correctly with IAL");
    }

    #[test]
    fn test_kramdown_ial_invalid_patterns_not_recognized() {
        let rule = MD022BlanksAroundHeadings::default();

        // Space before colon - not valid IAL
        let content = "# Heading\n{ :.class}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Invalid IAL syntax should trigger warning");

        // Missing colon entirely
        let content2 = "# Heading\n{.class}\n\nContent.";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let warnings2 = rule.check(&ctx2).unwrap();
        // {.class} IS valid kramdown syntax (starts with .)
        assert!(warnings2.is_empty(), "{{.class}} is valid kramdown block attribute");

        // Just text in braces
        let content3 = "# Heading\n{just text}\n\nContent.";
        let ctx3 = LintContext::new(content3, crate::config::MarkdownFlavor::Standard, None);
        let warnings3 = rule.check(&ctx3).unwrap();
        assert_eq!(
            warnings3.len(),
            1,
            "Text in braces is not IAL and should trigger warning"
        );
    }

    #[test]
    fn test_kramdown_ial_toc_marker() {
        // {:toc} is a special kramdown table of contents marker
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n{:toc}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // {:toc} starts with {: so it's recognized as IAL
        assert!(warnings.is_empty(), "{{:toc}} should be recognized as IAL");
    }

    #[test]
    fn test_kramdown_ial_mixed_headings_in_document() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = r#"# ATX Heading
{:.atx-class}

Content after ATX.

Setext Heading
--------------
{:#setext-id}

Content after Setext.

## Another ATX
{:.another}

More content."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "Mixed headings with IAL should all work: {warnings:?}"
        );
    }

    #[test]
    fn test_kramdown_extension_block_before_first_heading_is_document_start() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "{::comment}\nhidden\n{:/comment}\n# Heading\n\nBody\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Kramdown, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "Kramdown extension preamble should not require blank above first heading: {warnings:?}"
        );
    }

    #[test]
    fn test_kramdown_ial_before_first_heading_is_document_start() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "{:.doc-class}\n# Heading\n\nBody\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Kramdown, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "Kramdown IAL preamble should not require blank above first heading: {warnings:?}"
        );
    }

    // ==================== Quarto Flavor Tests ====================

    #[test]
    fn test_quarto_div_marker_transparent_above_heading() {
        // Quarto div markers should be transparent for blank line counting
        // The blank line before the div opening should count toward the heading
        let rule = MD022BlanksAroundHeadings::default();
        // Content ends, blank, div opens, blank counts through div marker, heading
        let content = "Content\n\n::: {.callout-note}\n# Heading\n\nMore content\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        // The blank line before div opening should count as separation for heading
        assert!(
            warnings.is_empty(),
            "Quarto div marker should be transparent above heading: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_div_marker_transparent_below_heading() {
        // Quarto div opening marker should be transparent for blank line counting below heading
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading\n\n::: {.callout-note}\nContent\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        // The blank line after heading should count, and ::: should be transparent
        assert!(
            warnings.is_empty(),
            "Quarto div marker should be transparent below heading: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_heading_inside_callout() {
        // Heading inside Quarto callout should work normally
        let rule = MD022BlanksAroundHeadings::default();
        let content = "::: {.callout-note}\n\n## Note Title\n\nNote content\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Heading inside Quarto callout should have no warnings: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_heading_at_start_after_div_open() {
        // Heading immediately after div open counts as being at document start
        // because div marker is transparent for "first heading" detection
        let rule = MD022BlanksAroundHeadings::default();
        // This is the first heading in the document (div marker is transparent)
        let content = "::: {.callout-warning}\n# Warning\n\nContent\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        // The heading is at document start (after transparent div marker)
        // BUT the default config has allowed_at_start = true, AND there's content inside the div
        // that needs blank line below the heading. Let's check what we get.
        // Actually, the heading needs a blank below (before "Content"), so let's fix the test.
        // For this test, we want to verify the "above" requirement works with div marker transparency.
        assert!(
            warnings.is_empty(),
            "Heading at start after div open should pass: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_heading_before_div_close() {
        // Heading immediately before div close: the div close is at end of doc, so no blank needed after
        let rule = MD022BlanksAroundHeadings::default();
        let content = "::: {.callout-note}\nIntro\n\n## Section\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        // The div closing marker is transparent, and at end of document there's nothing after it
        // So technically the heading is at the end (nothing follows the div close).
        // We need to check if the transparent marker logic works for end-of-document.
        assert!(
            warnings.is_empty(),
            "Heading before div close should pass: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_div_markers_not_transparent_in_standard_flavor() {
        // In standard flavor, ::: is regular text and breaks blank line sequences
        let rule = MD022BlanksAroundHeadings::default();
        let content = "Content\n\n:::\n# Heading\n\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        // In standard flavor, the ::: is just text. So there's no blank between ::: and heading.
        assert!(
            !warnings.is_empty(),
            "Standard flavor should not treat ::: as transparent: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_nested_divs_with_heading() {
        // Nested Quarto divs with heading inside
        let rule = MD022BlanksAroundHeadings::default();
        let content = "::: {.outer}\n::: {.inner}\n\n# Heading\n\nContent\n:::\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Nested divs with heading should work: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_fix_preserves_div_markers() {
        // Fix should preserve Quarto div markers
        let rule = MD022BlanksAroundHeadings::default();
        let content = "::: {.callout-note}\n\n## Note\n\nContent\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Should preserve all the div markers
        assert!(fixed.contains("::: {.callout-note}"), "Should preserve div opening");
        assert!(fixed.contains(":::"), "Should preserve div closing");
        assert!(fixed.contains("## Note"), "Should preserve heading");
    }

    #[test]
    fn test_quarto_heading_needs_blank_without_div_transparency() {
        // Without a blank line, heading after content should warn even with div marker between
        // This tests that blank lines are still required, div markers just don't "reset" the count
        let rule = MD022BlanksAroundHeadings::default();
        // Content directly followed by div opening, then heading - should warn
        let content = "Content\n::: {.callout-note}\n# Heading\n\nMore\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        // The div marker is transparent, so we look through it.
        // "Content" followed by heading with only a div marker in between - no blank!
        assert!(
            !warnings.is_empty(),
            "Should still require blank line when not present: {warnings:?}"
        );
    }

    #[test]
    fn test_pandoc_div_marker_transparent_above_heading() {
        // Pandoc div marker should be transparent for blank line counting above heading,
        // mirroring the Quarto behavior tested in test_quarto_div_marker_transparent_above_heading.
        let rule = MD022BlanksAroundHeadings::default();
        let content = "Content\n\n::: {.callout-note}\n# Heading\n\nMore content\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Pandoc, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "MD022 should treat Pandoc div marker as transparent above heading: {warnings:?}"
        );
    }

    #[test]
    fn test_hugo_block_attribute_after_heading_not_flagged() {
        // Issue #756: a Goldmark/Hugo block attribute list directly under a heading
        // describes that heading, so MD022 must not require a blank between them.
        let rule = MD022BlanksAroundHeadings::default();
        let content = "Intro text.\n\n# Heading\n{class=\"anchor\"}\n\nContent.\n";

        for flavor in [
            crate::config::MarkdownFlavor::Hugo,
            crate::config::MarkdownFlavor::MkDocs,
            crate::config::MarkdownFlavor::Kramdown,
        ] {
            let ctx = LintContext::new(content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert!(
                warnings.is_empty(),
                "MD022 should not flag the block attribute line under {flavor:?}: {warnings:?}"
            );
        }

        // Negative control: in Standard `{class="a"}` is literal text, so the heading
        // genuinely has no blank line below it.
        let ctx_std = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings_std = rule.check(&ctx_std).unwrap();
        assert!(
            warnings_std.iter().any(|w| w.message.contains("below heading")),
            "MD022 must flag the missing blank below the heading under Standard: {warnings_std:?}"
        );
    }

    #[test]
    fn test_mdg_keeps_tags_attached_only_to_structure_headings() {
        let rule = MD022BlanksAroundHeadings::default();

        // A `Keyword: name` heading is a Gherkin structure, so its tag line stays attached.
        let attached = "`@browser`\n`@checkout` `@smoke`\n# Feature: Checkout\n";
        let mdg_ctx = LintContext::new(attached, crate::config::MarkdownFlavor::MDG, None);
        assert!(rule.check(&mdg_ctx).unwrap().is_empty());
        let fixed = rule.fix(&mdg_ctx).unwrap();
        assert_eq!(fixed, attached);
        let fixed_ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::MDG, None);
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed, "MDG fix should be idempotent");

        let standard_ctx = LintContext::new(attached, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            rule.check(&standard_ctx)
                .unwrap()
                .iter()
                .any(|warning| warning.message.contains("above heading"))
        );
    }

    #[test]
    fn test_mdg_requires_blank_line_above_a_non_structure_heading() {
        // Without a colon the heading is ordinary prose, so the exemption must
        // not apply even though the line above looks like a tag line.
        let rule = MD022BlanksAroundHeadings::default();
        let content = "`@browser`\n# Notes\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);

        assert!(
            rule.check(&ctx)
                .unwrap()
                .iter()
                .any(|warning| warning.message.contains("above heading")),
            "a non-Gherkin heading keeps the normal requirement"
        );
        assert_eq!(rule.fix(&ctx).unwrap(), "`@browser`\n\n# Notes\n");
    }

    #[test]
    fn test_mdg_colon_inside_a_code_span_names_no_structure() {
        // A keyword is a plain dialect term, so a colon behind a backtick sits
        // inside a code span and names nothing. MD063 already read it that way;
        // MD022 now shares the split so the two cannot drift apart.
        let rule = MD022BlanksAroundHeadings::default();
        let content = "`@browser`\n# See `x: y` Notes\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);

        assert!(
            rule.check(&ctx)
                .unwrap()
                .iter()
                .any(|warning| warning.message.contains("above heading")),
            "the code span holds the only colon, so the heading is ordinary prose"
        );
        assert_eq!(rule.fix(&ctx).unwrap(), "`@browser`\n\n# See `x: y` Notes\n");

        // A colon outside the span still names a structure, backtick or not.
        let structure = "`@browser`\n# Scenario: use `a: b` here\n";
        let structure_ctx = LintContext::new(structure, crate::config::MarkdownFlavor::MDG, None);
        assert!(rule.check(&structure_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&structure_ctx).unwrap(), structure);
    }

    #[test]
    fn test_mdg_tag_line_matches_gherkin_reference_scan() {
        // The reference matcher scans for wrapped tags anywhere on the line,
        // including the comment-bearing examples in Cucumber's own fixture.
        let rule = MD022BlanksAroundHeadings::default();

        for above in [
            "`@comment_tag1` #a comment",
            "`@comment_tag#2` #a comment",
            "`@browser` and prose",
            "prose `@browser`",
            "`@a b`",
        ] {
            let content = format!("{above}\n# Feature: Checkout\n");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::MDG, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{above:?} is a Gherkin tag line");
            assert_eq!(rule.fix(&ctx).unwrap(), content);
        }

        let prose = "plain prose\n# Feature: Checkout\n";
        let ctx = LintContext::new(prose, crate::config::MarkdownFlavor::MDG, None);
        assert!(
            rule.check(&ctx)
                .unwrap()
                .iter()
                .any(|warning| warning.message.contains("above heading"))
        );
    }

    #[test]
    fn test_fix_keeps_a_setext_heading_suppressed_on_a_later_line_as_written() {
        // A suppression on any line of a setext heading drops its warning, and
        // the rewrite goes with it. The heading's paragraph opens directly
        // below the ATX heading, so the blank line written there belongs to
        // that heading, and nothing is written below the underline.
        let rule = MD022BlanksAroundHeadings::default();
        let content = "Intro paragraph.\n# Heading one\nText after.\nTitle\nsecond <!-- rumdl-disable-line MD022 -->\n===\nMore text.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "Intro paragraph.\n\n# Heading one\n\nText after.\nTitle\nsecond <!-- rumdl-disable-line MD022 -->\n===\nMore text.\n"
        );
    }
}

/// A heading written directly under a list marker that has nothing else on its
/// line is the first thing in the item. A blank line between the two would leave
/// the item empty and put the heading outside it.
#[cfg(test)]
mod list_item_tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    /// The document after the rule's fix, and after applying the fixes of its
    /// warnings: the two paths must agree.
    fn fixed(content: &str) -> String {
        let rule = MD022BlanksAroundHeadings::default();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let by_fix = rule.fix(&ctx).unwrap();
        let warnings = rule.check(&ctx).unwrap();
        let by_warnings = crate::utils::fix_utils::apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(by_fix, by_warnings, "{content:?}: fix and warnings disagree");
        by_fix
    }

    #[test]
    fn a_heading_under_a_bare_marker_gets_no_blank_line_above() {
        for (content, expected) in [
            ("-\n  ## a\n  text\n", "-\n  ## a\n\n  text\n"),
            ("1.\n   ## a\n   text\n", "1.\n   ## a\n\n   text\n"),
            ("-   \n  ## a\n  text\n", "-   \n  ## a\n\n  text\n"),
            ("-\n  ##  a\n  text\n", "-\n  ##  a\n\n  text\n"),
            ("-\n  -\n    ## a\n    text\n", "-\n  -\n    ## a\n\n    text\n"),
            ("-\n  a\n  ---\n  text\n", "-\n  a\n  ---\n\n  text\n"),
            ("-\n  a\n  b\n  ===\n  text\n", "-\n  a\n  b\n  ===\n\n  text\n"),
            ("1.\n   ## a\n", "1.\n   ## a\n"),
            // The innermost marker on the line is the bare one.
            ("- -\n    ## a\n    text\n", "- -\n    ## a\n\n    text\n"),
            ("- -\n    a\n    ---\n    text\n", "- -\n    a\n    ---\n\n    text\n"),
            ("1. -\n     ## a\n", "1. -\n     ## a\n"),
            ("- 1.\n     ## a\n", "- 1.\n     ## a\n"),
        ] {
            assert_eq!(fixed(content), expected, "{content:?}");
            assert_eq!(fixed(expected), expected, "{expected:?} is not stable");
        }
    }

    #[test]
    fn a_blank_line_is_still_added_where_it_changes_no_container() {
        for (content, expected) in [
            // Under item text the heading is already inside the item.
            ("- x\n  ## a\n  text\n", "- x\n\n  ## a\n\n  text\n"),
            // Indented one column, the heading is outside the empty item.
            ("-\n ## a\n", "-\n\n ## a\n"),
            ("Text\n## a\n", "Text\n\n## a\n"),
            ("1. x\n   -\n\n   ## a\n", "1. x\n   -\n\n   ## a\n"),
        ] {
            assert_eq!(fixed(content), expected, "{content:?}");
        }
    }

    #[test]
    fn check_does_not_ask_for_a_blank_line_under_a_bare_marker() {
        let rule = MD022BlanksAroundHeadings::default();
        for content in ["-\n  ## a\n", "1.\n   ## a\n", "-\n  -\n    ## a\n", "-\n  a\n  ---\n"] {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert!(warnings.is_empty(), "{content:?}: {warnings:?}");
        }
        let ctx = LintContext::new("-\n ## a\n", MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
    }
}

#[cfg(test)]
mod exact_spacing_tests {
    use super::*;
    use crate::{config::MarkdownFlavor, lint_context::LintContext, utils::fix_utils::apply_warning_fixes};

    #[test]
    fn diagnostic_edits_match_full_fix_without_deleting_comments() {
        let rule = MD022BlanksAroundHeadings::from_config_struct(MD022Config {
            lines_above_mode: HeadingBlankMode::Exact,
            lines_below: md022_config::HeadingLevelConfig::scalar(0),
            ..Default::default()
        });
        let input = "# Title\n\n\n\n<!-- first -->\n<!-- second -->\n## Section\n";
        let expected = "# Title\n\n<!-- first -->\n<!-- second -->\n## Section\n";
        let ctx = LintContext::new(input, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 7);
        assert_eq!(warnings[0].fix.as_ref().unwrap().additional_edits.len(), 1);
        assert_eq!(apply_warning_fixes(input, &warnings).unwrap(), expected);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
        let ctx = LintContext::new(expected, MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty());
    }
}
