//! Lossless lexical candidates shared by rules, including malformed syntax a
//! Markdown parser does not report. Candidates only narrow a traversal; each
//! rule still decides syntax, dialect, context and diagnostic eligibility.

/// The source shape required by a line-oriented inline analysis.
#[derive(Clone, Copy)]
pub(crate) enum LineCandidateKind {
    /// A line containing `*` or `_`, including unpaired and spaced markers.
    Emphasis,
    /// A line containing `:`, `@` or `www.`, including malformed destinations.
    Url,
    /// A line containing `#`, including malformed headings and ordinary text.
    Hash,
}

pub(super) const EMPHASIS: u8 = 1;
pub(super) const URL: u8 = 2;
pub(super) const DOLLAR: u8 = 4;
pub(super) const EQUALS: u8 = 8;

pub(super) const HASH: u8 = 16;

#[derive(Default)]
pub(super) struct LineCandidates {
    emphasis: Vec<usize>,
    urls: Vec<usize>,

    hashes: Vec<usize>,
    seen: u8,
}

impl LineCandidates {
    pub(super) fn record(&mut self, line: usize, signals: u8) {
        self.seen |= signals;
        if signals & EMPHASIS != 0 {
            self.emphasis.push(line);
        }
        if signals & URL != 0 {
            self.urls.push(line);
        }

        if signals & HASH != 0 {
            self.hashes.push(line);
        }
    }

    pub(super) fn has_signal(&self, signal: u8) -> bool {
        self.seen & signal != 0
    }

    pub(super) fn lines(&self, kind: LineCandidateKind) -> &[usize] {
        match kind {
            LineCandidateKind::Emphasis => &self.emphasis,
            LineCandidateKind::Url => &self.urls,

            LineCandidateKind::Hash => &self.hashes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::filtered_lines::FilteredLinesExt;
    use crate::lint_context::LintContext;
    use proptest::prelude::*;

    fn matches(line: &str, kind: LineCandidateKind) -> bool {
        match kind {
            LineCandidateKind::Emphasis => line.contains(['*', '_']),
            LineCandidateKind::Url => line.contains([':', '@']) || line.contains("www."),

            LineCandidateKind::Hash => line.contains('#'),
        }
    }

    proptest! {
        #[test]
        fn lossless_candidates_match_independent_source_scan(source in proptest::prop_oneof!["(?s).{0,128}", "[\\r\\n*#_:@$=w.]{0,512}"]) {
            let layout = crate::lint_context::line_computation::source_layout(&source);
            prop_assert_eq!(layout.candidates.has_signal(DOLLAR), source.contains('$'));
            prop_assert_eq!(layout.candidates.has_signal(EQUALS), source.contains('='));
            for kind in [LineCandidateKind::Emphasis, LineCandidateKind::Url,
                 LineCandidateKind::Hash] {
                let expected: Vec<_> = source.lines().enumerate()
                    .filter_map(|(index, line)| matches(line, kind).then_some(index)).collect();
                prop_assert_eq!(layout.candidates.lines(kind), expected);
            }
        }
    }

    #[test]
    fn candidate_iteration_preserves_source_positions_and_context_filters() {
        let fragments = [
            "",
            "plain",
            "*",
            "#",
            "# malformed #Heading",
            "_ spaced _",
            "www.",
            ":",
            "@",
            "é🦀",
            "\r",
            "\n",
            "\r\n",
        ];
        for a in fragments {
            for b in fragments {
                for c in fragments {
                    let source = format!("{a}{b}{c}");
                    let layout = crate::lint_context::line_computation::source_layout(&source);
                    for kind in [
                        LineCandidateKind::Emphasis,
                        LineCandidateKind::Url,
                        LineCandidateKind::Hash,
                    ] {
                        let expected: Vec<_> = source
                            .lines()
                            .enumerate()
                            .filter_map(|(index, line)| matches(line, kind).then_some(index))
                            .collect();
                        assert_eq!(layout.candidates.lines(kind), expected, "{source:?}");
                    }
                }
            }
        }

        let source = "---\ntitle: *literal*\n---\n\ntext * spaced * www.example.com\n\n```\n_x_ https://code\n```\n\n<!-- _comment_ -->\n\n::: note\n_ prose _\n:::\n\nexport const x = 'www.x';\n\n<span> * x * </span>\n";
        for flavor in [
            MarkdownFlavor::Standard,
            MarkdownFlavor::MDX,
            MarkdownFlavor::MkDocs,
            MarkdownFlavor::Pandoc,
            MarkdownFlavor::Quarto,
            MarkdownFlavor::Obsidian,
            MarkdownFlavor::Kramdown,
            MarkdownFlavor::AzureDevOps,
            MarkdownFlavor::MyST,
            MarkdownFlavor::Hugo,
            MarkdownFlavor::MDG,
            MarkdownFlavor::GhAw,
        ] {
            let ctx = LintContext::new(source, flavor, None);
            for kind in [
                LineCandidateKind::Emphasis,
                LineCandidateKind::Url,
                LineCandidateKind::Hash,
            ] {
                let filtered = || {
                    ctx.filtered_lines()
                        .skip_front_matter()
                        .skip_code_blocks()
                        .skip_html_blocks()
                        .skip_html_comments()
                        .skip_esm_blocks()
                        .skip_math_blocks()
                        .skip_obsidian_comments()
                        .skip_mkdocstrings()
                };
                let expected: Vec<_> = filtered()
                    .into_iter()
                    .filter(|line| matches(line.content, kind))
                    .map(|line| (line.line_num, line.content))
                    .collect();
                let actual: Vec<_> = filtered()
                    .candidates(kind)
                    .map(|line| (line.line_num, line.content))
                    .collect();
                assert_eq!(actual, expected, "{flavor:?}");
            }
        }
    }
}
