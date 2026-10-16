//! Carry the parser's facts for an exact candidate into the next context.
//!
//! A list safety check already parses its proposed output. Keeping the facts
//! from that read avoids parsing that same output again after it is applied.
//! Flavor carving and every source-derived context pass still run afresh.

use super::{ParsedDocument, PendingInlineLinks, PendingLineEvents, PendingLinks, blocks};
use crate::lint_context::types::BrokenLinkInfo;
use crate::utils::code_block_utils::ParseResult;
use pulldown_cmark::Event;
use std::ops::Range;

pub(crate) struct OwnedParsedDocument {
    source: String,

    list_guard_policy: Option<crate::utils::list_fix_guard::Allowed>,

    format_proof_events: Option<crate::format_proof::events::ProofEvents>,
    structure: ParseResult,

    has_math: bool,
    parser_blocks: Option<blocks::BlockFacts>,
    events: Vec<(Event<'static>, Range<usize>)>,
    inline_links: Option<PendingInlineLinks>,
    broken_links: Vec<BrokenLinkInfo>,
    pending_lines: PendingLineEvents,
    text_start_lines: Option<Vec<usize>>,
}

impl ParsedDocument<'_> {
    pub(crate) fn into_handoff(self) -> OwnedParsedDocument {
        OwnedParsedDocument {
            source: self.pending_links.content.to_owned(),

            list_guard_policy: None,

            format_proof_events: None,
            structure: self.structure,

            has_math: self.has_math,
            parser_blocks: self.parser_blocks,
            events: self
                .pending_links
                .events
                .into_iter()
                .map(|(event, range)| (event.into_static(), range))
                .collect(),
            inline_links: self.pending_links.inline_links,
            broken_links: self.pending_links.broken_links,
            pending_lines: self.pending_lines,
            text_start_lines: self.text_start_lines,
        }
    }
}

impl OwnedParsedDocument {
    pub(crate) fn with_list_guard_policy(mut self, allowed: crate::utils::list_fix_guard::Allowed) -> Self {
        self.list_guard_policy = Some(allowed);
        self
    }

    pub(crate) fn certifies_list_guard(&self, source: &str, allowed: crate::utils::list_fix_guard::Allowed) -> bool {
        self.list_guard_policy == Some(allowed) && self.matches(source)
    }

    pub(crate) fn with_format_proof_events(mut self, events: Option<crate::format_proof::events::ProofEvents>) -> Self {
        self.format_proof_events = events;
        self
    }

    pub(crate) fn has_format_proof_events(&self) -> bool {
        self.format_proof_events.is_some()
    }

    pub(crate) fn take_format_proof_events(&mut self) -> Option<crate::format_proof::events::ProofEvents> {
        self.format_proof_events.take()
    }

    pub(crate) fn matches(&self, source: &str) -> bool {
        self.source == source
    }

