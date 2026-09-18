//! Fix mode logic for Rust doc comment linting.
//!
//! Handles applying markdown fixes to doc comment blocks while preserving
//! the `///` or `//!` prefixes and original indentation.

use rumdl_lib::config as rumdl_config;
use rumdl_lib::doc_comment_lint::{DocCommentBlock, DocCommentKind, SKIPPED_RULES, extract_doc_comment_blocks};
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::md013_line_length::MD013LineLength;

/// Apply markdown fixes to all doc comment blocks in a Rust source file.
///
/// Processes blocks in reverse order to maintain byte offsets. For each block:
///
/// 1. Lint the extracted markdown
/// 2. Apply fixes via the fix coordinator
/// 3. Restore doc comment prefixes
/// 4. Replace the original block in the content
///
/// Returns the number of blocks that were formatted.
pub fn format_doc_comment_blocks(
    content: &mut String,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
) -> usize {
    if rumdl_lib::merge_conflict::detect_for_rules(content, rules, config, None).is_some() {
        return 0;
    }
    let blocks = extract_doc_comment_blocks(content);

    if blocks.is_empty() {
        return 0;
    }

    let mut formatted_count = 0;

    // Process in reverse order to maintain byte offsets
    for block in blocks.into_iter().rev() {
        if block.markdown.trim().is_empty() {
            continue;
        }

        // Filter out skipped rules and apply doc-comment config overrides
        let block_rules: Vec<Box<dyn Rule>> = rules
            .iter()
            .filter(|rule| !SKIPPED_RULES.contains(&rule.name()))
            .map(|r| {
                // Disable code block checking for MD013 in doc comments.
                // Code blocks contain Rust code formatted by rustfmt.
                if r.name() == "MD013"
                    && let Some(md013) = r.as_any().downcast_ref::<MD013LineLength>()
                {
                    return Box::new(md013.with_code_blocks_disabled()) as Box<dyn Rule>;
                }
                dyn_clone::clone_box(&**r)
            })
            .collect();

        // Lint the extracted markdown
        let ctx = LintContext::new(&block.markdown, config.markdown_flavor(), None);
        let mut warnings = Vec::new();
        for rule in &block_rules {
            if let Ok(rule_warnings) = rule.check(&ctx) {
                warnings.extend(rule_warnings);
            }
        }

        if warnings.is_empty() {
            continue;
        }

        // Apply fixes to the markdown
        let mut formatted = block.markdown.clone();
        let content_changed =
            super::processing::apply_fixes_coordinated(&block_rules, &mut formatted, true, true, config, None);

        if !content_changed {
            continue;
        }

        // Determine if original block ended with a trailing newline
        let byte_end = block.byte_end.min(content.len());
        let original_ends_with_newline = content.as_bytes().get(byte_end.wrapping_sub(1)) == Some(&b'\n');

        // Restore doc comment prefixes
        let restored = restore_doc_comment_prefixes(&formatted, &block, original_ends_with_newline);
        content.replace_range(block.byte_start..byte_end, &restored);
        formatted_count += 1;
    }

    formatted_count
}

