use pulldown_cmark::Options;

/// Standard pulldown-cmark options for rumdl parsing.
///
/// Uses an explicit allowlist rather than `Options::all()` to prevent
/// future pulldown-cmark releases from silently changing parse behavior.
///
/// Notably excludes `ENABLE_YAML_STYLE_METADATA_BLOCKS` and
/// `ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS` because rumdl handles
/// front matter detection independently. These options cause pulldown-cmark
/// to misinterpret `---` horizontal rules as metadata delimiters,
/// corrupting code block detection across the entire document.
pub fn rumdl_parser_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options.insert(Options::ENABLE_MATH);
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_DEFINITION_LIST);
    options.insert(Options::ENABLE_SUPERSCRIPT);
    options.insert(Options::ENABLE_SUBSCRIPT);
    options.insert(Options::ENABLE_WIKILINKS);
    options
}

/// The byte ranges of the code spans in `content` as a reader without math
/// finds them, sorted by start.
///
/// rumdl reads `$...$` and `$$...$$` as math, and a backtick inside math opens
/// no code span. CommonMark has no math, and there that backtick opens a span
/// closing at the next backtick run of its length, inside the math or past it.
/// A rewrite that has to keep a code span's text as written needs both
/// readings, so this one comes from the same parse with only math left out.
pub fn code_span_ranges_without_math(content: &str) -> Vec<(usize, usize)> {
    if !content.contains('`') {
        return Vec::new();
    }
    let mut options = rumdl_parser_options();
    options.remove(Options::ENABLE_MATH);
    pulldown_cmark::Parser::new_ext(content, options)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            matches!(event, pulldown_cmark::Event::Code(_)).then_some((range.start, range.end))
        })
        .collect()
}
