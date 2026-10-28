//! Ordered cleanup using native findings from one source revision. This is
//! deliberately separate from generic disjoint-edit batching: an unrecognized
//! reader is a boundary even when it currently has no findings or a proof.

use super::{Fix, LintContext, LintWarning, Rule, hash_content};
use crate::rules::*;

pub(super) fn eligible(ctx: &LintContext<'_>) -> bool {
    ctx.flavor == crate::config::MarkdownFlavor::Standard
        && memchr::memchr3(b'\r', b'\t', 0, ctx.content.as_bytes()).is_none()
        && !crate::inline_config::has_inline_config_markers(ctx.content)
}

fn cleanup_writer(rule: &dyn Rule) -> bool {
    let r = rule.as_any();
    crate::format_proof::NativeFormatPolicy::validated(rule).is_some()
        && (r.is::<MD034NoBareUrls>()
            || r.is::<MD049EmphasisStyle>()
            || r.is::<MD009TrailingSpaces>()
            || r.is::<MD012NoMultipleBlanks>()
            || r.is::<MD064NoMultipleConsecutiveSpaces>()
            || r.is::<MD022BlanksAroundHeadings>()
            || r.is::<MD031BlanksAroundFences>()
            || r.is::<MD032BlanksAroundLists>()
            || r.is::<MD077ListContinuationIndent>())
}

pub(super) type PreparedNativeFix = Option<Result<String, crate::rule::LintError>>;

#[cfg(test)]
fn capture(rule: &dyn Rule, ctx: &LintContext<'_>, warnings: &[LintWarning]) -> Option<Vec<Fix>> {
    capture_with_native(rule, ctx, warnings, &mut None)
}

/// Keep a native result on this configured rule and immutable context even
/// when its edits cannot join a cleanup batch. The ordinary fix path can then
/// consume that exact result rather than repeating the native check and fix.
pub(super) fn capture_with_native(
    rule: &dyn Rule,
    ctx: &LintContext<'_>,
    warnings: &[LintWarning],
    prepared: &mut PreparedNativeFix,
) -> Option<Vec<Fix>> {
    if !cleanup_writer(rule) {
        return None;
    }
    let fixes: Vec<_> = warnings
        .iter()
        .filter(|w| crate::utils::fix_utils::warning_is_enabled(w, ctx.inline_config(), rule.name()))
        .filter_map(|w| w.fix.clone())
        .collect();
    if fixes.is_empty() {
        return None;
    }
    if rule.as_any().is::<MD064NoMultipleConsecutiveSpaces>() {
        let edits = normalize(ctx.content, rule.name(), fixes)?;
        if edits.iter().any(|edit| {
            ctx.is_in_code_block_or_span(edit.range.start)
                || ctx.overlaps_template_code(edit.range.start, edit.range.end)
                || ctx
                    .line_info(ctx.offset_to_line_col(edit.range.start).0)
                    .is_some_and(|line| line.in_front_matter)
        }) {
            return None;
        }
        let native = prepared
            .get_or_insert_with(|| rule.fix_with_diagnostics(ctx, warnings))
            .as_ref()
            .ok()?
            .as_str();
        let applied = crate::utils::fix_utils::apply_edits(ctx.content, edits.iter()).ok()?;
        if applied.as_ref() != native {
            return None;
        }
        use crate::utils::list_fix_guard::Parsed;
        let before = ctx.list_guard_parse();
        return before.equivalent_word_spacing(&Parsed::new(native)).then_some(edits);
    }
    if rule.as_any().is::<MD077ListContinuationIndent>() {
        let edits = normalize(ctx.content, rule.name(), fixes)?;
        for edit in &edits {
            let line = ctx.line_info(ctx.offset_to_line_col(edit.range.start).0)?;
            let text_start = line.byte_offset + line.indent;
            let link = ctx.link_starting_at(text_start)?;
            if line.list_item.is_some()
                || line.in_code_block
                || line.in_front_matter
                || line.in_html_block
                || line.in_html_comment
                || ctx.overlaps_template_code(line.byte_offset, line.byte_offset + line.byte_len)
                || link.byte_end > line.byte_offset + line.byte_len
                || !ctx.content[link.byte_end..line.byte_offset + line.byte_len]
                    .trim()
                    .is_empty()
            {
                return None;
            }
        }
        let native = prepared
            .get_or_insert_with(|| rule.fix_with_diagnostics(ctx, warnings))
            .as_ref()
            .ok()?
            .as_str();
        let applied = crate::utils::fix_utils::apply_edits(ctx.content, edits.iter()).ok()?;
        if applied.as_ref() != native {
            return None;
        }

        let before = ctx.list_guard_parse();
        return preserves_structure(ctx, before, native).then_some(edits);
    }
    // The native custom fixer is authoritative, including MD022's inserts
    // omitted from its diagnostics. Only qualified outer gaps may cross later
    // source readers; every other heading/list/fence rewrite stays native.
    if rule.as_any().is::<MD022BlanksAroundHeadings>()
        || rule.as_any().is::<MD031BlanksAroundFences>()
        || rule.as_any().is::<MD032BlanksAroundLists>()
    {
        let native = prepared
            .get_or_insert_with(|| rule.fix_with_diagnostics(ctx, warnings))
            .as_ref()
            .ok()?
            .as_str();
        let edits = native_blank_insertions(ctx.content, native)?;

        let before = ctx.list_guard_parse();
        // Only create one blank physical line in a previously empty gap.
        // Starting/ending gaps and any existing blank lines stay native.
        for edit in &edits {
            let at = edit.range.start;
            let previous = ctx.content.get(..at.checked_sub(1)?)?.rsplit('\n').next()?;
            let following = ctx.content.get(at..)?.split('\n').next()?;
            if edit.replacement != "\n"
                || at == 0
                || at == ctx.content.len()
                || previous.trim().is_empty()
                || following.trim().is_empty()
                || before.inside_list(at)
                || previous.contains('|')
                || following.contains('|')
                || crate::lint_context::is_horizontal_rule_line(following)
                || ctx.is_inside_template_code(at)
                || ctx
                    .line_info(ctx.offset_to_line_col(at).0)
                    .is_some_and(|line| line.in_front_matter)
                || ctx
                    .links()
                    .iter()
                    .any(|link| link.byte_offset < at && at < link.byte_end)
                || ctx
                    .images()
                    .iter()
                    .any(|image| image.byte_offset < at && at < image.byte_end)
                || ctx
                    .broken_links()
                    .iter()
                    .any(|link| link.span.start < at && at < link.span.end)
            {
                return None;
            }
        }
        // Compare every native event, including text, code, inline spans,
        // list membership and tightness. List counts alone are insufficient.
        return preserves_structure(ctx, before, native).then_some(edits);
    }
    if !rule.fixes_from_diagnostics() {
        let native = prepared
            .get_or_insert_with(|| rule.fix_with_diagnostics(ctx, warnings))
            .as_ref()
            .ok()?;
        let applied = crate::utils::fix_utils::apply_edits(ctx.content, fixes.iter()).ok()?;
        if applied.as_ref() != native {
            return None;
        }
    }
    Some(fixes)
}

