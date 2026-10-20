use super::*;
use crate::lint_context::LintContext;
use crate::utils::range_utils::byte_to_char_count;
use crate::utils::skip_context::{is_in_inline_html_code, is_in_math_context, is_in_mkdocs_markup};

/// Check if a byte position is within a link (inline links, reference links, or reference definitions)
fn legacy_is_in_link(ctx: &crate::lint_context::LintContext, byte_pos: usize) -> bool {
    if ctx.is_range_in_rendered_link(&(byte_pos..byte_pos + 1))
        || ctx
            .images()
            .iter()
            .any(|image| image.byte_offset <= byte_pos && byte_pos < image.byte_end)
    {
        return true;
    }

    // Check reference definitions [ref]: url "title" using pre-computed data (O(1) vs O(n))
    ctx.is_in_reference_def(byte_pos)
}

// Frozen warning-filter oracle. Line-level detection is tested independently.
fn legacy_check(rule: &MD037NoSpaceInEmphasis, ctx: &crate::lint_context::LintContext) -> Vec<LintWarning> {
    let _timer = crate::profiling::ScopedTimer::new("MD037_check");
    let has_html = ctx.has_char('<');

    // Early return: if no emphasis markers at all, skip processing
    if !ctx.has_char('*') && !ctx.has_char('_') {
        return vec![];
    }

    // Source-location queries preserve correct offsets across all line ending types.

    let mut warnings = Vec::new();

    // Process content lines, automatically skipping front matter, code blocks, math blocks,
    // and Obsidian comments (when in Obsidian flavor)
    // Math blocks contain LaTeX syntax where _ and * have special meaning
    for syntax in ctx.lexical_emphasis() {
        let line_num = syntax.line_num;
        let line_content = syntax.source();
        if ctx.lines[line_num - 1].in_table_block {
            // Each table cell is its own inline context, so scan cells separately.
            // A marker in one cell can never pair with a marker in the next, and a
            // cell holds inline content only (never a list item), so the line-level
            // list-marker handling does not apply here.
            for cell in table_cell_ranges(line_content, ctx.flavor) {
                let Some(cell_content) = line_content.get(cell.clone()) else {
                    continue;
                };
                if !cell_content.contains('*') && !cell_content.contains('_') {
                    continue;
                }
                if has_doc_patterns(cell_content) {
                    continue;
                }
                rule.check_line_content_for_emphasis_fast(cell_content, line_num, cell.start, &mut warnings);
            }
            continue;
        }

        // Check for emphasis issues on the original line
        rule.check_line_for_emphasis_issues_fast(syntax, &mut warnings);
    }

    // Filter out warnings for emphasis markers that are inside links, HTML comments, math, or MkDocs markup
    let mut filtered_warnings = Vec::new();
    let lines = ctx.raw_lines();
    let span_ends = if warnings.is_empty() {
        Vec::new()
    } else {
        MD037NoSpaceInEmphasis::emphasis_span_ends(ctx)
    };

    for warning in &warnings {
        let line_num = warning.line;
        let Some(line) = line_num.checked_sub(1).and_then(|index| lines.get(index)) else {
            continue;
        };
        let line_start_pos = ctx.line_start_byte(line_num).unwrap_or(0);
        // `warning.column` is a byte offset within the line at this stage.
        // Derive the byte position (for byte-based skip checks) and the
        // character column (for char-based checks and the emitted warning).
        let byte_pos = line_start_pos + (warning.column - 1);
        // Calculate byte position within the line (0-indexed)
        let line_pos = warning.column - 1;
        let char_col = byte_to_char_count(line, warning.column - 1);

        // Skip if inside links, HTML comments, math contexts, code spans, MDX constructs, or MkDocs markup
        // Note: is_in_code_span uses pulldown-cmark and correctly handles multi-line spans
        // Pandoc bracketed spans `[text]{.class}` may contain spaced
        // emphasis markers as literal content; suppress MD037 there.
        // Subscripts/superscripts cannot contain whitespace per the
        // detector grammar, so MD037's spaced-emphasis warnings can
        // never land inside one.
        // A shortcode tag is an argument list read by a template, so
        // `*` and `_` in it are literal characters the template sees;
        // closing the spaces around them changes what it receives.
        let in_pandoc_construct = ctx.flavor.is_pandoc_compatible() && ctx.is_in_bracketed_span(byte_pos);
        let byte_end = line_start_pos + (warning.end_column - 1);
        let closing_pos = byte_end.saturating_sub(1);
        let marker_in_html = has_html
            && [byte_pos, closing_pos]
                .into_iter()
                .any(|pos| ctx.is_in_html_comment(pos) || ctx.is_in_html_tag(pos) || ctx.is_in_jsx_component_tag(pos));
        if !in_pandoc_construct
                && !marker_in_html
                && !ctx.is_in_jinja_string(byte_pos)
                && !ctx.is_in_jinja_string(closing_pos)
                && !MD037NoSpaceInEmphasis::closes_earlier_emphasis(&span_ends, byte_pos, byte_end)
                && !legacy_is_in_link(ctx, byte_pos)
                && !ctx.is_in_shortcode(byte_pos)
                && !ctx.is_in_shortcode(closing_pos)
                && !is_in_math_context(ctx, byte_pos)
                && !ctx.is_in_code_span(line_num, char_col)
                // A marker on the opening line of a multiline code span
                // can appear to close emphasis that started outside code.
                && !ctx.is_in_code_span_byte(closing_pos)
                && !is_in_inline_html_code(line, line_pos)
                && !ctx.overlaps_mdx_inline_code(byte_pos, byte_end)
                && !is_in_mkdocs_markup(line, line_pos, ctx.flavor)
                && !ctx.is_position_in_obsidian_comment(line_num, char_col)
        {
            let mut adjusted_warning = warning.clone();
            // Emit character-based columns (byte-based skip checks done above).
            adjusted_warning.column = char_col;
            adjusted_warning.end_column = byte_to_char_count(line, warning.end_column - 1);
            filtered_warnings.push(adjusted_warning);
        }
    }

    filtered_warnings
}

