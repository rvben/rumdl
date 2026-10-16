/// Rule MD023: Headings must start at the left margin
///
/// See [docs/md023.md](../../docs/md023.md) for full documentation, configuration, and examples.
use crate::lint_context::HeadingContainer;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_single_line_range;

#[derive(Clone)]
pub struct MD023HeadingStartLeft;

impl MD023HeadingStartLeft {
    /// The warning for the indentation of one line of a heading held by
    /// `container`, or `None` when the line sits at its container's edge.
    ///
    /// The container's own indentation is not the heading's: removing it takes the
    /// heading out of the list item holding it. Only the columns past the item's
    /// content column are removed. The warning carries no fix where the edit
    /// cannot be shown to keep the heading where it is: a container the pass does
    /// not model, or a tab among the indentation of a line inside an item, whose
    /// width depends on the column it starts at.
    fn indentation_warning(
        &self,
        ctx: &crate::lint_context::LintContext,
        line_idx: usize,
        container: HeadingContainer,
        quoted: Option<QuotedHeading<'_>>,
        continuation: bool,
        message: impl Fn(usize) -> String,
    ) -> Option<LintWarning> {
        if let Some(quoted) = quoted {
            return self.quoted_indentation_warning(ctx, line_idx, quoted, continuation, message);
        }
        let line = &ctx.lines[line_idx];
        let (edge, fixable) = match container {
            HeadingContainer::Outside => (0, true),
            HeadingContainer::Item { content_col } => {
                let tab_free = !line.content(ctx.content)[..line.indent].contains('\t');
                (content_col, tab_free)
            }
            HeadingContainer::Unknown => (0, false),
        };
        let indent = line.visual_indent.saturating_sub(edge);
        if indent == 0 {
            return None;
        }
        // A continuation line of a setext heading's text indented four columns
        // or more is text, whatever it spells. With the indentation gone it could
        // start a code fence, a quote or a list, and end the heading.
        let fixable = fixable && !(continuation && indent >= 4);
        // The indentation to remove is the bytes from the container's edge to the
        // text; the edge is spaces on every line that is fixed, so columns are bytes.
        let (start_line, start_col, end_line, end_col) = calculate_single_line_range(
            line_idx + 1,
            edge.min(line.indent) + 1,
            line.indent.saturating_sub(edge),
        );
        let fix = fixable.then(|| {
            let line_start = ctx.line_start_byte(line_idx + 1).unwrap_or(0);
            Fix::new(
                line_start + edge.min(line.indent)..line_start + line.indent,
                String::new(),
            )
        });
        Some(LintWarning {
            rule_name: Some(self.name().to_string()),
            line: start_line,
            column: start_col,
            end_line,
            end_column: end_col,
            severity: Severity::Warning,
            message: message(indent),
            fix,
        })
    }

