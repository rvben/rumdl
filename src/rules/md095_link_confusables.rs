use std::collections::HashSet;

use crate::lint_context::LintContext;
use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::html_elements::extract_attribute;

/// ASCII-relevant entries from Unicode UTS #39 [`confusables.txt`](https://www.unicode.org/Public/security/latest/confusables.txt).
fn ascii_confusable(c: char) -> bool {
    matches!(
        c,
        '\u{0406}'
            | '\u{0408}'
            | '\u{0410}'
            | '\u{0412}'
            | '\u{0415}'
            | '\u{041A}'
            | '\u{041C}'
            | '\u{041D}'
            | '\u{041E}'
            | '\u{041F}'
            | '\u{0421}'
            | '\u{0422}'
            | '\u{0425}'
            | '\u{0423}'
            | '\u{0430}'
            | '\u{0435}'
            | '\u{043E}'
            | '\u{0440}'
            | '\u{0441}'
            | '\u{0445}'
            | '\u{0443}'
            | '\u{0456}'
            | '\u{0458}'
            | '\u{04CF}'
            | '\u{0391}'
            | '\u{0392}'
            | '\u{0395}'
            | '\u{0396}'
            | '\u{0397}'
            | '\u{0399}'
            | '\u{039A}'
            | '\u{039C}'
            | '\u{039D}'
            | '\u{039F}'
            | '\u{03A1}'
            | '\u{03A4}'
            | '\u{03A5}'
            | '\u{03A7}'
            | '\u{03B1}'
            | '\u{03B2}'
            | '\u{03B5}'
            | '\u{03B7}'
            | '\u{03B9}'
            | '\u{03BA}'
            | '\u{03BD}'
            | '\u{03BF}'
            | '\u{03C1}'
            | '\u{03C4}'
            | '\u{03C5}'
            | '\u{03C7}'
            | '\u{0131}'
            | '\u{017F}'
            | '\u{0261}'
            | '\u{0555}'
            | '\u{0585}'
            | '\u{2010}'
            | '\u{2011}'
            | '\u{2044}'
            | '\u{2215}'
            | '\u{3002}'
    ) || ('\u{1D400}'..='\u{1D7FF}').contains(&c)
        || ('\u{FF01}'..='\u{FF5E}').contains(&c)
}

fn first_confusable(text: &str) -> Option<(usize, char)> {
    text.char_indices().find(|(_, c)| ascii_confusable(*c))
}

#[derive(Debug, Default, Clone)]
pub struct MD095LinkConfusables;

impl Rule for MD095LinkConfusables {
    fn name(&self) -> &'static str {
        "MD095"
    }

    fn description(&self) -> &'static str {
        "Link destinations should not contain confusable non-ASCII characters"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        !ctx.content.chars().any(ascii_confusable)
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let mut reported = HashSet::new();

        let mut report = |offset: usize, destination: &str, kind: &str| {
            let Some((character_offset, character)) = first_confusable(destination) else {
                return;
            };
            let character_byte = offset + character_offset;
            if !reported.insert(character_byte) {
                return;
            }
            let (line, column) = ctx.offset_to_line_col(character_byte);
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line,
                column,
                end_line: line,
                end_column: column + 1,
                message: format!(
                    "{kind} contains confusable non-ASCII character U+{:04X} ({character:?})",
                    character as u32
                ),
                severity: Severity::Warning,
                fix: None,
            });
        };

        for link in ctx.links() {
            if link.is_reference {
                continue;
            }
            let raw_link = &ctx.content[link.byte_offset..link.byte_end];
            let destination_offset = raw_link
                .find(link.url.as_ref())
                .map_or(link.byte_offset, |offset| link.byte_offset + offset);
            report(destination_offset, &link.url, "Link destination");
        }

        for definition in ctx.reference_definitions() {
            let raw_definition = &ctx.content[definition.byte_offset..definition.byte_end];
            let destination_offset = raw_definition
                .find(&definition.url)
                .map_or(definition.byte_offset, |offset| definition.byte_offset + offset);
            report(destination_offset, &definition.url, "Link reference destination");
        }

        for url in ctx.bare_urls().iter() {
            report(url.byte_offset, &url.url, "Bare URL");
        }

        for tag in ctx.html_tags().iter() {
            if tag.is_closing || !matches!(tag.tag_name.as_str(), "a" | "area" | "link") {
                continue;
            }
            let raw_tag = &ctx.content[tag.byte_offset..tag.byte_end];
            let Some(value) = extract_attribute(raw_tag, "href") else {
                continue;
            };
            let Some(value_offset) = raw_tag.find(&value) else {
                continue;
            };
            let absolute_offset = tag.byte_offset + value_offset;
            if ctx.is_byte_offset_in_code_span(absolute_offset) || ctx.is_in_html_comment(absolute_offset) {
                continue;
            }
            report(absolute_offset, &value, "HTML link destination");
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        Ok(ctx.content.to_string())
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(Self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    #[test]
    fn detects_markdown_html_bare_and_angle_links() {
        let content = concat!(
            "[markdown](https://еxample.com)\n",
            "<https://example.com/dοcs>\n",
            "Visit https://ｅxample.com\n",
            "<a href=\"https://exаmple.com\">HTML</a>\n",
            "<a href=https://еxample.com>Unquoted HTML</a>\n",
        );
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = MD095LinkConfusables.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 5);
        assert!(warnings.iter().all(|warning| warning.fix.is_none()));
    }

    #[test]
    fn leaves_legitimate_non_ascii_destination_alone() {
        let content = "[Munich](https://münchen.de/straße)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert!(MD095LinkConfusables.check(&ctx).unwrap().is_empty());
    }

    #[test]
    fn detects_uts39_ascii_confusables() {
        let content = concat!(
            "[Cyrillic](https://Еxample.com)\n",
            "[Greek](https://Αlpha.example)\n",
            "[Latin](https://ıong.example)\n",
            "[Punctuation](https://example。com)\n",
        );
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = MD095LinkConfusables.check(&ctx).unwrap();

        assert_eq!(warnings.len(), 4);
    }
}
