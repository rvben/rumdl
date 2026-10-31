//! "Did you mean" suggestions for link fragments that match no anchor.
//!
//! A fragment that matches no anchor is usually a near miss: a typo, a
//! different case, or hyphens that the renderer adds or drops (for example a
//! heading ending in an HTML comment keeps the hyphen for the space before the
//! comment). [`AnchorSuggester`] names the one existing anchor such a fragment
//! most plausibly meant, and nothing when no single anchor stands out.

use std::cell::Cell;
use std::collections::HashMap;

/// The most candidate comparisons one suggester performs over its lifetime.
/// Each missing fragment costs one comparison per anchor of similar length, so
/// a document with a great many broken links and headings would otherwise do
/// work quadratic in its size to improve messages. A query that would exceed
/// what is left is skipped whole, never cut short, so a suggestion is never
/// chosen from a partial scan.
const COMPARISON_BUDGET: usize = 100_000;

/// The most edits at which an existing anchor is offered for a missing
/// fragment of `len` characters: about one per four characters, at most two.
/// A short fragment such as `faq` therefore never matches a different anchor,
/// while a typo in a long slug does.
fn max_typo_distance(len: usize) -> usize {
    match len {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    }
}

/// Anchors of one document, indexed for suggestions.
pub struct AnchorSuggester {
    /// Distinct anchor names in a fixed order, so results never depend on the
    /// iteration order of the set they came from.
    names: Vec<String>,
    /// Indexes into `names` by the name lowercased and stripped of leading and
    /// trailing hyphens. Names that strip to nothing are absent.
    by_key: HashMap<String, Vec<usize>>,
    /// Indexes into `names` by length in characters.
    by_length: HashMap<usize, Vec<usize>>,
    budget: Cell<usize>,
}

impl AnchorSuggester {
    pub fn new<'a>(names: impl Iterator<Item = &'a str>) -> Self {
        let mut names: Vec<String> = names.map(str::to_string).collect();
        names.sort();
        names.dedup();
        let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_length: HashMap<usize, Vec<usize>> = HashMap::new();
        for (index, name) in names.iter().enumerate() {
            let key = normalized_key(name);
            if !key.is_empty() {
                by_key.entry(key).or_default().push(index);
            }
            by_length.entry(name.chars().count()).or_default().push(index);
        }
        Self {
            names,
            by_key,
            by_length,
            budget: Cell::new(COMPARISON_BUDGET),
        }
    }

    /// The text appended to a missing-anchor message: a suggestion when one
    /// anchor is clearly meant, nothing otherwise.
    pub fn hint(&self, fragment: &str) -> String {
        match self.suggest(fragment) {
            Some(anchor) => format!(" (did you mean '#{anchor}'?)"),
            None => String::new(),
        }
    }

    /// The existing anchor a missing `fragment` most plausibly meant, if
    /// exactly one qualifies.
    ///
    /// 1. An anchor equal to the fragment once both are lowercased and stripped
    ///    of leading and trailing hyphens. Several such anchors are ambiguous
    ///    and end the search without a suggestion.
    /// 2. Otherwise the anchor with the fewest edits from the fragment, within
    ///    [`max_typo_distance`]. A tie for the fewest gives no suggestion.
    fn suggest(&self, fragment: &str) -> Option<&str> {
        let key = normalized_key(fragment);
        if !key.is_empty()
            && let Some(matches) = self.by_key.get(&key)
        {
            let mut others = matches
                .iter()
                .map(|&i| self.names[i].as_str())
                .filter(|name| *name != fragment);
            return match (others.next(), others.next()) {
                (Some(only), None) => Some(only),
                (Some(_), Some(_)) => None,
                (None, _) => self.nearest(fragment),
            };
        }
        self.nearest(fragment)
    }

    fn nearest(&self, fragment: &str) -> Option<&str> {
        let fragment_chars: Vec<char> = fragment.chars().collect();
        let max = max_typo_distance(fragment_chars.len());
        if max == 0 {
            return None;
        }
        let lengths = fragment_chars.len().saturating_sub(max)..=fragment_chars.len() + max;
        let cost: usize = lengths
            .clone()
            .filter_map(|len| self.by_length.get(&len))
            .map(Vec::len)
            .sum();
        if cost > self.budget.get() {
            return None;
        }
        self.budget.set(self.budget.get() - cost);

        let fragment_is_ascii = fragment.is_ascii();
        let mut rows = EditRows::default();
        // The fewest edits seen, its anchor, and whether another anchor shares it.
        let mut best: Option<(usize, &str, bool)> = None;
        for &index in lengths.filter_map(|len| self.by_length.get(&len)).flatten() {
            let name = self.names[index].as_str();
            if name == fragment {
                continue;
            }
            let distance = if fragment_is_ascii && name.is_ascii() {
                bounded_edit_distance(fragment.as_bytes(), name.as_bytes(), max, &mut rows)
            } else {
                let name_chars: Vec<char> = name.chars().collect();
                bounded_edit_distance(&fragment_chars, &name_chars, max, &mut rows)
            };
            let Some(distance) = distance else { continue };
            best = match best {
                Some((fewest, _, _)) if distance > fewest => best,
                Some((fewest, seen, _)) if distance == fewest => Some((fewest, seen, true)),
                _ => Some((distance, name, false)),
            };
        }
        match best {
            Some((_, anchor, false)) => Some(anchor),
            _ => None,
        }
    }
}