    /// The warning for the indentation of one line of a heading inside a
    /// blockquote, or `None` when its text follows the quote marker directly.
    ///
    /// The quote marker and the one optional space after it are the quote's own
    /// and stay; only the whitespace past them is the heading's indentation. A
    /// line that does not begin with a quote marker (a heading opening a list item
    /// that holds the quote) has no indentation of this kind. A tab after the
    /// marker has a width that depends on its column, so it is reported without a
    /// fix. In a quote that holds a list item, the item's content column is the edge.
    fn quoted_indentation_warning(
        &self,
        ctx: &crate::lint_context::LintContext,
        line_idx: usize,
        quoted: QuotedHeading<'_>,
        continuation: bool,
        message: impl Fn(usize) -> String,
    ) -> Option<LintWarning> {
        let line = &ctx.lines[line_idx];
        let text = line.content(ctx.content);
        let (_, start) = quote_margin(text, quoted.depth)?;
        let content = &text[start..];
        let indentation = content.len() - content.trim_start_matches([' ', '\t']).len();
        // A tab that serves as the optional space after the marker is only
        // partly the marker's own: the rest of its width is indentation.
        let tab_rest = if text[..start].ends_with('\t') {
            let column = visual_width(&text[..start - 1]);
            3 - column % 4
        } else {
            0
        };
        if indentation == 0 && tab_rest == 0 {
            return None;
        }
        // A list item opened in this quote owns the columns up to its content
        // column: the heading is over-indented only by what lies past it.
        let (edge, item_margin) = quoted.edges[line_idx];
        // A line that opens an item has its marker's indentation, not a heading's.
        if item_margin > 0 && edge > item_margin && line.list_item.is_none() && !text[..start].contains('\t') {
            // Both columns are measured from the text after the line's own quote
            // markers, which may be written with another width than the item's.
            let line_margin = visual_width(&text[..start]);
            let text_column = visual_width(&text[..start + indentation]) - line_margin;
            let extra = text_column.saturating_sub(edge - item_margin);
            if extra == 0 {
                return None;
            }
            let spaces_only = !content[..indentation].contains('\t') && extra <= indentation;
            let fixable = spaces_only && !(continuation && extra >= 4);
            let removed_from = start + indentation - extra.min(indentation);
            let (start_line, start_col, end_line, end_col) = calculate_single_line_range(
                line_idx + 1,
                text[..removed_from].chars().count() + 1,
                text[removed_from..start + indentation].chars().count(),
            );
            let fix = fixable.then(|| {
                let line_start = ctx.line_start_byte(line_idx + 1).unwrap_or(0);
                Fix::new(
                    line_start + removed_from..line_start + start + indentation,
                    String::new(),
                )
            });
            return Some(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                severity: Severity::Warning,
                message: message(extra),
                fix,
            });
        }
        if quoted.list_flags[line_idx] {
            return None;
        }
        let spaces_only = !text[..start].ends_with('\t') && !content[..indentation].contains('\t');
        // Text indented four columns or more is not a heading line, except as a
        // continuation of setext text, where removing the indentation could open
        // a fence, a quote or a list and end the heading.
        let fixable = spaces_only && !(continuation && indentation >= 4);
        // The range starts at the marker's tab when part of that tab is indentation.
        let reported_from = if tab_rest > 0 { start - 1 } else { start };
        let (start_line, start_col, end_line, end_col) = calculate_single_line_range(
            line_idx + 1,
            text[..reported_from].chars().count() + 1,
            text[reported_from..start + indentation].chars().count(),
        );
        let fix = fixable.then(|| {
            let line_start = ctx.line_start_byte(line_idx + 1).unwrap_or(0);
            Fix::new(line_start + start..line_start + start + indentation, String::new())
        });
        Some(LintWarning {
            rule_name: Some(self.name().to_string()),
            line: start_line,
            column: start_col,
            end_line,
            end_column: end_col,
            severity: Severity::Warning,
            message: message(indentation + tab_rest),
            fix,
        })
    }
}

/// For each line, whether a list item is open in the run of quoted lines above
/// it: the nearest earlier line deciding it opens an item, or is text written
/// flush with the quote after a blank line, or a flush heading, either of which
/// ends any list before it. Flush text without a blank line above may be lazy continuation of an item, so it
/// decides nothing, and an unprefixed line under quoted text is such a
/// continuation of the quote.
fn quote_list_flags(ctx: &crate::lint_context::LintContext) -> Vec<bool> {
    let mut flags = Vec::with_capacity(ctx.lines.len());
    let mut open = false;
    // The shallowest quote depth a line of the open list was seen at: text in a
    // quote nested deeper says nothing about the list around that quote.
    let mut open_depth = usize::MAX;
    let mut blank_above = true;
    for line in &ctx.lines {
        let Some((depth, content)) = quoted_content(line.content(ctx.content)) else {
            // Unprefixed text straight under quoted text continues its paragraph,
            // so the quote and its open item go on; anything else ends the quote.
            if blank_above || line.content(ctx.content).trim().is_empty() {
                open = false;
                open_depth = usize::MAX;
                blank_above = true;
            }
            flags.push(false);
            continue;
        };
        flags.push(open);
        // List-shaped text inside a fenced block decides nothing.
        if line.in_code_block {
            continue;
        }
        let blank = content.trim().is_empty();
        if !blank {
            if opens_list_item(content.trim_start()) {
                open = true;
                open_depth = open_depth.min(depth);
            } else if depth <= open_depth
                && !content.starts_with([' ', '\t'])
                && (blank_above || starts_atx_heading(content))
            {
                open = false;
                open_depth = usize::MAX;
            }
        }
        blank_above = blank;
    }
    flags
}