#[test]
fn cached_warning_filter_matches_scalar_for_all_flavors() {
    for padding in [0, 40] {
        let prefix = "é日本🙂e\u{301}\t".repeat(padding);
        let source = format!(
            "{prefix}* bad * _ other _ `é日本🙂 * code *` <code>_ literal _</code> * last *\n\n\
             {prefix}<CODE title='>'>* odd *</CODE> <codeword>* visible *</codeword>\n\n\
             {prefix}[* link *](target) ![_ image _](image.png) [* unresolved *][missing]\n\n\
             {prefix}*foo * bar* _a *b* c_ * outer _ nested _ end *\n\n\
             {prefix}$é_日$ * visible * $$ _ math _ $$ :icon_with_underscore: _ shown _\n\n\
             {prefix}^^* insert *^^ ^_ super _^ ==* mark *== ++_ key _++ _ after _\n\n\
             {prefix}%% * hidden * %% _ shown _ <!-- * hidden * --> * visible *\n\n\
             {prefix}`code\n* literal *\nend` * visible *\n\n\
             {prefix}*a\nline* * visible *\n\n\
             | a | b |\n| --- | --- |\n| * spaced * | _ other _ |\n\n\
             <Card title=\"* attr *\">{{/* _ comment _ */}} * text *</Card>\n\n\
             {{{{< note title=\"* literal *\" >}}}} * visible *\n\n\
             [ref]: target \"* title *\"\n\n* eof *"
        );
        for flavor in crate::format_proof::ALL_FLAVORS {
            for newline in ["\n", "\r\n"] {
                let content = source.replace('\n', newline);
                let ctx = LintContext::new(&content, flavor, None);
                assert_eq!(
                    MD037NoSpaceInEmphasis.check(&ctx).unwrap(),
                    legacy_check(&MD037NoSpaceInEmphasis, &ctx),
                    "{flavor:?}, padding={padding}, newline={newline:?}",
                );
            }
        }
    }
}

#[test]
fn nested_valid_ranges_suppress_only_contained_spacing() {
    for source in [
        "*foo * bar* _ spaced _",
        "_foo _ bar_ * spaced *",
        "*outer _inside _ text_ end* _ spaced _",
        "_outer *inside * text* end_ * spaced *",
    ] {
        let ctx = LintContext::new(source, MarkdownFlavor::Standard, None);
        let warnings = MD037NoSpaceInEmphasis.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{source:?}: {warnings:?}");
        assert_eq!(warnings[0].column, source.rfind(" spaced ").unwrap());
    }
}
