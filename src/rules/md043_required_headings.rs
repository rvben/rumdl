use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::range_utils::calculate_heading_range;
use serde::{Deserialize, Serialize};

/// Configuration for MD043 rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD043Config {
    /// Required heading patterns
    #[serde(default = "default_headings")]
    pub headings: Vec<String>,
    /// Case-sensitive matching (default: false)
    #[serde(default = "default_match_case")]
    pub match_case: bool,
}

impl Default for MD043Config {
    fn default() -> Self {
        Self {
            headings: default_headings(),
            match_case: default_match_case(),
        }
    }
}

fn default_headings() -> Vec<String> {
    Vec::new()
}

fn default_match_case() -> bool {
    false
}

impl RuleConfig for MD043Config {
    const RULE_NAME: &'static str = "MD043";
}

/// Rule MD043: Required headings present
///
/// See [docs/md043.md](../../docs/md043.md) for full documentation, configuration, and examples.
#[derive(Clone, Default)]
pub struct MD043RequiredHeadings {
    config: MD043Config,
}

#[derive(Debug, Clone)]
struct DocumentHeading {
    text: String,
    match_key: String,
    line_index: usize,
}

#[derive(Debug, Clone, Copy)]
enum WildcardKind {
    One,
    OneOrMore,
}

impl WildcardKind {
    fn pattern(self) -> &'static str {
        match self {
            Self::One => "?",
            Self::OneOrMore => "+",
        }
    }

    fn requirement(self) -> &'static str {
        match self {
            Self::One => "one heading",
            Self::OneOrMore => "one or more headings",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PatternToken {
    Literal { source_index: usize },
    RequiredWildcard { source_index: usize, kind: WildcardKind },
    RepeatingWildcard { anchor_index: Option<usize> },
}

/// Alignment score that minimizes edits, then favors exact matches, wildcard
/// absorption, and substitutions in that order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AlignmentScore {
    cost: usize,
    exact_literals: usize,
    wildcard_absorptions: usize,
    substitutions: usize,
}

impl AlignmentScore {
    fn with_cost(mut self) -> Self {
        self.cost += 1;
        self
    }

    fn with_exact(mut self) -> Self {
        self.exact_literals += 1;
        self
    }

    fn with_absorption(mut self) -> Self {
        self.wildcard_absorptions += 1;
        self
    }

    fn with_substitution(mut self) -> Self {
        self.cost += 1;
        self.substitutions += 1;
        self
    }