/// The width of `text` in columns, with tabs advancing to multiples of four.
fn visual_width(text: &str) -> usize {
    text.chars().fold(0, |column, c| match c {
        '\t' => column + 4 - column % 4,
        _ => column + 1,
    })
}

/// Whether `text` opens with an ATX heading marker: one to six `#`, then a space,
/// a tab or the end of the line.
fn starts_atx_heading(text: &str) -> bool {
    let hashes = text.len() - text.trim_start_matches('#').len();
    (1..=6).contains(&hashes) && (text.len() == hashes || text[hashes..].starts_with([' ', '\t']))
        || (1..=6).contains(&hashes) && text.len() == hashes
}

/// What the indentation check needs to know about a heading inside a quote.
#[derive(Clone, Copy)]
struct QuotedHeading<'a> {
    /// Per line, the column its innermost open list item holds its content at
    /// (`container_edges`), zero outside a list.
    edges: &'a [(usize, usize)],
    /// Per line, whether a list item may be open in the quote above it: an
    /// unindented line under an item can be its lazy continuation, which
    /// `edges` reads as ending the item.
    list_flags: &'a [bool],
    /// How many quote markers the heading's own line has.
    depth: usize,
}

/// The quote depth of `line` and its text inside the quote, where the quote may
/// open after list markers (`- > text`), or `None` for a line outside any quote.
fn quoted_content(line: &str) -> Option<(usize, &str)> {
    let start = quote_start(line)?;
    let parsed = crate::utils::blockquote::parse_blockquote_prefix(&line[start..])?;
    Some((parsed.nesting_level, parsed.content))
}

/// How many quote markers `line` opens with, up to `max`, and the byte offset
/// where its content starts after them and the one optional space or tab that
/// follows the last. The markers may be split by list markers (`> - > text`).
/// `None` when the line has no marker.
fn quote_margin(line: &str, max: usize) -> Option<(usize, usize)> {
    let mut offset = 0;
    let mut markers = 0;
    loop {
        let mut consumed = false;
        while markers < max {
            let rest = &line[offset..];
            let next = rest.trim_start_matches([' ', '\t']);
            if !next.starts_with('>') {
                break;
            }
            offset += rest.len() - next.len() + 1;
            markers += 1;
            consumed = true;
        }
        if consumed {
            offset += usize::from(line[offset..].starts_with([' ', '\t']));
        }
        if markers >= max {
            break;
        }
        // Step over list markers to a quote that opens inside the item.
        let mut after = offset;
        loop {
            let rest = &line[after..];
            let trimmed = rest.trim_start_matches([' ', '\t']);
            let token = trimmed.split([' ', '\t']).next().unwrap_or("");
            if !opens_list_item(token) {
                return (markers > 0).then_some((markers, offset));
            }
            after += rest.len() - trimmed.len() + token.len();
            if line[after..].trim_start_matches([' ', '\t']).starts_with('>') {
                break;
            }
        }
        offset = after;
    }
    (markers > 0).then_some((markers, offset))
}

/// The byte offset in `line` where its quote markers begin: 0 for a line that
/// opens with one, after the list markers for a quote inside a list item
/// (`- > text`), or `None` for a line outside any quote.
fn quote_start(line: &str) -> Option<usize> {
    let mut start = 0;
    loop {
        let rest = &line[start..];
        if crate::utils::blockquote::parse_blockquote_prefix(rest).is_some() {
            return Some(start);
        }
        let trimmed = rest.trim_start_matches([' ', '\t']);
        let token = trimmed.split([' ', '\t']).next().unwrap_or("");
        if token.is_empty() || !opens_list_item(token) {
            return None;
        }
        start += rest.len() - trimmed.len() + token.len();
    }
}

