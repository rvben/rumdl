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

/// The parser options for parsing `content`.
///
/// [`rumdl_parser_options`] with the heading attribute extension left out of
/// text where it corrupts the parser's tree (see [`has_setext_attribute_line`]).
/// Every parse of document text takes its options from here.
pub fn rumdl_parser_options_for(content: &str) -> Options {
    let mut options = rumdl_parser_options();
    if has_setext_attribute_line(content) {
        options.remove(Options::ENABLE_HEADING_ATTRIBUTES);
    }
    options
}

/// Whether `content` has a line that is nothing but a `{...}` block directly
/// followed by a setext underline, once block quote markers and indentation are
/// set aside.
///
/// With heading attributes enabled, pulldown-cmark cuts the attribute block off
/// the end of a setext heading's text. When that block is a whole line inside a
/// block quote or list item, the cut lands on the wrong bytes: the tree is left
/// corrupt, and the parse panics either immediately (debug builds) or when the
/// events are read (release builds). Without the extension the same lines are an
/// ordinary setext heading. The test over-approximates the failing shapes (it
/// also holds at top level, where the parser copes), so that no container,
/// indentation or tab arrangement of the failing shape is missed; the price of a
/// false positive is that such a document reads `{...}` as text.
fn has_setext_attribute_line(content: &str) -> bool {
    if !content.contains('{') {
        return false;
    }
    // A line ends at `\n`, `\r\n` or a lone `\r`.
    let lines = content
        .split('\n')
        .flat_map(|line| line.strip_suffix('\r').unwrap_or(line).split('\r'));
    let mut previous_is_attribute_block = false;
    for line in lines {
        let text = line
            .trim_start_matches([' ', '\t', '\x0b', '\x0c', '>'])
            .trim_end_matches([' ', '\t', '\x0b', '\x0c']);
        let is_underline = !text.is_empty() && (text.bytes().all(|b| b == b'=') || text.bytes().all(|b| b == b'-'));
        if previous_is_attribute_block && is_underline {
            return true;
        }
        previous_is_attribute_block = text.starts_with('{') && text.ends_with('}');
    }
    false
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
    let mut options = rumdl_parser_options_for(content);
    options.remove(Options::ENABLE_MATH);
    pulldown_cmark::Parser::new_ext(content, options)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            matches!(event, pulldown_cmark::Event::Code(_)).then_some((range.start, range.end))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Parser;

    /// Drive a parse to its end the way a consumer does.
    fn parse_fully(content: &str) -> usize {
        Parser::new_ext(content, rumdl_parser_options_for(content))
            .into_offset_iter()
            .count()
    }

    #[test]
    fn parser_survives_a_setext_heading_whose_last_line_is_an_attribute_block() {
        for content in [
            ">o\n>  {}\n>=\n",
            ">o\n>      {}\n>=\n",
            ">o\n>    {}\n>-\n",
            "- o\n   {}\n  =\n",
            "1. o\n     {}\n   =\n",
            ">>o\n>>      {}\n>>=\n",
            "> - o\n>    {}\n>   =\n",
            ">o\n>  \u{c}{}\n>=\n",
            ">o\n>  \u{b}{}\n>=\n",
            ">o\r\n>  {}\r\n>=\r\n",
            ">o\r>  {}\r>=\r",
        ] {
            assert!(parse_fully(content) > 0, "{content:?}");
        }
    }

    #[test]
    fn parser_survives_every_short_combination_of_container_prefixes_and_attribute_lines() {
        let prefixes = ["", ">", "> ", ">  ", ">    ", "- ", "   ", "\t"];
        let bodies = ["o", "{}", "{#a}", "x {}", "=", "-", "---", "", "{ }"];
        let lines: Vec<String> = prefixes
            .iter()
            .flat_map(|p| bodies.iter().map(move |b| format!("{p}{b}")))
            .collect();
        let mut parsed = 0usize;
        for a in &lines {
            for b in &lines {
                parse_fully(&format!("{a}\n{b}\n"));
                for c in &lines {
                    parse_fully(&format!("{a}\n{b}\n{c}\n"));
                    parsed += 1;
                }
            }
        }
        assert!(parsed > 100_000, "corpus too small: {parsed}");
    }

    #[test]
    fn setext_attribute_line_detects_the_shapes_that_corrupt_the_parse() {
        for content in [
            ">o\n>  {}\n>=\n",
            "- o\n   {}\n  =\n",
            ">o\n>  \u{c}{}\n>=\n",
            ">o\n>  {}\u{b}\n>-\n",
            ">o\r\n>  {}\r\n>=\r\n",
            ">o\r>  {}\r>=\r",
            "> > o\n> >   {#a .b}\n> > ---\n",
        ] {
            assert!(has_setext_attribute_line(content), "{content:?}");
            assert!(
                !rumdl_parser_options_for(content).contains(Options::ENABLE_HEADING_ATTRIBUTES),
                "{content:?}"
            );
        }
    }

    #[test]
    fn setext_attribute_line_leaves_other_text_with_attributes_enabled() {
        for content in [
            "# Heading {#id}\n",
            "Title {#id}\n=====\n",
            ">o\n>    x{}\n>=\n",
            "{}\n\n---\n",
            "{\"a\": 1}\n",
            "plain text\n",
            "{}\nnot an underline\n",
            "{}\n",
        ] {
            assert!(!has_setext_attribute_line(content), "{content:?}");
            assert!(
                rumdl_parser_options_for(content).contains(Options::ENABLE_HEADING_ATTRIBUTES),
                "{content:?}"
            );
        }
    }

    /// Every parse of document text goes through `rumdl_parser_options_for`, so no
    /// call site can hand the parser text it cannot survive.
    #[test]
    fn production_code_derives_parser_options_from_the_text_it_parses() {
        fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    rust_files(&path, out);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    out.push(path);
                }
            }
        }

        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        assert!(files.len() > 100, "source scan found only {} files", files.len());

        let mut offenders = Vec::new();
        for path in files {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name == "parser_options.rs" || name == "tests.rs" || name.ends_with("_tests.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
            for (number, line) in production.lines().enumerate() {
                let code = line.split("//").next().unwrap_or(line);
                if code.contains("rumdl_parser_options()") {
                    offenders.push(format!("{}:{}", path.display(), number + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "use rumdl_parser_options_for(<text>) instead of rumdl_parser_options() at: {offenders:#?}"
        );
    }
}