    fn is_better_than(self, other: Self) -> bool {
        self.cost < other.cost
            || (self.cost == other.cost
                && (self.exact_literals > other.exact_literals
                    || (self.exact_literals == other.exact_literals
                        && (self.wildcard_absorptions > other.wildcard_absorptions
                            || (self.wildcard_absorptions == other.wildcard_absorptions
                                && self.substitutions > other.substitutions)))))
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlignmentDecision {
    Done,
    MatchLiteral,
    Substitute,
    OmitLiteral,
    ConsumeRequiredWildcard,
    OmitRequiredWildcard,
    ConsumeRepeatingWildcard,
    SkipRepeatingWildcard,
    Unexpected,
}

impl AlignmentDecision {
    fn tie_break_priority(self) -> u8 {
        match self {
            Self::MatchLiteral | Self::Substitute | Self::ConsumeRequiredWildcard | Self::SkipRepeatingWildcard => 3,
            Self::OmitLiteral | Self::OmitRequiredWildcard | Self::ConsumeRepeatingWildcard => 2,
            Self::Unexpected => 1,
            Self::Done => 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AlignmentCell {
    score: AlignmentScore,
    decision: AlignmentDecision,
}

#[derive(Debug, Clone, Copy)]
enum AlignmentStep {
    MatchLiteral {
        expected_index: usize,
        actual_index: usize,
    },
    Substitution {
        expected_index: usize,
        actual_index: usize,
    },
    MissingLiteral {
        expected_index: usize,
        next_actual_index: usize,
    },
    RequiredWildcardConsumed {
        source_index: usize,
        actual_index: usize,
    },
    RepeatingWildcardConsumed {
        actual_index: usize,
    },
    UnsatisfiedWildcard {
        source_index: usize,
        kind: WildcardKind,
        next_actual_index: usize,
    },
    Unexpected {
        actual_index: usize,
    },
}

#[derive(Debug, Clone, Copy)]
enum AlignmentEvent {
    LiteralMatch {
        expected_index: usize,
        actual_index: usize,
    },
    RequiredWildcardMatch {
        source_index: usize,
        actual_index: usize,
    },
    RepeatingWildcardMatch {
        actual_index: usize,
    },
    Substitution {
        expected_index: usize,
        actual_index: usize,
    },
    MissingLiteral {
        expected_index: usize,
        next_actual_index: usize,
    },
    UnsatisfiedWildcard {
        source_index: usize,
        kind: WildcardKind,
        next_actual_index: usize,
    },
    Unexpected {
        actual_index: usize,
    },
    OutOfOrder {
        expected_index: usize,
        actual_index: usize,
    },
}

#[derive(Debug)]
struct AlignmentResult {
    events: Vec<AlignmentEvent>,
}

impl MD043RequiredHeadings {
    pub fn new(headings: Vec<String>) -> Self {
        Self {
            config: MD043Config {
                headings,
                match_case: default_match_case(),
            },
        }
    }

    /// Create a new instance with the given configuration
    pub fn from_config_struct(config: MD043Config) -> Self {
        Self { config }
    }

    /// Compare two headings based on the match_case configuration
    fn headings_match(&self, expected: &str, actual: &str) -> bool {
        self.match_key(expected) == self.match_key(actual)
    }

    fn match_key(&self, heading: &str) -> String {
        if self.config.match_case {
            heading.to_string()
        } else {
            heading.to_lowercase()
        }
    }

    fn extract_headings(&self, ctx: &crate::lint_context::LintContext) -> Vec<DocumentHeading> {
        let mut result = Vec::new();

        for (line_index, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Skip invalid headings (e.g., `#NoSpace` which lacks required space after #)
                if !heading.is_valid {
                    continue;
                }

                // Reconstruct the full heading format with the hash symbols
                let full_heading = format!("{} {}", heading.marker, heading.text.trim());
                let match_key = self.match_key(&full_heading);
                result.push(DocumentHeading {
                    text: full_heading,
                    match_key,
                    line_index,
                });
            }
        }

        result
    }

    fn compile_pattern(&self) -> Vec<PatternToken> {
        let mut tokens = Vec::new();
        let mut index = 0;

        while index < self.config.headings.len() {
            if !matches!(self.config.headings[index].as_str(), "*" | "+" | "?") {
                tokens.push(PatternToken::Literal { source_index: index });
                index += 1;
                continue;
            }

            let run_start = index;
            while index < self.config.headings.len() && matches!(self.config.headings[index].as_str(), "*" | "+" | "?")
            {
                index += 1;
            }
            let anchor_index = (index < self.config.headings.len()).then_some(index);
            let mut has_repeating_slot = false;

            for source_index in run_start..index {
                match self.config.headings[source_index].as_str() {
                    "?" => tokens.push(PatternToken::RequiredWildcard {
                        source_index,
                        kind: WildcardKind::One,
                    }),
                    "+" => {
                        tokens.push(PatternToken::RequiredWildcard {
                            source_index,
                            kind: WildcardKind::OneOrMore,
                        });
                        has_repeating_slot = true;
                    }
                    "*" => has_repeating_slot = true,
                    _ => unreachable!(),
                }
            }

            if has_repeating_slot {
                tokens.push(PatternToken::RepeatingWildcard { anchor_index });
            }
        }

        tokens
    }

    fn is_anchor(actual: &DocumentHeading, anchor_index: Option<usize>, expected_keys: &[String]) -> bool {
        anchor_index.is_some_and(|index| expected_keys[index] == actual.match_key)
    }

    /// Builds a suffix DP over compiled pattern tokens and actual headings.
    /// Scores use two rolling rows; compact decisions are retained for backtracking.
    fn align_steps(&self, actual: &[DocumentHeading], expected_keys: &[String]) -> Vec<AlignmentStep> {
        let tokens = self.compile_pattern();
        let columns = actual.len() + 1;
        let cell_count = (tokens.len() + 1)
            .checked_mul(columns)
            .expect("MD043 alignment table dimensions overflow");
        let mut decisions = vec![AlignmentDecision::Done; cell_count];
        let table_index = |token_index: usize, actual_index: usize| token_index * columns + actual_index;
        let mut next_scores = vec![AlignmentScore::default(); columns];
        let mut current_scores = vec![AlignmentScore::default(); columns];

        for actual_index in (0..actual.len()).rev() {
            next_scores[actual_index] = next_scores[actual_index + 1].with_cost();
            decisions[table_index(tokens.len(), actual_index)] = AlignmentDecision::Unexpected;
        }

        for token_index in (0..tokens.len()).rev() {
            for actual_index in (0..=actual.len()).rev() {
                let mut best = AlignmentCell {
                    score: AlignmentScore {
                        cost: usize::MAX,
                        ..AlignmentScore::default()
                    },
                    decision: AlignmentDecision::Done,
                };
                let mut consider = |score: AlignmentScore, decision: AlignmentDecision| {
                    if score.is_better_than(best.score)
                        || (score == best.score && decision.tie_break_priority() > best.decision.tie_break_priority())
                    {
                        best = AlignmentCell { score, decision };
                    }
                };

                match tokens[token_index] {
                    PatternToken::Literal { source_index } => {
                        if actual_index < actual.len() {
                            let diagonal = next_scores[actual_index + 1];
                            if expected_keys[source_index] == actual[actual_index].match_key {
                                consider(diagonal.with_exact(), AlignmentDecision::MatchLiteral);
                            } else {
                                consider(diagonal.with_substitution(), AlignmentDecision::Substitute);
                            }
                        }
                        consider(next_scores[actual_index].with_cost(), AlignmentDecision::OmitLiteral);
                        if actual_index < actual.len() {
                            consider(
                                current_scores[actual_index + 1].with_cost(),
                                AlignmentDecision::Unexpected,
                            );
                        }
                    }
                    PatternToken::RequiredWildcard { .. } => {
                        if actual_index < actual.len() {
                            consider(
                                next_scores[actual_index + 1].with_absorption(),
                                AlignmentDecision::ConsumeRequiredWildcard,
                            );
                        }
                        consider(
                            next_scores[actual_index].with_cost(),
                            AlignmentDecision::OmitRequiredWildcard,
                        );
                    }
                    PatternToken::RepeatingWildcard { anchor_index } => {
                        consider(next_scores[actual_index], AlignmentDecision::SkipRepeatingWildcard);
                        if actual_index < actual.len()
                            && !Self::is_anchor(&actual[actual_index], anchor_index, expected_keys)
                        {
                            consider(
                                current_scores[actual_index + 1].with_absorption(),
                                AlignmentDecision::ConsumeRepeatingWildcard,
                            );
                        }
                    }
                }

                current_scores[actual_index] = best.score;
                decisions[table_index(token_index, actual_index)] = best.decision;
            }

            std::mem::swap(&mut current_scores, &mut next_scores);
        }

        let mut steps = Vec::new();
        let mut token_index = 0;
        let mut actual_index = 0;
        while token_index < tokens.len() || actual_index < actual.len() {
            let decision = decisions[table_index(token_index, actual_index)];
            match decision {
                AlignmentDecision::Done => break,
                AlignmentDecision::MatchLiteral => {
                    let PatternToken::Literal { source_index } = tokens[token_index] else {
                        unreachable!()
                    };
                    steps.push(AlignmentStep::MatchLiteral {
                        expected_index: source_index,
                        actual_index,
                    });
                    token_index += 1;
                    actual_index += 1;
                }
                AlignmentDecision::Substitute => {
                    let PatternToken::Literal { source_index } = tokens[token_index] else {
                        unreachable!()
                    };
                    steps.push(AlignmentStep::Substitution {
                        expected_index: source_index,
                        actual_index,
                    });
                    token_index += 1;
                    actual_index += 1;
                }
                AlignmentDecision::OmitLiteral => {
                    let PatternToken::Literal { source_index } = tokens[token_index] else {
                        unreachable!()
                    };
                    steps.push(AlignmentStep::MissingLiteral {
                        expected_index: source_index,
                        next_actual_index: actual_index,
                    });
                    token_index += 1;
                }
                AlignmentDecision::ConsumeRequiredWildcard => {
                    let PatternToken::RequiredWildcard { source_index, .. } = tokens[token_index] else {
                        unreachable!()
                    };
                    steps.push(AlignmentStep::RequiredWildcardConsumed {
                        source_index,
                        actual_index,
                    });
                    token_index += 1;
                    actual_index += 1;
                }
                AlignmentDecision::OmitRequiredWildcard => {
                    let PatternToken::RequiredWildcard { source_index, kind, .. } = tokens[token_index] else {
                        unreachable!()
                    };
                    steps.push(AlignmentStep::UnsatisfiedWildcard {
                        source_index,
                        kind,
                        next_actual_index: actual_index,
                    });
                    token_index += 1;
                }
                AlignmentDecision::ConsumeRepeatingWildcard => {
                    steps.push(AlignmentStep::RepeatingWildcardConsumed { actual_index });
                    actual_index += 1;
                }
                AlignmentDecision::SkipRepeatingWildcard => token_index += 1,
                AlignmentDecision::Unexpected => {
                    steps.push(AlignmentStep::Unexpected { actual_index });
                    actual_index += 1;
                }
            }
        }

        steps
    }

    /// Converts equivalent unmatched literal and actual occurrences into out-of-order
    /// events. Required wildcard matches retain ownership of their consumed heading.
    fn alignment(&self, actual: &[DocumentHeading]) -> AlignmentResult {
        let expected_keys = self
            .config
            .headings
            .iter()
            .map(|heading| self.match_key(heading))
            .collect::<Vec<_>>();
        let steps = self.align_steps(actual, &expected_keys);
        let mut paired_expected = vec![false; steps.len()];
        let mut reordered = vec![None; steps.len()];
        let unmatched_expected = steps
            .iter()
            .enumerate()
            .filter_map(|(step_index, step)| match step {
                AlignmentStep::MissingLiteral { expected_index, .. }
                | AlignmentStep::Substitution { expected_index, .. } => Some((step_index, *expected_index)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let candidates = steps
            .iter()
            .enumerate()
            .filter_map(|(step_index, step)| match step {
                AlignmentStep::Unexpected { actual_index }
                | AlignmentStep::RepeatingWildcardConsumed { actual_index }
                | AlignmentStep::Substitution { actual_index, .. } => Some((step_index, *actual_index)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut used_candidates = vec![false; candidates.len()];

        // Pair equal unmatched occurrences in sequence order. A substitution contributes both an
        // unmatched expected side and an unmatched actual side, allowing crossed substitutions to
        // become moves. Required wildcard slots retain ownership of their consumed heading.
        for (expected_step, expected_index) in unmatched_expected {
            if let Some((candidate_slot, (candidate_step, _actual_index))) =
                candidates
                    .iter()
                    .enumerate()
                    .find(|(candidate_slot, (_, actual_index))| {
                        !used_candidates[*candidate_slot]
                            && expected_keys[expected_index] == actual[*actual_index].match_key
                    })
            {
                used_candidates[candidate_slot] = true;
                paired_expected[expected_step] = true;
                reordered[*candidate_step] = Some(expected_index);
            }
        }

        let mut events = Vec::with_capacity(steps.len());
        for (step_index, step) in steps.iter().enumerate() {
            match *step {
                AlignmentStep::MatchLiteral {
                    expected_index,
                    actual_index,
                } => events.push(AlignmentEvent::LiteralMatch {
                    expected_index,
                    actual_index,
                }),
                AlignmentStep::RequiredWildcardConsumed {
                    source_index,
                    actual_index,
                } => events.push(AlignmentEvent::RequiredWildcardMatch {
                    source_index,
                    actual_index,
                }),
                AlignmentStep::RepeatingWildcardConsumed { actual_index } => {
                    if let Some(expected_index) = reordered[step_index] {
                        events.push(AlignmentEvent::OutOfOrder {
                            expected_index,
                            actual_index,
                        });
                    } else {
                        events.push(AlignmentEvent::RepeatingWildcardMatch { actual_index });
                    }
                }
                AlignmentStep::Substitution {
                    expected_index,
                    actual_index,
                } => match (paired_expected[step_index], reordered[step_index]) {
                    (false, None) => events.push(AlignmentEvent::Substitution {
                        expected_index,
                        actual_index,
                    }),
                    (true, None) => events.push(AlignmentEvent::Unexpected { actual_index }),
                    (true, Some(reordered_expected)) => events.push(AlignmentEvent::OutOfOrder {
                        expected_index: reordered_expected,
                        actual_index,
                    }),
                    (false, Some(reordered_expected)) => {
                        events.push(AlignmentEvent::MissingLiteral {
                            expected_index,
                            next_actual_index: actual_index,
                        });
                        events.push(AlignmentEvent::OutOfOrder {
                            expected_index: reordered_expected,
                            actual_index,
                        });
                    }
                },
                AlignmentStep::MissingLiteral {
                    expected_index,
                    next_actual_index,
                } => {
                    if !paired_expected[step_index] {
                        events.push(AlignmentEvent::MissingLiteral {
                            expected_index,
                            next_actual_index,
                        });
                    }
                }
                AlignmentStep::UnsatisfiedWildcard {
                    source_index,
                    kind,
                    next_actual_index,
                } => events.push(AlignmentEvent::UnsatisfiedWildcard {
                    source_index,
                    kind,
                    next_actual_index,
                }),
                AlignmentStep::Unexpected { actual_index } => {
                    if let Some(expected_index) = reordered[step_index] {
                        events.push(AlignmentEvent::OutOfOrder {
                            expected_index,
                            actual_index,
                        });
                    } else {
                        events.push(AlignmentEvent::Unexpected { actual_index });
                    }
                }
            }
        }

        AlignmentResult { events }
    }

    fn heading_range(
        &self,
        heading: &DocumentHeading,
        ctx: &crate::lint_context::LintContext,
    ) -> (usize, usize, usize, usize) {
        calculate_heading_range(
            heading.line_index + 1,
            ctx.lines[heading.line_index].content(ctx.content),
        )
    }

    fn omission_range(
        &self,
        next_actual_index: usize,
        actual: &[DocumentHeading],
        ctx: &crate::lint_context::LintContext,
    ) -> (usize, usize, usize, usize) {
        actual
            .get(next_actual_index)
            .or_else(|| actual.last())
            .map_or((1, 1, 1, 2), |heading| self.heading_range(heading, ctx))
    }

    fn reorder_message(&self, expected_index: usize, actual: &str) -> String {
        let previous = self.config.headings[..expected_index]
            .iter()
            .rev()
            .find(|heading| !matches!(heading.as_str(), "*" | "+" | "?"));
        let next = self.config.headings[expected_index + 1..]
            .iter()
            .find(|heading| !matches!(heading.as_str(), "*" | "+" | "?"));
        let location = match (previous, next) {
            (Some(previous), Some(next)) => format!("expected between '{previous}' and '{next}'"),
            (Some(previous), None) => format!("expected after '{previous}'"),
            (None, Some(next)) => format!("expected before '{next}'"),
            (None, None) => "expected at its configured position".to_string(),
        };
        format!("Heading structure does not match required structure. Heading '{actual}' is out of order; {location}")
    }

    fn warning(&self, range: (usize, usize, usize, usize), message: String) -> LintWarning {
        LintWarning {
            rule_name: Some(self.name().to_string()),
            line: range.0,
            column: range.1,
            end_line: range.2,
            end_column: range.3,
            message,
            severity: Severity::Warning,
            fix: None,
        }
    }
}

impl Rule for MD043RequiredHeadings {
    fn name(&self) -> &'static str {
        "MD043"
    }

    fn description(&self) -> &'static str {
        "Required heading structure"
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        if self.config.headings.is_empty() || ctx.content.is_empty() {
            return Ok(Vec::new());
        }

        let actual = self.extract_headings(ctx);
        let alignment = self.alignment(&actual);
        let prefix = "Heading structure does not match required structure.";
        let mut warnings = Vec::new();
        for event in alignment.events {
            match event {
                AlignmentEvent::LiteralMatch {
                    expected_index,
                    actual_index,
                } => debug_assert!(
                    self.headings_match(&self.config.headings[expected_index], &actual[actual_index].text)
                ),
                AlignmentEvent::RequiredWildcardMatch {
                    source_index,
                    actual_index,
                } => {
                    debug_assert!(matches!(self.config.headings[source_index].as_str(), "+" | "?"));
                    debug_assert!(actual_index < actual.len());
                }
                AlignmentEvent::RepeatingWildcardMatch { actual_index } => {
                    debug_assert!(actual_index < actual.len());
                }
                AlignmentEvent::Substitution {
                    expected_index,
                    actual_index,
                } => warnings.push(self.warning(
                    self.heading_range(&actual[actual_index], ctx),
                    format!(
                        "{prefix} Expected heading '{}', but found '{}'",
                        self.config.headings[expected_index], actual[actual_index].text
                    ),
                )),
                AlignmentEvent::MissingLiteral {
                    expected_index,
                    next_actual_index,
                } => warnings.push(self.warning(
                    self.omission_range(next_actual_index, &actual, ctx),
                    format!(
                        "{prefix} Missing required heading '{}'",
                        self.config.headings[expected_index]
                    ),
                )),
                AlignmentEvent::UnsatisfiedWildcard {
                    source_index,
                    kind,
                    next_actual_index,
                } => warnings.push(self.warning(
                    self.omission_range(next_actual_index, &actual, ctx),
                    format!(
                        "{prefix} Wildcard '{}' at position {} requires {}, but none was available",
                        kind.pattern(),
                        source_index + 1,
                        kind.requirement()
                    ),
                )),
                AlignmentEvent::Unexpected { actual_index } => warnings.push(self.warning(
                    self.heading_range(&actual[actual_index], ctx),
                    format!(
                        "{prefix} Unexpected heading '{}' at position {}",
                        actual[actual_index].text,
                        actual_index + 1
                    ),
                )),
                AlignmentEvent::OutOfOrder {
                    expected_index,
                    actual_index,
                } => warnings.push(self.warning(
                    self.heading_range(&actual[actual_index], ctx),
                    self.reorder_message(expected_index, &actual[actual_index].text),
                )),
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Auto-fixing MD043 would require restructuring the document (inserting,
        // renaming, or reordering headings), which risks data loss. Return the
        // content unchanged and let the user address the violation manually.
        Ok(ctx.content.to_string())
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        if self.config.headings.is_empty() || ctx.content.is_empty() {
            return true;
        }

        let has_valid_heading = ctx
            .lines
            .iter()
            .any(|line| line.heading.as_ref().is_some_and(|heading| heading.is_valid));
        !has_valid_heading && self.config.headings.iter().all(|pattern| pattern == "*")
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD043Config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_extract_headings_code_blocks() {
        // Create rule with required headings (now with hash symbols)
        let required = vec!["# Test Document".to_string(), "## Real heading 2".to_string()];
        let rule = MD043RequiredHeadings::new(required);

        // Test 1: Basic content with code block
        let content = "# Test Document\n\nThis is regular content.\n\n```markdown\n# This is a heading in a code block\n## Another heading in code block\n```\n\n## Real heading 2\n\nSome content.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let actual_headings = rule.extract_headings(&ctx);
        assert_eq!(
            actual_headings
                .iter()
                .map(|heading| heading.text.clone())
                .collect::<Vec<_>>(),
            vec!["# Test Document".to_string(), "## Real heading 2".to_string()],
            "Should extract correct headings and ignore code blocks"
        );

        // Test 2: Content with invalid headings
        let content = "# Test Document\n\nThis is regular content.\n\n```markdown\n# This is a heading in a code block\n## This should be ignored\n```\n\n## Not Real heading 2\n\nSome content.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let actual_headings = rule.extract_headings(&ctx);
        assert_eq!(
            actual_headings
                .iter()
                .map(|heading| heading.text.clone())
                .collect::<Vec<_>>(),
            vec!["# Test Document".to_string(), "## Not Real heading 2".to_string()],
            "Should extract actual headings including mismatched ones"
        );
    }

    #[test]
    fn test_with_document_structure() {
        // Test with required headings (now with hash symbols)
        let required = vec![
            "# Introduction".to_string(),
            "# Method".to_string(),
            "# Results".to_string(),
        ];
        let rule = MD043RequiredHeadings::new(required);

        // Test with matching headings
        let content = "# Introduction\n\nContent\n\n# Method\n\nMore content\n\n# Results\n\nFinal content";
        let warnings = rule
            .check(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None,
            ))
            .unwrap();
        assert!(warnings.is_empty(), "Expected no warnings for matching headings");

        // Test with mismatched headings
        let content = "# Introduction\n\nContent\n\n# Results\n\nSkipped method";
        let warnings = rule
            .check(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None,
            ))
            .unwrap();
        assert!(!warnings.is_empty(), "Expected warnings for mismatched headings");

        // Test with no headings but requirements exist
        let content = "No headings here, just plain text";
        let warnings = rule
            .check(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None,
            ))
            .unwrap();
        assert!(!warnings.is_empty(), "Expected warnings when headings are missing");

        // Test with setext headings - use the correct format (marker text)
        let required_setext = vec![
            "=========== Introduction".to_string(),
            "------ Method".to_string(),
            "======= Results".to_string(),
        ];
        let rule_setext = MD043RequiredHeadings::new(required_setext);
        let content = "Introduction\n===========\n\nContent\n\nMethod\n------\n\nMore content\n\nResults\n=======\n\nFinal content";
        let warnings = rule_setext
            .check(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None,
            ))
            .unwrap();
        assert!(warnings.is_empty(), "Expected no warnings for matching setext headings");
    }

    #[test]
    fn test_should_not_skip_headingless_documents_with_literal_requirements() {
        // Create rule with required headings
        let required = vec!["Test".to_string()];
        let rule = MD043RequiredHeadings::new(required);

        // Test 1: Content with '#' character in normal text (not a heading)
        let content = "This paragraph contains a # character but is not a heading";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should check headingless content when a literal heading is required"
        );

        // Test 2: Content with code block containing heading-like syntax
        let content = "Regular paragraph\n\n```markdown\n# This is not a real heading\n```\n\nMore text";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should check content whose only heading syntax is in a code block"
        );

        // Test 3: Content with list items using '-' character
        let content = "Some text\n\n- List item 1\n- List item 2\n\nMore text";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should check headingless list content"
        );

        // Test 4: Content with horizontal rule that uses '---'
        let content = "Some text\n\n---\n\nMore text below the horizontal rule";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should check headingless content containing a horizontal rule"
        );

        // Test 5: Content with equals sign in normal text
        let content = "This is a normal paragraph with equals sign x = y + z";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should check headingless content containing an equals sign"
        );

        // Test 6: Content with dash/minus in normal text
        let content = "This is a normal paragraph with minus sign x - y = z";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should check headingless content containing a minus sign"
        );

        let optional = MD043RequiredHeadings::new(vec!["*".to_string()]);
        assert!(optional.should_skip(&LintContext::new(
            "No headings",
            crate::config::MarkdownFlavor::Standard,
            None
        )));
    }

    #[test]
    fn test_should_skip_heading_detection() {
        // Create rule with required headings
        let required = vec!["Test".to_string()];
        let rule = MD043RequiredHeadings::new(required);

        // Test 1: Content with ATX heading
        let content = "# This is a heading\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should not skip content with ATX heading"
        );

        // Test 2: Content with Setext heading (equals sign)
        let content = "This is a heading\n================\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should not skip content with Setext heading (=)"
        );

        // Test 3: Content with Setext heading (dash)
        let content = "This is a subheading\n------------------\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should not skip content with Setext heading (-)"
        );

        // Test 4: Content with ATX heading with closing hashes
        let content = "## This is a heading ##\n\nAnd some content";
        assert!(
            !rule.should_skip(&LintContext::new(
                content,
                crate::config::MarkdownFlavor::Standard,
                None
            )),
            "Should not skip content with ATX heading with closing hashes"
        );
    }