fn normalized_key(anchor: &str) -> String {
    anchor.trim_matches('-').to_lowercase()
}

/// Stands for a distance too large to matter; small enough that adding one
/// to it cannot overflow.
const UNREACHABLE: usize = usize::MAX / 2;

/// Reusable rows for [`bounded_edit_distance`], so comparing a fragment with
/// every anchor of a large document allocates once.
#[derive(Default)]
struct EditRows {
    before_previous: Vec<usize>,
    previous: Vec<usize>,
    current: Vec<usize>,
}

/// Edit distance between `a` and `b` counting an adjacent transposition as one
/// edit (optimal string alignment), or `None` once it exceeds `max`.
///
/// The shared prefix and suffix are set aside first, and the table is
/// abandoned as soon as a whole row exceeds `max`, so anchors that differ
/// from the fragment early cost a few cells rather than the full table.
fn bounded_edit_distance<T: PartialEq>(a: &[T], b: &[T], max: usize, rows: &mut EditRows) -> Option<usize> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let (a, b) = (&a[prefix..], &b[prefix..]);
    let suffix = a.iter().rev().zip(b.iter().rev()).take_while(|(x, y)| x == y).count();
    let (a, b) = (&a[..a.len() - suffix], &b[..b.len() - suffix]);
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    if a.is_empty() || b.is_empty() {
        return Some(a.len().max(b.len()));
    }

    let EditRows {
        before_previous,
        previous,
        current,
    } = rows;
    for row in [&mut *before_previous, &mut *previous, &mut *current] {
        row.clear();
        row.resize(b.len() + 1, UNREACHABLE);
    }
    for (j, cell) in previous.iter_mut().enumerate().take(max + 1) {
        *cell = j;
    }
    // Only cells within `max` of the diagonal can hold a distance of `max` or
    // less, so each row computes that band and nothing else: the work is
    // proportional to `max * len`, not `len * len`. The cells just outside the
    // band are marked unreachable because the next row reads them.
    for i in 1..=a.len() {
        let low = i.saturating_sub(max).max(1);
        let high = (i + max).min(b.len());
        current[0] = if i <= max { i } else { UNREACHABLE };
        if low > 1 {
            current[low - 1] = UNREACHABLE;
        }
        if high < b.len() {
            current[high + 1] = UNREACHABLE;
        }
        let mut row_min = current[0];
        if low > high {
            return None;
        }
        for j in low..=high {
            let substitution = usize::from(a[i - 1] != b[j - 1]);
            let mut cost = (previous[j] + 1)
                .min(current[j - 1] + 1)
                .min(previous[j - 1] + substitution);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cost = cost.min(before_previous[j - 2] + 1);
            }
            current[j] = cost;
            row_min = row_min.min(cost);
        }
        if row_min > max {
            return None;
        }
        std::mem::swap(before_previous, previous);
        std::mem::swap(previous, current);
    }
    Some(previous[b.len()]).filter(|distance| *distance <= max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggest(fragment: &str, candidates: &[&str]) -> Option<String> {
        // Every ordering must agree: the candidates come from hash sets.
        let forward = AnchorSuggester::new(candidates.iter().copied())
            .suggest(fragment)
            .map(str::to_string);
        let backward = AnchorSuggester::new(candidates.iter().rev().copied())
            .suggest(fragment)
            .map(str::to_string);
        assert_eq!(forward, backward, "suggestion depends on candidate order");
        forward
    }

    #[test]
    fn suggestion_strips_hyphens_and_case_on_both_sides() {
        assert_eq!(suggest("purpose", &["purpose-", "other"]), Some("purpose-".to_string()));
        assert_eq!(suggest("-purpose-", &["purpose"]), Some("purpose".to_string()));
        assert_eq!(suggest("FAQ", &["faq"]), Some("faq".to_string()));
    }

    #[test]
    fn suggestion_ignores_an_all_hyphen_fragment() {
        assert_eq!(suggest("---", &["-", "--", "a"]), None);
    }

    #[test]
    fn distinct_anchors_with_one_trimmed_form_are_ambiguous_even_when_a_typo_match_exists() {
        assert_eq!(suggest("-purpose", &["purpose", "purpose-", "purposes"]), None);
    }

    #[test]
    fn the_same_anchor_in_two_sets_is_one_candidate() {
        assert_eq!(
            suggest("purpose", &["purpose-", "purpose-"]),
            Some("purpose-".to_string())
        );
        assert_eq!(
            suggest("instalation", &["installation", "installation"]),
            Some("installation".to_string())
        );
    }

    #[test]
    fn typo_distance_is_bounded_by_fragment_length() {
        assert_eq!(suggest("faz", &["faq"]), None, "3 chars allow no edits");
        assert_eq!(
            suggest("setp", &["step"]),
            Some("step".to_string()),
            "a transposition is one edit"
        );
        assert_eq!(suggest("stap", &["step"]), Some("step".to_string()));
        assert_eq!(suggest("stxx", &["step"]), None, "4 chars allow one edit");
        assert_eq!(
            suggest("installxtion", &["installation"]),
            Some("installation".to_string())
        );
        assert_eq!(
            suggest("instaltion", &["installation"]),
            Some("installation".to_string())
        );
        assert_eq!(suggest("instaltin", &["installation"]), None, "beyond two edits");
    }

    #[test]
    fn a_tie_for_the_nearest_anchor_gives_no_suggestion() {
        assert_eq!(suggest("alpha-ose", &["alpha-one", "alpha-owe"]), None);
        assert_eq!(
            suggest("alpha-xne", &["alpha-one", "alpha-owe"]),
            Some("alpha-one".to_string())
        );
    }

    #[test]
    fn edit_distance_counts_characters_not_bytes() {
        assert_eq!(suggest("caféx-bar", &["café-bar"]), Some("café-bar".to_string()));
    }

    #[test]
    fn no_candidates_give_no_suggestion_and_an_empty_hint() {
        assert_eq!(suggest("anything", &[]), None);
        assert_eq!(AnchorSuggester::new(std::iter::empty()).hint("anything"), "");
        assert_eq!(
            AnchorSuggester::new(["purpose-"].into_iter()).hint("purpose"),
            " (did you mean '#purpose-'?)"
        );
    }

    #[test]
    fn a_query_over_the_remaining_budget_is_skipped_whole() {
        let names: Vec<String> = (0..1000).map(|i| format!("section-{i:04}-alpha")).collect();
        let suggester = AnchorSuggester::new(names.iter().map(String::as_str));
        let typo = "sectxon-0001-alpha";
        assert_eq!(suggester.suggest(typo), Some("section-0001-alpha"));
        let mut answered = 1;
        while suggester.suggest(typo).is_some() {
            answered += 1;
            assert!(answered <= 200, "the budget never ran out");
        }
        assert_eq!(
            answered,
            COMPARISON_BUDGET / 1000,
            "budget is spent per candidate compared"
        );
        assert_eq!(suggester.hint(typo), "", "an exhausted budget gives the plain message");
        assert_eq!(
            suggester.suggest("section-0001-alpha-"),
            Some("section-0001-alpha"),
            "the hyphen match needs no comparisons"
        );
    }

    /// Plain optimal-string-alignment table, the oracle for the pruned version.
    fn naive_osa(a: &[u8], b: &[u8]) -> usize {
        let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
        for (i, row) in d.iter_mut().enumerate() {
            row[0] = i;
        }
        for (j, cell) in d[0].iter_mut().enumerate() {
            *cell = j;
        }
        for i in 1..=a.len() {
            for j in 1..=b.len() {
                let sub = usize::from(a[i - 1] != b[j - 1]);
                d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + sub);
                if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                    d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
                }
            }
        }
        d[a.len()][b.len()]
    }

    #[test]
    fn long_strings_that_differ_only_at_the_ends_are_compared_in_linear_work() {
        let middle = "x".repeat(200_000);
        let a = format!("a{middle}a");
        let b = format!("b{middle}b");
        let mut rows = EditRows::default();
        assert_eq!(bounded_edit_distance(a.as_bytes(), b.as_bytes(), 2, &mut rows), Some(2));
        assert_eq!(bounded_edit_distance(a.as_bytes(), b.as_bytes(), 1, &mut rows), None);
        let shifted = format!("{middle}a");
        let other = format!("a{middle}");
        assert_eq!(
            bounded_edit_distance(shifted.as_bytes(), other.as_bytes(), 2, &mut rows),
            Some(2)
        );
    }

    #[test]
    fn bounded_edit_distance_agrees_with_the_full_table() {
        fn strings(alphabet: &[u8], max_len: usize) -> Vec<Vec<u8>> {
            let mut all = vec![Vec::new()];
            let mut frontier = vec![Vec::new()];
            for _ in 0..max_len {
                let mut next = Vec::new();
                for s in &frontier {
                    for &c in alphabet {
                        let mut t: Vec<u8> = s.clone();
                        t.push(c);
                        next.push(t);
                    }
                }
                all.extend(next.iter().cloned());
                frontier = next;
            }
            all
        }
        let all = strings(b"abc", 6);
        let mut rows = EditRows::default();
        let mut compared = 0usize;
        for a in &all {
            for b in &all {
                let full = naive_osa(a, b);
                for max in 0..=2 {
                    let expected = (full <= max).then_some(full);
                    assert_eq!(
                        bounded_edit_distance(a, b, max, &mut rows),
                        expected,
                        "{:?} vs {:?} within {max}",
                        String::from_utf8_lossy(a),
                        String::from_utf8_lossy(b)
                    );
                    compared += 1;
                }
            }
        }
        assert!(compared > 100_000, "the comparison ran over {compared} cases");
    }
}