fn preserves_structure(ctx: &LintContext<'_>, before: &crate::utils::list_fix_guard::Parsed<'_>, native: &str) -> bool {
    use crate::utils::list_fix_guard::{Allowed, Parsed};

    if ctx.certifies_list_guard(native, Allowed::Nothing) {
        return true;
    }

    before.equivalent(&Parsed::new(native), Allowed::Nothing)
}

fn native_blank_insertions(source: &str, native: &str) -> Option<Vec<Fix>> {
    if native.len() <= source.len() {
        return None;
    }
    let (mut before, mut after) = (0, 0);
    let mut edits = Vec::new();
    while before < source.len() || after < native.len() {
        if before < source.len() && source.as_bytes().get(before) == native.as_bytes().get(after) {
            before += 1;
            after += 1;
            continue;
        }
        if (before > 0 && source.as_bytes()[before - 1] != b'\n') || !source.is_char_boundary(before) {
            return None;
        }
        let start = after;
        while native.as_bytes().get(after) == Some(&b'\n') {
            after += 1;
        }
        if start == after {
            return None;
        }
        edits.push(Fix::new(before..before, native[start..after].to_owned()));
    }
    let applied = crate::utils::fix_utils::apply_edits(source, edits.iter()).ok()?;
    (applied.as_ref() == native).then_some(edits)
}

struct Stage {
    edits: Vec<Fix>,
    hash: u64,
    name: &'static str,
    index: usize,
}

#[derive(Default)]
pub(super) struct CleanupBatch {
    stages: Vec<Stage>,
    output: Option<String>,
    stop: bool,
}

pub(super) struct FinishedBatch {
    pub content: String,
    pub stages: Vec<(u64, &'static str, usize)>,
}

impl CleanupBatch {
    pub(super) fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }
    pub(super) fn len(&self) -> usize {
        self.stages.len()
    }
    pub(super) fn terminal(&self) -> bool {
        self.stop
    }

    pub(super) fn can_check(&self, index: usize, rule: &dyn Rule, ctx: &LintContext<'_>) -> bool {
        if self.stop
            || index <= self.stages.last().unwrap().index
            || crate::format_proof::NativeFormatPolicy::validated(rule).is_none()
        {
            return false;
        }
        let r = rule.as_any();
        // The native policy exists only for the default proper-name settings,
        // whose name list is empty. Its negative result is independent of every
        // pending source edit. Keep configured names on the fresh-context path.
        if r.is::<MD044ProperNames>() {
            return true;
        }
        // Admitted cleanup edits cannot introduce a footnote marker. Keep the
        // old negative result only when there are no footnotes to reorder.
        if r.is::<MD067FootnoteDefinitionOrder>() {
            return !ctx.content.contains("[^");
        }
        // validated() admits only MD025's default configuration here.
        // Its separator exception uses a five-physical-line lookback, so
        // even structurally invisible blank insertions can change it. Only a
        // document with no metadata title and at most one native level-one
        // heading has a negative result independent of that lookback.
        if r.is::<MD025SingleTitle>() {
            return ctx
                .headings()
                .filter(|heading| heading.heading.level == 1)
                .take(2)
                .count()
                <= 1
                && !MD025SingleTitle::default_front_matter_lines_have_title(ctx.raw_lines());
        }
        // Every accumulated effect must authorize the reader. Neither a later
        // gap nor an inline edit can erase an earlier source restriction.
        self.stages.iter().all(|stage| match stage.name {
            "MD064" => {
                layout_reader(rule)
                    || r.is::<MD005ListIndent>()
                    || r.is::<MD007ULIndent>()
                    || r.is::<MD010NoHardTabs>()
                    || r.is::<MD011NoReversedLinks>()
                    || r.is::<MD014CommandsShowOutput>()
                    || r.is::<MD018NoMissingSpaceAtx>()
                    || r.is::<MD019NoMultipleSpaceAtx>()
                    || r.is::<MD020NoMissingSpaceClosedAtx>()
                    || r.is::<MD021NoMultipleSpaceClosedAtx>()
                    || r.is::<MD022BlanksAroundHeadings>()
            }
            "MD022" | "MD031" | "MD032" | "MD077" => layout_reader(rule),
            "MD034" | "MD049" => inline_reader(rule),
            // Trailing cleanup can remove a hard break; only excess-blank
            // cleanup, which reads physical blankness, may follow it.
            "MD009" => r.is::<MD012NoMultipleBlanks>(),
            _ => false,
        })
    }

