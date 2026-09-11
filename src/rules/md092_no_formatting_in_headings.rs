//! Rule MD092: inline formatting inside a heading.
//!
//! A heading is not only text on the page. It is also the source of generated
//! artifacts — a table of contents, an anchor, a sidebar or outline entry, a
//! PDF bookmark — and inline markup reaches those inconsistently. One generator
//! strips the markers, another emits them literally, a third keeps the markup in
//! the page and drops it from the anchor, so the same heading can read
//! `Method map()` in the body and ``Method `map()` `` in the table of contents.
//! Nothing in the source says which will happen, because the answer belongs to
//! the tool that consumes the document rather than to the document.
//!
//! The other half of the problem is that markup in a heading is often
//! unintentional. A heading that names a file or an identifier containing `_`
//! or `*` becomes emphasis on its own: the heading `## __tests__/gt.test.js`
//! renders as *tests*/gt.test.js. The underscores are gone from the page, from
//! the anchor and from the table of contents, and the document lints clean, so
//! nothing tells the author the path they published is not the path they wrote.
//!
//! The rule is off by default: a code span naming a method in a heading is
//! normal practice in plenty of projects, and a project that wants its headings
//! free of markup enables the rule deliberately.
//!
//! **Detection only, by design.** Removing the markers changes what renders
//! rather than only how the source reads:
//!
//! - a heading whose path sits in a code span — dropping the backticks turns
//!   the path into bold text;
//! - `## **tests**/sort.test.js` — dropping the emphasis yields
//!   `tests/sort.test.js`, a plausible path that was never in the source.
//!
//! Both rewrites are silent corruptions, and which one the author meant (rewrite
//! the heading, escape the markers, move the identifier out of the heading) is
//! not derivable from the source.

mod md092_config;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use crate::lint_context::LintContext;
use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use md092_config::MD092Config;

/// Longest excerpt of the offending span shown in a message.
const MAX_EXCERPT_CHARS: usize = 40;

/// What was found, for the message.
#[derive(Clone, Copy)]
enum Construct {
    Code,
    Strong,
    Emphasis,
}

impl Construct {
    fn noun(self) -> &'static str {
        match self {
            Construct::Code => "Inline code",
            Construct::Strong => "Strong emphasis",
            Construct::Emphasis => "Emphasis",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MD092NoFormattingInHeadings {
    config: MD092Config,
}

impl MD092NoFormattingInHeadings {
    fn from_config_struct(config: MD092Config) -> Self {
        Self { config }
    }

    /// Lines carrying the text of a valid heading. For a setext heading that is
    /// the text line rather than the underline, which is also the line every
    /// inline span of that heading sits on.
    fn heading_lines(ctx: &LintContext) -> HashSet<usize> {
        ctx.headings()
            .filter(|heading| heading.heading.is_valid)
            .map(|heading| heading.line_num)
            .collect()
    }

    fn warning(&self, ctx: &LintContext, span: (usize, usize), construct: Construct) -> LintWarning {
        let (line, column) = ctx.offset_to_line_col(span.0);
        let source = &ctx.content[span.0..span.1];
        LintWarning {
            rule_name: Some(self.name().to_string()),
            severity: Severity::Warning,
            line,
            column,
            end_line: line,
            end_column: column + source.chars().count(),
            message: format!("{} in heading: {}", construct.noun(), excerpt(source)),
            fix: None,
        }
    }
}

/// One line of source, shortened for a message.
fn excerpt(source: &str) -> String {
    match source.char_indices().nth(MAX_EXCERPT_CHARS) {
        Some((cut, _)) => format!("{}...", &source[..cut]),
        None => source.to_string(),
    }
}

impl Rule for MD092NoFormattingInHeadings {
    fn name(&self) -> &'static str {
        "MD092"
    }

    fn description(&self) -> &'static str {
        "Headings should not contain inline formatting"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn should_skip(&self, _ctx: &LintContext) -> bool {
        // Every switch off is the only cheap answer. Whether a document has
        // headings is not decidable from a character: an ATX heading needs `#`,
        // a setext heading needs neither.
        !self.config.code && !self.config.strong && !self.config.emphasis
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let heading_lines = Self::heading_lines(ctx);
        if heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        if self.config.code {
            for span in ctx.code_spans().iter() {
                if heading_lines.contains(&span.line) {
                    warnings.push(self.warning(ctx, (span.byte_offset, span.byte_end), Construct::Code));
                }
            }
        }

        for span in ctx.emphasis_spans().iter() {
            let construct = if span.is_strong {
                Construct::Strong
            } else {
                Construct::Emphasis
            };
            let wanted = if span.is_strong {
                self.config.strong
            } else {
                self.config.emphasis
            };
            if wanted && heading_lines.contains(&span.line) {
                warnings.push(self.warning(ctx, (span.byte_offset, span.byte_end), construct));
            }
        }

        warnings.sort_by_key(|warning| (warning.line, warning.column));
        Ok(warnings)
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        // Detection only: see the module comment. Every mechanical rewrite of
        // the markers changes the rendered heading.
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD092Config);
}