/// Whether `text` starts with a list marker followed by a space, a tab or the
/// end of the line.
fn opens_list_item(text: &str) -> bool {
    let token = text.split([' ', '\t']).next().unwrap_or("");
    let digits = token.trim_end_matches(['.', ')']);
    matches!(token, "-" | "+" | "*")
        || (token.len() == digits.len() + 1
            && (1..=9).contains(&digits.len())
            && digits.bytes().all(|b| b.is_ascii_digit()))
}

impl Rule for MD023HeadingStartLeft {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        Some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(23))
    }

    fn name(&self) -> &'static str {
        "MD023"
    }

    fn description(&self) -> &'static str {
        "Headings must start at the beginning of the line"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early return for empty content
        if ctx.lines.is_empty() {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();

        // Computed once, and only for a document with a heading in a quote.
        let quote_columns = ctx.headings().any(|parsed| parsed.is_blockquote()).then(|| {
            (
                crate::utils::blockquote::container_edges_with_margins(ctx),
                quote_list_flags(ctx),
            )
        });

        // Process all headings using cached heading information
        for parsed in ctx.headings() {
            let line_num = parsed.line_num - 1;
            let line_info = parsed.line_info;
            let quoted = parsed.is_blockquote();
            // The quote depth is read off the line that closes the heading: the
            // underline of a setext heading, whose text lines may start with
            // markers that are only text.
            let closing_line = if parsed.is_setext() { line_num + 1 } else { line_num };
            let quoted_flags = quote_columns
                .as_ref()
                .filter(|_| quoted)
                .map(|(edges, list_flags)| QuotedHeading {
                    edges,
                    list_flags,
                    depth: ctx
                        .lines
                        .get(closing_line)
                        .and_then(|line| quote_margin(line.content(ctx.content), usize::MAX))
                        .map_or(0, |(depth, _)| depth),
                });
            // Skip lines inside PyMdown blocks, admonitions, and content tabs:
            // those containers are indentation-scoped, so a heading indented
            // there is intentionally nested, not an accidental indent. This
            // deliberately excludes markdown="1" HTML divs (tag-scoped, content
            // needs no indentation, and detected in every flavor).
            if line_info.in_pymdown_block || line_info.in_admonition || line_info.in_content_tab {
                continue;
            }

            let heading = parsed.heading;
            // Skip hashtag-like patterns (e.g., #tag, #123, #29039) for ATX level 1
            // These are likely issue refs or social hashtags, not intended headings
            if heading.level == 1 && matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                // Get first "word" of heading text (up to space, comma, or closing paren)
                let first_word: String = heading
                    .text
                    .trim()
                    .chars()
                    .take_while(|c| !c.is_whitespace() && *c != ',' && *c != ')')
                    .collect();
                if let Some(first_char) = first_word.chars().next() {
                    // Skip if first word starts with lowercase or number
                    if first_char.is_lowercase() || first_char.is_numeric() {
                        continue;
                    }
                }
            }

            let is_setext = matches!(
                heading.style,
                crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
            );
            let container = if quoted {
                HeadingContainer::Unknown
            } else {
                ctx.heading_container(line_num + 1)
            };

            // A setext heading's text is the whole paragraph its underline
            // ends, and CommonMark strips the leading whitespace of every one
            // of those lines, so each of them carries indentation of its own
            if is_setext {
                let underline_line = line_num + 1;
                let first_text_line = line_num + 1 - heading.text_lines;

                for text_line in first_text_line..=line_num {
                    if let Some(warning) = self.indentation_warning(
                        ctx,
                        text_line,
                        container,
                        quoted_flags,
                        text_line > first_text_line,
                        |indent| format!("Setext heading should not be indented by {indent} spaces"),
                    ) {
                        warnings.push(warning);
                    }
                }

                // The underline is indented independently of the text
                if underline_line < ctx.lines.len()
                    && let Some(warning) =
                        self.indentation_warning(ctx, underline_line, container, quoted_flags, false, |_| {
                            "Setext heading underline should not be indented".to_string()
                        })
                {
                    warnings.push(warning);
                }
            } else if let Some(warning) =
                self.indentation_warning(ctx, line_num, container, quoted_flags, false, |indent| {
                    format!("Heading should not be indented by {indent} spaces")
                })
            {
                warnings.push(warning);
            }
        }

        // Protect the indentation edits themselves: visible template headings
        // can have Markdown indentation outside the template delimiters.
        warnings.retain(|warning| {
            warning
                .fix
                .as_ref()
                .is_none_or(|fix| !ctx.overlaps_template_code(fix.range.start, fix.range.end))
        });
        Ok(warnings)
    }

    fn fixes_from_diagnostics(&self) -> bool {
        true
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Fast path: check if document likely has headings
        if !ctx.likely_has_headings() {
            return true;
        }
        // Verify headings actually exist
        ctx.headings().next().is_none()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD023HeadingStartLeft)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    #[test]
    fn test_basic_functionality() {
        let rule = MD023HeadingStartLeft;

        // Test with properly aligned headings
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with indented headings
        let content = "  # Heading 1\n ## Heading 2\n   ### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3); // Should flag all three indented headings
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
        assert_eq!(result[2].line, 3);

        // Test with setext headings
        let content = "Heading 1\n=========\n  Heading 2\n  ---------";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should flag the indented heading and underline
        assert_eq!(result[0].line, 3);
        assert_eq!(result[1].line, 4);
    }

    #[test]
    fn test_issue_refs_skipped_but_real_headings_caught() {
        let rule = MD023HeadingStartLeft;

        // Issue refs should NOT be flagged (starts with number)
        let content = "- fix: issue\n  #29039)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "#29039) should not be flagged as indented heading. Got: {result:?}"
        );

        // Hashtags should NOT be flagged (starts with lowercase)
        let content = "Some text\n  #hashtag";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "#hashtag should not be flagged as indented heading. Got: {result:?}"
        );

        // A `#` run with no space after it is paragraph text in CommonMark,
        // whatever follows it, so there is no heading to move left
        for content in [
            "Some text\n  #Summary",
            "Some text\n  ##introduction",
            "Some text\n  ##123",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(result.is_empty(), "{content:?} is not a heading. Got: {result:?}");
        }

        // The same line with its space is an indented heading
        let content = "Some text\n\n  # Summary";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "indented `# Summary` is flagged. Got: {result:?}");

        // Properly aligned headings should pass
        let content = "# Summary\n## Details";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Properly aligned headings should pass. Got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_admonition_indented_heading_not_flagged() {
        // A heading intentionally indented to stay nested
        // inside a MkDocs admonition body must not be flagged, since the
        // indentation is required for it to belong to the admonition.
        let rule = MD023HeadingStartLeft;
        let content = "!!! note\n\n    # Foo";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "heading nested in an admonition body should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_content_tab_indented_heading_not_flagged() {
        // The same false positive occurs inside a MkDocs content tab body.
        let rule = MD023HeadingStartLeft;
        let content = "=== \"Tab A\"\n\n    # Foo";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "heading nested in a content tab body should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_accidental_indent_still_flagged() {
        // Control: a heading indented outside any admonition/tab (accidental
        // indentation, not container nesting) must still be flagged under
        // MkDocs flavor, and its fix must still de-indent it.
        let rule = MD023HeadingStartLeft;
        let content = "Some text\n\n  # Foo";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "accidentally indented top-level heading should still be flagged, got: {result:?}"
        );

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Some text\n\n# Foo", "fix should still de-indent it");
    }

    #[test]
    fn test_standard_flavor_indented_heading_still_flagged_and_fixed() {
        // Control: standard flavor has no MkDocs container concept, so an
        // indented heading is still an accidental indent and must still be
        // flagged and de-indented by the fix. Uses 3 spaces: at 4+ spaces
        // CommonMark parses the line as an indented code block rather than a
        // heading at all, which is unrelated to this rule's guard.
        let rule = MD023HeadingStartLeft;
        let content = "Some text\n\n   # Foo";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "indented heading should still be flagged under standard flavor, got: {result:?}"
        );

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Some text\n\n# Foo", "fix should still de-indent it");
    }

    #[test]
    fn test_html_markdown_div_indented_heading_still_flagged() {
        // A markdown="1" HTML div is tag-scoped, not indentation-scoped:
        // content needs no indentation to belong to it, so an indented
        // heading inside one is an accidental indent in every flavor.
        let rule = MD023HeadingStartLeft;
        let content = "<div markdown=\"1\">\n\n  # Bar\n\n</div>";
        for flavor in [
            crate::config::MarkdownFlavor::Standard,
            crate::config::MarkdownFlavor::MkDocs,
        ] {
            let ctx = LintContext::new(content, flavor, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.len(),
                1,
                "indented heading inside a markdown=\"1\" div must stay flagged under {flavor:?}, got: {result:?}"
            );
        }
    }
}