    pub(super) fn try_push(
        &mut self,
        source: &str,
        index: usize,
        name: &'static str,
        fixes: Vec<Fix>,
        history: &[(u64, &str)],
    ) -> bool {
        let Some(mut added) = normalize(source, name, fixes) else {
            return false;
        };
        let mut edits = self.stages.last().map_or_else(Vec::new, |s| s.edits.clone());
        edits.append(&mut added);
        edits.sort_by_key(|e| (e.range.start, e.range.end));
        let mut merged: Vec<Fix> = Vec::with_capacity(edits.len());
        for edit in edits {
            if let Some(last) = merged.last_mut()
                && edit.range.start <= last.range.end
            {
                // MD009 inside a blank line removed by MD012. Only empty
                // replacements commute across an overlapping source range.
                if last.replacement.is_empty() && edit.replacement.is_empty() {
                    last.range.end = last.range.end.max(edit.range.end);
                    continue;
                }
                return false;
            }
            merged.push(edit);
        }
        let Ok(output) = crate::utils::fix_utils::apply_edits(source, merged.iter()) else {
            return false;
        };
        if output.as_ref() == self.output.as_deref().unwrap_or(source) {
            return false;
        }
        let hash = hash_content(&output);
        self.stop =
            name == "MD012" || history.iter().any(|s| s.0 == hash) || self.stages.iter().any(|s| s.hash == hash);
        self.output = Some(output.into_owned());
        self.stages.push(Stage {
            edits: merged,
            hash,
            name,
            index,
        });
        true
    }

    pub(super) fn finish(self) -> Option<FinishedBatch> {
        Some(FinishedBatch {
            content: self.output?,
            stages: self.stages.into_iter().map(|s| (s.hash, s.name, s.index)).collect(),
        })
    }
}

// A strict word contraction preserves line prefixes, every native parser tag
// and all code/literal payloads. An admitted outer-gap insertion also preserves
// all original line contents, list-interior gaps and table/thematic adjacency.
// A continuation-link dedent changes only discarded indentation on a whole
// native link line. Positive unsupported fixers always flush before execution.
fn layout_reader(rule: &dyn Rule) -> bool {
    let r = rule.as_any();
    inline_reader(rule)
        || r.is::<MD023HeadingStartLeft>()
        || r.is::<MD026NoTrailingPunctuation>()
        || r.is::<MD027MultipleSpacesBlockquote>()
        || r.is::<MD028NoBlanksBlockquote>()
        || r.is::<MD029OrderedListPrefix>()
        || r.is::<MD030ListMarkerSpace>()
        || r.is::<MD031BlanksAroundFences>()
        || r.is::<MD032BlanksAroundLists>()
        || r.is::<MD034NoBareUrls>()
        || r.is::<MD077ListContinuationIndent>()
}

fn inline_reader(rule: &dyn Rule) -> bool {
    let r = rule.as_any();
    // Accepted edits leave physical line prefixes, blankness, strong runs,
    // existing link destinations and thematic-break markers unchanged.
    // They also preserve table row/delimiter boundaries, pipe positions,
    // escaped-pipe and code-span membership, and the number of cells.
    // A table reader with edits still ends this batch before its native fix.
    // MD054's certified default permits every link/image style.
    // These URL/emphasis edits also preserve every backtick/tilde run,
    // code-span body, physical code-block prefix and fence info string.
    // Native code readers can therefore prove their old negative result
    // against the same code regions. A positive custom fix still flushes
    // the batch before it runs, using a fresh native context.
    r.is::<MD009TrailingSpaces>()
        || r.is::<MD012NoMultipleBlanks>()
        || r.is::<MD035HRStyle>()
        || r.is::<MD038NoSpaceInCode>()
        || r.is::<MD039NoSpaceInLinks>()
        || r.is::<MD040FencedCodeLanguage>()
        || r.is::<MD042NoEmptyLinks>()
        || r.is::<MD046CodeBlockStyle>()
        || r.is::<MD047SingleTrailingNewline>()
        || r.is::<MD048CodeFenceStyle>()
        || r.is::<MD049EmphasisStyle>()
        || r.is::<MD050StrongStyle>()
        || r.is::<MD054LinkImageStyle>()
        || r.is::<MD055TablePipeStyle>()
        || r.is::<MD056TableColumnCount>()
        || r.is::<MD058BlanksAroundTables>()
        || r.is::<MD062LinkDestinationWhitespace>()
        || r.is::<MD065BlanksAroundHorizontalRules>()
        || r.is::<MD069NoDuplicateListMarkers>()
        || r.is::<MD071BlankLineAfterFrontmatter>()
        || r.is::<MD075OrphanedTableRows>()
        || r.is::<MD076ListItemSpacing>()
}