    pub(crate) fn into_document(self, source: &str) -> ParsedDocument<'_> {
        debug_assert!(self.matches(source));
        ParsedDocument {
            structure: self.structure,

            has_math: self.has_math,
            parser_blocks: self.parser_blocks,
            pending_lines: self.pending_lines,
            text_start_lines: self.text_start_lines,
            pending_links: PendingLinks {
                content: source,
                events: self.events,
                inline_links: self.inline_links,
                broken_links: self.broken_links,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, MarkdownFlavor};
    use crate::lint_context::LintContext;

    #[test]
    fn handed_off_context_preserves_rule_findings_and_diagnostic_edits() {
        let rules = crate::rules::all_rules(&Config::default());
        for flavor in crate::format_proof::ALL_FLAVORS {
            for source in [
                "[ref]: /t\n\n- item\n\n      | a | b |\n      | - | - |\n",
                "> [ref]: /t\n> text\n>     | a | b |\n>     | - | - |\n",
                "# Héading *with emphasis*\n\n- first **strong** [link](target \"title\")\n  - next `code`\n",
                "1. ordered\n   - nested\n\n     continuation\n2. second\n",
                "Term\n: definition *with* [missing]\n\n[^1]: body\n    continuation\n",
                "[reference]: <target> \"title\"\n\n[reference] and ![missing image][] and [collapsed][]\n",
                "- ```rust\n  let x = 1;\n  ```\n\n> quoted\n> ---\n",
                "| head | two |\n| --- | --- |\n| one | [link](target) |\n\n- | head | two |\n  | --- | --- |\n",
                "!!! note\n    # Heading\n    - list `span`\n\n=== \"Tab\"\n    *text*\n",
                "<div markdown=\"1\">\n    - [item](target)\n    | head |\n    | --- |\n</div>\n",
                "---\ntitle: Doc\n---\n\n{::comment}\n- ignored\n{:/comment}\n\n## Real\n",
                "::: note\n- list $*math*$\n:::\n\n{role}`text` and [@cite] and [[note|label]]\n",
                "import A from './a';\n\n<Tabs>\n- [text](target)\n</Tabs>\n\n{1 + 2}\n",
                "- text <!-- complete -->\n  [link](a\\(b) \"title\")\n\n> <!-- unclosed\n> text\n",
                "- [ ] tâche\r\n  continuation\r\n\r\n**strong\ncontinued** and \"quotes\" --\n",
                "one\ntwo\n---\n\n-\n\n- next\n",
            ] {
                let (document, _) = ParsedDocument::parse_for_guard(source);
                let handoff = document.into_handoff();
                let actual = LintContext::new_with_handoff(source, flavor, None, Some(handoff));
                let expected = LintContext::new(source, flavor, None);
                assert_eq!(
                    format!("{:?}", actual.lines),
                    format!("{:?}", expected.lines),
                    "{flavor}: {source:?}"
                );
                assert_eq!(actual.code_blocks, expected.code_blocks, "{flavor}: {source:?}");
                assert_eq!(
                    format!("{:?}", actual.code_block_details),
                    format!("{:?}", expected.code_block_details)
                );
                assert_eq!(format!("{:?}", actual.links), format!("{:?}", expected.links));
                assert_eq!(format!("{:?}", actual.images), format!("{:?}", expected.images));
                assert_eq!(
                    format!("{:?}", actual.broken_links),
                    format!("{:?}", expected.broken_links)
                );
                for rule in &rules {
                    assert_eq!(
                        format!("{:?}", rule.check(&actual)),
                        format!("{:?}", rule.check(&expected)),
                        "{} in {flavor}: {source:?}",
                        rule.name()
                    );
                }
            }
        }
    }

    #[test]
    fn native_regressions_preserve_proof_and_handoff_contexts() {
        let rules = crate::rules::all_rules(&Config::default());
        for original in crate::format_proof::native_regressions::SOURCES {
            for flavor in crate::format_proof::ALL_FLAVORS {
                for ending in ["\n", "\r\n", "\r", "\n\r\n"] {
                    let source = original.replace('\n', ending);
                    let expected = LintContext::new(&source, flavor, None);
                    let (parsed, _) = ParsedDocument::parse_for_guard(&source);
                    let handed = LintContext::new_with_handoff(&source, flavor, None, Some(parsed.into_handoff()));
                    let mut facts = crate::format_proof::FormatProof::analyze(&source, flavor);
                    let proven = LintContext::new_with_proof(&source, flavor, None, facts.take_parsed(), None);
                    for actual in [&handed, &proven] {
                        assert_eq!(
                            format!("{:?}", actual.lines),
                            format!("{:?}", expected.lines),
                            "{flavor}: {source:?}"
                        );
                        assert_eq!(actual.code_blocks, expected.code_blocks, "{flavor}: {source:?}");
                        assert_eq!(
                            format!("{:?}", actual.table_blocks),
                            format!("{:?}", expected.table_blocks)
                        );
                        assert_eq!(format!("{:?}", actual.links), format!("{:?}", expected.links));
                        assert_eq!(
                            format!("{:?}", actual.broken_links),
                            format!("{:?}", expected.broken_links)
                        );
                        assert_eq!(actual.parser_blocks().events, expected.parser_blocks().events);
                        for rule in &rules {
                            assert_eq!(
                                format!("{:?}", rule.check(actual)),
                                format!("{:?}", rule.check(&expected)),
                                "{} / {flavor}: {source:?}",
                                rule.name()
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn guard_offsets_and_context_offsets_agree_for_all_line_endings() {
        let original = "[ref]: /target\n\na | b\n--- | ---\n[ref] | `code`\n\n$x `y$ and z`\n";
        for ending in ["\n", "\r\n", "\r"] {
            let source = original.replace('\n', ending);
            let (_, guard) = ParsedDocument::parse_for_guard(&source);
            let layout = crate::lint_context::line_computation::source_layout(&source);
            assert_eq!(guard.line_starts(), layout.offsets, "{source:?}");
        }
    }

    #[test]
    fn production_proof_context_rejects_other_bytes() {
        let offsets = crate::lint_context::line_computation::source_layout("- old\n").offsets;
        let parsed = ParsedDocument::parse("- old\n", &offsets);
        let actual = LintContext::new_with_proof("# new\n", MarkdownFlavor::Standard, None, Some(parsed), None);
        let expected = LintContext::new("# new\n", MarkdownFlavor::Standard, None);
        assert_eq!(format!("{:?}", actual.lines), format!("{:?}", expected.lines));
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(64))]
        #[test]
        fn generated_handoffs_preserve_fresh_context_facts(
            pieces in proptest::collection::vec(proptest::sample::select(vec![
                "plain é\n", "# Title\n", "- item\n", "  tail\n", "> quote\n", "\n",
                "[ref]: /target\n", "[ref] [missing]\n", "| a | b |\n| --- | --- |\n",
                "`code` $x `y$ and z`\n", "```\n", "~~~\n", "$$\n", "[^n]: note\n",
                "{::comment}\n", "{:/comment}\n", "!!! note\n", "import A from './a';\n",
                "<Panel>\n", "</Panel>\n", "{/* comment */}\n", "    | a | b |\n", "> | a | b |\n",
            ]), 0..20),
        ) {
            let original = pieces.concat();
            for flavor in crate::format_proof::ALL_FLAVORS {
                for ending in ["\n", "\r\n", "\r"] {
                    let source = original.replace('\n', ending);
                    let (parsed, guard) = ParsedDocument::parse_for_guard(&source);
                    let layout = crate::lint_context::line_computation::source_layout(&source);
                    proptest::prop_assert_eq!(guard.line_starts(), layout.offsets);
                    let actual = LintContext::new_with_handoff(&source, flavor, None, Some(parsed.into_handoff()));
                    let expected = LintContext::new(&source, flavor, None);
                    if flavor == MarkdownFlavor::Standard {
                        let mut facts = crate::format_proof::FormatProof::analyze(&source, flavor);
                        let proven = LintContext::new_with_proof(&source, flavor, None, facts.take_parsed(), None);
                        proptest::prop_assert_eq!(format!("{:?}", proven.lines), format!("{:?}", expected.lines));
                        proptest::prop_assert_eq!(&proven.code_blocks, &expected.code_blocks);
                        proptest::prop_assert_eq!(format!("{:?}", proven.table_blocks), format!("{:?}", expected.table_blocks));
                        proptest::prop_assert_eq!(format!("{:?}", proven.links), format!("{:?}", expected.links));
                        proptest::prop_assert_eq!(format!("{:?}", proven.broken_links), format!("{:?}", expected.broken_links));
                        proptest::prop_assert_eq!(&proven.parser_blocks().events, &expected.parser_blocks().events);
                        for rule in crate::rules::all_rules(&Config::default()) {
                            proptest::prop_assert_eq!(format!("{:?}", rule.check(&proven)), format!("{:?}", rule.check(&expected)), "{}: {:?}", rule.name(), source);
                        }
                    }
                    proptest::prop_assert_eq!(format!("{:?}", actual.lines), format!("{:?}", expected.lines));
                    proptest::prop_assert_eq!(&actual.code_blocks, &expected.code_blocks);
                    proptest::prop_assert_eq!(format!("{:?}", actual.table_blocks), format!("{:?}", expected.table_blocks));
                    proptest::prop_assert_eq!(format!("{:?}", actual.broken_links), format!("{:?}", expected.broken_links));
                    proptest::prop_assert_eq!(format!("{:?}", actual.links), format!("{:?}", expected.links));
                    proptest::prop_assert_eq!(&actual.parser_blocks().events, &expected.parser_blocks().events);
                }
            }
        }
    }

    #[test]
    fn another_source_discards_the_handoff_even_at_the_same_length() {
        let (document, _) = ParsedDocument::parse_for_guard("- old\n");
        let actual =
            LintContext::new_with_handoff("# new\n", MarkdownFlavor::Standard, None, Some(document.into_handoff()));
        let expected = LintContext::new("# new\n", MarkdownFlavor::Standard, None);
        assert_eq!(format!("{:?}", actual.lines), format!("{:?}", expected.lines));
        assert!(actual.lines[0].heading.is_some());
        assert!(actual.lines[0].list_item.is_none());
    }

    #[test]
    fn filtered_edits_discard_the_candidate_before_the_next_context() {
        let ctx = LintContext::new("- old\n", MarkdownFlavor::Standard, None);
        let (document, _) = ParsedDocument::parse_for_guard("- new\n");
        ctx.store_parse_handoff(document.into_handoff());
        assert!(ctx.take_parse_handoff("- old\n").is_none());
        assert!(ctx.take_parse_handoff("- new\n").is_none());
    }

    #[test]
    fn a_matching_handoff_is_consumed_once() {
        let ctx = LintContext::new("- old\n", MarkdownFlavor::Standard, None);
        let (document, _) = ParsedDocument::parse_for_guard("- new\n");
        ctx.store_parse_handoff(document.into_handoff());
        assert!(ctx.take_parse_handoff("- new\n").is_some());
        assert!(ctx.take_parse_handoff("- new\n").is_none());
    }
}