    #[test]
    fn test_config_match_case_sensitive() {
        let config = MD043Config {
            headings: vec!["# Introduction".to_string(), "# Method".to_string()],
            match_case: true,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should fail with different case
        let content = "# introduction\n\n# method";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            !result.is_empty(),
            "Should detect case mismatch when match_case is true"
        );
    }

    #[test]
    fn test_config_match_case_insensitive() {
        let config = MD043Config {
            headings: vec!["# Introduction".to_string(), "# Method".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with different case
        let content = "# introduction\n\n# method";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Should allow case mismatch when match_case is false");
    }

    #[test]
    fn test_config_case_insensitive_mixed() {
        let config = MD043Config {
            headings: vec!["# Introduction".to_string(), "# METHOD".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with mixed case variations
        let content = "# INTRODUCTION\n\n# method";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should allow mixed case variations when match_case is false"
        );
    }

    #[test]
    fn test_config_case_sensitive_exact_match() {
        let config = MD043Config {
            headings: vec!["# Introduction".to_string(), "# Method".to_string()],
            match_case: true,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with exact case match
        let content = "# Introduction\n\n# Method";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should pass with exact case match when match_case is true"
        );
    }

    #[test]
    fn test_default_config() {
        let rule = MD043RequiredHeadings::default();

        // Should be disabled with empty headings
        let content = "# Any heading\n\n# Another heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Should be disabled with default empty headings");
    }

    #[test]
    fn test_default_config_section() {
        let rule = MD043RequiredHeadings::default();
        let config_section = rule.default_config_section();

        assert!(config_section.is_some());
        let (name, value) = config_section.unwrap();
        assert_eq!(name, "MD043");

        // Should contain both headings and match_case options with default values
        if let toml::Value::Table(table) = value {
            assert!(table.contains_key("headings"));
            assert!(table.contains_key("match-case"));
            assert_eq!(table["headings"], toml::Value::Array(vec![]));
            assert_eq!(table["match-case"], toml::Value::Boolean(false));
        } else {
            panic!("Expected TOML table");
        }
    }

    #[test]
    fn test_headings_match_case_sensitive() {
        let config = MD043Config {
            headings: vec![],
            match_case: true,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        assert!(rule.headings_match("Test", "Test"));
        assert!(!rule.headings_match("Test", "test"));
        assert!(!rule.headings_match("test", "Test"));
    }

    #[test]
    fn test_headings_match_case_insensitive() {
        let config = MD043Config {
            headings: vec![],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        assert!(rule.headings_match("Test", "Test"));
        assert!(rule.headings_match("Test", "test"));
        assert!(rule.headings_match("test", "Test"));
        assert!(rule.headings_match("TEST", "test"));
    }

    #[test]
    fn test_config_empty_headings() {
        let config = MD043Config {
            headings: vec![],
            match_case: true,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should skip processing when no headings are required
        let content = "# Any heading\n\n# Another heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Should be disabled with empty headings list");
    }

    #[test]
    fn test_fix_respects_configuration() {
        let config = MD043Config {
            headings: vec!["# Title".to_string(), "# Content".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "Wrong content";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // MD043 now preserves original content to prevent data loss
        let expected = "Wrong content";
        assert_eq!(fixed, expected);
    }

    // Wildcard pattern tests

    #[test]
    fn test_asterisk_wildcard_zero_headings() {
        // * allows zero headings
        let config = MD043Config {
            headings: vec!["# Start".to_string(), "*".to_string(), "# End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "# Start\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* should allow zero headings between Start and End");
    }

    #[test]
    fn test_asterisk_wildcard_multiple_headings() {
        // * allows multiple headings
        let config = MD043Config {
            headings: vec!["# Start".to_string(), "*".to_string(), "# End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "# Start\n\n## Section 1\n\n## Section 2\n\n## Section 3\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "* should allow multiple headings between Start and End"
        );
    }

    #[test]
    fn test_asterisk_wildcard_at_end() {
        // * at end allows any remaining headings
        let config = MD043Config {
            headings: vec!["# Introduction".to_string(), "*".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "# Introduction\n\n## Details\n\n### Subsection\n\n## More";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* at end should allow any trailing headings");
    }

    #[test]
    fn test_plus_wildcard_requires_at_least_one() {
        // + requires at least one heading
        let config = MD043Config {
            headings: vec!["# Start".to_string(), "+".to_string(), "# End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should fail with zero headings
        let content = "# Start\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "+ should require at least one heading");
    }

    #[test]
    fn test_plus_wildcard_allows_multiple() {
        // + allows multiple headings
        let config = MD043Config {
            headings: vec!["# Start".to_string(), "+".to_string(), "# End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with one heading
        let content = "# Start\n\n## Middle\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ should allow one heading");

        // Should pass with multiple headings
        let content = "# Start\n\n## Middle 1\n\n## Middle 2\n\n## Middle 3\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ should allow multiple headings");
    }

    #[test]
    fn test_question_wildcard_exactly_one() {
        // ? requires exactly one heading
        let config = MD043Config {
            headings: vec!["?".to_string(), "## Description".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with exactly one heading before Description
        let content = "# Project Name\n\n## Description";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "? should allow exactly one heading");
    }

    #[test]
    fn test_question_wildcard_fails_with_zero() {
        // ? fails with zero headings
        let config = MD043Config {
            headings: vec!["?".to_string(), "## Description".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "## Description";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "? should require exactly one heading");
    }

    #[test]
    fn test_complex_wildcard_pattern() {
        // Complex pattern: variable title, required sections, optional details
        let config = MD043Config {
            headings: vec![
                "?".to_string(),           // Any project title
                "## Overview".to_string(), // Required
                "*".to_string(),           // Optional sections
                "## License".to_string(),  // Required
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with minimal structure
        let content = "# My Project\n\n## Overview\n\n## License";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Complex pattern should match minimal structure");

        // Should pass with additional sections
        let content = "# My Project\n\n## Overview\n\n## Installation\n\n## Usage\n\n## License";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Complex pattern should match with optional sections");
    }

    #[test]
    fn test_multiple_asterisks() {
        // Multiple * wildcards in pattern
        let config = MD043Config {
            headings: vec![
                "# Title".to_string(),
                "*".to_string(),
                "## Middle".to_string(),
                "*".to_string(),
                "# End".to_string(),
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "# Title\n\n## Middle\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Multiple * wildcards should work");

        let content = "# Title\n\n### Details\n\n## Middle\n\n### More Details\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Multiple * wildcards should allow flexible structure"
        );
    }

    #[test]
    fn test_wildcard_with_case_sensitivity() {
        // Wildcards work with case-sensitive matching
        let config = MD043Config {
            headings: vec![
                "?".to_string(),
                "## Description".to_string(), // Case-sensitive
            ],
            match_case: true,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with correct case
        let content = "# Title\n\n## Description";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Wildcard should work with case-sensitive matching");

        // Should fail with wrong case
        let content = "# Title\n\n## description";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            !result.is_empty(),
            "Case-sensitive matching should detect case mismatch"
        );
    }

    #[test]
    fn test_all_wildcards_pattern() {
        // Pattern with only wildcards
        let config = MD043Config {
            headings: vec!["*".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass with any headings
        let content = "# Any\n\n## Headings\n\n### Work";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* alone should allow any heading structure");

        // Should pass with no headings
        let content = "No headings here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* alone should allow no headings");
    }

    #[test]
    fn test_wildcard_edge_cases() {
        // Edge case: + at end requires at least one more heading
        let config = MD043Config {
            headings: vec!["# Start".to_string(), "+".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should fail with no additional headings
        let content = "# Start";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "+ at end should require at least one more heading");

        // Should pass with additional headings
        let content = "# Start\n\n## More";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ at end should allow additional headings");
    }

    #[test]
    fn test_fix_with_wildcards() {
        // Fix should preserve content when wildcards are used
        let config = MD043Config {
            headings: vec!["?".to_string(), "## Description".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Matching content
        let content = "# Project\n\n## Description";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, content, "Fix should preserve matching wildcard content");

        // Non-matching content
        let content = "# Project\n\n## Other";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(
            fixed, content,
            "Fix should preserve non-matching content to prevent data loss"
        );
    }

    // Comprehensive edge case tests

    #[test]
    fn test_consecutive_wildcards() {
        // Multiple wildcards in a row
        let config = MD043Config {
            headings: vec![
                "# Start".to_string(),
                "*".to_string(),
                "+".to_string(),
                "# End".to_string(),
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should require at least one heading from +
        let content = "# Start\n\n## Middle\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Consecutive * and + should work together");

        // Should fail without the + requirement
        let content = "# Start\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should fail when + is not satisfied");
    }

    #[test]
    fn test_question_mark_doesnt_consume_literal_match() {
        // ? should match exactly one, not more
        let config = MD043Config {
            headings: vec!["?".to_string(), "## Description".to_string(), "## License".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should match with exactly one before Description
        let content = "# Title\n\n## Description\n\n## License";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "? should consume exactly one heading");

        // Should fail if Description comes first (? needs something to match)
        let content = "## Description\n\n## License";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "? requires exactly one heading to match");
    }

    #[test]
    fn test_asterisk_between_literals_complex() {
        // Test * matching when sandwiched between specific headings
        let config = MD043Config {
            headings: vec![
                "# Title".to_string(),
                "## Section A".to_string(),
                "*".to_string(),
                "## Section B".to_string(),
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should work with zero headings between A and B
        let content = "# Title\n\n## Section A\n\n## Section B";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* should allow zero headings");

        // Should work with many headings between A and B
        let content = "# Title\n\n## Section A\n\n### Sub1\n\n### Sub2\n\n### Sub3\n\n## Section B";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* should allow multiple headings");

        // Should fail if Section B is missing
        let content = "# Title\n\n## Section A\n\n### Sub1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            !result.is_empty(),
            "Should fail when required heading after * is missing"
        );
    }

    #[test]
    fn test_plus_requires_consumption() {
        // + must consume at least one heading
        let config = MD043Config {
            headings: vec!["+".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should fail with no headings
        let content = "No headings here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "+ should fail with zero headings");

        // Should pass with any heading
        let content = "# Any heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ should pass with one heading");

        // Should pass with multiple headings
        let content = "# First\n\n## Second\n\n### Third";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ should pass with multiple headings");
    }

    #[test]
    fn test_mixed_wildcard_and_literal_ordering() {
        // Ensure wildcards don't break literal matching order
        let config = MD043Config {
            headings: vec![
                "# A".to_string(),
                "*".to_string(),
                "# B".to_string(),
                "*".to_string(),
                "# C".to_string(),
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should pass in correct order
        let content = "# A\n\n# B\n\n# C";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Should match literals in correct order");

        // Should fail in wrong order
        let content = "# A\n\n# C\n\n# B";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should fail when literals are out of order");

        // Should fail with missing required literal
        let content = "# A\n\n# C";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should fail when required literal is missing");
    }

    #[test]
    fn test_only_wildcards_with_headings() {
        // Pattern with only wildcards and content
        let config = MD043Config {
            headings: vec!["?".to_string(), "+".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should require at least 2 headings (? = 1, + = 1+)
        let content = "# First\n\n## Second";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "? followed by + should require at least 2 headings");

        // Should fail with only one heading
        let content = "# First";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            !result.is_empty(),
            "Should fail with only 1 heading when ? + is required"
        );
    }

    #[test]
    fn test_asterisk_matching_algorithm_greedy_vs_lazy() {
        // Test that * correctly finds the next literal match
        let config = MD043Config {
            headings: vec![
                "# Start".to_string(),
                "*".to_string(),
                "## Target".to_string(),
                "# End".to_string(),
            ],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should correctly skip to first "Target" match
        let content = "# Start\n\n## Other\n\n## Target\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* should correctly skip to next literal match");

        // Should handle case where there are extra headings after the match
        // (First Target matches, second Target is extra - should fail)
        let content = "# Start\n\n## Target\n\n## Target\n\n# End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            !result.is_empty(),
            "Should fail with extra headings that don't match pattern"
        );
    }

    #[test]
    fn test_wildcard_at_start() {
        // Test wildcards at the beginning of pattern
        let config = MD043Config {
            headings: vec!["*".to_string(), "## End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should allow any headings before End
        let content = "# Random\n\n## Stuff\n\n## End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* at start should allow any preceding headings");

        // Test + at start
        let config = MD043Config {
            headings: vec!["+".to_string(), "## End".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should require at least one heading before End
        let content = "## End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "+ at start should require at least one heading");

        let content = "# First\n\n## End";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "+ at start should allow headings before End");
    }

    #[test]
    fn test_wildcard_with_setext_headings() {
        // Ensure wildcards work with setext headings too
        let config = MD043Config {
            headings: vec!["?".to_string(), "====== Section".to_string(), "*".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "Title\n=====\n\nSection\n======\n\nOptional\n--------";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Wildcards should work with setext headings");
    }

    #[test]
    fn test_empty_document_with_required_wildcards() {
        // Empty document should fail when + or ? are required
        let config = MD043Config {
            headings: vec!["?".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "No headings";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Empty document should fail with ? requirement");

        // Test with +
        let config = MD043Config {
            headings: vec!["+".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "No headings";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Empty document should fail with + requirement");
    }

    #[test]
    fn test_trailing_headings_after_pattern_completion() {
        // Extra headings after pattern is satisfied should fail
        let config = MD043Config {
            headings: vec!["# Title".to_string(), "## Section".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        // Should fail with extra headings
        let content = "# Title\n\n## Section\n\n### Extra";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "Should fail with trailing headings beyond pattern");

        // But * at end should allow them
        let config = MD043Config {
            headings: vec!["# Title".to_string(), "## Section".to_string(), "*".to_string()],
            match_case: false,
        };
        let rule = MD043RequiredHeadings::from_config_struct(config);

        let content = "# Title\n\n## Section\n\n### Extra";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "* at end should allow trailing headings");
    }

    #[test]
    fn test_reordering_respects_case_levels_and_duplicate_occurrences() {
        let insensitive = MD043RequiredHeadings::from_config_struct(MD043Config {
            headings: vec!["# A".into(), "# B".into(), "# A".into()],
            match_case: false,
        });
        let duplicate_move = LintContext::new("# A\n# a\n# B", crate::config::MarkdownFlavor::Standard, None);
        let result = insensitive.check(&duplicate_move).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].message,
            "Heading structure does not match required structure. Heading '# B' is out of order; expected between '# A' and '# A'"
        );
        assert_eq!(result[0].line, 3);

        let sensitive = MD043RequiredHeadings::from_config_struct(MD043Config {
            headings: vec!["# A".into(), "# B".into()],
            match_case: true,
        });
        let case_mismatch = LintContext::new("# B\n# a", crate::config::MarkdownFlavor::Standard, None);
        let result = sensitive.check(&case_mismatch).unwrap();
        assert!(result.iter().all(|warning| !warning.message.contains("out of order")));

        let wrong_level = LintContext::new("# B\n## A", crate::config::MarkdownFlavor::Standard, None);
        let result = sensitive.check(&wrong_level).unwrap();
        assert!(result.iter().all(|warning| !warning.message.contains("out of order")));
    }

    #[test]
    fn test_leading_and_trailing_moves_report_configured_neighbors() {
        let rule = MD043RequiredHeadings::new(vec!["# A".into(), "# B".into(), "# C".into()]);
        let leading = LintContext::new("# C\n# A\n# B", crate::config::MarkdownFlavor::Standard, None);
        let leading_result = rule.check(&leading).unwrap();
        assert_eq!(leading_result.len(), 1);
        assert_eq!(
            leading_result[0].message,
            "Heading structure does not match required structure. Heading '# C' is out of order; expected after '# B'"
        );

        let trailing = LintContext::new("# B\n# C\n# A", crate::config::MarkdownFlavor::Standard, None);
        let trailing_result = rule.check(&trailing).unwrap();
        assert_eq!(trailing_result.len(), 1);
        assert_eq!(
            trailing_result[0].message,
            "Heading structure does not match required structure. Heading '# A' is out of order; expected before '# B'"
        );
    }

    #[test]
    fn test_exhaustive_small_alignments_are_deterministic_and_owned() {
        let pattern_values = ["# A", "# B", "*", "+", "?"];
        let actual_values = ["# A", "# B", "# X"];

        for pattern_len in 1..=3 {
            for pattern_number in 0..pattern_values.len().pow(pattern_len as u32) {
                let mut number = pattern_number;
                let mut headings = Vec::with_capacity(pattern_len);
                for _ in 0..pattern_len {
                    headings.push(pattern_values[number % pattern_values.len()].to_string());
                    number /= pattern_values.len();
                }
                let obligations = headings.iter().filter(|heading| heading.as_str() != "*").count();

                for actual_len in 0..=3 {
                    for actual_number in 0..actual_values.len().pow(actual_len as u32) {
                        let mut number = actual_number;
                        let mut actual = Vec::with_capacity(actual_len);
                        for _ in 0..actual_len {
                            actual.push(actual_values[number % actual_values.len()]);
                            number /= actual_values.len();
                        }
                        let content = if actual.is_empty() {
                            "plain text".to_string()
                        } else {
                            actual.join("\n")
                        };
                        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);

                        for match_case in [false, true] {
                            let rule = MD043RequiredHeadings::from_config_struct(MD043Config {
                                headings: headings.clone(),
                                match_case,
                            });
                            let first = rule.check(&ctx).unwrap();
                            let second = rule.check(&ctx).unwrap();
                            assert_eq!(first, second, "pattern={headings:?}, actual={actual:?}");

                            let extracted = rule.extract_headings(&ctx);
                            let alignment = rule.alignment(&extracted);
                            let mut expected_uses = vec![0; headings.len()];
                            let mut actual_uses = vec![0; extracted.len()];
                            for event in &alignment.events {
                                match *event {
                                    AlignmentEvent::LiteralMatch {
                                        expected_index,
                                        actual_index,
                                    }
                                    | AlignmentEvent::Substitution {
                                        expected_index,
                                        actual_index,
                                    }
                                    | AlignmentEvent::OutOfOrder {
                                        expected_index,
                                        actual_index,
                                    } => {
                                        expected_uses[expected_index] += 1;
                                        actual_uses[actual_index] += 1;
                                    }
                                    AlignmentEvent::RequiredWildcardMatch {
                                        source_index,
                                        actual_index,
                                    } => {
                                        expected_uses[source_index] += 1;
                                        actual_uses[actual_index] += 1;
                                    }
                                    AlignmentEvent::RepeatingWildcardMatch { actual_index }
                                    | AlignmentEvent::Unexpected { actual_index } => {
                                        actual_uses[actual_index] += 1;
                                    }
                                    AlignmentEvent::MissingLiteral { expected_index, .. } => {
                                        expected_uses[expected_index] += 1;
                                    }
                                    AlignmentEvent::UnsatisfiedWildcard { source_index, .. } => {
                                        expected_uses[source_index] += 1;
                                    }
                                }
                            }

                            for (index, pattern) in headings.iter().enumerate() {
                                let expected_count = usize::from(pattern != "*");
                                assert_eq!(
                                    expected_uses[index], expected_count,
                                    "pattern={headings:?}, actual={actual:?}, events={:?}",
                                    alignment.events
                                );
                            }
                            assert!(
                                actual_uses.iter().all(|uses| *uses == 1),
                                "pattern={headings:?}, actual={actual:?}, events={:?}",
                                alignment.events
                            );
                            assert_eq!(
                                first.is_empty(),
                                wildcard_language_accepts(&rule, &extracted),
                                "pattern={headings:?}, actual={actual:?}, events={:?}",
                                alignment.events
                            );
                            assert!(first.len() <= obligations + actual.len());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_exhaustive_len4_alignment_matches_wildcard_language() {
        // Regression guard for the alignment DP. The size-3 exhaustive test above cannot
        // express interactions that need four tokens (two separate wildcard runs split by a
        // literal, a required + repeating wildcard followed by an anchor and a trailing
        // literal, etc.) -- exactly where an alignment regression would most plausibly hide.
        // This focuses on the core property only (accept/reject == the wildcard language) so
        // it stays cheap; determinism and ownership invariants are already covered at len 3.
        let pattern_values = ["# A", "# B", "*", "+", "?"];
        let actual_values = ["# A", "# B", "# X"];

        // Parse each document once and reuse it across every pattern. Rebuilding the
        // LintContext per pattern dominates the runtime; precomputing keeps this in the
        // ~1s range so it can live in the default test run.
        let mut contents = Vec::new();
        for actual_len in 0..=4usize {
            for actual_number in 0..actual_values.len().pow(actual_len as u32) {
                let mut number = actual_number;
                let mut actual = Vec::with_capacity(actual_len);
                for _ in 0..actual_len {
                    actual.push(actual_values[number % actual_values.len()]);
                    number /= actual_values.len();
                }
                contents.push((actual.join("\n"), actual));
            }
        }
        let documents: Vec<(LintContext, &Vec<&str>)> = contents
            .iter()
            .map(|(content, actual)| {
                let text = if actual.is_empty() { "plain text" } else { content };
                (
                    LintContext::new(text, crate::config::MarkdownFlavor::Standard, None),
                    actual,
                )
            })
            .collect();

        for pattern_len in 1..=4usize {
            for pattern_number in 0..pattern_values.len().pow(pattern_len as u32) {
                let mut number = pattern_number;
                let mut headings = Vec::with_capacity(pattern_len);
                for _ in 0..pattern_len {
                    headings.push(pattern_values[number % pattern_values.len()].to_string());
                    number /= pattern_values.len();
                }
                // Case sensitivity is orthogonal to the wildcard-run interactions this guards,
                // and is already exhaustively covered in both modes at len 3.
                let rule = MD043RequiredHeadings::from_config_struct(MD043Config {
                    headings: headings.clone(),
                    match_case: false,
                });

                for (ctx, actual) in &documents {
                    let extracted = rule.extract_headings(ctx);
                    assert_eq!(
                        rule.check(ctx).unwrap().is_empty(),
                        wildcard_language_accepts(&rule, &extracted),
                        "pattern={headings:?}, actual={actual:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_fully_equal_scores_prefer_the_earliest_literal_occurrence() {
        let rule = MD043RequiredHeadings::new(vec!["# A".into(), "# A".into()]);
        let ctx = LintContext::new("# A", crate::config::MarkdownFlavor::Standard, None);
        let actual = rule.extract_headings(&ctx);

        assert!(matches!(
            rule.alignment(&actual).events.as_slice(),
            [
                AlignmentEvent::LiteralMatch {
                    expected_index: 0,
                    actual_index: 0
                },
                AlignmentEvent::MissingLiteral { expected_index: 1, .. }
            ]
        ));

        let rule = MD043RequiredHeadings::new(vec!["# A".into()]);
        let ctx = LintContext::new("# A\n# A", crate::config::MarkdownFlavor::Standard, None);
        let actual = rule.extract_headings(&ctx);
        assert!(matches!(
            rule.alignment(&actual).events.as_slice(),
            [
                AlignmentEvent::LiteralMatch {
                    expected_index: 0,
                    actual_index: 0
                },
                AlignmentEvent::Unexpected { actual_index: 1 }
            ]
        ));
    }

    fn wildcard_language_accepts(rule: &MD043RequiredHeadings, actual: &[DocumentHeading]) -> bool {
        let mut pattern_index = 0;
        let mut actual_index = 0;

        while pattern_index < rule.config.headings.len() {
            if !matches!(rule.config.headings[pattern_index].as_str(), "*" | "+" | "?") {
                if actual
                    .get(actual_index)
                    .is_none_or(|actual| !rule.headings_match(&rule.config.headings[pattern_index], &actual.text))
                {
                    return false;
                }
                pattern_index += 1;
                actual_index += 1;
                continue;
            }

            let run_start = pattern_index;
            while pattern_index < rule.config.headings.len()
                && matches!(rule.config.headings[pattern_index].as_str(), "*" | "+" | "?")
            {
                pattern_index += 1;
            }
            let required = rule.config.headings[run_start..pattern_index]
                .iter()
                .filter(|pattern| matches!(pattern.as_str(), "+" | "?"))
                .count();
            if actual.len().saturating_sub(actual_index) < required {
                return false;
            }
            actual_index += required;

            let repeats = rule.config.headings[run_start..pattern_index]
                .iter()
                .any(|pattern| matches!(pattern.as_str(), "*" | "+"));
            if repeats {
                if let Some(anchor) = rule.config.headings.get(pattern_index) {
                    while actual_index < actual.len() && !rule.headings_match(anchor, &actual[actual_index].text) {
                        actual_index += 1;
                    }
                } else {
                    actual_index = actual.len();
                }
            }
        }

        actual_index == actual.len()
    }
}