fn normalize(source: &str, name: &str, fixes: Vec<Fix>) -> Option<Vec<Fix>> {
    let mut atomic = Vec::new();
    let mut previous_end = None;
    let mut fixes = fixes;
    fixes.sort_by_key(|f| (f.range.start, f.range.end));
    for fix in fixes {
        if !fix.additional_edits.is_empty()
            || (fix.range.is_empty() && !matches!(name, "MD022" | "MD031" | "MD032"))
            || previous_end.is_some_and(|end| fix.range.start < end)
        {
            return None;
        }
        let text = source.get(fix.range.clone())?;
        let line_start = source[..fix.range.start].rfind('\n').map_or(0, |p| p + 1);
        let prefix = &source[line_start..fix.range.start];
        match name {
            "MD064" => {
                if text.len() < 2
                    || !text.bytes().all(|b| b == b' ')
                    || !matches!(fix.replacement.as_str(), " " | "  ")
                    || fix.replacement.len() >= text.len()
                    || !fix
                        .range
                        .start
                        .checked_sub(1)
                        .and_then(|i| source.as_bytes().get(i))
                        .is_some_and(word_spacing_neighbor)
                    || !source.as_bytes().get(fix.range.end).is_some_and(word_spacing_neighbor)
                {
                    return None;
                }
                atomic.push(fix.clone());
            }
            "MD022" => {
                if !fix.range.is_empty() || !prefix.is_empty() || fix.replacement != "\n" {
                    return None;
                }
                atomic.push(fix.clone());
            }
            "MD077" => {
                if !prefix.is_empty()
                    || text.is_empty()
                    || !text.bytes().all(|byte| byte == b' ')
                    || fix.replacement.len() < 2
                    || fix.replacement.len() >= text.len()
                    || !fix.replacement.bytes().all(|byte| byte == b' ')
                {
                    return None;
                }
                atomic.push(fix.clone());
            }
            "MD031" | "MD032" => {
                if !fix.range.is_empty() || !prefix.is_empty() || fix.replacement != "\n" {
                    return None;
                }
                atomic.push(fix.clone());
            }
            "MD034" => {
                if !text
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".:/?#=&%+@-".contains(&b))
                    || !text.contains("://")
                    || fix.replacement != format!("<{text}>")
                    || [fix.range.start.checked_sub(1), Some(fix.range.end)]
                        .into_iter()
                        .flatten()
                        .filter_map(|i| source.as_bytes().get(i))
                        .any(|b| b"_*`\\<>".contains(b))
                {
                    return None;
                }
                atomic.push(fix.clone());
            }
            "MD049" => {
                let bytes = text.as_bytes();
                let replacement = fix.replacement.as_bytes();
                if bytes.len() < 3
                    || bytes.len() != replacement.len()
                    || !matches!((bytes[0], replacement[0]), (b'_', b'*') | (b'*', b'_'))
                    || bytes[bytes.len() - 1] != bytes[0]
                    || replacement[bytes.len() - 1] != replacement[0]
                    || bytes[1..bytes.len() - 1] != replacement[1..bytes.len() - 1]
                {
                    return None;
                }
                for offset in [fix.range.start, fix.range.end - 1] {
                    let start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
                    let leading_opener = offset == fix.range.start
                        && source.as_bytes().get(offset + 1).is_some_and(u8::is_ascii_alphanumeric);
                    if (source[start..offset].trim_matches(' ').is_empty() && !leading_opener)
                        || [offset.checked_sub(1), Some(offset + 1)]
                            .into_iter()
                            .flatten()
                            .filter_map(|i| source.as_bytes().get(i))
                            .any(|b| matches!(b, b'*' | b'_'))
                    {
                        return None;
                    }
                    atomic.push(Fix::new(offset..offset + 1, (replacement[0] as char).to_string()));
                }
            }
            "MD009" => {
                if !fix.replacement.is_empty()
                    || !text.bytes().all(|b| b == b' ')
                    || !matches!(source.as_bytes().get(fix.range.end), None | Some(b'\n'))
                {
                    return None;
                }
                atomic.push(fix.clone());
            }
            "MD012" => {
                if !fix.replacement.is_empty()
                    || !prefix.is_empty()
                    || !text.bytes().all(|b| matches!(b, b' ' | b'\n'))
                    || (fix.range.end != source.len() && !text.ends_with('\n'))
                {
                    return None;
                }
                atomic.push(fix.clone());
            }
            _ => return None,
        }
        previous_end = Some(fix.range.end);
    }
    (!atomic.is_empty()).then_some(atomic)
}