/// Restore doc comment prefixes to formatted markdown.
///
/// Each formatted line keeps the indentation and prefix (space, tab, or
/// nothing after `///`) of the source line it came from. A fix can remove lines
/// (MD012 collapsing blanks) or insert them (MD022 adding one below a heading),
/// so a line's position in the formatted markdown does not name its source
/// line; [`source_lines`] pairs them by aligning it with the extracted markdown.
///
/// A line with no source line takes the block's first-line indentation and the
/// standard `"/// "` or `"//! "` prefix, as does a line whose source line was
/// empty (that source's prefix is a bare `///`). An empty line gets the bare
/// prefix with no trailing whitespace.
///
/// Only appends a trailing newline if `trailing_newline` is true, matching the
/// original block's behavior to maintain idempotency.
fn restore_doc_comment_prefixes(markdown: &str, block: &DocCommentBlock, trailing_newline: bool) -> String {
    let source: Vec<&str> = block.markdown.split('\n').collect();
    let md_lines: Vec<&str> = markdown.split('\n').collect();
    let sources = source_lines(&source, &md_lines);
    let mut result = String::new();

    // Determine the dominant indentation for new lines (added by fixes)
    let dominant_indent = block
        .line_metadata
        .first()
        .map(|m| m.leading_whitespace.as_str())
        .unwrap_or("");

    let bare_prefix = match block.kind {
        DocCommentKind::Outer => "///",
        DocCommentKind::Inner => "//!",
    };

    for (md_line, source_index) in md_lines.iter().zip(sources) {
        if !result.is_empty() {
            result.push('\n');
        }

        let meta = source_index.and_then(|index| block.line_metadata.get(index));
        let indent = meta.map_or(dominant_indent, |m| m.leading_whitespace.as_str());
        result.push_str(indent);

        if md_line.is_empty() {
            result.push_str(bare_prefix);
        } else if let Some(meta) = meta.filter(|_| source_index.is_some_and(|index| !source[index].is_empty())) {
            result.push_str(&meta.prefix);
            result.push_str(md_line);
        } else {
            result.push_str(bare_prefix);
            result.push(' ');
            result.push_str(md_line);
        }
    }

    if trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

/// Cost of pairing a formatted line with a source line whose text a fix edited.
const EDITED_LINE_COST: usize = 3;
/// Cost of a source line a fix removed, or of a formatted line a fix inserted.
const GAP_LINE_COST: usize = 2;

/// One step of the alignment in [`source_lines`], read from its end.
#[derive(Clone, Copy)]
enum Step {
    /// The formatted line came from the source line, unchanged or edited.
    Paired,
    /// The source line has no formatted line.
    Removed,
    /// The formatted line has no source line.
    Inserted,
}

/// For each formatted line, the index of the source line it came from, or
/// `None` for a line a fix inserted.
///
/// The pairing is the cheapest alignment of the two, where an unchanged line
/// costs nothing, a line a fix edited in place (MD009 trimming trailing spaces)
/// costs [`EDITED_LINE_COST`], and a removed or inserted line costs
/// [`GAP_LINE_COST`]. An edit costs less than a removal plus an insertion, so a
/// line whose fixed text equals its neighbor's still pairs with its own source.
/// It costs more than a single gap, so the lines after a removal or insertion
/// pair with their unchanged selves, not with whatever held their position.
/// Equally cheap alignments are told apart from the last line back, preferring
/// a removed line, then an inserted one, then a pair, so the same lines always
/// pair the same way.
fn source_lines(source: &[&str], formatted: &[&str]) -> Vec<Option<usize>> {
    let mut sources = vec![None; formatted.len()];

    // Pairing the leading and trailing unchanged lines never makes an alignment
    // more expensive, so only the lines between them need aligning.
    let shorter = source.len().min(formatted.len());
    let prefix = (0..shorter).take_while(|&i| source[i] == formatted[i]).count();
    let suffix = (0..shorter - prefix)
        .take_while(|&k| source[source.len() - 1 - k] == formatted[formatted.len() - 1 - k])
        .count();
    for (index, slot) in sources.iter_mut().enumerate().take(prefix) {
        *slot = Some(index);
    }
    for k in 0..suffix {
        sources[formatted.len() - 1 - k] = Some(source.len() - 1 - k);
    }
    let old = &source[prefix..source.len() - suffix];
    let new = &formatted[prefix..formatted.len() - suffix];

    // `steps[i * width + j]` is the last step of the cheapest alignment of
    // `old[..i]` with `new[..j]`; two rows of costs are enough to fill it.
    let width = new.len() + 1;
    let mut steps = vec![Step::Inserted; (old.len() + 1) * width];
    let mut previous: Vec<usize> = (0..width).map(|j| j * GAP_LINE_COST).collect();
    let mut current = vec![0; width];
    for i in 1..=old.len() {
        current[0] = i * GAP_LINE_COST;
        steps[i * width] = Step::Removed;
        for j in 1..width {
            let unchanged = old[i - 1] == new[j - 1];
            let paired = (
                previous[j - 1] + if unchanged { 0 } else { EDITED_LINE_COST },
                Step::Paired,
            );
            let removed = (previous[j] + GAP_LINE_COST, Step::Removed);
            let inserted = (current[j - 1] + GAP_LINE_COST, Step::Inserted);
            let (cost, step) = [removed, inserted, paired]
                .into_iter()
                .reduce(|best, candidate| if candidate.0 < best.0 { candidate } else { best })
                .expect("three candidates");
            current[j] = cost;
            steps[i * width + j] = step;
        }
        std::mem::swap(&mut previous, &mut current);
    }

    let (mut i, mut j) = (old.len(), new.len());
    while i > 0 || j > 0 {
        match steps[i * width + j] {
            Step::Paired => {
                i -= 1;
                j -= 1;
                sources[prefix + j] = Some(prefix + i);
            }
            Step::Removed => i -= 1,
            Step::Inserted => j -= 1,
        }
    }
    sources
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one doc comment block in `source`, extracted as the fix pass extracts it.
    fn block_of(source: &str) -> DocCommentBlock {
        let mut blocks = extract_doc_comment_blocks(source);
        assert_eq!(blocks.len(), 1, "expected one doc comment block in {source:?}");
        blocks.remove(0)
    }

    #[test]
    fn test_restore_prefixes_basic() {
        let block = block_of("/// Hello\n/// World\n");
        let restored = restore_doc_comment_prefixes("Hello\nWorld", &block, true);
        assert_eq!(restored, "/// Hello\n/// World\n");
    }

    #[test]
    fn test_restore_prefixes_with_empty_line() {
        let block = block_of("/// First\n///\n/// Third\n");
        let restored = restore_doc_comment_prefixes("First\n\nThird", &block, true);
        assert_eq!(restored, "/// First\n///\n/// Third\n");
    }

    #[test]
    fn test_restore_prefixes_new_line_added() {
        // A fix adding a blank line below the heading (MD022)
        let block = block_of("/// # Heading\n/// Text\n");
        let restored = restore_doc_comment_prefixes("# Heading\n\nText", &block, true);
        assert_eq!(restored, "/// # Heading\n///\n/// Text\n");
    }

    #[test]
    fn test_restore_inner_doc_comment() {
        let block = block_of("//! Module\n");
        let restored = restore_doc_comment_prefixes("Module", &block, true);
        assert_eq!(restored, "//! Module\n");
    }

    #[test]
    fn test_restore_indented() {
        let block = block_of("    /// Indented\n");
        let restored = restore_doc_comment_prefixes("Indented", &block, true);
        assert_eq!(restored, "    /// Indented\n");
    }

    #[test]
    fn test_restore_no_trailing_newline() {
        let block = block_of("/// Hello");
        let restored = restore_doc_comment_prefixes("Hello", &block, false);
        assert_eq!(restored, "/// Hello");
    }

    #[test]
    fn test_restore_preserves_tab_prefix() {
        let block = block_of("///\tcontent\n");
        let restored = restore_doc_comment_prefixes("content", &block, true);
        assert_eq!(restored, "///\tcontent\n");
    }

    #[test]
    fn test_restore_preserves_no_space_prefix() {
        let block = block_of("///content\n");
        let restored = restore_doc_comment_prefixes("content", &block, true);
        assert_eq!(restored, "///content\n");
    }

    #[test]
    fn test_restore_after_removed_lines_keeps_each_lines_prefix() {
        // A fix collapsing blank lines (MD012): `more` and `end` keep their own
        // prefixes, not those of the blank lines that now sit at their positions.
        let block = block_of("//! text\n//!\n//!\n//!\n//! more\n//!\tend\n");
        let restored = restore_doc_comment_prefixes("text\n\nmore\nend", &block, true);
        assert_eq!(restored, "//! text\n//!\n//! more\n//!\tend\n");
    }

    #[test]
    fn test_restore_after_removed_lines_keeps_each_lines_indentation() {
        let block = block_of("/// text\n///\n///\n  /// more\n");
        let restored = restore_doc_comment_prefixes("text\n\nmore", &block, true);
        assert_eq!(restored, "/// text\n///\n  /// more\n");
    }

    #[test]
    fn test_restore_after_inserted_line_keeps_each_later_lines_separator() {
        let block = block_of("/// # Heading\n///\tTabbed\n///no-space\n");
        let restored = restore_doc_comment_prefixes("# Heading\n\nTabbed\nno-space", &block, true);
        assert_eq!(restored, "/// # Heading\n///\n///\tTabbed\n///no-space\n");
    }

    #[test]
    fn test_restore_line_edited_in_place_keeps_its_prefix() {
        // A fix trimming trailing spaces (MD009) changes the line's text, not its origin.
        let block = block_of("///\tfirst   \n///second\n");
        let restored = restore_doc_comment_prefixes("first\nsecond", &block, true);
        assert_eq!(restored, "///\tfirst\n///second\n");
    }

    #[test]
    fn test_restore_text_written_over_an_empty_line_gets_the_standard_prefix() {
        // The bare `///` of an empty source line has no separator to keep.
        let block = block_of("/// a\n///\n");
        let restored = restore_doc_comment_prefixes("a\nb", &block, true);
        assert_eq!(restored, "/// a\n/// b\n");
    }

    #[test]
    fn test_source_lines_pairs_equal_replaced_and_inserted_lines() {
        assert_eq!(
            source_lines(&["a", "", "", "b"], &["a", "", "b"]),
            [Some(0), Some(1), Some(3)]
        );
        assert_eq!(source_lines(&["# h", "t"], &["# h", "", "t"]), [Some(0), None, Some(1)]);
        assert_eq!(source_lines(&["x  ", "y"], &["x", "y"]), [Some(0), Some(1)]);
        assert_eq!(source_lines(&["x  "], &["x", "z"]), [Some(0), None]);
    }

    #[test]
    fn test_source_lines_pairs_an_edited_line_that_now_equals_its_neighbor_with_itself() {
        assert_eq!(source_lines(&["foo   ", "foo"], &["foo", "foo"]), [Some(0), Some(1)]);
        assert_eq!(source_lines(&["foo", "foo   "], &["foo", "foo"]), [Some(0), Some(1)]);
        assert_eq!(
            source_lines(&["a", "foo   ", "foo", "b"], &["a", "foo", "foo", "b"]),
            [Some(0), Some(1), Some(2), Some(3)]
        );
    }

    #[test]
    fn test_source_lines_prefers_moving_an_unchanged_line_over_editing_two() {
        // An inserted blank below the heading and a removed trailing blank: `x`
        // pairs with itself, not two edits in place.
        assert_eq!(
            source_lines(&["# H", "x", ""], &["# H", "", "x"]),
            [Some(0), None, Some(1)]
        );
    }

    #[test]
    fn test_source_lines_breaks_ties_the_same_way_every_time() {
        // Either copy of `a` can pair with the source line at the same cost.
        assert_eq!(source_lines(&["a"], &["b", "a", "a", "b"]), [None, Some(0), None, None]);
        // Keeping `a` or keeping `b` costs the same.
        assert_eq!(source_lines(&["a", "b"], &["b", "a"]), [None, Some(0)]);
    }

    #[test]
    fn test_source_lines_with_no_lines_on_one_side() {
        assert_eq!(source_lines(&["a", "b"], &[]), Vec::<Option<usize>>::new());
        assert_eq!(source_lines(&[], &["a", "b"]), [None, None]);
    }

    #[test]
    fn test_restore_line_trimmed_to_equal_its_neighbor_keeps_its_prefix_and_indentation() {
        let block = block_of("///foo   \n/// foo\n");
        let restored = restore_doc_comment_prefixes("foo\nfoo", &block, true);
        assert_eq!(restored, "///foo\n/// foo\n");

        let block = block_of("///foo   \n  /// foo\n");
        let restored = restore_doc_comment_prefixes("foo\nfoo", &block, true);
        assert_eq!(restored, "///foo\n  /// foo\n");
    }
}
