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