fn word_spacing_neighbor(byte: &u8) -> bool {
    // A colon after prose or a code delimiter is also an ordinary inter-word
    // boundary. Keeping at least one space preserves the delimiter runs. The
    // native event certificate still compares every tag and literal payload;
    // spacing inside code, attributes or destinations never becomes prose.
    byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'`')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, document_run::DocumentRun};

    #[derive(Clone, Default)]
    struct CountedHeadingFix {
        inner: MD022BlanksAroundHeadings,
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl Rule for CountedHeadingFix {
        fn name(&self) -> &'static str {
            self.inner.name()
        }
        fn description(&self) -> &'static str {
            self.inner.description()
        }
        fn as_any(&self) -> &dyn std::any::Any {
            &self.inner
        }
        fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
            self.inner.native_format_policy()
        }
        fn check(&self, ctx: &LintContext) -> crate::rule::LintResult {
            self.inner.check(ctx)
        }
        fn fix(&self, ctx: &LintContext) -> Result<String, crate::rule::LintError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.inner.fix(ctx)
        }
        fn category(&self) -> crate::rule::RuleCategory {
            self.inner.category()
        }
    }

    #[test]
    fn rejected_native_cleanup_reuses_exact_fix_without_a_second_call() {
        use crate::fix_coordinator::FixCoordinator;
        use std::sync::atomic::Ordering;

        let config = Config::default();
        for source in [
            "# Heading\n| a | b |\n| - | - |\n| 1 | 2 |\n",
            "| a | b |\n| - | - |\n| 1 | 2 |\n# Heading\n",
        ] {
            let inner = MD022BlanksAroundHeadings::default();
            let ctx = LintContext::new(source, config.markdown_flavor(), None);
            let warnings = inner.check(&ctx).unwrap();
            assert!(!warnings.is_empty());
            let mut prepared = None;
            assert!(capture_with_native(&inner, &ctx, &warnings, &mut prepared).is_none());
            let prepared = prepared.unwrap().unwrap();
            assert_ne!(prepared, source);

            let mut outcomes = Vec::new();
            for coordinator in [FixCoordinator::new(), FixCoordinator::for_formatter()] {
                let rule = CountedHeadingFix::default();
                let calls = rule.calls.clone();
                let rules: Vec<Box<dyn Rule>> = vec![Box::new(rule)];
                let mut content = source.to_owned();
                let result = coordinator
                    .apply_fixes_iterative(&rules, &[], &mut content, &config, 100, None)
                    .unwrap();
                assert_eq!(content, prepared);
                assert_eq!(calls.load(Ordering::Relaxed), 1, "{source:?}");
                outcomes.push((content, result));
            }
            let (native, actual) = (&outcomes[0].1, &outcomes[1].1);
            assert_eq!(actual.rules_fixed, native.rules_fixed);
            assert_eq!(actual.fixed_rule_names, native.fixed_rule_names);
            assert_eq!(actual.iterations, native.iterations);
            assert_eq!(actual.converged, native.converged);
            assert_eq!(actual.conflicting_rules, native.conflicting_rules);
            assert_eq!(actual.conflict_cycle, native.conflict_cycle);
        }
    }

    fn compare(source: &str, limit: usize) -> (super::super::FixResult, super::super::FixResult) {
        let config = Config::default();
        let rules = crate::rules::all_rules(&config);
        let run = DocumentRun::new(source, &rules, &config);
        let (native, a) = run.fix(limit).unwrap();
        let (candidate, b) = run.fix_for_formatter(limit).unwrap();
        assert_eq!(candidate, native, "{source:?}, limit {limit}");
        assert_eq!(a.rules_fixed, b.rules_fixed, "{source:?}");
        assert_eq!(a.iterations, b.iterations, "{source:?}");
        assert_eq!(a.fixed_rule_names, b.fixed_rule_names, "{source:?}");
        assert_eq!(a.converged, b.converged, "{source:?}");
        assert_eq!(a.conflict_cycle, b.conflict_cycle, "{source:?}");
        (a, b)
    }

    #[test]
    fn ordered_cleanup_reduces_contexts_and_preserves_iteration_budget() {
        let text = "# Title\n\nText https://example.com and _emphasis_ with *one* and *two*. \n\n\nEnd.\n";
        let (native, candidate) = compare(text, 100);
        assert!(candidate.context_creations < native.context_creations);
        for limit in 0..8 {
            compare(text, limit);
        }
    }

    #[test]
    fn native_word_spacing_and_heading_edits_share_one_context() {
        let source = "# Heading\nText  body\n## Next\nMore  words\n";
        audit_native_readers(source);
        let (native, candidate) = compare(source, 100);
        assert!(candidate.context_creations < native.context_creations);
        for limit in 0..8 {
            compare(source, limit);
        }
    }

    #[test]
    fn native_word_spacing_next_to_code_and_colons_keeps_literal_payloads() {
        let source = "# Changelog\n- `multiple-build-scripts`:  Accessing each build script's `OUT_DIR`\n- Clarify the precedence of  `RUSTC_WORKSPACE_WRAPPER`.\n- Add unstable `--output-format` option to  `cargo rustdoc`.\n- An ordinary unaligned item\n## Next\nMore  words\n";
        let ctx = LintContext::new(source, Config::default().markdown_flavor(), None);
        let rule = MD064NoMultipleConsecutiveSpaces::default();
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 4);
        let edits = capture(&rule, &ctx, &warnings).expect("native changelog word contractions must be admitted");
        let applied = crate::utils::fix_utils::apply_edits(source, edits.iter()).unwrap();
        assert_eq!(applied.as_ref(), rule.fix(&ctx).unwrap());
        assert!(applied.contains("`multiple-build-scripts`: Accessing"));
        assert!(applied.contains("of `RUSTC_WORKSPACE_WRAPPER`"));
        audit_native_readers(source);
        let (native, candidate) = compare(source, 100);
        assert!(candidate.context_creations < native.context_creations);
        for limit in 0..12 {
            compare(source, limit);
        }
    }

    #[test]
    fn word_spacing_certificate_preserves_code_and_structural_payloads() {
        use crate::utils::list_fix_guard::Parsed;
        assert!(Parsed::new("ordinary  words\n").equivalent_word_spacing(&Parsed::new("ordinary words\n")));
        for (before, after) in [
            ("`literal  words`\n", "`literal words`\n"),
            ("```rust\nliteral  words\n```\n", "```rust\nliteral words\n```\n"),
            ("$literal  words$\n", "$literal words$\n"),
            ("[label](<destination  words>)\n", "[label](<destination words>)\n"),
            (
                "<span title=\"literal  words\">text</span>\n",
                "<span title=\"literal words\">text</span>\n",
            ),
            ("ordinary  words\n", "ordinary\nwords\n"),
            ("- ordinary  words\n", "ordinary words\n"),
        ] {
            assert!(
                !Parsed::new(before).equivalent_word_spacing(&Parsed::new(after)),
                "{before:?}"
            );
        }
    }

    #[test]
    fn title_separator_lookback_requires_a_source_independent_negative() {
        let config = Config::default();
        let reader = crate::rules::create_rule_by_name("MD025", &config).unwrap();
        for (source, admitted) in [
            ("# First\nword  word\n", true),
            ("# First\nword  word\n\n***\n\n\n\n# Second\n", false),
            ("---\ntitle: Metadata\n---\n# First\nword  word\n", false),
        ] {
            let ctx = LintContext::new(source, config.markdown_flavor(), None);
            let start = source.find("  word").unwrap();
            let mut batch = CleanupBatch::default();
            assert!(batch.try_push(
                source,
                0,
                "MD064",
                vec![Fix::new(start..start + 2, " ".to_owned())],
                &[]
            ));
            assert_eq!(batch.can_check(1, reader.as_ref(), &ctx), admitted);
            audit_native_readers(source);
            for limit in 0..10 {
                compare(source, limit);
            }
        }
    }

    #[test]
    fn native_heading_word_and_link_continuation_effects_share_analysis() {
        let source = "# Title\nText  body\n## Next\nMore  words\n- item\n    [reference](https://example.test/ref)\n- next\n\nText https://example.com and _word_ with *one* and *two*. \n\n\nEnd.\n";
        let config = Config::default();
        let ctx = LintContext::new(source, config.markdown_flavor(), None);
        for name in ["MD064", "MD022", "MD077", "MD032"] {
            let rule = crate::rules::create_rule_by_name(name, &config).unwrap();
            let warnings = rule.check(&ctx).unwrap();
            let edits = capture(rule.as_ref(), &ctx, &warnings).expect("fixture must exercise admitted native effect");
            assert_eq!(
                crate::utils::fix_utils::apply_edits(source, edits.iter())
                    .unwrap()
                    .as_ref(),
                rule.fix(&ctx).unwrap(),
                "{name}"
            );
        }
        let readers = audit_native_readers(source);
        for name in ["MD032", "MD034", "MD058", "MD065", "MD071", "MD075", "MD076", "MD077"] {
            assert!(
                readers.contains(name),
                "combined fixture did not exercise native reader {name}"
            );
        }
        let (native, candidate) = compare(source, 100);
        assert!(
            candidate.context_creations + 2 <= native.context_creations,
            "expected multiple avoided analyses, native {}, candidate {}",
            native.context_creations,
            candidate.context_creations
        );
        for limit in 0..12 {
            compare(source, limit);
        }
    }

    #[test]
    fn expanded_source_effects_preserve_each_native_intermediate_revision() {
        for prefix in ["", "- ", "> ", "    "] {
            for body in [
                "word  word\n## Heading\nAfter  words",
                "word  word\n\n- item\n    [reference](https://example.test/ref)\n- next",
                "word  word\n```rust\nlet x = 1;\n```\nAfter",
                "| a  b | c |\n| --- | --- |\n| one | two |\n\n| orphan | row |",
                "word  word\n---\nAfter",
                "word  word\n***\nAfter",
                "[multi  word\nlabel](https://example.test/ref)",
                "word  word\n\n- one\n\n  continued\n\n- two",
                "{{ template  value }}\nword  word",
                "[reference  label]: <https://example.test/ref>\n\n[reference label]",
                "`literal  code` and word  word",
                "<!-- literal  comment -->\nword  word",
                "word:  `literal  code` and word  word",
                "[reference]:  <https://example.test/ref> \"literal  title\"\n\n[reference]",
                "#  Heading  with `literal  code`\nAfter  words",
                "1.  item  with `literal  code`\n2. next",
            ] {
                let body = body.lines().fold(String::new(), |mut text, line| {
                    text.push_str(prefix);
                    text.push_str(line);
                    text.push('\n');
                    text
                });
                let source =
                    format!("# Title\n{body}\nText https://example.com and _word_ with *one* and *two*. \n\n\nEnd.\n");
                audit_native_readers(&source);
                compare(&source, 100);
            }
        }
    }

    #[test]
    fn native_gap_writers_preserve_intermediate_readers_and_iteration_budget() {
        let config = Config::default();
        let sources = [
            (
                "MD031",
                "# Title\n\nIntro\n```rust\nlet x = 1;\n```\nTail https://example.com and _word_ with *one* and *two*. \n\n\nEnd.\n",
            ),
            (
                "MD032",
                "# Title\n\nIntro\n- Item\n\nText https://example.com and _word_ with *one* and *two*. \n\n\nEnd.\n",
            ),
        ];
        let mut covered = std::collections::BTreeSet::new();
        for (name, source) in sources {
            let rule = crate::rules::create_rule_by_name(name, &config).unwrap();
            let ctx = LintContext::new(source, config.markdown_flavor(), None);
            let warnings = rule.check(&ctx).unwrap();
            let edits = capture(rule.as_ref(), &ctx, &warnings).expect("fixture must admit its native gap writer");
            let mut batch = CleanupBatch::default();
            assert!(batch.try_push(source, 0, rule.name(), edits, &[]));
            assert_eq!(batch.output.as_deref().unwrap(), rule.fix(&ctx).unwrap());
            covered.extend(audit_native_readers(source));
            for limit in 0..12 {
                compare(source, limit);
            }
        }
        for name in [
            "MD032", "MD034", "MD038", "MD040", "MD044", "MD046", "MD048", "MD049", "MD058", "MD065", "MD069", "MD071",
            "MD075", "MD076",
        ] {
            assert!(
                covered.contains(name),
                "gap fixture did not exercise native reader {name}"
            );
        }
        let (native, candidate) = compare(sources[1].1, 100);
        assert!(candidate.context_creations < native.context_creations);
    }

    #[test]
    fn repeated_gap_insertion_flushes_without_inventing_a_second_native_fix() {
        let source = "First\nSecond\n";
        let mut batch = CleanupBatch::default();
        assert!(batch.try_push(source, 0, "MD031", vec![Fix::new(6..6, "\n".to_owned())], &[]));
        assert!(!batch.try_push(source, 1, "MD032", vec![Fix::new(6..6, "\n".to_owned())], &[]));
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.output.as_deref(), Some("First\n\nSecond\n"));
    }

    #[test]
    fn native_gap_crossproduct_keeps_each_native_intermediate_result() {
        for prefix in ["", "> ", "- ", "  "] {
            for body in [
                "Intro\n- Item\n\nAfter",
                "Intro\n1. Item\n\nAfter",
                "- Item\n## Heading\nAfter",
                "Intro\n```rust\nlet x = 1;\n```\nAfter",
                "Intro\n~~~rust\nlet x = 1;\n~~~\nAfter",
                "- Item\n  ```rust\n  let x = 1;\n  ```\nAfter",
                "- Item\n\n  Continued\n\n- Next",
                "[multiline\n- label](https://example.com)",
                "[multiline\n```rust\nlabel](https://example.com)",
                "$$\n- math\n$$",
                "<!--\n- literal\n-->\nIntro\n- Item",
                "<div>\n- literal\n</div>\nIntro\n- Item",
            ] {
                let body = body.lines().fold(String::new(), |mut text, line| {
                    text.push_str(prefix);
                    text.push_str(line);
                    text.push('\n');
                    text
                });
                for tail in [
                    "Text https://example.com and _word_ with *one* and *two*. \n\n\nEnd.\n",
                    "| Header | Other |\n| --- | --- |\n| _word_ | Cell |\n",
                    "Text [ x ](url) and ` code ` and ![image](img.png).\n",
                    "\n***\n\nEnd.\n",
                ] {
                    let source = format!("# Title\n\n{body}\n{tail}");
                    audit_native_readers(&source);
                    compare(&source, 100);
                }
            }
        }
    }

    #[test]
    fn heading_spacing_effects_match_each_native_intermediate_revision() {
        for prefix in ["", "- ", "  - ", "> ", "1. ", "# "] {
            for body in [
                "word  word",
                "word   word",
                "word.  Word",
                "_word_  word",
                "é  word",
                "(word  word)[https://example.com]",
                "(https://example.com)[word  word]",
                "$ echo word  word",
                "# word  word #",
            ] {
                for heading in ["# Heading", "## Heading", "Title\n---", "#Heading", "<!-- comment -->"] {
                    for gap in ["\n", "\n\n", "\n \n"] {
                        let source = format!("# Title\n{prefix}{body}{gap}{heading}\nAfter  text\n");
                        audit_native_readers(&source);
                        compare(&source, 100);
                    }
                }
            }
        }
    }

    #[test]
    fn inline_cleanup_preserves_native_table_readers_at_each_revision() {
        let mut readers = std::collections::BTreeSet::new();
        for prefix in ["", "> ", "  ", "    ", "- ", "  - "] {
            for value in [
                "https://example.com",
                "_word_ with *one* and *two*",
                "https://example.com and _word_ with *one* and *two*",
                "https://example.com/a_b and _word_",
                "`x | y` and _word_ with *one* and *two*",
                "a \\| b and _word_ with *one* and *two*",
            ] {
                for table in [
                    format!("| Header | Other |\n| --- | --- |\n| {value} | Cell |\n"),
                    format!("Header | Other\n--- | ---\n{value} | Cell\n"),
                    format!("| Header | Other |\n| --- | --- |\n{value} | Cell\n"),
                    format!("| Header | Other |\n| --- | --- |\n| {value} | Cell | Extra |\n"),
                    format!("| Header | Other |\n| --- | --- |\n| Cell | Cell |\n\n| {value} | Cell |\n"),
                    format!("| {value} | Cell |\n| Other | Cell |\n"),
                    format!("`not | a table` {value}\n"),
                    format!("| --- | --- |\n| {value} |\n"),
                ] {
                    let table = table.lines().fold(String::new(), |mut text, line| {
                        text.push_str(prefix);
                        text.push_str(line);
                        text.push('\n');
                        text
                    });
                    for gap in ["\n", "\n\n", "\n \n"] {
                        let source = format!(
                            "# Title\n\nText https://example.com and _word_ with *one* and *two*.{gap}{table}\n\nEnd. \n"
                        );
                        readers.extend(audit_native_readers(&source));
                        compare(&source, 100);
                    }
                }
            }
        }
        for name in ["MD055", "MD056", "MD058", "MD075"] {
            assert!(readers.contains(name), "native table oracle did not exercise {name}");
        }
    }

    #[test]
    fn structural_and_literal_boundaries_preserve_native_order() {
        for source in [
            "#NoSpace\n\n# Other\n\n# Third\n",
            "<!-- comment\n~~~\n~~~\n```rust\n-->\n",
            "Text _https://example.com_ and *one* and *two*. \n\n\nEnd\n",
            "Text https://example.com/a_b and _a_ with **b**. \n\n\nEnd\n",
            "Text https://example.com and _a\n b_ with *c* and *d*. \n\n\nEnd\n",
            "Text https://example.com and _a_ with *b* and *c*.   \n\n\nEnd\n",
            "Text https://example.com and _a_ with *b* and *c*. \n   \n \nEnd\n",
            "Text https://example.com <!-- rumdl-disable MD049 --> _a_ and *b*. \n\n\nEnd\n",
            "[label]: https://example.com\n\nText _a_ and *b* and *c*. \n\n\nEnd\n",
            "https://example.com\n\n_word_ and *one* and *two*.   \n \n  \nEnd\n",
            "_word_ and *one* and *two*.   \n---\n\n\nEnd\n",
            "Text   \n    indented\n\n\nEnd\n",
            "Text _word\n_ and *one* and *two*.   \n\n\nEnd\n",
            "Text   \n\n\n\n\nEnd\n\n\n\n",
            "Text   \n \n  \n   \n    \nEnd\n",
            "# Title\n\nText https://example.com and _word_ with *one* and *two*.\n---\nEnd\n",
            "---\nname: value\n---\nText https://example.com and _word_ with *one* and *two*.\n",
            "Text https://example.com and _word_ with *one* and *two*.\n***\nEnd\n",
            "---\n---\n#  heading!  ##\ntext with a fairly long sentence that should wrap at the configured line width.\n",
        ] {
            compare(source, 100);
            audit_native_readers(source);
        }
    }

    // A final fixed point can hide an invalid stale negative check. Audit the
    // native reader at the exact point the new execution would reuse it.
    fn audit_native_readers(source: &str) -> std::collections::BTreeSet<&'static str> {
        let config = Config::default();
        let rules = crate::rules::all_rules(&config);
        let mut readers = audit_native_readers_with_rules(source, &rules);
        // The CLI excludes opt-in policies. Also audit that real ordering: an
        // opt-in custom fixer may otherwise end every batch before a later
        // default rule and leave its new reader contract entirely unexercised.
        let opt_in = crate::rules::opt_in_rules();
        let default_rules: Vec<_> = rules.into_iter().filter(|rule| !opt_in.contains(rule.name())).collect();
        readers.extend(audit_native_readers_with_rules(source, &default_rules));
        readers
    }

    fn audit_native_readers_with_rules(
        source: &str,
        rules: &[Box<dyn Rule>],
    ) -> std::collections::BTreeSet<&'static str> {
        let mut readers = std::collections::BTreeSet::new();
        let config = Config::default();
        let coordinator = super::super::FixCoordinator::new();
        let ordered = coordinator.get_optimal_order(rules);
        let old = LintContext::new(source, config.markdown_flavor(), None);
        for (first, &writer) in ordered.iter().enumerate() {
            let warnings = writer.check(&old).unwrap();
            let Some(edits) = capture(writer, &old, &warnings) else {
                continue;
            };
            let mut batch = CleanupBatch::default();
            if !batch.try_push(source, first, writer.name(), edits, &[]) {
                continue;
            }
            assert_eq!(batch.output.as_deref().unwrap(), writer.fix(&old).unwrap());
            for (index, &reader) in ordered.iter().enumerate().skip(first + 1) {
                if reader.fix_capability() == crate::rule::FixCapability::Unfixable
                    || reader
                        .native_format_policy()
                        .is_some_and(crate::format_proof::NativeFormatPolicy::configuration_only)
                {
                    continue;
                }
                if !batch.can_check(index, reader, &old) {
                    // Optional trace for this independent test oracle only.
                    if std::env::var_os("RUMDL_CLEANUP_TRACE").is_some() {
                        eprintln!("native effect {} stops at reader {}", writer.name(), reader.name());
                    }
                    break;
                }
                readers.insert(reader.name());
                let after = batch.output.as_ref().unwrap().clone();
                let fresh = LintContext::new(&after, config.markdown_flavor(), None);
                let warnings = reader.check(&old).unwrap();
                if matches!(
                    super::super::FixPlan::capture(reader, &old, &warnings),
                    super::super::FixPlan::Unchanged
                ) {
                    assert_eq!(
                        reader.fix(&fresh).unwrap(),
                        after,
                        "stale negative {}, after {}, source {source:?}",
                        reader.name(),
                        writer.name()
                    );
                    continue;
                }
                let Some(edits) = capture(reader, &old, &warnings) else {
                    break;
                };
                if !batch.try_push(source, index, reader.name(), edits, &[]) {
                    break;
                }
                assert_eq!(
                    batch.output.as_deref().unwrap(),
                    reader.fix(&fresh).unwrap(),
                    "stale edits {}, after {}, source {source:?}",
                    reader.name(),
                    writer.name()
                );
            }
        }
        readers
    }

    #[test]
    fn cleanup_effect_crossproduct_matches_native() {
        let prefixes = ["Text ", "- Text ", "> Text ", "# Text ", "", "    Text "];
        let urls = [
            "https://example.com",
            "https://example.com/a_b",
            "www.example.com",
            "user@example.com",
            "_https://example.com_",
            "[x](https://example.com)",
        ];
        let emph = [
            " and _a_ with *b* and *c*.",
            " and _a **b** c_ and *d* and *e*.",
            " and _a\n b_ and *c* and *d*.",
            " and `x _a_` and *b*.",
        ];
        for prefix in prefixes {
            for url in urls {
                for e in emph {
                    for spaces in ["", " ", "  ", "   "] {
                        for blanks in ["\n", "\n\n", "\n \n  \n", ""] {
                            let source = format!("# Title\n\n{prefix}{url}{e}{spaces}{blanks}End.\n");
                            compare(&source, 100);
                            audit_native_readers(&source);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn code_readers_preserve_native_negatives_after_url_and_emphasis_edits() {
        let mut covered = std::collections::BTreeSet::new();
        for prefix in ["", "- ", "> "] {
            for text in [
                "https://example.com and _word_ with *one* and *two*.",
                "`code` https://example.com `backtick` and _word_ with *one* and *two*.",
                "` code ` https://example.com `` backtick `` and _word_ with *one* and *two*.",
                "https://example.com/code and ``code ` nested`` with _word_ and *one* and *two*.",
            ] {
                for code in [
                    "",
                    "```rust\nlet x = 1;\n```\n",
                    "```\nlet x = 1;\n```\n",
                    "~~~rust\nlet x = 1;\n~~~\n",
                    "```rust\n~~~\n```\n",
                    "```rust\nunclosed\n",
                    "    indented code\n",
                    "<!-- ` literal ` and ~~~ -->\n",
                ] {
                    for gap in ["\n", "\n\n"] {
                        let source = format!("# Title\n\n{prefix}{text}{gap}{code}\nEnd.\n");
                        covered.extend(audit_native_readers(&source));
                        compare(&source, 100);
                    }
                }
            }
        }
        for name in ["MD038", "MD040", "MD044", "MD046", "MD048"] {
            assert!(covered.contains(name), "native reader contract never exercised: {name}");
        }
    }
}