/// A heading written inside a list item is indented by the item's own content
/// column. Only the indentation past that column is the heading's, and removing
/// the rest moves the heading out of the item.
#[cfg(test)]
mod list_item_tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::lint_context::LintContext;

    /// The warnings for `content` and the document after the rule's fix.
    fn run(content: &str) -> (Vec<LintWarning>, String) {
        let rule = MD023HeadingStartLeft;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        let fixed = rule.fix(&ctx).unwrap();
        (warnings, fixed)
    }

    /// Content the rule leaves alone: no warning and no change.
    fn assert_clean(content: &str) {
        let (warnings, fixed) = run(content);
        assert!(warnings.is_empty(), "{content:?} warned: {warnings:?}");
        assert_eq!(fixed, content, "{content:?} was rewritten");
    }

    /// Content the rule flags once per entry of `lines`, fixed to `expected`,
    /// and that stays fixed when fixed again.
    fn assert_fixed(content: &str, lines: &[usize], expected: &str) {
        let (warnings, fixed) = run(content);
        assert_eq!(
            warnings.iter().map(|warning| warning.line).collect::<Vec<_>>(),
            lines,
            "{content:?}: {warnings:?}"
        );
        assert_eq!(fixed, expected, "{content:?}");
        assert_clean(expected);
    }

    #[test]
    fn a_heading_at_its_items_content_column_is_not_indented() {
        for content in [
            "-\n  ## a\n  text\n",
            "- x\n\n  ## a\n\n  text\n",
            "- x\n  ## a\n  text\n",
            "1. x\n\n   ## a\n",
            "10. x\n\n    ## a\n",
            "- x\n  - y\n\n    ## a\n",
            "-\n  -\n    ## a\n",
            "- x\n\n  a\n  ---\n",
            "1. x\n\n   a\n   ===\n",
            "-\n  a\n  ---\n  text\n",
            "-\tx\n\n    ## a\n",
            "-     code\n\n  ## a\n",
        ] {
            assert_clean(content);
        }
    }

    #[test]
    fn only_the_indentation_past_the_content_column_is_removed() {
        assert_fixed("- x\n\n    ## a\n", &[3], "- x\n\n  ## a\n");
        assert_fixed("- x\n  - y\n\n      ## a\n", &[4], "- x\n  - y\n\n    ## a\n");
        assert_fixed("1. x\n\n    ## a\n", &[3], "1. x\n\n   ## a\n");
        assert_fixed("- x\n\n    a\n    ---\n", &[3, 4], "- x\n\n  a\n  ---\n");
        assert_fixed("- x\n\n    a\n    b\n    ===\n", &[3, 4, 5], "- x\n\n  a\n  b\n  ===\n");
    }

    #[test]
    fn the_item_is_found_by_what_the_line_re_enters() {
        // Text between the item and the heading does not end the item.
        assert_fixed("- x\n  text\n    ## a\n", &[3], "- x\n  text\n  ## a\n");
        // A lazy continuation line holds the heading in the item.
        assert_clean("- x\nlazy\n  ## a\n");
        assert_clean("- x\n\n  a\nlazy\n  ---\n");
        // An item that opened empty is closed by the blank line, and the heading
        // belongs to the item around it, whose margin it keeps.
        assert_fixed("-\n  -\n\n    ## a\n", &[4], "-\n  -\n\n  ## a\n");
        // A paragraph that re-enters only the outer item closes the inner one.
        assert_fixed(
            "- x\n  - y\n\n  paragraph\n\n    ## a\n",
            &[6],
            "- x\n  - y\n\n  paragraph\n\n  ## a\n",
        );
    }

    #[test]
    fn a_tab_in_the_indentation_is_reported_but_not_rewritten() {
        let content = "- x\n\n\t## a\n";
        let (warnings, fixed) = run(content);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].fix.is_none(), "{:?}", warnings[0].fix);
        assert_eq!(fixed, content);
        // Two tabs reach column 8, six past the item's edge: indented code.
        assert_clean("- x\n\n\t\t## a\n");
    }

    #[test]
    fn a_heading_outside_every_item_is_still_moved_to_the_margin() {
        assert_fixed("  ## a\n", &[1], "## a\n");
        assert_fixed("a\n\n  b\n  ---\n", &[3, 4], "a\n\nb\n---\n");
        // Written under the content column of its item, a heading is outside it.
        assert_fixed("1. x\n\n  ## a\n", &[3], "1. x\n\n## a\n");
        assert_fixed("- x\n\n ## a\n", &[3], "- x\n\n## a\n");
        assert_fixed("- x\n\nplain\n\n  ## a\n", &[5], "- x\n\nplain\n\n## a\n");
        assert_fixed("-\n\n  ## a\n", &[3], "-\n\n## a\n");
    }

    #[test]
    fn deep_continuation_lines_of_setext_text_are_reported_but_not_rewritten() {
        // The fence line is text of the heading; unindented it would open a fence.
        for content in ["- x\n\n  a\n      ```\n  ---\n", "a\n    ```\n---\n"] {
            let (warnings, fixed) = run(content);
            assert!(!warnings.is_empty(), "{content:?}");
            assert_eq!(fixed, content, "{content:?}");
        }
        // The shallow continuation line and the underline are still fixed.
        let (_, fixed) = run("- x\n\n  a\n    b\n  ---\n");
        assert_eq!(fixed, "- x\n\n  a\n  b\n  ---\n");
    }

    #[test]
    fn test_heading_past_a_quoted_list_items_content_column_is_flagged() {
        let rule = MD023HeadingStartLeft;
        for (item, bad, fixed) in [
            ("> - item\n>\n", ">    ## Sub\n", ">   ## Sub\n"),
            ("> 1. item\n>\n", ">     ## Sub\n", ">    ## Sub\n"),
            ("> > - item\n> >\n", "> >    ## Sub\n", "> >   ## Sub\n"),
            ("> - a\n>   - b\n>\n", ">      ## Sub\n", ">     ## Sub\n"),
        ] {
            let content = format!("# T\n\n{item}{bad}");
            let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(result.len(), 1, "{content:?}: {result:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), format!("# T\n\n{item}{fixed}"), "{content:?}");
        }
    }

    #[test]
    fn test_heading_at_a_quoted_list_items_content_column_is_not_flagged() {
        let rule = MD023HeadingStartLeft;
        for content in [
            "# T\n\n> - item\n>\n>   ## Sub\n",
            "# T\n\n> 1. item\n>\n>    ## Sub\n",
            "# T\n\n> > - item\n> >\n> >   ## Sub\n",
        ] {
            let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{content:?}");
        }
    }

    #[test]
    fn test_heading_under_a_bare_marker_in_a_quote_is_judged_from_the_marker_plus_one() {
        let rule = MD023HeadingStartLeft;
        let ok = "> -\n>   ## a\n>   text\n";
        let ctx = crate::lint_context::LintContext::new(ok, crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty());
        let bad = "> -\n>    ## a\n>   text\n";
        let ctx = crate::lint_context::LintContext::new(bad, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), ok);
    }

    #[test]
    fn test_quoted_list_edges_follow_the_innermost_item_and_the_lines_own_markers() {
        let rule = MD023HeadingStartLeft;
        for content in [
            "# T\n\n> - - item\n>\n>     ## Sub\n",
            "# T\n\n>> - item\n>>\n> >   ## Sub\n",
        ] {
            let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&ctx).unwrap().is_empty(), "{content:?}");
        }
        let bad = "# T\n\n>> - item\n>>\n> >    ## Sub\n";
        let ctx = crate::lint_context::LintContext::new(bad, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), "# T\n\n>> - item\n>>\n> >   ## Sub\n");
        let bad = "# T\n\n> - - item\n>\n>      ## Sub\n";
        let ctx = crate::lint_context::LintContext::new(bad, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), "# T\n\n> - - item\n>\n>     ## Sub\n");
    }

    #[test]
    fn test_indented_quote_marker_before_a_list_item_counts_once_in_the_margin() {
        let rule = MD023HeadingStartLeft;
        let content = "# T\n\n > - item\n>\n>   ## Sub\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty(), "{content:?}");
    }

    #[test]
    fn test_tab_among_the_quote_markers_of_a_list_item_is_not_judged() {
        let rule = MD023HeadingStartLeft;
        for content in [
            "# T\n\n>\t- item\n>\n>     ## Sub\n",
            "# T\n\n> 1.\titem\n>\n>       ## Sub\n",
        ] {
            let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&ctx).unwrap(), content);
        }
    }

    #[test]
    fn test_heading_after_the_quote_of_a_list_item_ends_is_measured_from_its_own_quote() {
        let rule = MD023HeadingStartLeft;
        let content = "# T\n\n> > - item\n>\n>    ## H\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        // The heading is in the outer quote, outside the item: it is not moved to the item's column.
        assert_ne!(rule.fix(&ctx).unwrap(), "# T\n\n> > - item\n>\n>   ## H\n");
    }

    #[test]
    fn test_quote_nested_inside_a_quoted_list_item_is_not_measured_against_the_item() {
        let rule = MD023HeadingStartLeft;
        let content = "> - item\n>\n>   >    ## H\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        // Not rewritten as if the heading sat in the item's own quote.
        assert_ne!(rule.fix(&ctx).unwrap(), "> - item\n>\n>   >   ## H\n");
    }

    #[test]
    fn test_possible_lazy_continuation_of_an_inner_item_withholds_the_item_edge() {
        let rule = MD023HeadingStartLeft;
        let content = "> - - text\n>   lazy\n>\n>     ## H\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_bare_inner_marker_on_a_quoted_item_line_sets_the_item_edge() {
        let rule = MD023HeadingStartLeft;
        let content = "> - -\n>     ## Head\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_thematic_break_inside_a_quoted_item_is_not_a_nested_list() {
        let rule = MD023HeadingStartLeft;
        let content = "> - * * *\n>\n>      ## Head\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), "> - * * *\n>\n>   ## Head\n");
    }

    #[test]
    fn test_setext_text_opening_a_nested_quoted_item_is_not_re_indented() {
        let rule = MD023HeadingStartLeft;
        let content = "> - item\n>     - other\n>       ---\n>     text\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_quote_after_a_list_holding_a_quoted_item_does_not_inherit_its_edge() {
        let rule = MD023HeadingStartLeft;
        let content = "- > - item\n  > - item\n>   lazy\n>      ## Head\n>  -\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_unquoted_lazy_continuation_of_an_inner_item_withholds_the_item_edge() {
        let rule = MD023HeadingStartLeft;
        let content = "> - - item\n    lazy\n>     ## Head\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }
}
